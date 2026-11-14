//! The runtime and font module a page gets hold only what the page can reach.
//!
//! `eldur_runtime.js` and `eldur_fonts.js` are `.pragma library` files of
//! top-level functions and variables. A page's QML and `page.js` call into
//! them as `Eldur.<name>` and `EldurFonts.<name>`; from
//! those names, every top-level statement a kept statement refers to is kept,
//! until nothing more is reached. A statement whose effect can be seen without
//! being named (a polyfill, an IIFE, a call) is always kept, and so is
//! everything it refers to. A statement that only fills in a declaration
//! (`X.prototype.f = function…`, `Object.defineProperty(X, …)`) goes with `X`.
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
    let roles: Vec<Role> = statements.iter().map(|s| role(s, scoping)).collect();
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
    let mut from = 0usize;
    for (index, statement) in statements.iter().enumerate() {
        let end = statement.span().end as usize;
        let line_end = body[end..].find('\n').map_or(body.len(), |n| end + n + 1);
        let next_start = statements
            .get(index + 1)
            .map_or(body.len(), |s| s.span().start as usize);
        let to = if next_start < line_end { end } else { line_end };
        if kept[index] {
            out.push_str(&body[from..to]);
        }
        from = to;
    }
    out.push_str(&body[from..]);
    out
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

fn role(statement: &Statement<'_>, scoping: &Scoping) -> Role {
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
                if declarator.init.as_ref().is_some_and(|e| !pure(e)) {
                    return Role::Effect;
                }
                names.push(id.name.to_string());
            }
            Role::Declares(names)
        }
        Statement::ExpressionStatement(e) => {
            extends(&e.expression, scoping).map_or(Role::Effect, Role::Extends)
        }
        Statement::EmptyStatement(_) => Role::Declares(Vec::new()),
        _ => Role::Effect,
    }
}

/// The top-level name an expression statement only adds to, if that is all it does.
fn extends(expression: &Expression<'_>, scoping: &Scoping) -> Option<String> {
    match expression {
        Expression::AssignmentExpression(a) => {
            if !pure(&a.right) {
                return None;
            }
            match &a.left {
                AssignmentTarget::AssignmentTargetIdentifier(id) => top_level(id, scoping),
                target => {
                    let member = target.as_simple_assignment_target()?;
                    let object = match member {
                        SimpleAssignmentTarget::StaticMemberExpression(m) => &m.object,
                        SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                            if !pure(&m.expression) {
                                return None;
                            }
                            &m.object
                        }
                        _ => return None,
                    };
                    base(object, scoping)
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
                    .any(|a| a.as_expression().is_none_or(|e| !pure(e)))
            {
                return None;
            }
            match call.arguments.first()? {
                Argument::SpreadElement(_) => None,
                first => base(first.as_expression()?, scoping),
            }
        }
        _ => None,
    }
}

/// The top-level name a member chain starts from: `X` in `X.prototype.f`.
fn base(expression: &Expression<'_>, scoping: &Scoping) -> Option<String> {
    match expression {
        Expression::Identifier(id) => top_level(id, scoping),
        Expression::StaticMemberExpression(m) => base(&m.object, scoping),
        Expression::ComputedMemberExpression(m) if pure(&m.expression) => base(&m.object, scoping),
        _ => None,
    }
}

fn top_level(id: &IdentifierReference<'_>, scoping: &Scoping) -> Option<String> {
    let symbol = scoping.get_reference(id.reference_id.get()?).symbol_id()?;
    (scoping.symbol_scope_id(symbol) == scoping.root_scope_id()).then(|| id.name.to_string())
}

/// Evaluating it runs no code of anyone's: the statement holding it can go
/// when nothing names what it declares. A function is pure here, as defining
/// one runs nothing; a member read is too, since nothing here has getters on
/// the objects it reads at the top level.
fn pure(expression: &Expression<'_>) -> bool {
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
        Expression::TemplateLiteral(t) => t.expressions.iter().all(pure),
        Expression::StaticMemberExpression(m) => pure(&m.object),
        Expression::ComputedMemberExpression(m) => pure(&m.object) && pure(&m.expression),
        Expression::ArrayExpression(a) => a.elements.iter().all(|e| match e {
            ArrayExpressionElement::SpreadElement(_) => false,
            ArrayExpressionElement::Elision(_) => true,
            other => other.as_expression().is_some_and(pure),
        }),
        Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(p) => {
                pure(&p.value)
                    && match &p.key {
                        PropertyKey::StaticIdentifier(_) | PropertyKey::PrivateIdentifier(_) => {
                            true
                        }
                        key => key.as_expression().is_some_and(pure),
                    }
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::UnaryExpression(u) => {
            u.operator != oxc_syntax::operator::UnaryOperator::Delete && pure(&u.argument)
        }
        Expression::BinaryExpression(b) => pure(&b.left) && pure(&b.right),
        Expression::LogicalExpression(l) => pure(&l.left) && pure(&l.right),
        Expression::ConditionalExpression(c) => {
            pure(&c.test) && pure(&c.consequent) && pure(&c.alternate)
        }
        Expression::ParenthesizedExpression(p) => pure(&p.expression),
        // The built-in collections make an empty object and run nothing; a
        // Proxy's traps run only when the proxy is used, which keeps it.
        Expression::NewExpression(n) => {
            matches!(&n.callee, Expression::Identifier(id)
                if matches!(id.name.as_str(), "Map" | "Set" | "WeakMap" | "WeakSet" | "Object" | "Array" | "RegExp" | "Proxy"))
                && n.arguments
                    .iter()
                    .all(|a| a.as_expression().is_some_and(pure))
        }
        _ => false,
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
