use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BindingPattern,
    CallExpression, Class, ComputedMemberExpression, Expression, Function, IdentifierReference,
    MethodDefinition, ObjectProperty, ReturnStatement, SimpleAssignmentTarget, Statement,
    StaticMemberExpression, ThisExpression, UnaryExpression, UpdateExpression, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping, SymbolId};
use oxc_span::Span;
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator};
use oxc_syntax::scope::ScopeFlags;

use crate::parse::{
    GlobalDeclTracker, ParsedScript, collect_unsupported_diagnostics, line_for_span, parse_script,
};
use crate::resolve::{
    ScriptQueryBindings, build_cross_script_query_map, extract_dom_query_from_call,
    extract_string_literal, is_global_document, kebab_to_camel, known_dom_event_from_on_prop,
    resolve_module_specifier, unwrap_parens,
};
use crate::{
    Analysis, ClassWrite, CreatedElement, CreatedId, Diagnostic, DomQuery, ElementRef, ScriptInput,
    ScriptKind, Severity,
};

pub(crate) fn analyze(scripts: &[ScriptInput]) -> Analysis {
    let allocator = Allocator::default();
    let mut analysis = Analysis::default();

    let mut decl_tracker = GlobalDeclTracker::new();
    let mut valid_scripts: Vec<(usize, &ScriptInput, ParsedScript<'_>)> = Vec::new();

    for (idx, input) in scripts.iter().enumerate() {
        let parsed = match parse_script(&allocator, input) {
            Ok(p) => p,
            Err(diags) => {
                analysis.diagnostics.extend(diags);
                continue;
            }
        };

        if let Err(diags) = decl_tracker.check_and_register(input, &parsed.program) {
            analysis.diagnostics.extend(diags);
            continue;
        }

        if matches!(input.kind, ScriptKind::Module) {
            collect_unresolvable_module_diagnostics(
                input,
                &parsed.program,
                scripts,
                &mut analysis.diagnostics,
            );
        }

        let unsupported = collect_unsupported_diagnostics(input, &parsed.program, &parsed.scoping);
        analysis.diagnostics.extend(unsupported);

        valid_scripts.push((idx, input, parsed));
    }

    let valid_refs: Vec<(usize, &ScriptInput, &ParsedScript<'_>)> = valid_scripts
        .iter()
        .map(|(i, inp, p)| (*i, *inp, p))
        .collect();
    let cross_script = build_cross_script_query_map(&valid_refs);

    let mut seen_queries: HashSet<DomQuery> = HashSet::new();
    let mut seen_listened: HashSet<(DomQuery, String)> = HashSet::new();
    let mut seen_writes: HashSet<(DomQuery, String)> = HashSet::new();
    let mut dynamic = DynState::default();

    for (_, input, parsed) in &valid_scripts {
        let scoping = &parsed.scoping;
        let bindings = ScriptQueryBindings::build(input, &parsed.program, scoping, &cross_script);

        let mut created_at = HashMap::new();
        CreatedSiteCollector {
            scoping,
            input,
            created_at: &mut created_at,
            sites: &mut dynamic.sites,
        }
        .visit_program(&parsed.program);
        let eval = Eval {
            scoping,
            bindings: &bindings,
            created_at: &created_at,
        };
        let facts = {
            let mut collector = FactCollector {
                eval: &eval,
                facts: Facts::default(),
                frames: Vec::new(),
                pending_fn: None,
            };
            collector.visit_program(&parsed.program);
            collector.facts
        };

        let mut visitor = AnalysisVisitor {
            scoping,
            bindings: &bindings,
            analysis: &mut analysis,
            seen_queries: &mut seen_queries,
            seen_listened: &mut seen_listened,
            seen_writes: &mut seen_writes,
            input,
            eval: &eval,
            facts: &facts,
            dynamic: &mut dynamic,
            path: Vec::new(),
            this_is_element: vec![matches!(input.kind, ScriptKind::HandlerAttribute { .. })],
            pending_method: false,
            class_reads_ok: HashSet::new(),
        };
        visitor.visit_program(&parsed.program);
    }

    analysis.created = dynamic.finish();
    analysis.runtime_dom |= analysis.mutates_dom;
    analysis
}

pub(crate) fn collect_unresolvable_module_diagnostics(
    input: &ScriptInput,
    program: &oxc_ast::ast::Program<'_>,
    scripts: &[ScriptInput],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(imp) => {
                let spec = imp.source.value.as_str();
                if resolve_module_specifier(&input.name, spec, scripts).is_none() {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Unsupported,
                        source: input.name.clone(),
                        line: Some(line_for_span(input, imp.span)),
                        message: format!("cannot inline external or missing module '{spec}'"),
                    });
                }
            }
            Statement::ExportAllDeclaration(exp) => {
                let spec = exp.source.value.as_str();
                if resolve_module_specifier(&input.name, spec, scripts).is_none() {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Unsupported,
                        source: input.name.clone(),
                        line: Some(line_for_span(input, exp.span)),
                        message: format!("cannot inline re-exported module '{spec}'"),
                    });
                }
            }
            Statement::ExportFromDeclaration(exp) => {
                let spec = exp.source.value.as_str();
                if resolve_module_specifier(&input.name, spec, scripts).is_none() {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Unsupported,
                        source: input.name.clone(),
                        line: Some(line_for_span(input, exp.span)),
                        message: format!("cannot inline re-exported module '{spec}'"),
                    });
                }
            }
            _ => {}
        }
    }
}

struct AnalysisVisitor<'a, 'b> {
    scoping: &'a Scoping,
    bindings: &'b ScriptQueryBindings<'a>,
    analysis: &'b mut Analysis,
    seen_queries: &'b mut HashSet<DomQuery>,
    seen_listened: &'b mut HashSet<(DomQuery, String)>,
    seen_writes: &'b mut HashSet<(DomQuery, String)>,
    input: &'b ScriptInput,
    eval: &'b Eval<'a, 'b>,
    facts: &'b Facts,
    dynamic: &'b mut DynState,
    /// Enclosing functions and conditional constructs (`true` = function).
    path: Vec<(u32, bool)>,
    /// Whether `this` may be an element, per enclosing function.
    this_is_element: Vec<bool>,
    /// The next function visited is a class or object method.
    pending_method: bool,
    /// `className`/`classList` member expressions that only write classes.
    class_reads_ok: HashSet<SpanKey>,
}

impl AnalysisVisitor<'_, '_> {
    fn record_query(&mut self, query: DomQuery) {
        if self.seen_queries.insert(query.clone()) {
            self.analysis.queries.push(query);
        }
    }

    fn record_listened(&mut self, query: DomQuery, event: impl Into<String>) {
        let pair = (query, event.into());
        if self.seen_listened.insert(pair.clone()) {
            self.analysis.listened.push(pair);
        }
    }

    fn record_write(&mut self, query: DomQuery, prop: impl Into<String>) {
        let pair = (query, prop.into());
        if self.seen_writes.insert(pair.clone()) {
            self.analysis.writes.push(pair);
        }
    }

    fn inspect_member_write(
        &mut self,
        static_member: Option<&StaticMemberExpression<'_>>,
        computed_member: Option<&ComputedMemberExpression<'_>>,
    ) {
        if let Some(member) = static_member {
            let prop = member.property.name.as_str();
            if prop == "innerHTML" || prop == "outerHTML" {
                self.analysis.mutates_dom = true;
            }

            if let Some(query) = self.bindings.resolve_expr_for_analysis(&member.object) {
                if let Some(ev) = known_dom_event_from_on_prop(prop) {
                    self.record_listened(query, ev);
                } else {
                    self.record_write(query, prop);
                }
                return;
            }

            // `el.style.<prop> = ...` or `el.dataset.<key> = ...`
            if let Expression::StaticMemberExpression(parent_obj) = unwrap_parens(&member.object) {
                let mid = parent_obj.property.name.as_str();
                if (mid == "style" || mid == "dataset")
                    && let Some(query) = self.bindings.resolve_expr_for_analysis(&parent_obj.object)
                {
                    self.record_write(query, format!("{mid}.{prop}"));
                }
            }
        } else if let Some(comp) = computed_member
            && let Some(prop) = extract_string_literal(&comp.expression)
        {
            if prop == "innerHTML" || prop == "outerHTML" {
                self.analysis.mutates_dom = true;
            }
            if let Some(query) = self.bindings.resolve_expr_for_analysis(&comp.object) {
                if let Some(ev) = known_dom_event_from_on_prop(&prop) {
                    self.record_listened(query, ev);
                } else {
                    self.record_write(query, prop);
                }
            } else if let Expression::StaticMemberExpression(parent_obj) =
                unwrap_parens(&comp.object)
            {
                let mid = parent_obj.property.name.as_str();
                if (mid == "style" || mid == "dataset")
                    && let Some(query) = self.bindings.resolve_expr_for_analysis(&parent_obj.object)
                {
                    self.record_write(query, format!("{mid}.{prop}"));
                }
            }
        }
    }
}

impl AnalysisVisitor<'_, '_> {
    fn pos(&self, span: Span) -> Pos {
        Pos {
            path: self.path.clone(),
            at: span.start,
        }
    }

    fn target(&self, expr: &Expression<'_>) -> ElementRef {
        self.facts.resolve_elem(&self.eval.elem(expr), true, 0)
    }

    fn class_value(&self, expr: &Expression<'_>, append: bool) -> Vec<ClassAlt> {
        class_alts(&self.facts.resolve_str(&self.eval.strings(expr), 0), append)
    }

    fn class_arg(&self, arg: Option<&Argument<'_>>) -> Vec<ClassAlt> {
        match arg.and_then(|a| a.as_expression()) {
            Some(e) => self.class_value(e, false),
            // A spread or a missing argument.
            None => vec![ClassAlt {
                names: BTreeSet::new(),
                open: true,
            }],
        }
    }

    fn record_class_write(
        &mut self,
        object: &Expression<'_>,
        op: ClassOp,
        assigns: bool,
        span: Span,
    ) {
        let target = self.target(object);
        let mut names = BTreeSet::new();
        let mut open = false;
        let mut take = |alts: &[ClassAlt]| {
            for a in alts {
                names.extend(a.names.iter().cloned());
                open |= a.open;
            }
        };
        match &op {
            ClassOp::Assign(a) | ClassOp::Append(a) | ClassOp::Toggle(a, _) => take(a),
            ClassOp::Add(args) | ClassOp::Remove(args) => args.iter().for_each(|a| take(a)),
            ClassOp::Replace(old, new) => {
                take(old);
                take(new);
            }
        }
        if let ElementRef::Created(id) = target {
            let pos = self.pos(span);
            self.dynamic.class_ops.push((id, op, pos));
        }
        self.analysis.class_writes.push(ClassWrite {
            target,
            names: names.into_iter().collect(),
            open,
            assigns,
            source: self.input.name.clone(),
            line: Some(line_for_span(self.input, span)),
        });
    }

    fn record_inserts<'x, 'y: 'x>(
        &mut self,
        parent: &ElementRef,
        children: impl IntoIterator<Item = &'x Argument<'y>>,
        span: Span,
    ) {
        for arg in children {
            if let Some(e) = arg.as_expression()
                && let ElementRef::Created(id) = self.target(e)
            {
                let pos = self.pos(span);
                self.dynamic.inserts.push((id, parent.clone(), pos));
            }
        }
    }

    fn record_input_type(&mut self, object: &Expression<'_>, value: Option<&Expression<'_>>) {
        if let ElementRef::Created(id) = self.target(object) {
            let value = value
                .and_then(extract_string_literal)
                .map(|t| t.to_ascii_lowercase());
            self.dynamic.input_types.push((id, value));
        }
    }

    fn inspect_dynamic_call(
        &mut self,
        it: &CallExpression<'_>,
        member: &StaticMemberExpression<'_>,
    ) {
        let method = member.property.name.as_str();
        let args = &it.arguments;
        let literal_arg = |i: usize| {
            args.get(i)
                .and_then(|a| a.as_expression())
                .and_then(extract_string_literal)
        };

        if let Expression::StaticMemberExpression(inner) = unwrap_parens(&member.object)
            && inner.property.name == "classList"
        {
            let op = match method {
                "add" => ClassOp::Add(args.iter().map(|a| self.class_arg(Some(a))).collect()),
                "remove" => ClassOp::Remove(args.iter().map(|a| self.class_arg(Some(a))).collect()),
                "toggle" => {
                    let force = match args
                        .get(1)
                        .and_then(|a| a.as_expression())
                        .map(unwrap_parens)
                    {
                        None => Force::Flip,
                        Some(Expression::BooleanLiteral(b)) if b.value => Force::Add,
                        Some(Expression::BooleanLiteral(_)) => Force::Remove,
                        Some(_) => Force::Either,
                    };
                    ClassOp::Toggle(self.class_arg(args.first()), force)
                }
                "replace" => {
                    ClassOp::Replace(self.class_arg(args.first()), self.class_arg(args.get(1)))
                }
                // `contains`, `item`, … read the classes: the member visit flags them.
                _ => return,
            };
            // `toggle` without a force depends on the classes already there.
            if method != "toggle" || args.len() >= 2 {
                self.class_reads_ok.insert(span_key(inner.span));
            }
            self.record_class_write(&inner.object, op, false, it.span);
            return;
        }

        let dom_receiver = !is_non_dom_collection_receiver(&member.object, self.scoping);
        match method {
            "setAttribute" | "removeAttribute" | "getAttribute" | "hasAttribute"
            | "getAttributeNode" => {
                let Some(attr) = literal_arg(0).map(|a| a.to_ascii_lowercase()) else {
                    return;
                };
                match (method, attr.as_str()) {
                    ("setAttribute", "class") => {
                        let value = match args.get(1).and_then(|a| a.as_expression()) {
                            Some(v) => self.class_value(v, false),
                            None => self.class_arg(None),
                        };
                        self.record_class_write(
                            &member.object,
                            ClassOp::Assign(value),
                            true,
                            it.span,
                        );
                    }
                    ("removeAttribute", "class") => {
                        let empty = vec![ClassAlt::default()];
                        self.record_class_write(
                            &member.object,
                            ClassOp::Assign(empty),
                            true,
                            it.span,
                        );
                    }
                    ("setAttribute", "type") => {
                        let value = args.get(1).and_then(|a| a.as_expression());
                        self.record_input_type(&member.object, value);
                    }
                    ("getAttribute" | "hasAttribute" | "getAttributeNode", "class") => {
                        self.analysis.runtime_dom = true;
                    }
                    _ => {}
                }
            }
            "appendChild" | "insertBefore" | "replaceChild" => {
                let parent = self.target(&member.object);
                self.record_inserts(&parent, args.first(), it.span);
            }
            "append" | "prepend" | "replaceChildren" if dom_receiver => {
                let parent = self.target(&member.object);
                self.record_inserts(&parent, args.iter(), it.span);
            }
            "insertAdjacentElement" => {
                let inside = literal_arg(0).is_some_and(|p| {
                    matches!(p.to_ascii_lowercase().as_str(), "afterbegin" | "beforeend")
                });
                let parent = if inside {
                    self.target(&member.object)
                } else {
                    ElementRef::Unknown
                };
                self.record_inserts(&parent, args.get(1), it.span);
            }
            // The parent is the receiver's parent, which analysis does not know.
            "before" | "after" | "replaceWith" if dom_receiver => {
                self.record_inserts(&ElementRef::Unknown, args.iter(), it.span);
            }
            "closest"
            | "matches"
            | "webkitMatchesSelector"
            | "msMatchesSelector"
            | "contains"
            | "getRootNode"
            | "compareDocumentPosition"
            | "elementFromPoint"
            | "elementsFromPoint"
            | "getElementsByName" => {
                self.analysis.runtime_dom = true;
            }
            "querySelector"
            | "querySelectorAll"
            | "getElementsByClassName"
            | "getElementsByTagName"
            | "getElementById"
                if !is_global_document(&member.object, self.scoping)
                    || extract_dom_query_from_call(it, self.scoping).is_none() =>
            {
                self.analysis.runtime_dom = true;
            }
            _ => {}
        }
    }

    fn inspect_dynamic_assignment(&mut self, it: &AssignmentExpression<'_>) {
        let AssignmentTarget::StaticMemberExpression(member) = &it.left else {
            return;
        };
        match (member.property.name.as_str(), it.operator) {
            ("className" | "classList", AssignmentOperator::Assign) => {
                self.class_reads_ok.insert(span_key(member.span));
                let value = self.class_value(&it.right, false);
                self.record_class_write(&member.object, ClassOp::Assign(value), true, it.span);
            }
            ("className", AssignmentOperator::Addition) => {
                self.analysis.runtime_dom = true;
                let value = self.class_value(&it.right, true);
                self.record_class_write(&member.object, ClassOp::Append(value), false, it.span);
            }
            ("type", AssignmentOperator::Assign) => {
                self.record_input_type(&member.object, Some(&it.right));
            }
            _ => {}
        }
    }
}

/// Whether a node opens a control-flow frame, and whether that frame is a function.
fn frame_kind(kind: &AstKind<'_>) -> Option<bool> {
    match kind {
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => Some(true),
        AstKind::IfStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::ForInStatement(_)
        | AstKind::ForOfStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::DoWhileStatement(_)
        | AstKind::SwitchStatement(_)
        | AstKind::TryStatement(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::LogicalExpression(_)
        | AstKind::ChainExpression(_) => Some(false),
        _ => None,
    }
}

impl<'a> Visit<'a> for AnalysisVisitor<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(id) = self.eval.created_at.get(&span_key(it.span))
            && let Some(site) = self.dynamic.sites.get_mut(id.0 as usize)
        {
            site.path = self.path.clone();
        }
        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee) {
            self.inspect_dynamic_call(it, member);
        }

        if let Some(query) = extract_dom_query_from_call(it, self.scoping) {
            self.record_query(query);
        }

        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee) {
            let method = member.property.name.as_str();

            if is_global_document(&member.object, self.scoping)
                && matches!(
                    method,
                    "createElement"
                        | "createElementNS"
                        | "createTextNode"
                        | "createDocumentFragment"
                )
            {
                self.analysis.mutates_dom = true;
            }

            if matches!(
                method,
                "appendChild"
                    | "removeChild"
                    | "insertBefore"
                    | "replaceChild"
                    | "replaceChildren"
                    | "replaceWith"
                    | "insertAdjacentHTML"
                    | "insertAdjacentElement"
                    | "cloneNode"
            ) || (matches!(method, "append" | "prepend" | "before" | "after" | "remove")
                && !is_non_dom_collection_receiver(&member.object, self.scoping))
            {
                self.analysis.mutates_dom = true;
            }

            if method == "addEventListener"
                && let Some(ev) = it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                && let Some(query) = self.bindings.resolve_expr_for_analysis(&member.object)
            {
                self.record_listened(query, ev);
            } else if matches!(
                method,
                "setAttribute" | "removeAttribute" | "toggleAttribute"
            ) && let Some(query) = self.bindings.resolve_expr_for_analysis(&member.object)
            {
                if let Some(attr) = it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                {
                    let mapped = map_attribute_to_write_name(&attr);
                    self.record_write(query, mapped);
                } else {
                    self.record_write(query, "attributes");
                }
            } else if matches!(method, "add" | "remove" | "toggle" | "replace")
                && let Expression::StaticMemberExpression(inner) = unwrap_parens(&member.object)
                && inner.property.name == "classList"
                && let Some(query) = self.bindings.resolve_expr_for_analysis(&inner.object)
            {
                self.record_write(query, "classList");
            }
        }

        walk::walk_call_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        self.inspect_dynamic_assignment(it);
        match &it.left {
            AssignmentTarget::StaticMemberExpression(member) => {
                self.inspect_member_write(Some(member), None);
            }
            AssignmentTarget::ComputedMemberExpression(comp) => {
                self.inspect_member_write(None, Some(comp));
            }
            _ => {}
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        match &it.argument {
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                self.inspect_member_write(Some(member), None);
            }
            SimpleAssignmentTarget::ComputedMemberExpression(comp) => {
                self.inspect_member_write(None, Some(comp));
            }
            _ => {}
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Delete {
            match unwrap_parens(&it.argument) {
                Expression::StaticMemberExpression(member) => {
                    self.inspect_member_write(Some(member), None);
                }
                Expression::ComputedMemberExpression(comp) => {
                    self.inspect_member_write(None, Some(comp));
                }
                _ => {}
            }
        }
        walk::walk_unary_expression(self, it);
    }

    fn enter_node(&mut self, kind: AstKind<'a>) {
        if let Some(is_function) = frame_kind(&kind) {
            self.dynamic.next_frame += 1;
            self.path.push((self.dynamic.next_frame, is_function));
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        if frame_kind(&kind).is_some() {
            self.path.pop();
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        // A listener's `this` is its element; a method's is its object.
        let is_method = std::mem::take(&mut self.pending_method);
        self.this_is_element.push(!is_method);
        walk::walk_function(self, it, flags);
        self.this_is_element.pop();
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.pending_method = true;
        walk::walk_method_definition(self, it);
        self.pending_method = false;
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.pending_method = matches!(unwrap_parens(&it.value), Expression::FunctionExpression(_));
        walk::walk_object_property(self, it);
        self.pending_method = false;
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        self.this_is_element.push(false);
        walk::walk_class(self, it);
        self.this_is_element.pop();
    }

    fn visit_this_expression(&mut self, it: &ThisExpression) {
        if self.this_is_element.last().copied().unwrap_or(false) {
            self.analysis.runtime_dom = true;
        }
        walk::walk_this_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        let prop = it.property.name.as_str();
        let reads_classes = matches!(prop, "className" | "classList")
            && !self.class_reads_ok.contains(&span_key(it.span));
        if reads_classes
            || is_traversal_property(prop)
            || is_event_element_property(prop)
            || matches!(prop, "tagName" | "nodeName" | "localName")
            || (is_runtime_document_property(prop) && is_global_document(&it.object, self.scoping))
        {
            self.analysis.runtime_dom = true;
        }
        walk::walk_static_member_expression(self, it);
    }
}

fn is_non_dom_collection_receiver(expr: &Expression<'_>, scoping: &Scoping) -> bool {
    let e = unwrap_parens(expr);
    if let Expression::StaticMemberExpression(member) = e
        && member.property.name == "classList"
    {
        return true;
    }
    if let Expression::Identifier(id) = e
        && id.is_global_reference(scoping)
        && matches!(id.name.as_str(), "localStorage" | "sessionStorage")
    {
        return true;
    }
    false
}

fn map_attribute_to_write_name(attr: &str) -> String {
    if attr == "class" {
        return "classList".to_string();
    }
    if let Some(rest) = attr.strip_prefix("data-")
        && !rest.is_empty()
    {
        return format!("dataset.{}", kebab_to_camel(rest));
    }
    attr.to_string()
}

// ---------------------------------------------------------------------------
// Dynamic styles (`docs/design/dynamic-styles.md` §6.1, §9.1): which classes
// scripts write, which elements they create, and whether elements are reached
// in ways only the runtime can answer.
//
// The dataflow is deliberately shallow: `const`/`let`/`var` bindings that are
// never reassigned, one level of function return, and the element parameter
// of `forEach`/`for..of` over a literal `querySelectorAll` (which
// `ScriptQueryBindings` already resolves). Anything else is `Unknown` or
// `open`, never a guessed literal.
// ---------------------------------------------------------------------------

/// Above this many alternatives a value is treated as open rather than enumerated.
const MAX_ALTS: usize = 64;
/// Bound on alias chains, which also stops cycles.
const MAX_DEPTH: u32 = 16;

type SpanKey = (u32, u32);

fn span_key(span: Span) -> SpanKey {
    (span.start, span.end)
}

/// A fragment of a string value: literal text, an unknown part, or the value
/// of a never-reassigned binding (resolved once every binding is known).
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Lit(String),
    Open,
    Sym(SymbolId),
}

/// The alternatives a string expression can evaluate to.
type Alts = Vec<Vec<Piece>>;

fn open_alts() -> Alts {
    vec![vec![Piece::Open]]
}

fn product(a: &Alts, b: &Alts) -> Alts {
    if a.len().saturating_mul(b.len()) > MAX_ALTS {
        return open_alts();
    }
    let mut out = Vec::with_capacity(a.len() * b.len());
    for x in a {
        for y in b {
            let mut v = x.clone();
            v.extend(y.iter().cloned());
            out.push(v);
        }
    }
    out
}

fn union(mut a: Alts, b: Alts) -> Alts {
    a.extend(b);
    if a.len() > MAX_ALTS {
        return open_alts();
    }
    a
}

/// What an expression refers to, before bindings and returns are resolved.
#[derive(Debug, Clone, PartialEq)]
enum ElemExpr {
    Ref(ElementRef),
    Sym(SymbolId),
    CallOf(SymbolId),
    Unknown,
}

/// One possible class set: its literal names and whether some part is not literal.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ClassAlt {
    names: BTreeSet<String>,
    open: bool,
}

/// Splits each alternative into class tokens. A token that touches an unknown
/// part (`"btn-" + kind`) is not a class name, so it only makes the value open.
/// `append` is `className +=`: without leading whitespace the first token
/// joins the last existing class.
fn class_alts(alts: &Alts, append: bool) -> Vec<ClassAlt> {
    let mut out: Vec<ClassAlt> = Vec::new();
    for alt in alts {
        let mut text = String::new();
        for piece in alt {
            match piece {
                Piece::Lit(s) => text.push_str(s),
                Piece::Open | Piece::Sym(_) => text.push('\0'),
            }
        }
        let mut class = ClassAlt::default();
        let mut tokens = text.split_ascii_whitespace();
        if append && !text.is_empty() && !text.starts_with(|c: char| c.is_ascii_whitespace()) {
            tokens.next();
            class.open = true;
        }
        for token in tokens {
            if token.contains('\0') {
                class.open = true;
            } else {
                class.names.insert(token.to_string());
            }
        }
        if !out.contains(&class) {
            out.push(class);
        }
    }
    out
}

/// Syntax-only evaluation of one script's expressions.
struct Eval<'a, 'b> {
    scoping: &'a Scoping,
    bindings: &'b ScriptQueryBindings<'a>,
    created_at: &'b HashMap<SpanKey, CreatedId>,
}

impl Eval<'_, '_> {
    fn stable_symbol(&self, ident: &IdentifierReference<'_>) -> Option<SymbolId> {
        let sym = self
            .scoping
            .get_reference(ident.reference_id.get()?)
            .symbol_id()?;
        (!self.scoping.symbol_is_mutated(sym) && self.scoping.symbol_redeclarations(sym).is_empty())
            .then_some(sym)
    }

    fn elem(&self, expr: &Expression<'_>) -> ElemExpr {
        let e = unwrap_parens(expr);
        if let Some(q) = self.bindings.resolve_expr_for_analysis(e) {
            return ElemExpr::Ref(ElementRef::Query(q));
        }
        match e {
            Expression::CallExpression(call) => {
                if let Some(id) = self.created_at.get(&span_key(call.span)) {
                    return ElemExpr::Ref(ElementRef::Created(*id));
                }
                if let Expression::Identifier(callee) = unwrap_parens(&call.callee)
                    && let Some(sym) = self.stable_symbol(callee)
                {
                    return ElemExpr::CallOf(sym);
                }
                ElemExpr::Unknown
            }
            Expression::Identifier(ident) => self
                .stable_symbol(ident)
                .map_or(ElemExpr::Unknown, ElemExpr::Sym),
            Expression::StaticMemberExpression(m)
                if is_global_document(&m.object, self.scoping) =>
            {
                // These name exactly one static element, like a literal query.
                let selector = match m.property.name.as_str() {
                    "body" => "body",
                    "head" => "head",
                    "documentElement" => "html",
                    _ => return ElemExpr::Unknown,
                };
                ElemExpr::Ref(ElementRef::Query(DomQuery::Selector(selector.to_string())))
            }
            _ => ElemExpr::Unknown,
        }
    }

    fn strings(&self, expr: &Expression<'_>) -> Alts {
        match unwrap_parens(expr) {
            Expression::StringLiteral(s) => vec![vec![Piece::Lit(s.value.to_string())]],
            Expression::TemplateLiteral(tpl) => {
                let mut acc: Alts = vec![Vec::new()];
                for (i, quasi) in tpl.quasis.iter().enumerate() {
                    let lit = quasi
                        .value
                        .cooked
                        .as_ref()
                        .map_or(Piece::Open, |c| Piece::Lit(c.to_string()));
                    acc = product(&acc, &vec![vec![lit]]);
                    if let Some(e) = tpl.expressions.get(i) {
                        acc = product(&acc, &self.strings(e));
                    }
                }
                acc
            }
            // Numbers and other non-strings stay open, so `1 + 2 + "a"` is
            // never mistaken for a concatenation.
            Expression::BinaryExpression(b) if b.operator == BinaryOperator::Addition => {
                product(&self.strings(&b.left), &self.strings(&b.right))
            }
            Expression::ConditionalExpression(c) => {
                union(self.strings(&c.consequent), self.strings(&c.alternate))
            }
            // `a && "x"` is `a` itself when `a` is falsy.
            Expression::LogicalExpression(l) => match l.operator {
                LogicalOperator::And => union(open_alts(), self.strings(&l.right)),
                LogicalOperator::Or | LogicalOperator::Coalesce => {
                    union(self.strings(&l.left), self.strings(&l.right))
                }
            },
            Expression::Identifier(ident) => self
                .stable_symbol(ident)
                .map_or_else(open_alts, |s| vec![vec![Piece::Sym(s)]]),
            _ => open_alts(),
        }
    }
}

/// Never-reassigned bindings and function returns of one script.
#[derive(Default)]
struct Facts {
    sym_elem: HashMap<SymbolId, ElemExpr>,
    sym_str: HashMap<SymbolId, Alts>,
    fn_returns: HashMap<SymbolId, Vec<ElemExpr>>,
}

impl Facts {
    fn resolve_elem(&self, value: &ElemExpr, allow_call: bool, depth: u32) -> ElementRef {
        if depth > MAX_DEPTH {
            return ElementRef::Unknown;
        }
        match value {
            ElemExpr::Ref(r) => r.clone(),
            ElemExpr::Unknown => ElementRef::Unknown,
            ElemExpr::Sym(s) => self.sym_elem.get(s).map_or(ElementRef::Unknown, |v| {
                self.resolve_elem(v, allow_call, depth + 1)
            }),
            ElemExpr::CallOf(f) => {
                if !allow_call {
                    return ElementRef::Unknown;
                }
                let Some(returns) = self.fn_returns.get(f) else {
                    return ElementRef::Unknown;
                };
                let mut found: Option<ElementRef> = None;
                for r in returns {
                    let r = self.resolve_elem(r, false, depth + 1);
                    match &found {
                        _ if r == ElementRef::Unknown => return ElementRef::Unknown,
                        None => found = Some(r),
                        Some(prev) if *prev == r => {}
                        Some(_) => return ElementRef::Unknown,
                    }
                }
                found.unwrap_or(ElementRef::Unknown)
            }
        }
    }

    fn resolve_str(&self, alts: &Alts, depth: u32) -> Alts {
        let mut out: Alts = Vec::new();
        for alt in alts {
            let mut acc: Alts = vec![Vec::new()];
            for piece in alt {
                let options = match piece {
                    Piece::Sym(s) if depth < MAX_DEPTH => self
                        .sym_str
                        .get(s)
                        .map_or_else(open_alts, |a| self.resolve_str(a, depth + 1)),
                    Piece::Sym(_) => open_alts(),
                    other => vec![vec![other.clone()]],
                };
                acc = product(&acc, &options);
            }
            out = union(out, acc);
        }
        out
    }
}

/// Gives every `document.createElement("<literal>")` call its [`CreatedId`].
struct CreatedSiteCollector<'a, 'b> {
    scoping: &'a Scoping,
    input: &'b ScriptInput,
    created_at: &'b mut HashMap<SpanKey, CreatedId>,
    sites: &'b mut Vec<CreatedSite>,
}

impl<'a> Visit<'a> for CreatedSiteCollector<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee)
            && member.property.name == "createElement"
            && is_global_document(&member.object, self.scoping)
            && let Some(tag) = it
                .arguments
                .first()
                .and_then(|a| a.as_expression())
                .and_then(extract_string_literal)
        {
            let id = CreatedId(u32::try_from(self.sites.len()).unwrap_or(u32::MAX));
            self.created_at.insert(span_key(it.span), id);
            self.sites.push(CreatedSite {
                tag: tag.to_ascii_lowercase(),
                source: self.input.name.clone(),
                line: Some(line_for_span(self.input, it.span)),
                path: Vec::new(),
            });
        }
        walk::walk_call_expression(self, it);
    }
}

/// Collects [`Facts`]: binding initialisers and what each function returns.
struct FactCollector<'a, 'b, 'c> {
    eval: &'c Eval<'a, 'b>,
    facts: Facts,
    frames: Vec<(Option<SymbolId>, Vec<ElemExpr>)>,
    pending_fn: Option<SymbolId>,
}

impl FactCollector<'_, '_, '_> {
    fn finish_frame(&mut self) {
        if let Some((Some(sym), returns)) = self.frames.pop() {
            self.facts.fn_returns.insert(sym, returns);
        }
    }
}

impl<'a> Visit<'a> for FactCollector<'_, '_, '_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(bind) = &it.id
            && let Some(sym) = bind.symbol_id.get()
            && !self.eval.scoping.symbol_is_mutated(sym)
            && self.eval.scoping.symbol_redeclarations(sym).is_empty()
            && let Some(init) = &it.init
        {
            self.facts.sym_elem.insert(sym, self.eval.elem(init));
            self.facts.sym_str.insert(sym, self.eval.strings(init));
            if matches!(
                unwrap_parens(init),
                Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
            ) {
                self.pending_fn = Some(sym);
            }
        }
        walk::walk_variable_declarator(self, it);
        self.pending_fn = None;
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let declared = it
            .id
            .as_ref()
            .filter(|_| it.is_declaration())
            .and_then(|id| id.symbol_id.get())
            .filter(|&sym| {
                !self.eval.scoping.symbol_is_mutated(sym)
                    && self.eval.scoping.symbol_redeclarations(sym).is_empty()
            });
        let sym = self.pending_fn.take().or(declared);
        self.frames.push((sym, Vec::new()));
        walk::walk_function(self, it, flags);
        self.finish_frame();
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let sym = self.pending_fn.take();
        let mut returns = Vec::new();
        if let Some(body) = it.get_expression() {
            returns.push(self.eval.elem(body));
        }
        self.frames.push((sym, returns));
        walk::walk_arrow_function_expression(self, it);
        self.finish_frame();
    }

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        let value = it
            .argument
            .as_ref()
            .map_or(ElemExpr::Unknown, |a| self.eval.elem(a));
        if let Some((_, returns)) = self.frames.last_mut() {
            returns.push(value);
        }
        walk::walk_return_statement(self, it);
    }
}

/// Where an operation sits: its enclosing control-flow frames (`true` marks
/// a function) and its source offset.
#[derive(Debug, Clone)]
struct Pos {
    path: Vec<(u32, bool)>,
    at: u32,
}

fn function_frames(path: &[(u32, bool)]) -> Vec<u32> {
    path.iter().filter(|(_, f)| *f).map(|(id, _)| *id).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Force {
    Flip,
    Add,
    Remove,
    Either,
}

#[derive(Debug, Clone)]
enum ClassOp {
    Assign(Vec<ClassAlt>),
    Append(Vec<ClassAlt>),
    Add(Vec<Vec<ClassAlt>>),
    Remove(Vec<Vec<ClassAlt>>),
    Toggle(Vec<ClassAlt>, Force),
    Replace(Vec<ClassAlt>, Vec<ClassAlt>),
}

fn apply_class_op(states: &[ClassAlt], op: &ClassOp) -> Vec<ClassAlt> {
    let mut out: Vec<ClassAlt> = Vec::new();
    let add = |s: &ClassAlt, a: &ClassAlt| ClassAlt {
        names: s.names.union(&a.names).cloned().collect(),
        open: s.open || a.open,
    };
    let remove = |s: &ClassAlt, a: &ClassAlt| ClassAlt {
        names: s.names.difference(&a.names).cloned().collect(),
        open: s.open || a.open,
    };
    match op {
        ClassOp::Assign(alts) => out.extend(alts.iter().cloned()),
        ClassOp::Append(alts) => {
            for s in states {
                out.extend(alts.iter().map(|a| add(s, a)));
            }
        }
        ClassOp::Add(args) | ClassOp::Remove(args) => {
            let adding = matches!(op, ClassOp::Add(_));
            let mut cur = states.to_vec();
            for arg in args {
                let mut next = Vec::new();
                for s in &cur {
                    for a in arg {
                        next.push(if adding { add(s, a) } else { remove(s, a) });
                    }
                }
                cur = next;
            }
            out = cur;
        }
        ClassOp::Toggle(alts, force) => {
            for s in states {
                for a in alts {
                    match force {
                        Force::Add => out.push(add(s, a)),
                        Force::Remove => out.push(remove(s, a)),
                        Force::Either => {
                            out.push(add(s, a));
                            out.push(remove(s, a));
                        }
                        Force::Flip => {
                            let mut t = s.clone();
                            t.open |= a.open;
                            for name in &a.names {
                                if !t.names.remove(name) {
                                    t.names.insert(name.clone());
                                }
                            }
                            out.push(t);
                        }
                    }
                }
            }
        }
        ClassOp::Replace(old, new) => {
            for s in states {
                for o in old {
                    for n in new {
                        let present = !o.names.is_empty() && o.names.is_subset(&s.names);
                        if present || o.open {
                            let mut t = add(&remove(s, o), n);
                            t.open |= o.open;
                            out.push(t);
                        }
                        if !present {
                            let mut t = s.clone();
                            t.open |= o.open || n.open;
                            out.push(t);
                        }
                    }
                }
            }
        }
    }
    out
}

/// A created element as collected, before its class sets are computed.
#[derive(Debug, Clone)]
struct CreatedSite {
    tag: String,
    source: String,
    line: Option<u32>,
    /// Frames at the `createElement` call.
    path: Vec<(u32, bool)>,
}

/// Dynamic-style facts gathered across all scripts.
#[derive(Default)]
struct DynState {
    sites: Vec<CreatedSite>,
    class_ops: Vec<(CreatedId, ClassOp, Pos)>,
    inserts: Vec<(CreatedId, ElementRef, Pos)>,
    input_types: Vec<(CreatedId, Option<String>)>,
    next_frame: u32,
}

impl DynState {
    /// Class sets at insertion: operations in the creating function (or one
    /// nested in it) before the first insertion there, then operations in
    /// other functions before their own first insertion. Only straight-line
    /// operations next to the `createElement` call are certain; the rest may
    /// or may not have run.
    fn finish(self) -> Vec<CreatedElement> {
        let mut out = Vec::with_capacity(self.sites.len());
        for (index, site) in self.sites.iter().enumerate() {
            let id = CreatedId(u32::try_from(index).unwrap_or(u32::MAX));
            let home = function_frames(&site.path);
            let inserts: Vec<&(CreatedId, ElementRef, Pos)> =
                self.inserts.iter().filter(|(i, _, _)| *i == id).collect();
            let first_insert = |group: &[u32]| {
                inserts
                    .iter()
                    .filter(|(_, _, p)| function_frames(&p.path) == group)
                    .map(|(_, _, p)| p.at)
                    .min()
            };

            let ops: Vec<&(CreatedId, ClassOp, Pos)> =
                self.class_ops.iter().filter(|(i, _, _)| *i == id).collect();
            let (home_ops, other_ops): (Vec<_>, Vec<_>) = ops
                .into_iter()
                .partition(|(_, _, p)| function_frames(&p.path).starts_with(&home));

            let mut states = vec![ClassAlt::default()];
            let mut capped = false;
            for (_, op, pos) in home_ops.into_iter().chain(other_ops) {
                let frames = function_frames(&pos.path);
                let group = if frames.starts_with(&home) {
                    home.clone()
                } else {
                    frames
                };
                if first_insert(&group).is_some_and(|at| pos.at > at) {
                    continue;
                }
                let applied = apply_class_op(&states, op);
                let mut next = if pos.path == site.path {
                    applied
                } else {
                    let mut both = states.clone();
                    both.extend(applied);
                    both
                };
                next.sort();
                next.dedup();
                if next.len() > MAX_ALTS {
                    next.truncate(MAX_ALTS);
                    capped = true;
                }
                states = next;
            }

            let class_sets: BTreeSet<Vec<String>> = states
                .iter()
                .map(|s| s.names.iter().cloned().collect())
                .collect();
            let open_classes = capped || states.iter().any(|s| s.open);

            let mut parents: Vec<ElementRef> = Vec::new();
            for (_, parent, _) in &inserts {
                if !parents.contains(parent) {
                    parents.push(parent.clone());
                }
            }

            let input_type = if site.tag == "input" {
                let types: Vec<&Option<String>> = self
                    .input_types
                    .iter()
                    .filter(|(i, _)| *i == id)
                    .map(|(_, t)| t)
                    .collect();
                match types.first() {
                    Some(Some(first)) if types.iter().all(|t| t.as_ref() == Some(first)) => {
                        Some(first.clone())
                    }
                    _ => None,
                }
            } else {
                None
            };

            out.push(CreatedElement {
                id,
                tag: site.tag.clone(),
                input_type,
                class_sets: class_sets.into_iter().collect(),
                open_classes,
                parents,
                source: site.source.clone(),
                line: site.line,
            });
        }
        out
    }
}

/// Element properties that walk the tree from an element.
fn is_traversal_property(name: &str) -> bool {
    matches!(
        name,
        "parentElement"
            | "parentNode"
            | "children"
            | "childNodes"
            | "firstChild"
            | "lastChild"
            | "firstElementChild"
            | "lastElementChild"
            | "nextSibling"
            | "previousSibling"
            | "nextElementSibling"
            | "previousElementSibling"
            | "offsetParent"
            | "assignedSlot"
    )
}

/// Event and observer properties that hold an element only known at run time.
fn is_event_element_property(name: &str) -> bool {
    matches!(
        name,
        "target" | "currentTarget" | "srcElement" | "relatedTarget" | "submitter"
    )
}

/// `document` properties that find elements the converter cannot name.
fn is_runtime_document_property(name: &str) -> bool {
    matches!(
        name,
        "activeElement"
            | "forms"
            | "images"
            | "links"
            | "anchors"
            | "scripts"
            | "embeds"
            | "plugins"
            | "all"
            | "children"
            | "scrollingElement"
            | "fullscreenElement"
            | "pointerLockElement"
            | "pictureInPictureElement"
    )
}
