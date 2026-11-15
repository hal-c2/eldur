//! The runtime and font module a page gets hold only what the page can reach.
//!
//! `eldur_runtime.js` and `eldur_fonts.js` are `.pragma library` files of
//! top-level functions and variables. A page's QML and `page.js` call into
//! them as `Eldur.<name>` and `EldurFonts.<name>`; from
//! those names, every top-level statement a kept statement refers to is kept,
//! until nothing more is reached. A statement whose effect can be seen without
//! being named (a polyfill, an IIFE, a call) is always kept, and so is
//! everything it refers to. A statement that only fills in a declaration
//! (`X.prototype.f = function…`, `Object.defineProperty(X, …)`) goes with `X`,
//! unless the object it changes may be one made elsewhere (`var X = Promise;`,
//! `X.prototype = Array.prototype;`, `var X = { p: Promise };`): then filling
//! it in changes that object, and the statement is always kept.
//!
//! References are resolved with oxc's semantic analysis, so a parameter or
//! local that shares a top-level name does not keep it.

use std::collections::{BTreeSet, HashMap};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, BindingPattern, Expression,
    IdentifierReference, ObjectPropertyKind, PropertyKey, SimpleAssignmentTarget, Statement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::{GetSpan, SourceType};

/// What a top-level statement is to the shaker.
enum Role {
    /// Declares these names, and runs nothing anyone sees.
    Declares(Vec<String>),
    /// Fills in the declaration of this name: kept with it.
    Extends(String),
    /// Kept whatever the page uses.
    Effect,
}

/// `library` with only the top-level statements reachable from `used` (the
/// names a page reaches in it, as `Eldur.<name>`) and from the statements that
/// run on import. The QML directives at the top (`.pragma`, `.import`) are kept
/// as they are. A library oxc cannot parse is returned whole.
#[must_use]
pub fn shake_library<'u>(library: &str, used: impl IntoIterator<Item = &'u str>) -> String {
    // QML's directives are not JavaScript.
    let (directives, body) = split_directives(library);

    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, body, SourceType::script()).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return library.to_owned();
    }
    let program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program).semantic;
    let scoping = semantic.scoping();

    let statements = &program.body;
    let lib = Library {
        scoping,
        replaced: replaced_names(&program),
    };
    let foreign = foreign_paths(statements, &lib);
    let roles: Vec<Role> = statements
        .iter()
        .map(|s| match role(s, &lib) {
            Role::Extends(_)
                if changed_object(s).is_some_and(|object| may_be_foreign(&object, &foreign)) =>
            {
                Role::Effect
            }
            role => role,
        })
        .collect();
    let mut declared_by: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut extended_by: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, role) in roles.iter().enumerate() {
        match role {
            Role::Declares(names) => {
                for name in names {
                    declared_by.entry(name).or_default().push(index);
                }
            }
            Role::Extends(name) => extended_by.entry(name).or_default().push(index),
            Role::Effect => {}
        }
    }
    let refers: Vec<BTreeSet<String>> = statements
        .iter()
        .map(|statement| {
            let mut found = TopLevelReferences {
                scoping,
                names: BTreeSet::new(),
            };
            found.visit_statement(statement);
            found.names
        })
        .collect();

    let mut kept = vec![false; statements.len()];
    let mut queue: Vec<usize> = Vec::new();
    let reach_name = |name: &str, queue: &mut Vec<usize>, kept: &mut Vec<bool>| {
        for list in [declared_by.get(name), extended_by.get(name)]
            .into_iter()
            .flatten()
        {
            for &index in list {
                if !kept[index] {
                    kept[index] = true;
                    queue.push(index);
                }
            }
        }
    };
    for (index, role) in roles.iter().enumerate() {
        if matches!(role, Role::Effect) {
            kept[index] = true;
            queue.push(index);
        }
    }
    for name in used {
        reach_name(name, &mut queue, &mut kept);
    }
    while let Some(index) = queue.pop() {
        if let Role::Extends(name) = &roles[index] {
            reach_name(name, &mut queue, &mut kept);
        }
        for name in &refers[index] {
            reach_name(name, &mut queue, &mut kept);
        }
    }

    // Each statement takes the comments and blank lines before it, and the rest
    // of the line it ends on, with it.
    let mut out = String::with_capacity(library.len());
    out.push_str(directives);
    // The script's own directives (`"use strict";`) stay whatever goes.
    let mut from = program.directives.last().map_or(0, |d| {
        let end = d.span.end as usize;
        let line_end = body[end..].find('\n').map_or(body.len(), |n| end + n + 1);
        statements
            .first()
            .map_or(line_end, |s| line_end.min(s.span().start as usize))
    });
    out.push_str(&body[..from]);
    // A string statement that comes first once what was before it goes would
    // become a directive; an empty statement before it keeps it one.
    let mut in_prologue = true;
    for (index, statement) in statements.iter().enumerate() {
        let end = statement.span().end as usize;
        let line_end = body[end..].find('\n').map_or(body.len(), |n| end + n + 1);
        let next_start = statements
            .get(index + 1)
            .map_or(body.len(), |s| s.span().start as usize);
        let to = if next_start < line_end { end } else { line_end };
        if kept[index] {
            if in_prologue && is_string_statement(statement) {
                out.push_str(";\n");
            }
            in_prologue = false;
            out.push_str(&body[from.min(to)..to]);
        }
        from = from.max(to);
    }
    out.push_str(&body[from..]);
    out
}

fn is_string_statement(statement: &Statement<'_>) -> bool {
    matches!(statement, Statement::ExpressionStatement(e)
        if matches!(e.expression, Expression::StringLiteral(_)))
}

/// The leading QML directives (`.pragma`, `.import`) and blank lines, and the
/// JavaScript after them.
fn split_directives(runtime: &str) -> (&str, &str) {
    let mut body_start = 0;
    for line in runtime.split_inclusive('\n') {
        if line.trim_start().starts_with('.') || line.trim().is_empty() {
            body_start += line.len();
        } else {
            break;
        }
    }
    runtime.split_at(body_start)
}

/// A step of a member path: a property name, or `None` for one that is
/// computed and may be any.
type Step = Option<String>;

/// The member paths, from a top-level name, that a top-level statement stores
/// an object made elsewhere at (`var P = Promise;` stores at `P`,
/// `X.prototype = Array.prototype;` at `X.prototype`, `var o = { p: Promise };`
/// at `o.p`). Filling in anything under such a path changes that object.
fn foreign_paths(statements: &[Statement<'_>], lib: &Library<'_>) -> Vec<Vec<Step>> {
    let mut found = Vec::new();
    for statement in statements {
        match statement {
            Statement::VariableDeclaration(v) => {
                for declarator in &v.declarations {
                    match (&declarator.id, &declarator.init) {
                        (BindingPattern::BindingIdentifier(id), Some(init)) => {
                            stores(vec![Some(id.name.to_string())], init, lib, &mut found);
                        }
                        // `var { Promise: P } = globalThis;`: what each name
                        // gets is read out of an object, so may be anything.
                        (pattern, Some(_)) => found.extend(
                            pattern
                                .get_binding_identifiers()
                                .into_iter()
                                .map(|id| vec![Some(id.name.to_string())]),
                        ),
                        (_, None) => {}
                    }
                }
            }
            Statement::ExpressionStatement(e) => {
                if let Expression::AssignmentExpression(a) = &e.expression {
                    if let Some(at) = target_path(&a.left) {
                        stores(at, &a.right, lib, &mut found);
                    } else if let Some(pattern) = a.left.as_assignment_target_pattern() {
                        let mut names = Names(Vec::new());
                        names.visit_assignment_target_pattern(pattern);
                        found.extend(names.0.into_iter().map(|n| vec![Some(n)]));
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Adds the paths under `at` where `value` puts an object it does not make.
fn stores(at: Vec<Step>, value: &Expression<'_>, lib: &Library<'_>, found: &mut Vec<Vec<Step>>) {
    match value {
        Expression::ParenthesizedExpression(p) => stores(at, &p.expression, lib, found),
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::UnaryExpression(_)
        | Expression::BinaryExpression(_) => {}
        // A built-in made with nothing to wrap is new; `new Object(Promise)`
        // is `Promise`, and any other constructor may return what it likes.
        Expression::NewExpression(n) if fresh_construction(n, lib) => {}
        Expression::ObjectExpression(o) => {
            for property in &o.properties {
                match property {
                    // A getter may return anything.
                    ObjectPropertyKind::ObjectProperty(p)
                        if p.kind != oxc_ast::ast::PropertyKind::Init =>
                    {
                        let mut under = at.clone();
                        under.push(p.key.static_name().map(|n| n.to_string()));
                        found.push(under);
                    }
                    ObjectPropertyKind::ObjectProperty(p) => {
                        let mut under = at.clone();
                        under.push(p.key.static_name().map(|n| n.to_string()));
                        stores(under, &p.value, lib, found);
                    }
                    ObjectPropertyKind::SpreadProperty(_) => {
                        let mut under = at.clone();
                        under.push(None);
                        found.push(under);
                    }
                }
            }
        }
        Expression::ArrayExpression(a) => {
            for element in &a.elements {
                if let Some(e) = element.as_expression() {
                    let mut under = at.clone();
                    under.push(None);
                    stores(under, e, lib, found);
                }
            }
        }
        _ => found.push(at),
    }
}

fn target_path(target: &AssignmentTarget<'_>) -> Option<Vec<Step>> {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(id) => Some(vec![Some(id.name.to_string())]),
        target => match target.as_simple_assignment_target()? {
            SimpleAssignmentTarget::StaticMemberExpression(m) => {
                let mut at = member_path(&m.object)?;
                at.push(Some(m.property.name.to_string()));
                Some(at)
            }
            SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                let mut at = member_path(&m.object)?;
                at.push(computed_step(&m.expression));
                Some(at)
            }
            _ => None,
        },
    }
}

/// `X.a["b"]` as `[X, a, b]`.
fn member_path(expression: &Expression<'_>) -> Option<Vec<Step>> {
    match expression {
        Expression::Identifier(id) => Some(vec![Some(id.name.to_string())]),
        Expression::ParenthesizedExpression(p) => member_path(&p.expression),
        Expression::StaticMemberExpression(m) => {
            let mut at = member_path(&m.object)?;
            at.push(Some(m.property.name.to_string()));
            Some(at)
        }
        Expression::ComputedMemberExpression(m) => {
            let mut at = member_path(&m.object)?;
            at.push(computed_step(&m.expression));
            Some(at)
        }
        _ => None,
    }
}

fn computed_step(key: &Expression<'_>) -> Step {
    match key {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

/// The path whose object a filling-in statement changes: `X.prototype` for
/// `X.prototype.f = …` and for `Object.defineProperty(X.prototype, …)`, and
/// none for `X = …`, which changes no object.
fn changed_object(statement: &Statement<'_>) -> Option<Vec<Step>> {
    let Statement::ExpressionStatement(e) = statement else {
        return None;
    };
    match &e.expression {
        Expression::AssignmentExpression(a) => {
            let mut at = target_path(&a.left)?;
            at.pop();
            (!at.is_empty()).then_some(at)
        }
        Expression::CallExpression(call) => member_path(call.arguments.first()?.as_expression()?),
        _ => None,
    }
}

/// Whether `object` may be, or be inside, an object stored at a foreign path.
fn may_be_foreign(object: &[Step], foreign: &[Vec<Step>]) -> bool {
    foreign.iter().any(|path| {
        path.len() <= object.len()
            && path
                .iter()
                .zip(object)
                .all(|(a, b)| a.is_none() || b.is_none() || a == b)
    })
}

fn role(statement: &Statement<'_>, lib: &Library<'_>) -> Role {
    match statement {
        Statement::FunctionDeclaration(f) => {
            f.id.as_ref()
                .map_or(Role::Effect, |id| Role::Declares(vec![id.name.to_string()]))
        }
        Statement::VariableDeclaration(v) => {
            let mut names = Vec::new();
            for declarator in &v.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    return Role::Effect;
                };
                if declarator.init.as_ref().is_some_and(|e| !pure(e, lib)) {
                    return Role::Effect;
                }
                names.push(id.name.to_string());
            }
            Role::Declares(names)
        }
        Statement::ExpressionStatement(e) => {
            extends(&e.expression, lib).map_or(Role::Effect, Role::Extends)
        }
        Statement::EmptyStatement(_) => Role::Declares(Vec::new()),
        _ => Role::Effect,
    }
}

/// The top-level name an expression statement only adds to, if that is all it does.
fn extends(expression: &Expression<'_>, lib: &Library<'_>) -> Option<String> {
    match expression {
        Expression::AssignmentExpression(a) => {
            if !pure(&a.right, lib) {
                return None;
            }
            match &a.left {
                AssignmentTarget::AssignmentTargetIdentifier(id) => top_level(id, lib.scoping),
                target => {
                    let member = target.as_simple_assignment_target()?;
                    let object = match member {
                        SimpleAssignmentTarget::StaticMemberExpression(m) => &m.object,
                        SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                            if !pure(&m.expression, lib) {
                                return None;
                            }
                            &m.object
                        }
                        _ => return None,
                    };
                    base(object, lib)
                }
            }
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(callee) = &call.callee else {
                return None;
            };
            let Expression::Identifier(object) = &callee.object else {
                return None;
            };
            if object.name != "Object"
                || !matches!(
                    callee.property.name.as_str(),
                    "defineProperty" | "defineProperties"
                )
                || call
                    .arguments
                    .iter()
                    .any(|a| a.as_expression().is_none_or(|e| !pure(e, lib)))
            {
                return None;
            }
            match call.arguments.first()? {
                Argument::SpreadElement(_) => None,
                first => base(first.as_expression()?, lib),
            }
        }
        _ => None,
    }
}

/// The top-level name a member chain starts from: `X` in `X.prototype.f`.
fn base(expression: &Expression<'_>, lib: &Library<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(id) => top_level(id, lib.scoping),
        Expression::StaticMemberExpression(m) => base(&m.object, lib),
        Expression::ComputedMemberExpression(m) if pure(&m.expression, lib) => base(&m.object, lib),
        _ => None,
    }
}

fn top_level(id: &IdentifierReference<'_>, scoping: &Scoping) -> Option<String> {
    let symbol = scoping.get_reference(id.reference_id.get()?).symbol_id()?;
    (scoping.symbol_scope_id(symbol) == scoping.root_scope_id()).then(|| id.name.to_string())
}

/// Evaluating it runs no code of anyone's: the statement holding it can go
/// when nothing names what it declares. A function is pure here, as defining
/// one runs nothing. A member read is not: it may run a getter.
fn pure(expression: &Expression<'_>, lib: &Library<'_>) -> bool {
    match expression {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::Identifier(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.iter().all(|e| quiet(e, lib)),
        Expression::ArrayExpression(a) => a.elements.iter().all(|e| match e {
            ArrayExpressionElement::SpreadElement(_) => false,
            ArrayExpressionElement::Elision(_) => true,
            other => other.as_expression().is_some_and(|e| pure(e, lib)),
        }),
        Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(p) => {
                pure(&p.value, lib)
                    && match &p.key {
                        PropertyKey::StaticIdentifier(_) | PropertyKey::PrivateIdentifier(_) => {
                            true
                        }
                        key => key.as_expression().is_some_and(|e| quiet(e, lib)),
                    }
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::UnaryExpression(u) => {
            use oxc_syntax::operator::UnaryOperator as U;
            match u.operator {
                U::Delete => false,
                U::Typeof | U::Void | U::LogicalNot => pure(&u.argument, lib),
                U::UnaryPlus | U::UnaryNegation | U::BitwiseNot => quiet(&u.argument, lib),
            }
        }
        Expression::BinaryExpression(b) => {
            use oxc_syntax::operator::BinaryOperator as B;
            match b.operator {
                B::StrictEquality | B::StrictInequality => {
                    pure(&b.left, lib) && pure(&b.right, lib)
                }
                // `in` and `instanceof` read the right side's traps and `Symbol.hasInstance`.
                B::In | B::Instanceof => false,
                _ => quiet(&b.left, lib) && quiet(&b.right, lib),
            }
        }
        Expression::LogicalExpression(l) => pure(&l.left, lib) && pure(&l.right, lib),
        Expression::ConditionalExpression(c) => {
            pure(&c.test, lib) && pure(&c.consequent, lib) && pure(&c.alternate, lib)
        }
        Expression::ParenthesizedExpression(p) => pure(&p.expression, lib),
        Expression::NewExpression(n) => fresh_construction(n, lib),
        _ => false,
    }
}

/// A built-in made with no arguments: an empty object, made without running
/// anyone's code. With arguments, a collection runs their iterators and a
/// regular expression reads their `Symbol.match` and `toString`. The name
/// must still be the built-in: declared nowhere in the script, not assigned,
/// and not written as a property by name (`globalThis.Map = …`; the forms
/// followed are those [`replaced_names`] lists).
fn fresh_construction(n: &oxc_ast::ast::NewExpression<'_>, lib: &Library<'_>) -> bool {
    let Expression::Identifier(id) = &n.callee else {
        return false;
    };
    let global = id
        .reference_id
        .get()
        .is_some_and(|r| lib.scoping.get_reference(r).symbol_id().is_none());
    let assigned = lib.replaced.contains(id.name.as_str())
        || lib
            .scoping
            .root_unresolved_references()
            .iter()
            .any(|(name, refs)| {
                name.as_str() == id.name.as_str()
                    && refs
                        .iter()
                        .any(|r| lib.scoping.get_reference(*r).is_write())
            });
    n.arguments.is_empty()
        && global
        && !assigned
        && matches!(
            id.name.as_str(),
            "Map" | "Set" | "WeakMap" | "WeakSet" | "Object" | "Array" | "RegExp"
        )
}

/// Whether `expression` is pure and turning it into a primitive runs no code:
/// an object's `valueOf`, `toString` or `Symbol.toPrimitive` may.
fn quiet(expression: &Expression<'_>, lib: &Library<'_>) -> bool {
    primitive(expression) && pure(expression, lib)
}

/// Whether `expression` always gives a primitive. A name may hold an object.
fn primitive(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::UnaryExpression(_)
        | Expression::BinaryExpression(_) => true,
        Expression::LogicalExpression(l) => primitive(&l.left) && primitive(&l.right),
        Expression::ConditionalExpression(c) => primitive(&c.consequent) && primitive(&c.alternate),
        Expression::ParenthesizedExpression(p) => primitive(&p.expression),
        _ => false,
    }
}

/// The name a key written as a literal stands for.
fn literal_key(key: &Expression<'_>) -> Option<String> {
    match key {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => t
            .quasis
            .first()
            .map(|q| q.value.cooked.unwrap_or(q.value.raw).to_string()),
        _ => None,
    }
}

/// What the shaker reads a library with.
struct Library<'s> {
    scoping: &'s Scoping,
    /// Property names written anywhere in the library by name.
    replaced: BTreeSet<String>,
}

/// The property names `program` writes by name: `x.N = …`, `x["N"] = …`,
/// ``x[`N`] = …``, `Object.defineProperty(x, "N", …)` (or `Reflect`'s),
/// `Reflect.set(x, "N", …)`, and the
/// keys of an object literal given to `Object.defineProperties` or
/// `Object.assign`. A name only known at run time is not followed: a key
/// computed from a variable, or an object passed by name.
fn replaced_names(program: &oxc_ast::ast::Program<'_>) -> BTreeSet<String> {
    struct Writes(BTreeSet<String>);
    impl<'a> Visit<'a> for Writes {
        fn visit_assignment_expression(&mut self, a: &oxc_ast::ast::AssignmentExpression<'a>) {
            match &a.left {
                AssignmentTarget::StaticMemberExpression(m) => {
                    self.0.insert(m.property.name.to_string());
                }
                AssignmentTarget::ComputedMemberExpression(m) => {
                    self.0.extend(literal_key(&m.expression));
                }
                _ => {}
            }
            walk::walk_assignment_expression(self, a);
        }
        fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
            if let Expression::StaticMemberExpression(callee) = &call.callee {
                let argument = |i: usize| call.arguments.get(i).and_then(|a| a.as_expression());
                match callee.property.name.as_str() {
                    "defineProperty" | "set" => self.0.extend(argument(1).and_then(literal_key)),
                    "defineProperties" | "assign" => {
                        for i in 1..call.arguments.len() {
                            if let Some(Expression::ObjectExpression(o)) = argument(i) {
                                for p in &o.properties {
                                    if let ObjectPropertyKind::ObjectProperty(p) = p {
                                        self.0.extend(match &p.key {
                                            PropertyKey::StaticIdentifier(id) => {
                                                Some(id.name.to_string())
                                            }
                                            key => key.as_expression().and_then(literal_key),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            walk::walk_call_expression(self, call);
        }
    }
    let mut writes = Writes(BTreeSet::new());
    writes.visit_program(program);
    writes.0
}

/// The names an assignment pattern assigns, and any it reads for defaults.
struct Names(Vec<String>);

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, id: &IdentifierReference<'a>) {
        self.0.push(id.name.to_string());
    }
}

/// The top-level names a statement refers to, wherever in it.
struct TopLevelReferences<'s> {
    scoping: &'s Scoping,
    names: BTreeSet<String>,
}

impl<'a> Visit<'a> for TopLevelReferences<'_> {
    fn visit_identifier_reference(&mut self, id: &IdentifierReference<'a>) {
        if let Some(name) = top_level(id, self.scoping) {
            self.names.insert(name);
        }
        walk::walk_identifier_reference(self, id);
    }
}

/// The names `text` reaches in a library it imports `as alias`: every
/// `<alias>.<name>` (`Eldur.document`, `EldurFonts.family`).
pub fn library_names(text: &str, alias: &str, names: &mut BTreeSet<String>) {
    let prefix = format!("{alias}.");
    let mut rest = text;
    while let Some(at) = rest.find(&prefix) {
        let before = rest[..at].chars().next_back();
        rest = &rest[at + prefix.len()..];
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
            continue;
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(rest.len());
        if end > 0 {
            names.insert(rest[..end].to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNTIME: &str = ".pragma library\n.import \"x.js\" as X\n\n\
// The polyfill every page needs.\n\
Array.prototype.at = Array.prototype.at || function(i) { return this[i]; };\n\
\n\
// Used.\n\
function used() { return helper(1); }\n\
function helper(x) { var unused = 0; return x + _table.a; }\n\
var _table = { a: 1 };\n\
\n\
// Not used by anyone.\n\
function lonely(helper) { return helper; }\n\
function Thing() {}\n\
Thing.prototype.go = function() { return lonely(1); };\n\
var _later = registerNow();\n\
function registerNow() { return 1; }\n";

    #[test]
    fn keeps_what_is_reached_and_what_runs() {
        let out = shake_library(RUNTIME, ["used"]);
        assert_eq!(
            out,
            ".pragma library\n.import \"x.js\" as X\n\n\
// The polyfill every page needs.\n\
Array.prototype.at = Array.prototype.at || function(i) { return this[i]; };\n\
\n\
// Used.\n\
function used() { return helper(1); }\n\
function helper(x) { var unused = 0; return x + _table.a; }\n\
var _table = { a: 1 };\n\
var _later = registerNow();\n\
function registerNow() { return 1; }\n"
        );
    }

    #[test]
    fn a_prototype_goes_with_its_constructor() {
        let out = shake_library(RUNTIME, ["Thing"]);
        assert!(
            out.contains("function Thing() {}\nThing.prototype.go"),
            "{out}"
        );
        assert!(out.contains("function lonely(helper)"), "{out}");
        assert!(!out.contains("function used()"), "{out}");
    }

    #[test]
    fn filling_in_another_name_for_a_built_in_is_kept() {
        let library = "var _P = Promise;\n\
_P.prototype.finally = function(f) { return this.then(f); };\n\
var _Q;\n\
_Q = Array;\n\
_Q.prototype.last = function() { return this[this.length - 1]; };\n\
var _o = { p: Promise };\n\
_o.p.prototype.fin = function() {};\n\
function _X() {}\n\
_X.prototype = Array.prototype;\n\
_X.prototype.first = function() { return this[0]; };\n\
var _own = { a: { b: 1 } };\n\
_own.a.f = function() {};\n\
function used() {}\n";
        let out = shake_library(library, ["used"]);
        assert_eq!(
            out,
            "var _P = Promise;\n\
_P.prototype.finally = function(f) { return this.then(f); };\n\
var _Q;\n\
_Q = Array;\n\
_Q.prototype.last = function() { return this[this.length - 1]; };\n\
var _o = { p: Promise };\n\
_o.p.prototype.fin = function() {};\n\
function _X() {}\n\
_X.prototype = Array.prototype;\n\
_X.prototype.first = function() { return this[0]; };\n\
function used() {}\n"
        );
    }

    #[test]
    fn filling_in_an_object_read_or_wrapped_from_elsewhere_is_kept() {
        let library = "var { Promise: _P } = globalThis;\n\
_P.prototype.a = function() {};\n\
var _Q;\n\
({ Array: _Q } = globalThis);\n\
_Q.prototype.b = function() {};\n\
var _box = { p: Promise.prototype };\n\
_box.p.c = function() {};\n\
var _get = { get p() { return Promise; } };\n\
_get.p.d = function() {};\n\
var _W = new Object(Promise);\n\
_W.prototype.e = function() {};\n\
var _own = new Map();\n\
_own.f = function() {};\n";
        let out = shake_library(library, []);
        for kept in [".a =", ".b =", ".c =", ".d =", ".e ="] {
            assert!(out.contains(kept), "{kept} went:\n{out}");
        }
        assert!(!out.contains("_own"), "{out}");
    }

    #[test]
    fn turning_an_object_into_a_primitive_may_run_code() {
        let object = "({ valueOf() { externalNotify(); return 0; }, toString() { externalNotify(); return \"\"; } })";
        for (i, form) in [
            format!("{object} + 1"),
            format!("1 < {object}"),
            format!("-{object}"),
            format!("`${{{object}}}`"),
            format!("{{ [{object}]: 1 }}"),
            format!("\"a\" in {object}"),
            "someName + 1".to_owned(),
        ]
        .iter()
        .enumerate()
        {
            let out = shake_library(&format!("var unused{i} = {form};\n"), []);
            assert!(out.contains(&format!("var unused{i} =")), "{form}: {out}");
        }
        let out = shake_library(
            "var a = 1 + 2 * 3;\nvar b = `x${1 + 1}`;\nvar c = typeof someName === \"undefined\";\nvar d = !someName;\n",
            [],
        );
        assert!(out.trim().is_empty(), "{out}");
    }

    #[test]
    fn a_member_read_may_run_a_getter() {
        let out = shake_library(
            "var unused = ({ get value() { externalNotify(); return 0; } }).value;\n\
var _k = { a: 1 }[\"a\"];\n",
            [],
        );
        assert!(
            out.contains("var unused =") && out.contains("var _k ="),
            "{out}"
        );
    }

    #[test]
    fn a_constructor_given_arguments_may_run_code() {
        let library = "var _s = new Set({ [Symbol.iterator]: function() { return [][Symbol.iterator](); } });\n\
var _r = new RegExp({ toString: function() { return \"x\"; } });\n\
var _empty = new Set();\n";
        let out = shake_library(library, []);
        assert!(
            out.contains("var _s =") && out.contains("var _r ="),
            "{out}"
        );
        assert!(!out.contains("_empty"), "{out}");
    }

    #[test]
    fn a_string_statement_does_not_become_a_directive() {
        let out = shake_library(
            "\"use strict\";\nvar unused = 0;\n\"not a directive\";\nfunction used() { return this; }\n",
            ["used"],
        );
        assert_eq!(
            out,
            "\"use strict\";\n;\n\"not a directive\";\nfunction used() { return this; }\n"
        );
        let out = shake_library(
            "var unused = 0;\n\"use strict\";\nfunction used() { return this; }\n",
            ["used"],
        );
        assert_eq!(
            out,
            ";\n\"use strict\";\nfunction used() { return this; }\n"
        );
        // A statement on the directive's line goes or stays on its own.
        let out = shake_library(
            "\"use strict\"; var a = helper;\nfunction helper() {}\nfunction used() {}\n",
            ["used"],
        );
        assert_eq!(out, "\"use strict\"; function used() {}\n");
        let out = shake_library(
            "\"use strict\"; var unused = {\n  a: 1\n};\nfunction used() {}\n",
            ["used"],
        );
        assert_eq!(out, "\"use strict\"; function used() {}\n");
    }

    #[test]
    fn a_local_constructor_with_a_built_in_name_may_run_code() {
        let out = shake_library(
            "function Set() { externalNotify(); }\nvar unused = new Set();\n",
            [],
        );
        assert!(out.contains("var unused = new Set();"), "{out}");
        for replace in [
            "Map = function() { externalNotify(); };",
            "globalThis.Map = function() { externalNotify(); };",
            "this[\"Map\"] = function() { externalNotify(); };",
            "Object.defineProperty(globalThis, \"Map\", { value: function() { externalNotify(); } });",
            "this[`Map`] = function() { externalNotify(); };",
            "Object.defineProperties(globalThis, { Map: { value: function() { externalNotify(); } } });",
            "Object.assign(globalThis, { \"Map\": function() { externalNotify(); } });",
            "Object.assign(globalThis, {}, { Map() { externalNotify(); } });",
            "Object.defineProperties(self, { [\"Map\"]: { value: function() { externalNotify(); } } });",
            "Reflect.set(globalThis, \"Map\", function() { externalNotify(); });",
        ] {
            let out = shake_library(&format!("{replace}\nvar unused = new Map();\n"), []);
            assert!(out.contains("var unused = new Map();"), "{replace}: {out}");
        }
    }

    #[test]
    fn names_are_read_from_qml_and_scripts() {
        let mut names = BTreeSet::new();
        library_names(
            "onClicked: Page.f(Eldur.event(x)); Eldur.wrap(a).hidden; MyEldur.no; x.Eldur.no",
            "Eldur",
            &mut names,
        );
        assert_eq!(names.iter().collect::<Vec<_>>(), ["event", "wrap"]);
        names.clear();
        library_names(
            "font.family: EldurFonts.family(\"serif\"); Eldur.no",
            "EldurFonts",
            &mut names,
        );
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["family"]);
    }

    #[test]
    fn the_shipped_runtime_parses() {
        let whole = crate::RUNTIME_JS;
        let shaken = shake_library(whole, ["init"]);
        assert!(
            shaken.len() < whole.len(),
            "the runtime did not parse, or nothing was dropped"
        );
        let allocator = Allocator::default();
        let check = Parser::new(
            &allocator,
            split_directives(&shaken).1,
            SourceType::script(),
        )
        .parse();
        assert!(check.diagnostics.is_empty(), "{:?}", check.diagnostics);
    }

    #[test]
    fn the_shipped_fonts_parse() {
        let whole = crate::FONTS_JS;
        let shaken = shake_library(whole, ["family"]);
        assert!(
            shaken.len() < whole.len(),
            "the fonts did not parse, or nothing was dropped"
        );
        assert!(shaken.contains("function family("), "{shaken}");
        assert!(!shaken.contains("function placeRun("), "{shaken}");
    }
}
