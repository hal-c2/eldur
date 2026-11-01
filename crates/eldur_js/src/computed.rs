//! Whether a computed member with a name no one can read may reach
//! `addEventListener` or an `on…` handler, so that a listener may exist for any
//! event. Only calling or writing such a member can add one; a read has to go on
//! to be called first, so a read counts only where its value can leave.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionExpression, AssignmentExpression,
    AssignmentTarget, BindingPattern, CallExpression, ComputedMemberExpression, Expression,
    IdentifierReference, ObjectProperty, Program, ReturnStatement, SimpleAssignmentTarget,
    UpdateExpression, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{Scoping, SymbolId};
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

use crate::resolve::unwrap_parens;

pub(crate) fn may_hear_unnamed(program: &Program<'_>, scoping: &Scoping) -> bool {
    let mut writes = Writes {
        scoping,
        of: HashMap::new(),
    };
    writes.visit_program(program);
    let mut uses = Uses {
        scoping,
        writes: &writes.of,
        hears: false,
        held: HashSet::new(),
        left: HashSet::new(),
    };
    uses.visit_program(program);
    uses.hears || uses.held.iter().any(|sym| uses.left.contains(sym))
}

fn symbol_of(scoping: &Scoping, id: &IdentifierReference<'_>) -> Option<SymbolId> {
    scoping.get_reference(id.reference_id.get()?).symbol_id()
}

/// What a variable is given, as far as a member's name or owner goes.
#[derive(Clone, Copy, PartialEq)]
enum Written {
    Number,
    /// An object or array the page wrote, or parsed: what it holds is what the
    /// analysis has read already, and it is no element.
    Data,
    Alias(SymbolId),
    Unknown,
}

fn written(scoping: &Scoping, value: &Expression<'_>) -> Written {
    match unwrap_parens(value) {
        Expression::NumericLiteral(_) | Expression::UpdateExpression(_) => Written::Number,
        Expression::UnaryExpression(u)
            if matches!(
                u.operator,
                UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus | UnaryOperator::BitwiseNot
            ) =>
        {
            Written::Number
        }
        Expression::BinaryExpression(b) if b.operator == BinaryOperator::Addition => {
            let number = |e| written(scoping, e) == Written::Number;
            if number(&b.left) && number(&b.right) {
                Written::Number
            } else {
                Written::Unknown
            }
        }
        Expression::BinaryExpression(b)
            if b.operator.is_arithmetic() || b.operator.is_bitwise() =>
        {
            Written::Number
        }
        Expression::StaticMemberExpression(m) if m.property.name == "length" => Written::Number,
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => Written::Data,
        Expression::CallExpression(call) => match unwrap_parens(&call.callee) {
            Expression::StaticMemberExpression(m) => match (&m.object, m.property.name.as_str()) {
                (Expression::Identifier(o), _) if o.name == "Math" => Written::Number,
                (Expression::Identifier(o), "parse") if o.name == "JSON" => Written::Data,
                _ => Written::Unknown,
            },
            Expression::Identifier(f) if f.name == "parseInt" || f.name == "Number" => {
                Written::Number
            }
            _ => Written::Unknown,
        },
        Expression::Identifier(id) => {
            symbol_of(scoping, id).map_or(Written::Unknown, Written::Alias)
        }
        _ => Written::Unknown,
    }
}

struct Writes<'s> {
    scoping: &'s Scoping,
    of: HashMap<SymbolId, Vec<Written>>,
}

impl<'a> Visit<'a> for Writes<'_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(bind) = &it.id
            && let Some(sym) = bind.symbol_id.get()
            && let Some(init) = &it.init
        {
            let value = written(self.scoping, init);
            self.of.entry(sym).or_default().push(value);
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.left
            && let Some(sym) = symbol_of(self.scoping, id)
        {
            let value = if it.operator.is_assign() {
                written(self.scoping, &it.right)
            } else {
                Written::Unknown
            };
            self.of.entry(sym).or_default().push(value);
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        if let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &it.argument
            && let Some(sym) = symbol_of(self.scoping, id)
        {
            self.of.entry(sym).or_default().push(Written::Number);
        }
        walk::walk_update_expression(self, it);
    }
}

struct Uses<'s> {
    scoping: &'s Scoping,
    writes: &'s HashMap<SymbolId, Vec<Written>>,
    hears: bool,
    /// Variables a computed read went into.
    held: HashSet<SymbolId>,
    /// Variables whose value is called, or leaves for somewhere it may be.
    left: HashSet<SymbolId>,
}

impl Uses<'_> {
    /// Every write of `sym` is `want`. A parameter or a loop's variable has none
    /// to read, and could be anything.
    fn always(&self, sym: SymbolId, want: Written, seen: &mut HashSet<SymbolId>) -> bool {
        if !seen.insert(sym) {
            return true;
        }
        let Some(all) = self.writes.get(&sym) else {
            return false;
        };
        all.iter().all(|w| match *w {
            Written::Alias(other) => self.always(other, want, seen),
            w => w == want,
        })
    }

    /// A method's name could be in `it`, and the object could have methods.
    fn may_name_a_method(&self, it: &ComputedMemberExpression<'_>) -> bool {
        match unwrap_parens(&it.expression) {
            // A written name is read where it is written (the main analysis).
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return false,
            key => match written(self.scoping, key) {
                Written::Number => return false,
                Written::Alias(sym) if self.always(sym, Written::Number, &mut HashSet::new()) => {
                    return false;
                }
                _ => {}
            },
        }
        if let Expression::Identifier(owner) = unwrap_parens(&it.object)
            && let Some(sym) = symbol_of(self.scoping, owner)
            && self.always(sym, Written::Data, &mut HashSet::new())
        {
            return false;
        }
        true
    }

    /// The value of `value` may be such a member.
    fn is_member(&self, value: &Expression<'_>) -> bool {
        match unwrap_parens(value) {
            Expression::ComputedMemberExpression(m) => self.may_name_a_method(m),
            Expression::LogicalExpression(l) => self.is_member(&l.left) || self.is_member(&l.right),
            Expression::ConditionalExpression(c) => {
                self.is_member(&c.consequent) || self.is_member(&c.alternate)
            }
            Expression::SequenceExpression(s) => {
                s.expressions.last().is_some_and(|e| self.is_member(e))
            }
            _ => false,
        }
    }

    /// `value` goes where it may be called.
    fn leaves(&mut self, value: &Expression<'_>) {
        if self.is_member(value) {
            self.hears = true;
        }
        self.left_names(value);
    }

    fn left_names(&mut self, value: &Expression<'_>) {
        match unwrap_parens(value) {
            Expression::Identifier(id) => {
                if let Some(sym) = symbol_of(self.scoping, id) {
                    self.left.insert(sym);
                }
            }
            Expression::LogicalExpression(l) => {
                self.left_names(&l.left);
                self.left_names(&l.right);
            }
            Expression::ConditionalExpression(c) => {
                self.left_names(&c.consequent);
                self.left_names(&c.alternate);
            }
            Expression::SequenceExpression(s) => {
                if let Some(last) = s.expressions.last() {
                    self.left_names(last);
                }
            }
            _ => {}
        }
    }

    fn holds(&mut self, sym: SymbolId, value: &Expression<'_>) {
        if self.is_member(value) {
            self.held.insert(sym);
        }
        self.left_names(value);
    }
}

/// What is plainly no function, written to any member, adds no listener.
fn can_be_a_function(value: &Expression<'_>) -> bool {
    !matches!(
        unwrap_parens(value),
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::ArrayExpression(_)
            | Expression::ObjectExpression(_)
            | Expression::BinaryExpression(_)
            | Expression::UnaryExpression(_)
            | Expression::UpdateExpression(_)
    )
}

impl<'a> Visit<'a> for Uses<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.leaves(&it.callee);
        if let Expression::StaticMemberExpression(m) = unwrap_parens(&it.callee)
            && matches!(m.property.name.as_str(), "call" | "apply" | "bind")
        {
            self.leaves(&m.object);
        }
        for argument in &it.arguments {
            match argument {
                Argument::SpreadElement(s) => self.leaves(&s.argument),
                a => {
                    if let Some(value) = a.as_expression() {
                        self.leaves(value);
                    }
                }
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        match &it.left {
            AssignmentTarget::ComputedMemberExpression(m)
                if self.may_name_a_method(m) && can_be_a_function(&it.right) =>
            {
                self.hears = true;
            }
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                if let Some(sym) = symbol_of(self.scoping, id) {
                    self.holds(sym, &it.right);
                }
            }
            _ => self.leaves(&it.right),
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let Some(init) = &it.init {
            match &it.id {
                BindingPattern::BindingIdentifier(bind) => {
                    if let Some(sym) = bind.symbol_id.get() {
                        self.holds(sym, init);
                    }
                }
                _ => self.leaves(init),
            }
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        if let Some(value) = &it.argument {
            self.leaves(value);
        }
        walk::walk_return_statement(self, it);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if let Some(value) = it.get_expression() {
            self.leaves(value);
        }
        walk::walk_arrow_function_expression(self, it);
    }

    fn visit_array_expression_element(&mut self, it: &ArrayExpressionElement<'a>) {
        if let Some(value) = it.as_expression() {
            self.leaves(value);
        }
        walk::walk_array_expression_element(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.leaves(&it.value);
        walk::walk_object_property(self, it);
    }
}
