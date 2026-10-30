use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget,
    AwaitExpression, BindingPattern, CallExpression, CatchClause, Class, ClassElement, ClassType,
    ComputedMemberExpression, Expression, ForInStatement, ForOfStatement, ForStatement, Function,
    IdentifierReference, MethodDefinition, MethodDefinitionKind, ObjectExpression,
    ObjectPropertyKind, PropertyKey, ReturnStatement, SimpleAssignmentTarget, Statement,
    StaticMemberExpression, UnaryExpression, UpdateExpression, VariableDeclaration,
    VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{AssignmentOperator, UnaryOperator};
use oxc_syntax::scope::ScopeFlags;

use crate::edit::SpanEditor;
use crate::parse::{async_body_has_unlowerable_constructs, collect_binding_identifiers};
use crate::resolve::{
    ScriptQueryBindings, extract_dom_query_from_call, extract_string_literal, is_global_document,
    is_global_ident, is_global_window, known_dom_event_from_on_prop, unwrap_parens,
};
use crate::{DomQuery, DomResolver, EventBinding, QmlTarget, TargetKind};

/// Shared counters and generated top-level artifacts across all scripts in `page.js`.
#[derive(Default)]
pub(crate) struct SharedRewriteState {
    pub next_uid: u32,
    pub generated_decls: Vec<String>,
    pub bindings: Vec<EventBinding>,
    /// `(qml_id, event)` -> `slot_var_name` for dynamic `.on<event> = ...`
    pub prop_handler_slots: HashMap<(String, String), String>,
    /// Set of `(qml_id, event)` pairs that need dynamic slot assignment rather than
    /// single top-level static hoisting.
    pub dynamic_on_props: HashSet<(String, String)>,
    /// Set of `(qml_id, event)` pairs whose `addEventListener` calls must go through
    /// `H2Q.listen` rather than single top-level static hoisting because the script
    /// registers multiple listeners, removes listeners, or dispatches events dynamically.
    pub dynamic_listener_events: HashSet<(String, String)>,
    /// Tracks which `(qml_id, event)` pairs already have a forwarding `EventBinding`
    /// registered for `H2Q.listen` / `H2Q.dispatchEvent`.
    pub dynamic_listener_slots: HashSet<(String, String)>,
    /// Top-level identifiers already claimed so generated helper names never collide.
    pub used_names: HashSet<String>,
}

impl SharedRewriteState {
    pub(crate) fn fresh_id(&mut self) -> u32 {
        let id = self.next_uid;
        self.next_uid += 1;
        id
    }

    pub(crate) fn alloc_fn_name(&mut self, base: &str) -> String {
        if self.used_names.insert(base.to_string()) {
            return base.to_string();
        }
        let mut suffix = 2u32;
        loop {
            let candidate = format!("{base}_{suffix}");
            if self.used_names.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    pub(crate) fn ensure_prop_handler_slot(&mut self, target: &QmlTarget, event: &str) -> String {
        let key = (target.qml_id.clone(), event.to_string());
        if let Some(existing) = self.prop_handler_slots.get(&key) {
            return existing.clone();
        }
        let safe_ev = sanitize_ident(event);
        let slot_var = self.alloc_fn_name(&format!("__h2q_prop_{safe_ev}_{}", target.qml_id));
        let fn_name = self.alloc_fn_name(&format!("__h2q_on_{safe_ev}_{}_prop", target.qml_id));
        self.generated_decls.push(format!("var {slot_var} = null;"));
        self.generated_decls.push(format!(
            "function {fn_name}(event) {{\n    if ({slot_var}) {{\n        {slot_var}.call({}, event);\n    }}\n}}",
            target.qml_id
        ));
        self.bindings.push(EventBinding {
            target: target.clone(),
            event: event.to_string(),
            function: fn_name,
        });
        self.prop_handler_slots.insert(key, slot_var.clone());
        slot_var
    }

    pub(crate) fn register_dynamic_listener_slot(&mut self, target: &QmlTarget, event: &str) {
        let key = (target.qml_id.clone(), event.to_string());
        if !self.dynamic_listener_slots.insert(key) {
            return;
        }
        let uid = self.fresh_id();
        let safe_ev = sanitize_ident(event);
        let fn_name = self.alloc_fn_name(&format!("__h2q_on_{safe_ev}_{}_{uid}", target.qml_id));
        self.generated_decls.push(format!(
            "function {fn_name}(event) {{\n    H2Q.dispatchEvent({}, event);\n}}",
            target.qml_id
        ));
        self.bindings.push(EventBinding {
            target: target.clone(),
            event: event.to_string(),
            function: fn_name,
        });
    }
}

pub(crate) fn sanitize_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

pub(crate) fn resolve_query_to_single_target(
    query: &DomQuery,
    resolver: &dyn DomResolver,
) -> Option<QmlTarget> {
    match query {
        DomQuery::ById(_) => {
            let matches = resolver.resolve(query);
            if matches.len() == 1 {
                matches.into_iter().next()
            } else {
                None
            }
        }
        DomQuery::Selector(_) => resolver.resolve(query).into_iter().next(),
        DomQuery::SelectorAll(_) => None,
    }
}

/// Returns `true` when `decl` is an unmutated single-identifier binding to a pure
/// DOM query (or alias thereof) that statically resolves to a single [`QmlTarget`].
///
/// Since every reference to `decl` is rewritten directly to `target.qml_id`, keeping
/// `const btn = btn;` would collide with or shadow the QML id in `Main.qml`.
pub(crate) fn is_elidable_dom_declarator(
    decl: &VariableDeclarator<'_>,
    bindings: &ScriptQueryBindings<'_>,
    resolver: &dyn DomResolver,
) -> bool {
    let BindingPattern::BindingIdentifier(bind_id) = &decl.id else {
        return false;
    };
    let Some(sym_id) = bind_id.symbol_id.get() else {
        return false;
    };
    let Some(query) = bindings.symbol_queries.get(&sym_id) else {
        return false;
    };
    if resolve_query_to_single_target(query, resolver).is_none() {
        return false;
    }
    let Some(init) = &decl.init else {
        return false;
    };
    match unwrap_parens(init) {
        Expression::Identifier(_) => true,
        Expression::CallExpression(call) => call.arguments.len() == 1,
        _ => false,
    }
}

/// AST visitor that records DOM-to-QML and ES-syntax-lowering edits into a [`SpanEditor`].
pub(crate) struct AstLowerer<'a, 'b> {
    pub editor: SpanEditor<'a>,
    pub scoping: &'a Scoping,
    pub bindings: &'b ScriptQueryBindings<'a>,
    pub resolver: &'b dyn DomResolver,
    pub shared: &'b mut SharedRewriteState,
    /// Lexical stack of what `this` resolves to per non-arrow function boundary.
    pub this_stack: Vec<Option<QmlTarget>>,
    /// Whether the innermost function (regular or arrow) is a lowerable `async` function.
    pub async_stack: Vec<bool>,
    /// True when lowering an HTML inline handler attribute (`onclick="..."`).
    pub is_handler_attr: bool,
    /// Span of the final top-level statement in a handler attribute (so trailing
    /// `return false` becomes `event.preventDefault()` cleanly).
    pub handler_final_stmt_span: Option<Span>,
    /// Nesting depth inside functions (regular or arrow) relative to the root snippet.
    pub fn_depth: u32,
    /// Nesting depth inside classes relative to the root snippet.
    pub class_depth: u32,
    /// Nesting depth inside `for (...)` headers where variable declarations are not standalone lines.
    pub in_for_header: u32,
    /// Nesting depth inside `for (... in/of ...)` headers where a single LHS binding cannot be empty.
    pub in_for_in_of_header: u32,
    /// Nesting depth inside member/call object position where unary `!x.enabled` needs parens.
    pub member_object_depth: u32,
    /// Block-nested `var` names hoisted out of top-level blocks/loops (`fn_depth == 0 && class_depth == 0`)
    /// so they are declared at file scope in `page.js` rather than trapped inside `__h2q_init`.
    pub hoisted_top_vars: Vec<String>,
    /// Static field initializers (`ClassName.field = val;`) generated for top-level/block
    /// `ClassDeclaration`s, keyed by `Class::span`.
    pub post_class_static_inits: HashMap<(u32, u32), Vec<String>>,
    /// Span starts of `Function` nodes that belong to a `MethodDefinition`, so `visit_function`
    /// leaves method header syntax (`async foo(...)`) to `visit_method_definition`.
    method_fn_spans: HashSet<(u32, u32)>,
}

impl<'a, 'b> AstLowerer<'a, 'b> {
    pub(crate) fn new(
        source: &'a str,
        scoping: &'a Scoping,
        bindings: &'b ScriptQueryBindings<'a>,
        resolver: &'b dyn DomResolver,
        shared: &'b mut SharedRewriteState,
    ) -> Self {
        Self {
            editor: SpanEditor::new(source),
            scoping,
            bindings,
            resolver,
            shared,
            this_stack: vec![None],
            async_stack: Vec::new(),
            is_handler_attr: false,
            handler_final_stmt_span: None,
            fn_depth: 0,
            class_depth: 0,
            in_for_header: 0,
            in_for_in_of_header: 0,
            member_object_depth: 0,
            hoisted_top_vars: Vec::new(),
            post_class_static_inits: HashMap::new(),
            method_fn_spans: HashSet::new(),
        }
    }

    pub(crate) fn new_handler(
        source: &'a str,
        scoping: &'a Scoping,
        bindings: &'b ScriptQueryBindings<'a>,
        resolver: &'b dyn DomResolver,
        shared: &'b mut SharedRewriteState,
        target: QmlTarget,
        handler_final_stmt_span: Option<Span>,
    ) -> Self {
        let mut lowerer = Self::new(source, scoping, bindings, resolver, shared);
        lowerer.this_stack = vec![Some(target)];
        lowerer.is_handler_attr = true;
        lowerer.handler_final_stmt_span = handler_final_stmt_span;
        lowerer
    }

    pub(crate) fn resolve_single_target(&self, expr: &Expression<'_>) -> Option<QmlTarget> {
        let e = unwrap_parens(expr);
        if matches!(e, Expression::ThisExpression(_)) {
            return self.this_stack.last().and_then(Clone::clone);
        }
        if let Some(query) = self.bindings.resolve_expr(e) {
            return resolve_query_to_single_target(&query, self.resolver);
        }
        if let Expression::StaticMemberExpression(member) = e
            && is_global_document(&member.object, self.scoping)
        {
            match member.property.name.as_str() {
                "body" => {
                    return self
                        .resolver
                        .resolve(&DomQuery::Selector("body".to_string()))
                        .into_iter()
                        .next();
                }
                "documentElement" => {
                    return self
                        .resolver
                        .resolve(&DomQuery::Selector("html".to_string()))
                        .into_iter()
                        .next();
                }
                _ => {}
            }
        }
        None
    }

    /// Rewrites a function/callback expression while binding `this` inside a direct
    /// non-arrow `FunctionExpression` to `this_target`.
    pub(crate) fn rewrite_listener_callback_expr(
        &mut self,
        expr: &Expression<'a>,
        this_target: Option<QmlTarget>,
    ) -> String {
        let unwrapped = unwrap_parens(expr);
        if let Expression::FunctionExpression(func) = unwrapped {
            self.visit_function_with_this(func, ScopeFlags::Function, this_target);
        } else {
            self.visit_expression(expr);
        }
        self.editor.extract_and_consume(expr.span())
    }

    fn visit_function_with_this(
        &mut self,
        it: &Function<'a>,
        _flags: ScopeFlags,
        bound_this: Option<QmlTarget>,
    ) {
        let is_method_fn = self.method_fn_spans.contains(&(it.span.start, it.span.end));
        let lower_async = !is_method_fn
            && it.r#async
            && !it.generator
            && it.body.as_ref().is_some_and(|body| {
                !async_body_has_unlowerable_constructs(it.scope_id.get(), self.scoping, |f| {
                    f.visit_function_body(body);
                })
            });

        self.this_stack.push(bound_this);
        self.async_stack.push(lower_async);
        self.fn_depth += 1;

        if let Some(id) = &it.id {
            self.visit_binding_identifier(id);
        }
        self.visit_formal_parameters(&it.params);
        if let Some(body) = &it.body {
            self.visit_function_body(body);
        }

        self.fn_depth -= 1;
        self.async_stack.pop();
        self.this_stack.pop();

        if lower_async && let Some(body) = &it.body {
            // Strip leading `async` before `function`.
            let header_end = it
                .id
                .as_ref()
                .map_or(it.params.span.start, |id| id.span.start);
            let header_span = Span::new(it.span.start, header_end);
            let header_raw = self.editor.slice(header_span);
            if let Some(stripped) = strip_leading_async_keyword(header_raw) {
                self.editor.replace(header_span, stripped);
            }

            let inner_span = Span::new(body.span.start + 1, body.span.end.saturating_sub(1));
            let body_inner = self.editor.extract_and_consume(inner_span);
            let wrapped = wrap_async_generator_body(&body_inner);
            self.editor.replace(body.span, wrapped);
        }
    }

    fn render_call_args(&mut self, args: &[oxc_ast::ast::Argument<'a>]) -> String {
        if args.is_empty() {
            return String::new();
        }
        for arg in args {
            self.visit_argument(arg);
        }
        let first_start = args.first().map_or(0, |a| a.span().start);
        let last_end = args.last().map_or(0, |a| a.span().end);
        self.editor
            .extract_and_consume(Span::new(first_start, last_end))
    }

    fn format_negated_flag(&self, qml_expr: String) -> String {
        if self.member_object_depth > 0 {
            format!("(!{qml_expr})")
        } else {
            format!("!{qml_expr}")
        }
    }

    fn rewrite_resolved_prop_read(&self, target: &QmlTarget, prop: &str) -> Option<String> {
        let id = &target.qml_id;
        match prop {
            "textContent" | "innerText" => {
                if target.kind == TargetKind::Box {
                    Some(format!("{id}Text.text"))
                } else {
                    Some(format!("{id}.text"))
                }
            }
            "innerHTML" => Some(format!("H2Q.getInnerHTML({id})")),
            "value" => match target.kind {
                TargetKind::ComboBox => Some(format!("{id}.currentText")),
                TargetKind::Range | TargetKind::Progress => Some(format!("{id}.value")),
                _ => Some(format!("{id}.text")),
            },
            "selectedIndex" => Some(format!("{id}.currentIndex")),
            "checked" => Some(format!("{id}.checked")),
            "disabled" => Some(self.format_negated_flag(format!("{id}.enabled"))),
            "hidden" => Some(self.format_negated_flag(format!("{id}.visible"))),
            "src" => Some(format!("{id}.source")),
            "placeholder" => Some(format!("{id}.placeholderText")),
            "readOnly" => Some(format!("{id}.readOnly")),
            "offsetWidth" | "clientWidth" => Some(format!("{id}.width")),
            "offsetHeight" | "clientHeight" => Some(format!("{id}.height")),
            "classList" => Some(format!("H2Q.classList({id})")),
            "className" => Some(format!("H2Q.getClassName({id})")),
            "dataset" => Some(format!("H2Q.dataset({id})")),
            "style" => Some(format!("H2Q.wrap({id}).style")),
            _ => None,
        }
    }

    fn try_rewrite_member_assignment(
        &mut self,
        full_span: Span,
        op: AssignmentOperator,
        member_obj: &Expression<'a>,
        prop: &str,
        right: &Expression<'a>,
    ) -> bool {
        // 1. `el.on<event> = rhs`
        if let Some(ev) = known_dom_event_from_on_prop(prop)
            && op == AssignmentOperator::Assign
        {
            if is_global_window(member_obj, self.scoping) && ev == "load" {
                let rhs_str = self.rewrite_listener_callback_expr(right, None);
                self.editor
                    .replace(full_span, format!("H2Q.queueMicrotask({rhs_str})"));
                return true;
            }
            if let Some(target) = self.resolve_single_target(member_obj) {
                let slot = self.shared.ensure_prop_handler_slot(&target, ev);
                let rhs_str = self.rewrite_listener_callback_expr(right, Some(target));
                self.editor
                    .replace(full_span, format!("{slot} = {rhs_str}"));
                return true;
            }
            self.visit_expression(member_obj);
            let obj_str = self.editor.extract_and_consume(member_obj.span());
            let rhs_str = self.rewrite_listener_callback_expr(right, None);
            self.editor.replace(
                full_span,
                format!("H2Q.setOn({obj_str}, {ev:?}, {rhs_str})"),
            );
            return true;
        }

        // 2. `document.title <op> rhs`
        if is_global_document(member_obj, self.scoping) && prop == "title" {
            self.visit_expression(right);
            let rhs_str = self.editor.extract_and_consume(right.span());
            let replacement =
                format_setter_call_assignment("H2Q.getTitle()", op, &rhs_str, |val| {
                    format!("H2Q.setTitle({val})")
                });
            self.editor.replace(full_span, replacement);
            return true;
        }

        // 3. `el.style.<prop> <op> rhs`
        if let Expression::StaticMemberExpression(style_member) = unwrap_parens(member_obj)
            && style_member.property.name == "style"
            && let Some(target) = self.resolve_single_target(&style_member.object)
        {
            self.visit_expression(right);
            let rhs_str = self.editor.extract_and_consume(right.span());
            let id = &target.qml_id;
            let replacement = if prop == "display" {
                let read_expr = format!("({id}.visible ? \"\" : \"none\")");
                format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                    format!("{id}.visible = {val} !== \"none\"")
                })
            } else {
                let read_expr = format!("H2Q.getStyle({id}, {prop:?})");
                format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                    format!("H2Q.setStyle({id}, {prop:?}, {val})")
                })
            };
            self.editor.replace(full_span, replacement);
            return true;
        }

        // 4. `el.<prop> <op> rhs` where `el` resolves statically to a single `QmlTarget`
        if let Some(target) = self.resolve_single_target(member_obj) {
            self.visit_expression(right);
            let rhs_str = self.editor.extract_and_consume(right.span());
            let id = &target.qml_id;
            let replacement = match prop {
                "textContent" | "innerText" => {
                    let lhs = if target.kind == TargetKind::Box {
                        format!("{id}Text.text")
                    } else {
                        format!("{id}.text")
                    };
                    format_assignment(&lhs, op, &rhs_str)
                }
                "innerHTML" => {
                    let read_expr = format!("H2Q.getInnerHTML({id})");
                    format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                        format!("H2Q.setInnerHTML({id}, {val})")
                    })
                }
                "value" => match target.kind {
                    TargetKind::ComboBox => {
                        let read_expr = format!("{id}.currentText");
                        format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                            format!("{id}.currentIndex = {id}.find({val})")
                        })
                    }
                    TargetKind::Range | TargetKind::Progress => {
                        format_assignment(&format!("{id}.value"), op, &rhs_str)
                    }
                    _ => format_assignment(&format!("{id}.text"), op, &rhs_str),
                },
                "selectedIndex" => format_assignment(&format!("{id}.currentIndex"), op, &rhs_str),
                "checked" => format_assignment(&format!("{id}.checked"), op, &rhs_str),
                "disabled" => {
                    format_inverted_bool_assignment(&format!("{id}.enabled"), op, right, &rhs_str)
                }
                "hidden" => {
                    format_inverted_bool_assignment(&format!("{id}.visible"), op, right, &rhs_str)
                }
                "src" => format_assignment(&format!("{id}.source"), op, &rhs_str),
                "placeholder" => format_assignment(&format!("{id}.placeholderText"), op, &rhs_str),
                "readOnly" => format_assignment(&format!("{id}.readOnly"), op, &rhs_str),
                "className" => {
                    let read_expr = format!("H2Q.getClassName({id})");
                    format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                        format!("H2Q.setClassName({id}, {val})")
                    })
                }
                _ => format_assignment(&format!("{id}.{prop}"), op, &rhs_str),
            };
            self.editor.replace(full_span, replacement);
            return true;
        }

        // 5. Unknown receiver DOM-only property write (`x.textContent <op> v`, `x.innerHTML <op> v`)
        if matches!(prop, "textContent" | "innerText" | "innerHTML") {
            let simple_receiver = matches!(unwrap_parens(member_obj), Expression::Identifier(_));
            self.visit_expression(member_obj);
            let obj_str = self.editor.extract_and_consume(member_obj.span());
            self.visit_expression(right);
            let rhs_str = self.editor.extract_and_consume(right.span());
            let (getter, setter) = if prop == "innerHTML" {
                ("H2Q.getInnerHTML", "H2Q.setInnerHTML")
            } else {
                ("H2Q.getText", "H2Q.setText")
            };
            let replacement = if simple_receiver {
                let read_expr = format!("{getter}({obj_str})");
                format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                    format!("{setter}({obj_str}, {val})")
                })
            } else if op == AssignmentOperator::Assign {
                format!("((__o, __v) => ({setter}(__o, __v), __v))({obj_str}, {rhs_str})")
            } else {
                let read_expr = format!("{getter}(__o)");
                let inner = format_setter_call_assignment(&read_expr, op, &rhs_str, |val| {
                    format!("{setter}(__o, {val})")
                });
                format!("((__o) => {inner})({obj_str})")
            };
            self.editor.replace(full_span, replacement);
            return true;
        }

        false
    }
}

impl<'a> Visit<'a> for AstLowerer<'a, '_> {
    fn visit_this_expression(&mut self, it: &oxc_ast::ast::ThisExpression) {
        if let Some(Some(target)) = self.this_stack.last() {
            self.editor.replace(it.span, target.qml_id.clone());
        }
    }

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        if self.is_handler_attr
            && self.fn_depth == 0
            && let Some(arg) = &it.argument
            && let Expression::BooleanLiteral(b) = unwrap_parens(arg)
            && !b.value
        {
            let is_final = self.handler_final_stmt_span == Some(it.span);
            let raw = self.editor.slice(it.span);
            let has_semi = raw.trim_end().ends_with(';');
            let replacement = if is_final {
                if has_semi {
                    "event.preventDefault();".to_string()
                } else {
                    "event.preventDefault()".to_string()
                }
            } else {
                "{ event.preventDefault(); return; }".to_string()
            };
            self.editor.replace(it.span, replacement);
            return;
        }
        walk::walk_return_statement(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if let Some(query) = self.bindings.resolve_ident(it, false)
            && let Some(target) = resolve_query_to_single_target(&query, self.resolver)
        {
            self.editor.replace(it.span, target.qml_id);
            return;
        }

        if !it.is_global_reference(self.scoping) {
            return;
        }

        let mapped = match it.name.as_str() {
            "setTimeout" => Some("H2Q.setTimeout"),
            "setInterval" => Some("H2Q.setInterval"),
            "clearTimeout" => Some("H2Q.clearTimeout"),
            "clearInterval" => Some("H2Q.clearInterval"),
            "requestAnimationFrame" => Some("H2Q.requestAnimationFrame"),
            "cancelAnimationFrame" => Some("H2Q.cancelAnimationFrame"),
            "queueMicrotask" => Some("H2Q.queueMicrotask"),
            "structuredClone" => Some("H2Q.structuredClone"),
            "performance" => Some("H2Q.performance"),
            "localStorage" => Some("H2Q.localStorage"),
            "sessionStorage" => Some("H2Q.sessionStorage"),
            "fetch" => Some("H2Q.fetch"),
            "alert" => Some("H2Q.alert"),
            "confirm" => Some("H2Q.confirm"),
            "prompt" => Some("H2Q.prompt"),
            "atob" => Some("H2Q.atob"),
            "btoa" => Some("H2Q.btoa"),
            "matchMedia" => Some("H2Q.matchMedia"),
            "location" => Some("H2Q.location"),
            "history" => Some("H2Q.history"),
            "navigator" => Some("H2Q.navigator"),
            "Event" => Some("H2Q.Event"),
            "CustomEvent" => Some("H2Q.CustomEvent"),
            "document" => Some("H2Q.document"),
            "window" | "globalThis" | "self" => Some("H2Q.window"),
            _ => None,
        };
        if let Some(rep) = mapped {
            self.editor.replace(it.span, rep);
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        let prop = it.property.name.as_str();

        // 1. Statically resolved element property read
        if let Some(target) = self.resolve_single_target(&it.object) {
            if let Some(rep) = self.rewrite_resolved_prop_read(&target, prop) {
                self.editor.replace(it.span, rep);
                return;
            }
            self.editor.replace(it.object.span(), target.qml_id);
            return;
        }

        // 2. `el.style.<prop>` read on resolved element
        if let Expression::StaticMemberExpression(style_member) = unwrap_parens(&it.object)
            && style_member.property.name == "style"
            && let Some(target) = self.resolve_single_target(&style_member.object)
        {
            let id = &target.qml_id;
            let rep = if prop == "display" {
                format!("({id}.visible ? \"\" : \"none\")")
            } else {
                format!("H2Q.getStyle({id}, {prop:?})")
            };
            self.editor.replace(it.span, rep);
            return;
        }

        // 3. `document.<prop>` read
        if is_global_document(&it.object, self.scoping) {
            let rep = match prop {
                "title" => "H2Q.getTitle()".to_string(),
                "body" => self
                    .resolver
                    .resolve(&DomQuery::Selector("body".to_string()))
                    .into_iter()
                    .next()
                    .map_or_else(|| "H2Q.document.body".to_string(), |t| t.qml_id),
                "documentElement" => self
                    .resolver
                    .resolve(&DomQuery::Selector("html".to_string()))
                    .into_iter()
                    .next()
                    .map_or_else(|| "H2Q.document.documentElement".to_string(), |t| t.qml_id),
                _ => format!("H2Q.document.{prop}"),
            };
            self.editor.replace(it.span, rep);
            return;
        }

        // 4. `window.<prop>` read
        if is_global_window(&it.object, self.scoping) {
            let rep = match prop {
                "localStorage" => "H2Q.localStorage".to_string(),
                "sessionStorage" => "H2Q.sessionStorage".to_string(),
                "location" => "H2Q.location".to_string(),
                "history" => "H2Q.history".to_string(),
                "navigator" => "H2Q.navigator".to_string(),
                "performance" => "H2Q.performance".to_string(),
                "document" => "H2Q.document".to_string(),
                "setTimeout" => "H2Q.setTimeout".to_string(),
                "setInterval" => "H2Q.setInterval".to_string(),
                "clearTimeout" => "H2Q.clearTimeout".to_string(),
                "clearInterval" => "H2Q.clearInterval".to_string(),
                "requestAnimationFrame" => "H2Q.requestAnimationFrame".to_string(),
                "cancelAnimationFrame" => "H2Q.cancelAnimationFrame".to_string(),
                "fetch" => "H2Q.fetch".to_string(),
                "alert" => "H2Q.alert".to_string(),
                "confirm" => "H2Q.confirm".to_string(),
                "prompt" => "H2Q.prompt".to_string(),
                "atob" => "H2Q.atob".to_string(),
                "btoa" => "H2Q.btoa".to_string(),
                "matchMedia" => "H2Q.matchMedia".to_string(),
                _ => format!("H2Q.window.{prop}"),
            };
            self.editor.replace(it.span, rep);
            return;
        }

        // 5. Unknown receiver DOM-only property reads
        if matches!(
            prop,
            "textContent" | "innerText" | "innerHTML" | "classList" | "dataset"
        ) {
            self.visit_expression(&it.object);
            let obj_str = self.editor.extract_and_consume(it.object.span());
            let rep = match prop {
                "textContent" | "innerText" => format!("H2Q.getText({obj_str})"),
                "innerHTML" => format!("H2Q.getInnerHTML({obj_str})"),
                "classList" => format!("H2Q.classList({obj_str})"),
                "dataset" => format!("H2Q.dataset({obj_str})"),
                _ => unreachable!(),
            };
            self.editor.replace(it.span, rep);
            return;
        }

        self.member_object_depth += 1;
        self.visit_expression(&it.object);
        self.member_object_depth -= 1;
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        // `el.style["color"]` read on resolved element
        if let Some(prop) = extract_string_literal(&it.expression)
            && let Expression::StaticMemberExpression(style_member) = unwrap_parens(&it.object)
            && style_member.property.name == "style"
            && let Some(target) = self.resolve_single_target(&style_member.object)
        {
            let id = &target.qml_id;
            let rep = if prop == "display" {
                format!("({id}.visible ? \"\" : \"none\")")
            } else {
                format!("H2Q.getStyle({id}, {prop:?})")
            };
            self.editor.replace(it.span, rep);
            return;
        }

        self.member_object_depth += 1;
        self.visit_expression(&it.object);
        self.member_object_depth -= 1;
        self.visit_expression(&it.expression);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        match &it.left {
            AssignmentTarget::StaticMemberExpression(member) => {
                if self.try_rewrite_member_assignment(
                    it.span,
                    it.operator,
                    &member.object,
                    member.property.name.as_str(),
                    &it.right,
                ) {
                    return;
                }
            }
            AssignmentTarget::ComputedMemberExpression(comp) => {
                if let Some(prop) = extract_string_literal(&comp.expression)
                    && self.try_rewrite_member_assignment(
                        it.span,
                        it.operator,
                        &comp.object,
                        &prop,
                        &it.right,
                    )
                {
                    return;
                }
            }
            _ => {}
        }

        // General logical assignment lowering (`x ??= y`, `x ||= y`, `x &&= y`)
        if it.operator.is_logical() {
            self.visit_assignment_target(&it.left);
            let lhs_str = self.editor.extract_and_consume(it.left.span());
            self.visit_expression(&it.right);
            let rhs_str = self.editor.extract_and_consume(it.right.span());
            let rep = format_assignment(&lhs_str, it.operator, &rhs_str);
            self.editor.replace(it.span, rep);
            return;
        }

        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        if let SimpleAssignmentTarget::StaticMemberExpression(member) = &it.argument
            && let Some(target) = self.resolve_single_target(&member.object)
        {
            let id = &target.qml_id;
            let mapped_lhs = match member.property.name.as_str() {
                "value" => match target.kind {
                    TargetKind::Range | TargetKind::Progress => Some(format!("{id}.value")),
                    _ => Some(format!("{id}.text")),
                },
                "selectedIndex" => Some(format!("{id}.currentIndex")),
                _ => None,
            };
            if let Some(lhs) = mapped_lhs {
                let op_str = it.operator.as_str();
                let rep = if it.prefix {
                    format!("{op_str}{lhs}")
                } else {
                    format!("{lhs}{op_str}")
                };
                self.editor.replace(it.span, rep);
                return;
            }
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Delete
            && let Expression::StaticMemberExpression(member) = unwrap_parens(&it.argument)
            && let Expression::StaticMemberExpression(ds_member) = unwrap_parens(&member.object)
            && ds_member.property.name == "dataset"
            && let Some(target) = self.resolve_single_target(&ds_member.object)
        {
            let rep = format!(
                "delete H2Q.dataset({}).{}",
                target.qml_id, member.property.name
            );
            self.editor.replace(it.span, rep);
            return;
        }
        walk::walk_unary_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        // 1. Direct DOM query call on global `document`
        if let Some(query) = extract_dom_query_from_call(it, self.scoping) {
            match &query {
                DomQuery::ById(_) => {
                    let matches = self.resolver.resolve(&query);
                    if matches.len() == 1 {
                        self.editor.replace(it.span, matches[0].qml_id.clone());
                        return;
                    }
                }
                DomQuery::Selector(_) => {
                    let matches = self.resolver.resolve(&query);
                    if let Some(first) = matches.first() {
                        self.editor.replace(it.span, first.qml_id.clone());
                        return;
                    }
                }
                DomQuery::SelectorAll(_) => {
                    let matches = self.resolver.resolve(&query);
                    if !matches.is_empty() {
                        let joined = matches
                            .iter()
                            .map(|t| t.qml_id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.editor.replace(it.span, format!("[{joined}]"));
                        return;
                    }
                }
            }
        }

        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee) {
            let method = member.property.name.as_str();

            // 2. `console.*`
            if is_global_ident(&member.object, "console", self.scoping) {
                let args_str = self.render_call_args(&it.arguments);
                match method {
                    "table" => {
                        self.editor
                            .replace(it.span, format!("H2Q.consoleTable({args_str})"));
                    }
                    "dir" | "group" | "groupCollapsed" | "timeLog" => {
                        self.editor
                            .replace(it.span, format!("console.log({args_str})"));
                    }
                    "groupEnd" | "countReset" | "clear" => {
                        self.editor.replace(it.span, "(void 0)");
                    }
                    _ => {
                        self.editor
                            .replace(it.span, format!("console.{method}({args_str})"));
                    }
                }
                return;
            }

            // 3. `document.write` / `document.addEventListener` / other `document.*` calls
            if is_global_document(&member.object, self.scoping) {
                if method == "write" || method == "writeln" {
                    let _ = self.render_call_args(&it.arguments);
                    self.editor
                        .replace(it.span, "H2Q.unsupported(\"document.write\")");
                    return;
                }
                if method == "addEventListener"
                    && let Some(ev) = it
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(extract_string_literal)
                    && matches!(ev.as_str(), "DOMContentLoaded" | "load")
                    && let Some(cb) = it.arguments.get(1).and_then(|a| a.as_expression())
                {
                    let cb_str = self.rewrite_listener_callback_expr(cb, None);
                    self.editor
                        .replace(it.span, format!("H2Q.queueMicrotask({cb_str})"));
                    return;
                }
                let args_str = self.render_call_args(&it.arguments);
                self.editor
                    .replace(it.span, format!("H2Q.document.{method}({args_str})"));
                return;
            }

            // 4. `window.addEventListener('load'/'DOMContentLoaded', ...)`
            if is_global_window(&member.object, self.scoping)
                && method == "addEventListener"
                && let Some(ev) = it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                && matches!(ev.as_str(), "DOMContentLoaded" | "load")
                && let Some(cb) = it.arguments.get(1).and_then(|a| a.as_expression())
            {
                let cb_str = self.rewrite_listener_callback_expr(cb, None);
                self.editor
                    .replace(it.span, format!("H2Q.queueMicrotask({cb_str})"));
                return;
            }

            // 5. Method call on statically resolved single `QmlTarget`
            if let Some(target) = self.resolve_single_target(&member.object) {
                let id = &target.qml_id;
                match method {
                    "focus" => {
                        let _ = self.render_call_args(&it.arguments);
                        self.editor
                            .replace(it.span, format!("{id}.forceActiveFocus()"));
                        return;
                    }
                    "blur" => {
                        let _ = self.render_call_args(&it.arguments);
                        self.editor.replace(it.span, format!("{id}.focus = false"));
                        return;
                    }
                    "getBoundingClientRect" => {
                        let _ = self.render_call_args(&it.arguments);
                        self.editor
                            .replace(it.span, format!("H2Q.getBoundingClientRect({id})"));
                        return;
                    }
                    "setAttribute" | "getAttribute" | "removeAttribute" | "hasAttribute" => {
                        let args_str = self.render_call_args(&it.arguments);
                        let sep = if args_str.is_empty() { "" } else { ", " };
                        self.editor
                            .replace(it.span, format!("H2Q.{method}({id}{sep}{args_str})"));
                        return;
                    }
                    "addEventListener" => {
                        if let Some(ev) = it
                            .arguments
                            .first()
                            .and_then(|a| a.as_expression())
                            .and_then(extract_string_literal)
                            && let Some(cb) = it.arguments.get(1).and_then(|a| a.as_expression())
                        {
                            self.shared.register_dynamic_listener_slot(&target, &ev);
                            let ev_str = self.editor.extract_and_consume(it.arguments[0].span());
                            let cb_str =
                                self.rewrite_listener_callback_expr(cb, Some(target.clone()));
                            let mut call_args = vec![id.clone(), ev_str, cb_str];
                            for extra in it.arguments.iter().skip(2) {
                                self.visit_argument(extra);
                                call_args.push(self.editor.extract_and_consume(extra.span()));
                            }
                            self.editor
                                .replace(it.span, format!("H2Q.listen({})", call_args.join(", ")));
                            return;
                        }
                        let args_str = self.render_call_args(&it.arguments);
                        let sep = if args_str.is_empty() { "" } else { ", " };
                        self.editor
                            .replace(it.span, format!("H2Q.listen({id}{sep}{args_str})"));
                        return;
                    }
                    "removeEventListener" => {
                        let args_str = self.render_call_args(&it.arguments);
                        let sep = if args_str.is_empty() { "" } else { ", " };
                        self.editor
                            .replace(it.span, format!("H2Q.unlisten({id}{sep}{args_str})"));
                        return;
                    }
                    "dispatchEvent" => {
                        let args_str = self.render_call_args(&it.arguments);
                        let sep = if args_str.is_empty() { "" } else { ", " };
                        self.editor
                            .replace(it.span, format!("H2Q.dispatchEvent({id}{sep}{args_str})"));
                        return;
                    }
                    "appendChild" | "removeChild" | "insertBefore" | "remove" | "querySelector"
                    | "querySelectorAll" => {
                        let args_str = self.render_call_args(&it.arguments);
                        self.editor
                            .replace(it.span, format!("H2Q.wrap({id}).{method}({args_str})"));
                        return;
                    }
                    _ => {}
                }
            }

            // 6. Unknown receiver DOM-only method calls
            if matches!(
                method,
                "addEventListener"
                    | "removeEventListener"
                    | "dispatchEvent"
                    | "setAttribute"
                    | "getAttribute"
                    | "removeAttribute"
                    | "hasAttribute"
            ) {
                self.visit_expression(&member.object);
                let obj_str = self.editor.extract_and_consume(member.object.span());
                let args_str = self.render_call_args(&it.arguments);
                let sep = if args_str.is_empty() { "" } else { ", " };
                let runtime_fn = match method {
                    "addEventListener" => "listen",
                    "removeEventListener" => "unlisten",
                    other => other,
                };
                self.editor.replace(
                    it.span,
                    format!("H2Q.{runtime_fn}({obj_str}{sep}{args_str})"),
                );
                return;
            }
        }

        walk::walk_call_expression(self, it);
    }

    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        let has_spread = it
            .properties
            .iter()
            .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)));
        if !has_spread {
            walk::walk_object_expression(self, it);
            return;
        }

        walk::walk_object_expression(self, it);

        let mut assign_args: Vec<String> = Vec::new();
        let mut current_props: Vec<String> = Vec::new();

        for (idx, prop_kind) in it.properties.iter().enumerate() {
            match prop_kind {
                ObjectPropertyKind::SpreadProperty(spread) => {
                    if idx == 0 {
                        // Start with `{}` so spreading never mutates the first source object.
                        assign_args.push("{}".to_string());
                    } else if !current_props.is_empty() {
                        assign_args.push(format!("{{ {} }}", current_props.join(", ")));
                        current_props.clear();
                    }
                    let arg_str = self.editor.extract_and_consume(spread.argument.span());
                    assign_args.push(arg_str);
                }
                ObjectPropertyKind::ObjectProperty(prop) => {
                    let prop_str = self.editor.extract_and_consume(prop.span);
                    current_props.push(prop_str);
                }
            }
        }
        if !current_props.is_empty() {
            assign_args.push(format!("{{ {} }}", current_props.join(", ")));
        }

        self.editor.replace(
            it.span,
            format!("Object.assign({})", assign_args.join(", ")),
        );
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        if let Some(init) = &it.init {
            self.in_for_header += 1;
            self.visit_for_statement_init(init);
            self.in_for_header -= 1;
        }
        if let Some(test) = &it.test {
            self.visit_expression(test);
        }
        if let Some(update) = &it.update {
            self.visit_expression(update);
        }
        self.visit_statement(&it.body);
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.in_for_header += 1;
        self.in_for_in_of_header += 1;
        self.visit_for_statement_left(&it.left);
        self.in_for_in_of_header -= 1;
        self.in_for_header -= 1;
        self.visit_expression(&it.right);
        self.visit_statement(&it.body);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.in_for_header += 1;
        self.in_for_in_of_header += 1;
        self.visit_for_statement_left(&it.left);
        self.in_for_in_of_header -= 1;
        self.in_for_header -= 1;
        self.visit_expression(&it.right);
        self.visit_statement(&it.body);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        let hoist_top_var = !self.is_handler_attr
            && self.fn_depth == 0
            && self.class_depth == 0
            && it.kind == VariableDeclarationKind::Var;
        let can_elide = self.in_for_header == 0;
        let has_elidable = can_elide
            && it
                .declarations
                .iter()
                .any(|d| is_elidable_dom_declarator(d, self.bindings, self.resolver));
        let needs_rest_lowering = it.declarations.iter().any(|d| {
            matches!(&d.id, BindingPattern::ObjectPattern(obj) if obj.rest.is_some())
                && d.init.is_some()
        });
        if !hoist_top_var && !has_elidable && !needs_rest_lowering {
            walk::walk_variable_declaration(self, it);
            return;
        }

        walk::walk_variable_declaration(self, it);

        if hoist_top_var {
            let mut bound = Vec::new();
            for decl in &it.declarations {
                if can_elide && is_elidable_dom_declarator(decl, self.bindings, self.resolver) {
                    continue;
                }
                collect_binding_identifiers(&decl.id, &mut bound);
            }
            for id in bound {
                self.hoisted_top_vars.push(id.name.to_string());
            }
        }

        let kw = match it.kind {
            VariableDeclarationKind::Var => "var",
            VariableDeclarationKind::Let => "let",
            VariableDeclarationKind::Const => "const",
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => "let",
        };
        let mut out_decls: Vec<String> = Vec::new();
        let mut all_ok = true;

        for decl in &it.declarations {
            if can_elide && is_elidable_dom_declarator(decl, self.bindings, self.resolver) {
                let _ = self.editor.extract_and_consume(decl.span);
                continue;
            }
            if let BindingPattern::ObjectPattern(obj) = &decl.id
                && let Some(rest) = &obj.rest
                && let Some(init) = &decl.init
            {
                let BindingPattern::BindingIdentifier(rest_ident) = &rest.argument else {
                    all_ok = false;
                    break;
                };
                let mut excluded_keys: Vec<String> = Vec::new();
                let mut regular_props: Vec<String> = Vec::new();
                for prop in &obj.properties {
                    if prop.computed {
                        all_ok = false;
                        break;
                    }
                    let Some(k) = static_property_key_name(&prop.key) else {
                        all_ok = false;
                        break;
                    };
                    excluded_keys.push(format!("{k:?}"));
                    regular_props.push(self.editor.extract_and_consume(prop.span));
                }
                if !all_ok {
                    break;
                }
                let init_str = self.editor.extract_and_consume(init.span());
                if regular_props.is_empty() {
                    out_decls.push(format!(
                        "{} = H2Q.objectRest({init_str}, [])",
                        rest_ident.name
                    ));
                } else {
                    let tmp = format!("__h2q_obj_{}", self.shared.fresh_id());
                    if hoist_top_var {
                        self.hoisted_top_vars.push(tmp.clone());
                    }
                    out_decls.push(format!("{tmp} = {init_str}"));
                    if hoist_top_var {
                        out_decls.push(format!("({{ {} }} = {tmp})", regular_props.join(", ")));
                    } else {
                        out_decls.push(format!("{{ {} }} = {tmp}", regular_props.join(", ")));
                    }
                    out_decls.push(format!(
                        "{} = H2Q.objectRest({tmp}, [{}])",
                        rest_ident.name,
                        excluded_keys.join(", ")
                    ));
                }
            } else if hoist_top_var {
                if self.in_for_in_of_header > 0 && decl.init.is_none() {
                    out_decls.push(self.editor.extract_and_consume(decl.id.span()));
                } else if decl.init.is_some() {
                    let decl_str = self.editor.extract_and_consume(decl.span);
                    if matches!(decl.id, BindingPattern::ObjectPattern(_)) {
                        out_decls.push(format!("({decl_str})"));
                    } else {
                        out_decls.push(decl_str);
                    }
                } else {
                    let _ = self.editor.extract_and_consume(decl.span);
                }
            } else {
                out_decls.push(self.editor.extract_and_consume(decl.span));
            }
        }

        if all_ok {
            let had_semi = self.editor.slice(it.span).trim_end().ends_with(';');
            let semi = if had_semi { ";" } else { "" };
            if out_decls.is_empty() {
                if self.in_for_header > 0 {
                    self.editor.replace(it.span, semi.to_string());
                } else {
                    self.editor.remove_statement_line(it.span);
                }
            } else if hoist_top_var {
                self.editor
                    .replace(it.span, format!("{}{semi}", out_decls.join(", ")));
            } else {
                self.editor
                    .replace(it.span, format!("{kw} {}{semi}", out_decls.join(", ")));
            }
        }
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        walk::walk_catch_clause(self, it);
        if it.param.is_none() {
            let header_span = Span::new(it.span.start, it.body.span.start);
            self.editor.replace(header_span, "catch (__h2q_err) ");
        }
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        walk::walk_await_expression(self, it);
        if self.async_stack.last().copied() == Some(true) {
            let arg_str = self.editor.extract_and_consume(it.argument.span());
            self.editor.replace(it.span, format!("(yield ({arg_str}))"));
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.visit_function_with_this(it, flags, None);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let lower_async = it.r#async
            && !async_body_has_unlowerable_constructs(it.scope_id.get(), self.scoping, |f| {
                f.visit_arrow_function_body(&it.body);
            });

        self.async_stack.push(lower_async);
        self.fn_depth += 1;
        self.visit_formal_parameters(&it.params);
        self.visit_arrow_function_body(&it.body);
        self.fn_depth -= 1;
        self.async_stack.pop();

        if lower_async {
            let prefix_span = Span::new(it.span.start, it.params.span.start);
            let prefix_raw = self.editor.slice(prefix_span);
            if let Some(stripped) = strip_leading_async_keyword(prefix_raw) {
                self.editor.replace(prefix_span, stripped);
            }

            match &it.body {
                ArrowFunctionBody::FunctionBody(body) => {
                    let inner_span =
                        Span::new(body.span.start + 1, body.span.end.saturating_sub(1));
                    let body_inner = self.editor.extract_and_consume(inner_span);
                    self.editor
                        .replace(body.span, wrap_async_generator_body(&body_inner));
                }
                _ => {
                    let body_span = it.body.span();
                    let expr_str = self.editor.extract_and_consume(body_span);
                    self.editor.replace(
                        body_span,
                        format!("H2Q.asyncRun(this, function*() {{ return {expr_str}; }})"),
                    );
                }
            }
        }
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.method_fn_spans
            .insert((it.value.span.start, it.value.span.end));

        let lower_async = it.value.r#async
            && !it.value.generator
            && it.value.body.as_ref().is_some_and(|body| {
                !async_body_has_unlowerable_constructs(it.value.scope_id.get(), self.scoping, |f| {
                    f.visit_function_body(body)
                })
            });

        self.visit_property_key(&it.key);

        self.this_stack.push(None);
        self.async_stack.push(lower_async);
        self.fn_depth += 1;
        self.visit_formal_parameters(&it.value.params);
        if let Some(body) = &it.value.body {
            self.visit_function_body(body);
        }
        self.fn_depth -= 1;
        self.async_stack.pop();
        self.this_stack.pop();

        if lower_async && let Some(body) = &it.value.body {
            let prefix_span = Span::new(it.span.start, it.key.span().start);
            let prefix_raw = self.editor.slice(prefix_span);
            if let Some(stripped) = remove_async_token_in_method_prefix(prefix_raw) {
                self.editor.replace(prefix_span, stripped);
            }
            let inner_span = Span::new(body.span.start + 1, body.span.end.saturating_sub(1));
            let body_inner = self.editor.extract_and_consume(inner_span);
            self.editor
                .replace(body.span, wrap_async_generator_body(&body_inner));
        }
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        let has_unsupported_elements = it.body.body.iter().any(|el| match el {
            ClassElement::StaticBlock(_) | ClassElement::AccessorProperty(_) => true,
            ClassElement::PropertyDefinition(p) => {
                p.computed || matches!(p.key, PropertyKey::PrivateIdentifier(_))
            }
            ClassElement::MethodDefinition(m) => {
                matches!(m.key, PropertyKey::PrivateIdentifier(_))
            }
            ClassElement::TSIndexSignature(_) => false,
        });
        let has_fields = it
            .body
            .body
            .iter()
            .any(|el| matches!(el, ClassElement::PropertyDefinition(_)));

        self.class_depth += 1;
        walk::walk_class(self, it);
        self.class_depth -= 1;

        if has_unsupported_elements || !has_fields {
            return;
        }

        let mut instance_fields: Vec<(String, String)> = Vec::new();
        let mut static_fields: Vec<(String, String)> = Vec::new();

        for elem in &it.body.body {
            let ClassElement::PropertyDefinition(prop) = elem else {
                continue;
            };
            let Some(access) = format_class_field_member_access(&prop.key) else {
                continue;
            };
            let init_val = prop.value.as_ref().map_or_else(
                || "undefined".to_string(),
                |v| self.editor.extract_and_consume(v.span()),
            );
            if prop.r#static {
                static_fields.push((access, init_val));
            } else {
                instance_fields.push((access, init_val));
            }
            self.editor.remove_statement_line(prop.span);
        }

        if !instance_fields.is_empty() {
            let inits = instance_fields
                .iter()
                .map(|(acc, val)| format!("this{acc} = {val};"))
                .collect::<Vec<_>>()
                .join(" ");

            let existing_ctor = it.body.body.iter().find_map(|el| match el {
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    Some(m.as_ref())
                }
                _ => None,
            });

            if let Some(ctor) = existing_ctor
                && let Some(ctor_body) = &ctor.value.body
            {
                let insert_pos = if it.heritage.is_some() {
                    find_super_call_stmt_end(&ctor_body.statements)
                        .unwrap_or(ctor_body.span.start + 1)
                } else {
                    ctor_body.span.start + 1
                };
                self.editor
                    .replace(Span::new(insert_pos, insert_pos), format!(" {inits}"));
            } else {
                let ctor_code = if it.heritage.is_some() {
                    format!("\n    constructor(...__h2q_args) {{ super(...__h2q_args); {inits} }}")
                } else {
                    format!("\n    constructor() {{ {inits} }}")
                };
                let body_open = it.body.span.start + 1;
                self.editor
                    .replace(Span::new(body_open, body_open), ctor_code);
            }
        }

        if !static_fields.is_empty() {
            if it.r#type == ClassType::ClassDeclaration
                && let Some(cls_id) = &it.id
            {
                let cls_name = cls_id.name.as_str();
                let stmts = static_fields
                    .into_iter()
                    .map(|(acc, val)| format!("{cls_name}{acc} = {val};"))
                    .collect();
                self.post_class_static_inits
                    .insert((it.span.start, it.span.end), stmts);
            } else {
                let cls_str = self.editor.extract_and_consume(it.span);
                let tmp = format!("__H2QCls_{}", self.shared.fresh_id());
                let assigns = static_fields
                    .into_iter()
                    .map(|(acc, val)| format!("{tmp}{acc} = {val};"))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.editor.replace(
                    it.span,
                    format!(
                        "(function() {{ const {tmp} = {cls_str}; {assigns} return {tmp}; }})()"
                    ),
                );
            }
        }
    }
}

fn compound_binary_op(op: AssignmentOperator) -> Option<&'static str> {
    match op {
        AssignmentOperator::Addition => Some("+"),
        AssignmentOperator::Subtraction => Some("-"),
        AssignmentOperator::Multiplication => Some("*"),
        AssignmentOperator::Division => Some("/"),
        AssignmentOperator::Remainder => Some("%"),
        AssignmentOperator::Exponential => Some("**"),
        AssignmentOperator::ShiftLeft => Some("<<"),
        AssignmentOperator::ShiftRight => Some(">>"),
        AssignmentOperator::ShiftRightZeroFill => Some(">>>"),
        AssignmentOperator::BitwiseOR => Some("|"),
        AssignmentOperator::BitwiseXOR => Some("^"),
        AssignmentOperator::BitwiseAnd => Some("&"),
        _ => None,
    }
}

fn logical_binary_op(op: AssignmentOperator) -> Option<&'static str> {
    match op {
        AssignmentOperator::LogicalNullish => Some("??"),
        AssignmentOperator::LogicalOr => Some("||"),
        AssignmentOperator::LogicalAnd => Some("&&"),
        _ => None,
    }
}

fn format_setter_call_assignment<F>(
    read_expr: &str,
    op: AssignmentOperator,
    rhs_str: &str,
    make_call: F,
) -> String
where
    F: Fn(&str) -> String,
{
    let write_iife = |val_expr: &str| {
        let write_stmt = make_call("__v");
        format!("((__v) => ({write_stmt}, __v))({val_expr})")
    };
    if let Some(logic) = logical_binary_op(op) {
        let write_expr = write_iife(rhs_str);
        format!("{read_expr} {logic} {write_expr}")
    } else if let Some(bin_op) = compound_binary_op(op) {
        let combined = format!("{read_expr} {bin_op} ({rhs_str})");
        write_iife(&combined)
    } else {
        write_iife(rhs_str)
    }
}

fn format_inverted_bool_assignment(
    qml_lhs: &str,
    op: AssignmentOperator,
    right_ast: &Expression<'_>,
    rhs_str: &str,
) -> String {
    let inv = negate_js_expr(right_ast, rhs_str);
    if let Some(logic) = logical_binary_op(op) {
        let read_expr = if op == AssignmentOperator::LogicalNullish {
            format!("(!{qml_lhs})")
        } else {
            format!("!{qml_lhs}")
        };
        format!("{read_expr} {logic} !({qml_lhs} = {inv})")
    } else if let Some(bin_op) = compound_binary_op(op) {
        format!("!({qml_lhs} = !((!{qml_lhs}) {bin_op} ({rhs_str})))")
    } else {
        format!("!({qml_lhs} = {inv})")
    }
}

fn format_assignment(lhs: &str, op: AssignmentOperator, rhs_str: &str) -> String {
    if let Some(logic) = logical_binary_op(op) {
        format!("{lhs} {logic} ({lhs} = {rhs_str})")
    } else {
        format!("{lhs} {} {rhs_str}", op.as_str())
    }
}

pub(crate) fn negate_js_expr(ast_expr: &Expression<'_>, rendered: &str) -> String {
    match unwrap_parens(ast_expr) {
        Expression::BooleanLiteral(b) => {
            if b.value {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        e if is_atomic_or_call_expr(e) => format!("!{rendered}"),
        _ => format!("!({rendered})"),
    }
}

fn is_atomic_or_call_expr(expr: &Expression<'_>) -> bool {
    matches!(
        expr,
        Expression::Identifier(_)
            | Expression::BooleanLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::ArrayExpression(_)
            | Expression::ObjectExpression(_)
            | Expression::ParenthesizedExpression(_)
            | Expression::CallExpression(_)
            | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_)
            | Expression::ThisExpression(_)
            | Expression::UnaryExpression(_)
    )
}

pub(crate) fn static_property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        PropertyKey::NumericLiteral(n) => Some(n.value.to_string()),
        _ => None,
    }
}

fn format_class_field_member_access(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(format!(".{}", id.name)),
        PropertyKey::StringLiteral(s) => Some(format!("[{:?}]", s.value.as_str())),
        PropertyKey::NumericLiteral(n) => Some(format!("[{}]", n.value)),
        _ => None,
    }
}

fn find_super_call_stmt_end(stmts: &[Statement<'_>]) -> Option<u32> {
    for stmt in stmts {
        if let Statement::ExpressionStatement(es) = stmt
            && let Expression::CallExpression(call) = unwrap_parens(&es.expression)
            && matches!(unwrap_parens(&call.callee), Expression::Super(_))
        {
            return Some(stmt.span().end);
        }
    }
    None
}

fn strip_leading_async_keyword(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start();
    let rest = trimmed.strip_prefix("async")?;
    let leading_ws = &raw[..raw.len() - trimmed.len()];
    Some(format!("{leading_ws}{}", rest.trim_start()))
}

fn remove_async_token_in_method_prefix(raw: &str) -> Option<String> {
    // Handles `async `, `static async `, etc.
    let idx = raw.find("async")?;
    let before = &raw[..idx];
    let after = raw[idx + "async".len()..].trim_start();
    Some(format!("{before}{after}"))
}

fn wrap_async_generator_body(body_inner: &str) -> String {
    if body_inner.contains('\n') {
        format!("{{\n    return H2Q.asyncRun(this, function*() {{{body_inner}}});\n}}")
    } else {
        let trimmed = body_inner.trim();
        if trimmed.is_empty() {
            "{ return H2Q.asyncRun(this, function*() {}); }".to_string()
        } else {
            format!("{{ return H2Q.asyncRun(this, function*() {{ {trimmed} }}); }}")
        }
    }
}
