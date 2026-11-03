use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BindingPattern,
    CallExpression, Class, ComputedMemberExpression, Expression, Function, IdentifierReference,
    MethodDefinition, ObjectProperty, ObjectPropertyKind, ReturnStatement, SimpleAssignmentTarget,
    Statement, StaticMemberExpression, ThisExpression, UnaryExpression, UpdateExpression,
    VariableDeclarator,
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
    extract_string_literal, is_global_document, is_global_ident, kebab_to_camel,
    known_dom_event_from_on_prop, resolve_module_specifier, unwrap_parens,
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
    let mut lookups = LookupFacts::default();

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
            lookups: &mut lookups,
            path: Vec::new(),
            this_is_element: vec![matches!(input.kind, ScriptKind::HandlerAttribute { .. })],
            pending_method: false,
            class_reads_ok: HashSet::new(),
            listener_calls: 0,
            listener_mentions: 0,
        };
        visitor.visit_program(&parsed.program);
        for query in facts.derived.take() {
            visitor.record_query(query);
        }
        // Held or bound rather than called: no telling what it goes on to hear.
        if visitor.listener_mentions > visitor.listener_calls {
            visitor.analysis.hears_unnamed_events = true;
            visitor.analysis.listens_unresolved = true;
        }
        // `el["on" + x] = f` can be any listener; no name reads, none is ruled out.
        if crate::computed::may_hear_unnamed(&parsed.program, scoping) {
            analysis.hears_unnamed_events = true;
        }
    }

    analysis.created = dynamic.finish();
    analysis.open_elements |= analysis.created.iter().any(|c| c.open_classes)
        || analysis.class_writes.iter().any(|w| w.open);
    analysis.runtime_dom |= analysis.mutates_dom;
    for write in &analysis.class_writes {
        lookups.written_classes.extend(write.names.iter().cloned());
        lookups.any_class_changes |= write.open || write.assigns;
    }
    lookups.attrs_change |= !analysis.class_writes.is_empty();
    lookups.tree_changes = analysis.mutates_dom;
    analysis.live_queries = analysis
        .queries
        .iter()
        .filter(|q| lookups.is_live(q))
        .cloned()
        .collect();
    analysis.runtime_dom |= !analysis.live_queries.is_empty() || analysis.adopts_for_inline;
    analysis.reads_attributes |= analysis.queries.iter().any(|q| match q {
        DomQuery::ById(_) => false,
        DomQuery::Selector(s) | DomQuery::SelectorAll(s) => s.contains('['),
    });
    analysis.runtime_dom |= analysis.reads_attributes;
    analysis
}

/// What scripts do anywhere on the page that can change a lookup's answer.
///
/// Deliberately flow-insensitive: a handler can run at any time, and a value
/// passed around cannot be followed, so the question is whether a write exists
/// at all, not whether it precedes the lookup. Static resolution survives only
/// when no such write exists, which is what makes it equivalent.
#[derive(Default)]
struct LookupFacts {
    /// An element with an id can appear, disappear or change id: id writes,
    /// removals, parsed markup, clones, and moves of elements analysis cannot
    /// tell were created by script.
    ids_change: bool,
    /// Elements appear or move at all.
    tree_changes: bool,
    /// Class names scripts write, when that is all a class write can do.
    written_classes: HashSet<String>,
    /// A class write could change any class: a `className =` drops what it
    /// does not name, and a name may not be literal.
    any_class_changes: bool,
    /// Any property or attribute is written, which `[attr]` and pseudo-classes
    /// may read.
    attrs_change: bool,
}

impl LookupFacts {
    fn is_live(&self, query: &DomQuery) -> bool {
        match query {
            DomQuery::ById(_) => self.ids_change,
            DomQuery::Selector(s) | DomQuery::SelectorAll(s) => self.selector_is_live(s),
        }
    }

    fn selector_is_live(&self, selector: &str) -> bool {
        if has_state_pseudo(selector) || self.ids_change {
            return true;
        }
        // An element created without an id cannot match `#id`.
        if is_plain_id_selector(selector) {
            return false;
        }
        self.tree_changes
            || self.classes_in(selector)
            || (self.attrs_change && (selector.contains('[') || selector.contains(':')))
    }
}

impl LookupFacts {
    /// Whether a class write can affect a `.class` of `selector`. Reading it
    /// loosely only errs towards live: a `.` inside `[href=".x"]` looks like a
    /// class, and an escape does not match the name it stands for, so an
    /// escape counts as any class.
    fn classes_in(&self, selector: &str) -> bool {
        if !selector.contains('.') {
            return false;
        }
        self.any_class_changes
            || selector.contains('\\')
            || selector.split('.').skip(1).any(|rest| {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
                    .collect();
                self.written_classes.contains(&name)
            })
    }
}

/// `#name`, with nothing after it.
fn is_plain_id_selector(selector: &str) -> bool {
    selector.trim().strip_prefix('#').is_some_and(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

/// Pseudo-classes that follow the user as well as scripts, so no absence of
/// writes makes them static.
fn has_state_pseudo(selector: &str) -> bool {
    [
        ":checked",
        ":hover",
        ":focus",
        ":active",
        ":disabled",
        ":enabled",
        ":indeterminate",
        ":default",
        ":valid",
        ":invalid",
        ":placeholder-shown",
        ":read-only",
        ":read-write",
        ":target",
        ":visited",
        ":link",
        ":any-link",
    ]
    .iter()
    .any(|p| selector.contains(p))
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
    listener_calls: usize,
    listener_mentions: usize,
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
    lookups: &'b mut LookupFacts,
    /// Enclosing functions and conditional constructs (`true` = function).
    path: Vec<(u32, bool)>,
    /// Whether `this` may be an element, per enclosing function.
    this_is_element: Vec<bool>,
    /// The next function visited is a class or object method.
    pending_method: bool,
    /// `className`/`classList`/`dataset` member expressions that only write.
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
        self.note_inline_write(static_member, computed_member);
        if let Some(member) = static_member {
            let prop = member.property.name.as_str();
            if prop == "innerHTML" || prop == "outerHTML" {
                self.analysis.mutates_dom = true;
            }

            if let Some(ev) = known_dom_event_from_on_prop(prop) {
                self.analysis.heard_events.insert(ev.to_string());
            }
            if let Some(query) = self.bindings.resolve_expr_for_analysis(&member.object) {
                if let Some(ev) = known_dom_event_from_on_prop(prop) {
                    self.record_listened(query, ev);
                } else {
                    self.record_write(query, prop);
                }
                return;
            }
            if known_dom_event_from_on_prop(prop).is_some_and(crate::bubbles) {
                self.analysis.listens_unresolved = true;
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
    /// What only a generic box can answer to opens the page.
    fn note_inline_write(
        &mut self,
        static_member: Option<&StaticMemberExpression<'_>>,
        computed_member: Option<&ComputedMemberExpression<'_>>,
    ) {
        let opens = if let Some(member) = static_member {
            is_style_object(&member.object) && !inline_holds_in_place(member.property.name.as_str())
        } else if let Some(comp) = computed_member {
            is_style_object(&comp.object)
                && !extract_string_literal(&comp.expression)
                    .is_some_and(|prop| inline_holds_in_place(&prop))
        } else {
            false
        };
        self.analysis.open_elements |= opens;
        self.analysis.adopts_for_inline |= opens;
    }

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

    /// `object`'s children change (`sibling`: its parent's do).
    fn record_mutated(&mut self, object: &Expression<'_>, sibling: bool) {
        let target = self.target(object);
        if target == ElementRef::Unknown {
            return;
        }
        let list = if sibling {
            &mut self.analysis.mutated_siblings
        } else {
            &mut self.analysis.mutated_parents
        };
        if !list.contains(&target) {
            list.push(target);
        }
    }

    fn record_inserts<'x, 'y: 'x>(
        &mut self,
        parent: &ElementRef,
        children: impl IntoIterator<Item = &'x Argument<'y>>,
        span: Span,
    ) {
        for arg in children {
            match arg.as_expression() {
                Some(e) => {
                    if let ElementRef::Created(id) = self.target(e) {
                        // The child goes in once the arguments have run.
                        let pos = Pos {
                            path: self.path.clone(),
                            at: span.end,
                        };
                        self.dynamic.inserts.push((id, parent.clone(), pos));
                    } else if !is_text_node(e, self.scoping) {
                        self.record_opaque(parent);
                    }
                }
                None => self.record_opaque(parent),
            }
        }
    }

    fn record_opaque(&mut self, parent: &ElementRef) {
        if !self.analysis.opaque_inserts.contains(parent) {
            self.analysis.opaque_inserts.push(parent.clone());
        }
    }

    /// Where `insertAdjacentElement`/`insertAdjacentHTML` at `position` puts
    /// its argument: in the receiver, or beside it in a parent analysis does
    /// not know.
    fn adjacent_parent(&self, receiver: &Expression<'_>, position: Option<String>) -> ElementRef {
        let inside = position
            .is_some_and(|p| matches!(p.to_ascii_lowercase().as_str(), "afterbegin" | "beforeend"));
        if inside {
            self.target(receiver)
        } else {
            ElementRef::Unknown
        }
    }

    /// The elements of markup inserted into `parent` become created elements
    /// (`docs/design/dynamic-styles.md` §6.7). Each alternative value is
    /// parsed by the HTML parser with a sentinel for every part that is not
    /// literal. Inside an element such a part is taken as text or an
    /// attribute value; between elements, or in a tag or attribute name, it
    /// may be markup, which is reported.
    fn record_markup(&mut self, parent: &ElementRef, value: &Expression<'_>, span: Span) {
        let written = self.eval.strings(value);
        // A variable inside a tag may resolve to one literal here and still be
        // the script's choice of what the element is.
        self.analysis.open_elements |= written.iter().any(|alt| computed_in_a_tag(alt));
        let alts = self.facts.resolve_str(&written, 0);
        let line = Some(line_for_span(self.input, span));
        let mut shapes = MarkupShapes::default();
        let mut opaque = false;
        let mut unread = false;
        for alt in &alts {
            let mut text = String::new();
            for piece in alt {
                match piece {
                    Piece::Lit(s) => text.push_str(&s.replace(SENTINEL, "")),
                    Piece::Open | Piece::Sym(_) => {
                        unread = true;
                        text.push(SENTINEL);
                    }
                }
            }
            let doc = eldur_dom::Document::parse_fragment(&text);
            if let Some(top) = doc.html() {
                opaque |= shapes.walk(&doc, top, None, true);
            }
        }
        if unread || opaque {
            self.record_opaque(parent);
        }
        if opaque {
            self.analysis.open_elements = true;
            self.analysis.diagnostics.push(Diagnostic {
                severity: Severity::Unsupported,
                source: self.input.name.clone(),
                line,
                message: "markup is not literal: only the elements written literally take the page's styles; the rest use generic items (dynamic-markup)".to_string(),
            });
        }
        let base = self.dynamic.sites.len();
        let id_of = |i: usize| CreatedId(u32::try_from(base + i).unwrap_or(u32::MAX));
        for (i, shape) in shapes.shapes.into_iter().enumerate() {
            let id = id_of(i);
            self.dynamic.sites.push(CreatedSite {
                tag: shape.tag,
                source: self.input.name.clone(),
                line,
                path: self.path.clone(),
            });
            if let Some(ty) = shape.input_type {
                self.dynamic.input_types.push((id, ty));
            }
            let pos = self.pos(span);
            self.dynamic
                .class_ops
                .push((id, ClassOp::Assign(vec![shape.class]), pos.clone()));
            let into = shape
                .parent
                .map_or_else(|| parent.clone(), |p| ElementRef::Created(id_of(p)));
            self.dynamic.inserts.push((id, into, pos));
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

    /// Facts for [`LookupFacts`] from a method call.
    fn inspect_lookup_call(
        &mut self,
        it: &CallExpression<'_>,
        member: &StaticMemberExpression<'_>,
    ) {
        let method = member.property.name.as_str();
        let args = &it.arguments;
        let dom_receiver = !is_non_dom_collection_receiver(&member.object, self.scoping);
        // Inserting what analysis knows a script just created adds elements
        // without ids of their own; anything else may be a page element moving.
        let inserts_unknown = |me: &Self, args: &[&Argument<'_>]| {
            args.iter().any(|a| {
                a.as_expression()
                    .is_none_or(|e| !matches!(me.target(e), ElementRef::Created(_)))
            })
        };
        if matches!(
            method,
            "getAttribute"
                | "hasAttribute"
                | "getAttributeNode"
                | "getAttributeNames"
                | "cloneNode"
                | "matches"
                | "closest"
        ) || (matches!(method, "querySelector" | "querySelectorAll")
            && extract_dom_query_from_call(it, self.scoping).is_none())
        {
            self.analysis.reads_attributes = true;
        }
        if matches!(
            method,
            "setAttribute" | "removeAttribute" | "toggleAttribute"
        ) {
            self.note_root_write(&member.object);
        }
        if let Expression::StaticMemberExpression(inner) = unwrap_parens(&member.object)
            && inner.property.name == "classList"
        {
            self.note_root_write(&inner.object);
        }
        match method {
            "setAttribute" | "removeAttribute" | "toggleAttribute" if dom_receiver => {
                self.lookups.attrs_change = true;
                let names_id = args
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                    .is_none_or(|name| name.eq_ignore_ascii_case("id"));
                self.lookups.ids_change |= names_id;
            }
            "remove" if !dom_receiver || !args.is_empty() => {}
            "remove" | "removeChild" | "replaceChild" | "replaceWith" | "replaceChildren"
            | "insertAdjacentHTML" | "cloneNode" | "insertAdjacentText" => {
                self.lookups.ids_change = true;
            }
            "appendChild" | "insertBefore" => {
                let first: Vec<_> = args.iter().take(1).collect();
                self.lookups.ids_change |= inserts_unknown(self, &first);
            }
            "append" | "prepend" | "before" | "after" if dom_receiver => {
                let all: Vec<_> = args.iter().collect();
                self.lookups.ids_change |= inserts_unknown(self, &all);
            }
            "insertAdjacentElement" => {
                let second: Vec<_> = args.iter().skip(1).take(1).collect();
                self.lookups.ids_change |= inserts_unknown(self, &second);
            }
            "createDocumentFragment" if is_global_document(&member.object, self.scoping) => {
                self.lookups.ids_change = true;
            }
            "assign"
                if matches!(unwrap_parens(&member.object), Expression::Identifier(id) if id.name == "Object")
                    && args
                        .first()
                        .and_then(|a| a.as_expression())
                        .is_some_and(|e| self.target(e) != ElementRef::Unknown) =>
            {
                self.lookups.ids_change = true;
            }
            _ => {}
        }
    }

    /// Everything on the page can depend on the root's attributes, through a
    /// descendant selector or a custom property, and no variant of an element
    /// says how: the runtime has to resolve the page from its rules.
    /// What an `el.id = …` or `el.dataset.x = …` leaves for a selector to ask.
    fn inspect_attribute_write(&mut self, it: &AssignmentExpression<'_>) {
        let AssignmentTarget::StaticMemberExpression(member) = &it.left else {
            return;
        };
        if member.property.name == "id" {
            match extract_string_literal(&it.right) {
                Some(id) => {
                    self.analysis.written_ids.insert(id);
                }
                None => self.analysis.writes_computed_id = true,
            }
        } else if let Expression::StaticMemberExpression(inner) = unwrap_parens(&member.object)
            && inner.property.name == "dataset"
        {
            let mut name = String::from("data-");
            for c in member.property.name.chars() {
                if c.is_ascii_uppercase() {
                    name.push('-');
                }
                name.push(c.to_ascii_lowercase());
            }
            self.analysis.written_attributes.insert(name);
        }
    }

    fn note_root_write(&mut self, object: &Expression<'_>) {
        if self.target(object) == ElementRef::Query(DomQuery::Selector("html".to_string())) {
            self.analysis.open_elements = true;
        }
    }

    /// Facts for [`LookupFacts`] from an assignment to a member.
    fn inspect_lookup_write(&mut self, it: &AssignmentExpression<'_>) {
        let (object, prop) = match &it.left {
            AssignmentTarget::StaticMemberExpression(m) => {
                (&m.object, Some(m.property.name.to_string()))
            }
            AssignmentTarget::ComputedMemberExpression(m) => {
                (&m.object, extract_string_literal(&m.expression))
            }
            _ => return,
        };
        self.lookups.attrs_change = true;
        // Writing `el.dataset.x` reads nothing the markup said.
        if let Expression::StaticMemberExpression(inner) = unwrap_parens(object)
            && inner.property.name == "dataset"
        {
            self.class_reads_ok.insert(span_key(inner.span));
        }
        if let Expression::StaticMemberExpression(inner) = unwrap_parens(object)
            && inner.property.name == "dataset"
        {
            self.note_root_write(&inner.object);
        }
        if matches!(prop.as_deref(), Some("className")) {
            self.note_root_write(object);
        }
        let target = self.target(object);
        match prop.as_deref() {
            Some("id" | "innerHTML" | "outerHTML" | "outerText") => {
                self.lookups.ids_change = true;
            }
            Some("textContent" | "innerText") => match target {
                // Whether this removes elements depends on the document.
                ElementRef::Query(_) => {
                    if !self.analysis.text_writes.contains(&target) {
                        self.analysis.text_writes.push(target);
                    }
                }
                // Nothing with an id can be inside it without a move, which
                // is a fact of its own.
                ElementRef::Created(_) => {}
                ElementRef::Unknown => self.lookups.ids_change = true,
            },
            // `el[key] = v` could be `id`, but `list[i] = v` is everywhere;
            // only a receiver known to be an element counts.
            None => self.lookups.ids_change |= target != ElementRef::Unknown,
            Some(_) => {}
        }
    }

    /// `Object.assign(el, { className: …, id: … })` writes those as `el.className = …` would.
    fn record_assigned_props(&mut self, it: &CallExpression<'_>) {
        let Some(target) = it.arguments.first().and_then(|a| a.as_expression()) else {
            return;
        };
        for source in it.arguments.iter().skip(1) {
            let Some(Expression::ObjectExpression(object)) =
                source.as_expression().map(unwrap_parens)
            else {
                continue;
            };
            for prop in &object.properties {
                let ObjectPropertyKind::ObjectProperty(prop) = prop else {
                    continue;
                };
                let Some(key) = prop.key.static_name() else {
                    continue;
                };
                match key.as_ref() {
                    "className" => {
                        let value = self.class_value(&prop.value, false);
                        self.record_class_write(target, ClassOp::Assign(value), true, it.span);
                    }
                    "id" => match extract_string_literal(&prop.value) {
                        Some(id) => {
                            self.analysis.written_ids.insert(id);
                        }
                        None => self.analysis.writes_computed_id = true,
                    },
                    "textContent" | "innerText" => self.record_mutated(target, false),
                    "innerHTML" => {
                        self.record_mutated(target, false);
                        let parent = self.target(target);
                        self.record_markup(&parent, &prop.value, it.span);
                    }
                    _ => {}
                }
            }
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

        if matches!(method, "setProperty" | "removeProperty")
            && is_style_object(&member.object)
            && !literal_arg(0).is_some_and(|prop| inline_holds_in_place(&prop))
        {
            self.analysis.open_elements = true;
            self.analysis.adopts_for_inline = true;
        }
        if matches!(
            method,
            "setAttribute" | "removeAttribute" | "toggleAttribute"
        ) {
            let attr = literal_arg(0).map(|a| a.to_ascii_lowercase());
            match attr.as_deref() {
                Some("id") => match literal_arg(1) {
                    Some(id) => {
                        self.analysis.written_ids.insert(id);
                    }
                    None => self.analysis.writes_computed_id = true,
                },
                Some("class" | "style") => {}
                Some(name) => {
                    self.analysis.written_attributes.insert(name.to_owned());
                }
                None => {
                    self.analysis.writes_computed_id = true;
                    self.analysis.writes_computed_attribute = true;
                }
            }
            self.analysis.touches_inline_style |= attr
                .as_deref()
                .is_none_or(|a| matches!(a, "style" | "hidden"));
            let opens = attr.as_deref().is_none_or(|a| a == "style");
            self.analysis.open_elements |= opens;
            self.analysis.adopts_for_inline |= opens;
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
                self.record_mutated(&member.object, false);
                let parent = self.target(&member.object);
                self.record_inserts(&parent, args.first(), it.span);
            }
            "removeChild" => self.record_mutated(&member.object, false),
            "assign" if is_global_ident(&member.object, "Object", self.scoping) => {
                self.record_assigned_props(it);
            }
            m if table_part_made(m).is_some() => {
                self.record_mutated(&member.object, false);
                if let Some(id) = self.eval.created_at.get(&span_key(it.span)) {
                    let parent = self.target(&member.object);
                    let pos = self.pos(it.span);
                    self.dynamic.inserts.push((*id, parent, pos));
                }
            }
            "append" | "prepend" | "replaceChildren" if dom_receiver => {
                self.record_mutated(&member.object, false);
                let parent = self.target(&member.object);
                self.record_inserts(&parent, args.iter(), it.span);
            }
            "insertAdjacentHTML" => {
                let parent = self.adjacent_parent(&member.object, literal_arg(0));
                self.record_mutated(&member.object, parent == ElementRef::Unknown);
                if let Some(markup) = args.get(1).and_then(|a| a.as_expression()) {
                    self.record_markup(&parent, markup, it.span);
                }
            }
            "insertAdjacentElement" => {
                let parent = self.adjacent_parent(&member.object, literal_arg(0));
                self.record_mutated(&member.object, parent == ElementRef::Unknown);
                self.record_inserts(&parent, args.get(1), it.span);
            }
            // The parent is the receiver's parent, which analysis does not know.
            "before" | "after" | "replaceWith" if dom_receiver => {
                self.record_mutated(&member.object, true);
                self.record_inserts(&ElementRef::Unknown, args.iter(), it.span);
            }
            "remove" if dom_receiver && args.is_empty() => {
                self.record_mutated(&member.object, true);
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
                self.analysis.computed_lookups = true;
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
            // `+=` keeps the old children and parses the new markup after them.
            ("innerHTML", AssignmentOperator::Assign | AssignmentOperator::Addition) => {
                self.record_mutated(&member.object, false);
                let parent = self.target(&member.object);
                self.record_markup(&parent, &it.right, it.span);
            }
            ("textContent" | "innerText", AssignmentOperator::Assign) => {
                self.record_mutated(&member.object, false);
            }
            ("outerHTML" | "outerText", AssignmentOperator::Assign) => {
                self.record_mutated(&member.object, true);
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
        let reads_style = match unwrap_parens(&it.callee) {
            Expression::Identifier(name) => name.name == "getComputedStyle",
            Expression::StaticMemberExpression(member) => {
                member.property.name == "getComputedStyle"
            }
            _ => false,
        };
        if reads_style {
            self.analysis.reads_computed_style = true;
            self.analysis.runtime_dom = true;
        }
        if let Some(id) = self.eval.created_at.get(&span_key(it.span))
            && let Some(site) = self.dynamic.sites.get_mut(id.0 as usize)
        {
            site.path = self.path.clone();
        }
        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee) {
            let method = member.property.name.as_str();
            if method == "addEventListener" {
                self.listener_calls += 1;
                match it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                {
                    Some(ev) => {
                        self.analysis.heard_events.insert(ev.to_string());
                    }
                    None => self.analysis.hears_unnamed_events = true,
                }
            }
            if method == "getSelection" {
                self.analysis.reads_text_selection = true;
            }

            if member.property.name == "createElement"
                && it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                    .is_none()
            {
                self.analysis.open_elements = true;
            }
            // An animation object answers from the live style of its element,
            // and what starts or ends one is told by the same clock.
            let moves = matches!(member.property.name.as_str(), "getAnimations" | "animate")
                || (member.property.name == "addEventListener"
                    && it
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(extract_string_literal)
                        .is_some_and(|t| is_motion_event(&t)));
            if moves {
                self.analysis.open_elements = true;
                self.analysis.runtime_dom = true;
            }
            self.inspect_lookup_call(it, member);
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
            } else if method == "addEventListener"
                && it
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
                    .is_none_or(|ev| crate::bubbles(&ev))
            {
                self.analysis.listens_unresolved = true;
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
        // Later writes to the variable may mean either element.
        if matches!(it.left, AssignmentTarget::AssignmentTargetIdentifier(_))
            && matches!(unwrap_parens(&it.right), Expression::CallExpression(call)
                if matches!(unwrap_parens(&call.callee), Expression::StaticMemberExpression(m)
                    if m.property.name == "createElement"))
        {
            self.analysis.open_elements = true;
        }
        self.inspect_lookup_write(it);
        self.inspect_attribute_write(it);
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

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        // The rest is `computed`'s: it depends on where the member stands.
        if let Expression::StringLiteral(name) = &it.expression {
            let name = name.value.as_str();
            if name == "addEventListener" {
                self.listener_mentions += 1;
            }
            if let Some(event) = name.strip_prefix("on") {
                self.analysis.heard_events.insert(event.to_owned());
            }
        }
        walk::walk_computed_member_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        let prop = it.property.name.as_str();
        if prop == "addEventListener" {
            self.listener_mentions += 1;
        }
        self.analysis.touches_inline_style |= matches!(prop, "style" | "hidden");
        if prop.strip_prefix("on").is_some_and(is_motion_event) {
            self.analysis.open_elements = true;
            self.analysis.runtime_dom = true;
        }
        if prop == "content" {
            // What a `<template>` holds is made at run time, from the rules.
            self.analysis.reads_template_content = true;
            self.analysis.open_elements = true;
            self.analysis.runtime_dom = true;
        }
        self.analysis.reads_attributes |= prop == "attributes"
            || (prop == "dataset" && !self.class_reads_ok.contains(&span_key(it.span)));
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

fn is_style_object(expr: &Expression<'_>) -> bool {
    matches!(unwrap_parens(expr), Expression::StaticMemberExpression(m) if m.property.name == "style")
}

/// An inline property the page's item can take as it is, in either spelling.
/// The others change a box's size or need a paint the item may not have.
fn inline_holds_in_place(prop: &str) -> bool {
    prop.starts_with("--")
        || matches!(
            prop,
            "display"
                | "visibility"
                | "opacity"
                | "color"
                | "zIndex"
                | "z-index"
                | "left"
                | "top"
                | "right"
                | "bottom"
                | "cursor"
                | "pointerEvents"
                | "pointer-events"
        )
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
    /// A table part a table, section or row holds (`rows[i]`, `tHead`).
    Part(Box<ElemExpr>, TablePart),
    Unknown,
}

/// The table collections and parts HTML gives `<table>`, its sections and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TablePart {
    Rows,
    Cells,
    Bodies,
    Head,
    Foot,
}

impl TablePart {
    fn collection(name: &str) -> Option<Self> {
        Some(match name {
            "rows" => Self::Rows,
            "cells" => Self::Cells,
            "tBodies" => Self::Bodies,
            _ => return None,
        })
    }

    /// The children of `owner` this part is among. A table's rows are its own
    /// and its sections'; a section's rows are its own, which is the same
    /// selector, since a section holds no section.
    fn selector(self, owner: &str) -> String {
        let steps: &[&str] = match self {
            Self::Rows => &["tr", "thead > tr", "tbody > tr", "tfoot > tr"],
            Self::Cells => &["td", "th"],
            Self::Bodies => &["tbody"],
            Self::Head => &["thead"],
            Self::Foot => &["tfoot"],
        };
        let owners = split_selector_list(owner);
        owners
            .iter()
            .flat_map(|o| steps.iter().map(move |step| format!("{o} > {step}")))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A selector list's selectors, split at commas outside brackets and strings.
fn split_selector_list(list: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0_u32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (at, c) in list.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                out.push(list[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(list[start..].trim());
    out
}

/// The selector a query is, when it can be written as one.
fn query_selector(query: &DomQuery) -> Option<String> {
    match query {
        DomQuery::ById(id) => {
            let plain = id
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            plain.then(|| format!("#{id}"))
        }
        DomQuery::Selector(s) | DomQuery::SelectorAll(s) => Some(s.clone()),
    }
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
                // `Object.assign` returns its target.
                if let Expression::StaticMemberExpression(m) = unwrap_parens(&call.callee)
                    && m.property.name == "assign"
                    && is_global_ident(&m.object, "Object", self.scoping)
                    && let Some(target) = call.arguments.first().and_then(|a| a.as_expression())
                {
                    return self.elem(target);
                }
                if let Expression::Identifier(callee) = unwrap_parens(&call.callee)
                    && let Some(sym) = self.stable_symbol(callee)
                {
                    return ElemExpr::CallOf(sym);
                }
                if let Expression::StaticMemberExpression(m) = unwrap_parens(&call.callee)
                    && m.property.name == "item"
                {
                    return self.collection_item(&m.object);
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
            Expression::StaticMemberExpression(m) => match m.property.name.as_str() {
                "tHead" => ElemExpr::Part(Box::new(self.elem(&m.object)), TablePart::Head),
                "tFoot" => ElemExpr::Part(Box::new(self.elem(&m.object)), TablePart::Foot),
                _ => ElemExpr::Unknown,
            },
            // `table.rows[0]`: which one is not followed, only that it is one of them.
            Expression::ComputedMemberExpression(m) => self.collection_item(&m.object),
            _ => ElemExpr::Unknown,
        }
    }

    fn collection_item(&self, collection: &Expression<'_>) -> ElemExpr {
        if let Expression::StaticMemberExpression(m) = unwrap_parens(collection)
            && let Some(part) = TablePart::collection(&m.property.name)
        {
            return ElemExpr::Part(Box::new(self.elem(&m.object)), part);
        }
        ElemExpr::Unknown
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
    /// Queries a table part was resolved to: no script says them, but what
    /// they find is where its elements go.
    derived: std::cell::RefCell<Vec<DomQuery>>,
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
            ElemExpr::Part(owner, part) => {
                let ElementRef::Query(owner) = self.resolve_elem(owner, allow_call, depth + 1)
                else {
                    return ElementRef::Unknown;
                };
                let Some(owner) = query_selector(&owner) else {
                    return ElementRef::Unknown;
                };
                let query = DomQuery::SelectorAll(part.selector(&owner));
                self.derived.borrow_mut().push(query.clone());
                ElementRef::Query(query)
            }
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

/// The part a table method makes and puts in its receiver: HTML's
/// `insertRow`, `insertCell` and `create*` are `createElement` with the
/// insertion done, and what they make is styled as any created element.
fn table_part_made(method: &str) -> Option<&'static str> {
    Some(match method {
        "insertRow" => "tr",
        "insertCell" => "td",
        "createTHead" => "thead",
        "createTFoot" => "tfoot",
        "createTBody" => "tbody",
        "createCaption" => "caption",
        _ => return None,
    })
}

impl<'a> Visit<'a> for CreatedSiteCollector<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let tag = match unwrap_parens(&it.callee) {
            Expression::StaticMemberExpression(member)
                if member.property.name == "createElement"
                    && is_global_document(&member.object, self.scoping) =>
            {
                it.arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(extract_string_literal)
            }
            Expression::StaticMemberExpression(member) => {
                table_part_made(&member.property.name).map(str::to_owned)
            }
            _ => None,
        };
        if let Some(tag) = tag {
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

/// Stands for a part of markup that is not literal. A private-use character,
/// so the parser keeps it in names, values and text.
const SENTINEL: char = '\u{E000}';

fn computed_in_a_tag(pieces: &[Piece]) -> bool {
    let mut in_tag = false;
    for piece in pieces {
        match piece {
            Piece::Lit(text) => {
                if let Some(at) = text.rfind(['<', '>']) {
                    in_tag = text[at..].starts_with('<');
                }
            }
            Piece::Open | Piece::Sym(_) => {
                if in_tag {
                    return true;
                }
            }
        }
    }
    false
}

/// One element of literal markup. Elements with the same tag, input type
/// and class value under the same parent, in any alternative of one value,
/// are one shape: they take the same templates. Siblings with different
/// classes are different shapes, as separate `createElement` calls are.
struct MarkupShape {
    tag: String,
    /// For `<input>`: its type, `None` when not literal.
    input_type: Option<Option<String>>,
    class: ClassAlt,
    /// Index of the parent shape; `None` for the inserted elements themselves.
    parent: Option<usize>,
}

#[derive(Default)]
struct MarkupShapes {
    shapes: Vec<MarkupShape>,
}

impl MarkupShapes {
    /// Records the element children of `node`. Returns whether some part of
    /// the markup may not be literal.
    fn walk(
        &mut self,
        doc: &eldur_dom::Document,
        node: eldur_dom::NodeId,
        parent: Option<usize>,
        top: bool,
    ) -> bool {
        let mut opaque = false;
        for child in doc.children(node) {
            if let Some(text) = doc.text(child) {
                opaque |= top && text.contains(SENTINEL);
                continue;
            }
            let Some(el) = doc.element(child) else {
                continue;
            };
            let names_open = el.local_name().contains(SENTINEL)
                || el.attrs.iter().any(|a| a.name.local.contains(SENTINEL));
            if names_open {
                opaque = true;
                continue;
            }
            // The runtime makes HTML elements only, and no scripts or styles.
            if !el.is_html() || matches!(el.local_name(), "script" | "style" | "template") {
                continue;
            }
            let tag = el.local_name().to_string();
            let input_type = (tag == "input").then(|| {
                let ty = el.attr("type").unwrap_or("text");
                (!ty.contains(SENTINEL)).then(|| ty.to_ascii_lowercase())
            });
            let mut class = ClassAlt::default();
            for token in el.classes() {
                if token.contains(SENTINEL) {
                    class.open = true;
                } else {
                    class.names.insert(token.to_string());
                }
            }
            let at = match self.shapes.iter().position(|s| {
                s.parent == parent && s.tag == tag && s.input_type == input_type && s.class == class
            }) {
                Some(at) => at,
                None => {
                    self.shapes.push(MarkupShape {
                        tag,
                        input_type,
                        class,
                        parent,
                    });
                    self.shapes.len() - 1
                }
            };
            opaque |= self.walk(doc, child, Some(at), false);
        }
        opaque
    }
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
            let mut later = false;
            for (_, op, pos) in home_ops.into_iter().chain(other_ops) {
                let frames = function_frames(&pos.path);
                let group = if frames.starts_with(&home) {
                    home.clone()
                } else {
                    frames
                };
                if first_insert(&group).is_some_and(|at| pos.at > at) {
                    // In the page, it can change to a set no template was written
                    // for: only the cascade knows what that shows or hides.
                    later = true;
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
            let open_classes = capped || later || states.iter().any(|s| s.open);

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

/// A string or a text node made in place: an insert that adds no element.
fn is_text_node(expr: &Expression<'_>, scoping: &Scoping) -> bool {
    if extract_string_literal(expr).is_some() {
        return true;
    }
    let Expression::CallExpression(call) = unwrap_parens(expr) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = unwrap_parens(&call.callee) else {
        return false;
    };
    member.property.name == "createTextNode" && is_global_document(&member.object, scoping)
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

/// The events of CSS transitions and animations.
pub(crate) fn is_motion_event(name: &str) -> bool {
    matches!(
        name,
        "transitionrun"
            | "transitionstart"
            | "transitionend"
            | "transitioncancel"
            | "animationstart"
            | "animationiteration"
            | "animationend"
            | "animationcancel"
    )
}
