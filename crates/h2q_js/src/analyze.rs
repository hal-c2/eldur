use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, CallExpression, ComputedMemberExpression, Expression,
    SimpleAssignmentTarget, Statement, StaticMemberExpression, UnaryExpression, UpdateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping};
use oxc_syntax::operator::UnaryOperator;

use crate::parse::{
    GlobalDeclTracker, ParsedScript, collect_unsupported_diagnostics, line_for_span, parse_script,
};
use crate::resolve::{
    ScriptQueryBindings, build_cross_script_query_map, extract_dom_query_from_call,
    extract_string_literal, is_global_document, kebab_to_camel, known_dom_event_from_on_prop,
    resolve_module_specifier, unwrap_parens,
};
use crate::{Analysis, Diagnostic, DomQuery, ScriptInput, ScriptKind, Severity};

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

    for (_, input, parsed) in &valid_scripts {
        let scoping = &parsed.scoping;
        let bindings = ScriptQueryBindings::build(input, &parsed.program, scoping, &cross_script);
        let mut visitor = AnalysisVisitor {
            scoping,
            bindings: &bindings,
            analysis: &mut analysis,
            seen_queries: &mut seen_queries,
            seen_listened: &mut seen_listened,
            seen_writes: &mut seen_writes,
        };
        visitor.visit_program(&parsed.program);
    }

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

impl<'a> Visit<'a> for AnalysisVisitor<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
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
