use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    BindingPattern, CallExpression, Class, Expression, ForOfStatement, ForStatementLeft, Function,
    IdentifierReference, Program, Statement, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping, SymbolId};
use oxc_syntax::scope::ScopeFlags;

use crate::parse::ParsedScript;
use crate::{DomQuery, ScriptInput, ScriptKind};

pub(crate) fn unwrap_parens<'a, 'b>(mut expr: &'b Expression<'a>) -> &'b Expression<'a> {
    while let Expression::ParenthesizedExpression(p) = expr {
        expr = &p.expression;
    }
    expr
}

/// Returns `true` when `expr` is statically guaranteed NOT to evaluate to a DOM
/// `Element` / `Document` / `Window` / QML `QObject` (e.g. object literals,
/// array literals, `new ...` expressions, functions, classes, primitive literals,
/// or unmutated bindings initialized from such expressions).
pub(crate) fn is_statically_non_dom_expr(
    expr: &Expression<'_>,
    scoping: &Scoping,
    non_dom_symbols: &HashSet<SymbolId>,
) -> bool {
    match unwrap_parens(expr) {
        Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::NewExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::ClassExpression(_)
        | Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::UnaryExpression(_)
        | Expression::BinaryExpression(_)
        | Expression::UpdateExpression(_) => true,
        Expression::Identifier(ident) => ident
            .reference_id
            .get()
            .and_then(|rid| scoping.get_reference(rid).symbol_id())
            .is_some_and(|sym_id| non_dom_symbols.contains(&sym_id)),
        _ => false,
    }
}

pub(crate) fn extract_string_literal(expr: &Expression<'_>) -> Option<String> {
    match unwrap_parens(expr) {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => {
            tpl.quasis
                .first()
                .and_then(|q| q.value.cooked.as_ref())
                .map(ToString::to_string)
        }
        _ => None,
    }
}

pub(crate) fn is_global_ident(expr: &Expression<'_>, name: &str, scoping: &Scoping) -> bool {
    if let Expression::Identifier(ident) = unwrap_parens(expr) {
        ident.name == name && ident.is_global_reference(scoping)
    } else {
        false
    }
}

pub(crate) fn is_global_window(expr: &Expression<'_>, scoping: &Scoping) -> bool {
    let e = unwrap_parens(expr);
    is_global_ident(e, "window", scoping)
        || is_global_ident(e, "globalThis", scoping)
        || is_global_ident(e, "self", scoping)
}

pub(crate) fn is_global_document(expr: &Expression<'_>, scoping: &Scoping) -> bool {
    let e = unwrap_parens(expr);
    if is_global_ident(e, "document", scoping) {
        return true;
    }
    if let Expression::StaticMemberExpression(member) = e {
        return member.property.name == "document" && is_global_window(&member.object, scoping);
    }
    false
}

/// Recognizes literal DOM queries on unshadowed `document`:
/// `getElementById`, `querySelector`, `querySelectorAll`,
/// `getElementsByClassName`, `getElementsByTagName`.
pub(crate) fn extract_dom_query(expr: &Expression<'_>, scoping: &Scoping) -> Option<DomQuery> {
    let Expression::CallExpression(call) = unwrap_parens(expr) else {
        return None;
    };
    extract_dom_query_from_call(call, scoping)
}

pub(crate) fn extract_dom_query_from_call(
    call: &CallExpression<'_>,
    scoping: &Scoping,
) -> Option<DomQuery> {
    let Expression::StaticMemberExpression(member) = unwrap_parens(&call.callee) else {
        return None;
    };
    if !is_global_document(&member.object, scoping) {
        return None;
    }
    let first_arg = call.arguments.first()?.as_expression()?;
    let lit = extract_string_literal(first_arg)?;
    match member.property.name.as_str() {
        "getElementById" => Some(DomQuery::ById(lit)),
        "querySelector" => Some(DomQuery::Selector(lit)),
        "querySelectorAll" => Some(DomQuery::SelectorAll(lit)),
        "getElementsByClassName" => {
            let classes: Vec<&str> = lit.split_whitespace().collect();
            if classes.is_empty() {
                None
            } else {
                Some(DomQuery::SelectorAll(format!(".{}", classes.join("."))))
            }
        }
        "getElementsByTagName" => {
            let trimmed = lit.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(DomQuery::SelectorAll(trimmed.to_string()))
            }
        }
        _ => None,
    }
}

/// Maps `onclick` -> `Some("click")`, `onkeydown` -> `Some("keydown")`, etc.
pub(crate) fn known_dom_event_from_on_prop(prop: &str) -> Option<&'static str> {
    match prop {
        "onclick" => Some("click"),
        "ondblclick" => Some("dblclick"),
        "oncontextmenu" => Some("contextmenu"),
        "onmousedown" => Some("mousedown"),
        "onmouseup" => Some("mouseup"),
        "onmousemove" => Some("mousemove"),
        "onmouseenter" => Some("mouseenter"),
        "onmouseleave" => Some("mouseleave"),
        "onmouseover" => Some("mouseover"),
        "onmouseout" => Some("mouseout"),
        "onpointerdown" => Some("pointerdown"),
        "onpointermove" => Some("pointermove"),
        "onpointerup" => Some("pointerup"),
        "onpointercancel" => Some("pointercancel"),
        "onpointerenter" => Some("pointerenter"),
        "onpointerleave" => Some("pointerleave"),
        "ontouchstart" => Some("touchstart"),
        "ontouchmove" => Some("touchmove"),
        "ontouchend" => Some("touchend"),
        "ontouchcancel" => Some("touchcancel"),
        "onwheel" => Some("wheel"),
        "onkeydown" => Some("keydown"),
        "onkeyup" => Some("keyup"),
        "onkeypress" => Some("keypress"),
        "oninput" => Some("input"),
        "onchange" => Some("change"),
        "onsubmit" => Some("submit"),
        "onreset" => Some("reset"),
        "onfocus" => Some("focus"),
        "onblur" => Some("blur"),
        "onfocusin" => Some("focusin"),
        "onfocusout" => Some("focusout"),
        "onscroll" => Some("scroll"),
        "onresize" => Some("resize"),
        "onload" => Some("load"),
        "onerror" => Some("error"),
        "ondragstart" => Some("dragstart"),
        "ondrag" => Some("drag"),
        "ondragenter" => Some("dragenter"),
        "ondragover" => Some("dragover"),
        "ondragleave" => Some("dragleave"),
        "ondrop" => Some("drop"),
        "onanimationend" => Some("animationend"),
        "ontransitionend" => Some("transitionend"),
        "onbeforeunload" => Some("beforeunload"),
        "onunload" => Some("unload"),
        "onpagehide" => Some("pagehide"),
        "onpopstate" => Some("popstate"),
        "onhashchange" => Some("hashchange"),
        _ => None,
    }
}

pub(crate) fn normalize_handler_event(event: &str) -> &str {
    if event == "online" {
        "online"
    } else {
        event.strip_prefix("on").unwrap_or(event)
    }
}

pub(crate) fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = false;
    for ch in s.chars() {
        if ch == '-' {
            upper_next = true;
        } else if upper_next {
            for up in ch.to_uppercase() {
                out.push(up);
            }
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Resolves an ES module import specifier against the slice of [`ScriptInput`]s
/// provided to the converter.
pub(crate) fn resolve_module_specifier(
    importer_name: &str,
    specifier: &str,
    scripts: &[ScriptInput],
) -> Option<usize> {
    if let Some(exact) = scripts.iter().position(|s| {
        matches!(s.kind, ScriptKind::Module | ScriptKind::Classic) && s.name == specifier
    }) {
        return Some(exact);
    }

    if specifier.starts_with("http://")
        || specifier.starts_with("https://")
        || specifier.starts_with("data:")
        || specifier.starts_with("blob:")
    {
        return None;
    }

    // Bare specifiers (neither `./`, `../`, nor `/`) only match if a ScriptInput
    // was explicitly named with that exact bare path or filename.
    let is_relative_or_abs =
        specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/');

    let resolved_rel = if is_relative_or_abs {
        Some(normalize_rel_path(importer_name, specifier))
    } else {
        None
    };
    let stripped_spec = specifier.trim_start_matches("./").trim_start_matches('/');

    for (idx, candidate) in scripts.iter().enumerate() {
        if !matches!(candidate.kind, ScriptKind::Module | ScriptKind::Classic) {
            continue;
        }
        let cand_norm = normalize_slashes(&candidate.name);
        let cand_stripped = cand_norm.trim_start_matches("./").trim_start_matches('/');

        if let Some(rel) = &resolved_rel
            && (cand_stripped == rel.as_str() || cand_norm == rel.as_str())
        {
            return Some(idx);
        }

        if is_relative_or_abs
            && !stripped_spec.is_empty()
            && (cand_stripped == stripped_spec
                || cand_stripped
                    .strip_suffix(stripped_spec)
                    .is_some_and(|prefix| prefix.ends_with('/')))
        {
            return Some(idx);
        }
    }
    None
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

fn normalize_rel_path(importer_name: &str, specifier: &str) -> String {
    let spec_norm = normalize_slashes(specifier);
    let imp_norm = normalize_slashes(importer_name);
    let mut parts: Vec<&str> = Vec::new();
    if !spec_norm.starts_with('/')
        && let Some((dir, _)) = imp_norm.rsplit_once('/')
    {
        for seg in dir.split('/') {
            if !seg.is_empty() && seg != "." {
                parts.push(seg);
            }
        }
    }
    for seg in spec_norm.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Collects top-level variables in classic scripts that are bound once across
/// the entire page to a static [`DomQuery`] and never mutated or redeclared.
pub(crate) fn build_cross_script_query_map(
    scripts: &[(usize, &ScriptInput, &ParsedScript<'_>)],
) -> HashMap<String, DomQuery> {
    let mut mutated_globals: HashSet<String> = HashSet::new();
    let mut decl_counts: HashMap<String, usize> = HashMap::new();

    for (_, input, parsed) in scripts {
        let scoping = &parsed.scoping;
        // Record any unresolved global writes (`btn = ...`) across classic/handler scripts.
        for (name, ref_ids) in scoping.root_unresolved_references() {
            for &rid in ref_ids {
                if scoping.get_reference(rid).is_write() {
                    mutated_globals.insert(name.to_string());
                }
            }
        }
        if matches!(input.kind, ScriptKind::Classic) {
            for sym_id in scoping.iter_bindings_in(scoping.root_scope_id()) {
                let name = scoping.symbol_name(sym_id).to_string();
                *decl_counts.entry(name.clone()).or_insert(0) += 1;
                if scoping.symbol_is_mutated(sym_id)
                    || !scoping.symbol_redeclarations(sym_id).is_empty()
                {
                    mutated_globals.insert(name);
                }
            }
        }
    }

    let mut result = HashMap::new();
    for (_, input, parsed) in scripts {
        if !matches!(input.kind, ScriptKind::Classic) {
            continue;
        }
        let scoping = &parsed.scoping;
        let root_scope = scoping.root_scope_id();
        for stmt in &parsed.program.body {
            let Statement::VariableDeclaration(vd) = stmt else {
                continue;
            };
            for decl in &vd.declarations {
                let BindingPattern::BindingIdentifier(bind_id) = &decl.id else {
                    continue;
                };
                let Some(sym_id) = bind_id.symbol_id.get() else {
                    continue;
                };
                if scoping.symbol_scope_id(sym_id) != root_scope {
                    continue;
                }
                let name = bind_id.name.as_str();
                if decl_counts.get(name).copied() != Some(1) || mutated_globals.contains(name) {
                    continue;
                }
                if let Some(init) = &decl.init
                    && let Some(query) = extract_dom_query(init, scoping)
                {
                    result.insert(name.to_string(), query);
                }
            }
        }
    }
    result
}

/// Per-script resolution table mapping single-assignment `SymbolId`s (both
/// top-level and inside functions) to their [`DomQuery`].
pub(crate) struct ScriptQueryBindings<'a> {
    scoping: &'a Scoping,
    is_global_scope_shared: bool,
    /// Single-bound variables (`const btn = document.getElementById('b')`).
    pub symbol_queries: HashMap<SymbolId, DomQuery>,
    /// Loop / `.forEach` element parameters over a `SelectorAll` query
    /// (used by `analyze` to record writes and listeners on collection items).
    pub collection_item_queries: HashMap<SymbolId, DomQuery>,
    /// Unmutated local/module/script symbols initialized to expressions that
    /// statically cannot be DOM nodes (object literals, arrays, `new ...`, functions,
    /// classes, primitive literals).
    pub non_dom_symbols: HashSet<SymbolId>,
    cross_script: &'a HashMap<String, DomQuery>,
}

impl<'a> ScriptQueryBindings<'a> {
    pub(crate) fn build(
        input: &ScriptInput,
        program: &Program<'_>,
        scoping: &'a Scoping,
        cross_script: &'a HashMap<String, DomQuery>,
    ) -> Self {
        let is_global_scope_shared = matches!(
            input.kind,
            ScriptKind::Classic | ScriptKind::HandlerAttribute { .. }
        );
        let mut collector = BindingCollector {
            scoping,
            is_global_scope_shared,
            cross_script,
            direct: HashMap::new(),
            aliases: Vec::new(),
            collection_items: HashMap::new(),
            non_dom_symbols: HashSet::new(),
        };
        collector.visit_program(program);

        // Resolve transitive single-assignment aliases (`const b = a;`).
        for _ in 0..8 {
            let mut progressed = false;
            for &(dst, src) in &collector.aliases {
                if !collector.direct.contains_key(&dst)
                    && let Some(q) = collector.direct.get(&src).cloned()
                {
                    collector.direct.insert(dst, q);
                    progressed = true;
                }
                if collector.non_dom_symbols.contains(&src) && collector.non_dom_symbols.insert(dst)
                {
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }

        // Second quick pass for collection loops (`items.forEach(...)` / `for (const el of items)`)
        // where `items` was bound to a `SelectorAll` variable!
        let collection_items = {
            let mut loop_collector = CollectionLoopCollector {
                scoping,
                is_global_scope_shared,
                direct: &collector.direct,
                cross_script,
                collection_items: collector.collection_items,
            };
            loop_collector.visit_program(program);
            loop_collector.collection_items
        };

        Self {
            scoping,
            is_global_scope_shared,
            symbol_queries: collector.direct,
            collection_item_queries: collection_items,
            non_dom_symbols: collector.non_dom_symbols,
            cross_script,
        }
    }

    /// Resolves `expr` to a [`DomQuery`] when it is either a direct query call
    /// or a variable bound once to a [`DomQuery`].
    pub(crate) fn resolve_expr(&self, expr: &Expression<'_>) -> Option<DomQuery> {
        let e = unwrap_parens(expr);
        if let Some(q) = extract_dom_query(e, self.scoping) {
            return Some(q);
        }
        if let Expression::Identifier(ident) = e {
            return self.resolve_ident(ident, false);
        }
        None
    }

    /// Resolves `expr` for static analysis, additionally including `.forEach` /
    /// `for..of` element parameters over `SelectorAll` queries.
    pub(crate) fn resolve_expr_for_analysis(&self, expr: &Expression<'_>) -> Option<DomQuery> {
        let e = unwrap_parens(expr);
        if let Some(q) = extract_dom_query(e, self.scoping) {
            return Some(q);
        }
        if let Expression::Identifier(ident) = e {
            return self.resolve_ident(ident, true);
        }
        None
    }

    pub(crate) fn resolve_ident(
        &self,
        ident: &IdentifierReference<'_>,
        include_collection_items: bool,
    ) -> Option<DomQuery> {
        if let Some(rid) = ident.reference_id.get()
            && let Some(sym_id) = self.scoping.get_reference(rid).symbol_id()
        {
            if let Some(q) = self.symbol_queries.get(&sym_id) {
                return Some(q.clone());
            }
            if include_collection_items && let Some(q) = self.collection_item_queries.get(&sym_id) {
                return Some(q.clone());
            }
            return None;
        }
        if self.is_global_scope_shared {
            return self.cross_script.get(ident.name.as_str()).cloned();
        }
        None
    }
}

struct BindingCollector<'a> {
    scoping: &'a Scoping,
    is_global_scope_shared: bool,
    cross_script: &'a HashMap<String, DomQuery>,
    direct: HashMap<SymbolId, DomQuery>,
    aliases: Vec<(SymbolId, SymbolId)>,
    collection_items: HashMap<SymbolId, DomQuery>,
    non_dom_symbols: HashSet<SymbolId>,
}

impl<'a> Visit<'a> for BindingCollector<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if let Some(id) = &it.id
            && let Some(sym_id) = id.symbol_id.get()
            && !self.scoping.symbol_is_mutated(sym_id)
            && self.scoping.symbol_redeclarations(sym_id).is_empty()
        {
            self.non_dom_symbols.insert(sym_id);
        }
        walk::walk_function(self, it, flags);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id
            && let Some(sym_id) = id.symbol_id.get()
            && !self.scoping.symbol_is_mutated(sym_id)
            && self.scoping.symbol_redeclarations(sym_id).is_empty()
        {
            self.non_dom_symbols.insert(sym_id);
        }
        walk::walk_class(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(bind_id) = &it.id
            && let Some(sym_id) = bind_id.symbol_id.get()
            && !self.scoping.symbol_is_mutated(sym_id)
            && self.scoping.symbol_redeclarations(sym_id).is_empty()
            && let Some(init) = &it.init
        {
            if let Some(q) = extract_dom_query(init, self.scoping) {
                self.direct.insert(sym_id, q);
            } else if let Expression::Identifier(rhs_id) = unwrap_parens(init) {
                if let Some(rid) = rhs_id.reference_id.get()
                    && let Some(rhs_sym) = self.scoping.get_reference(rid).symbol_id()
                {
                    if !self.scoping.symbol_is_mutated(rhs_sym)
                        && self.scoping.symbol_redeclarations(rhs_sym).is_empty()
                    {
                        self.aliases.push((sym_id, rhs_sym));
                    }
                } else if self.is_global_scope_shared
                    && let Some(q) = self.cross_script.get(rhs_id.name.as_str())
                {
                    self.direct.insert(sym_id, q.clone());
                }
            } else if is_statically_non_dom_expr(init, self.scoping, &self.non_dom_symbols) {
                self.non_dom_symbols.insert(sym_id);
            }
        }
        walk::walk_variable_declarator(self, it);
    }
}

struct CollectionLoopCollector<'a> {
    scoping: &'a Scoping,
    is_global_scope_shared: bool,
    direct: &'a HashMap<SymbolId, DomQuery>,
    cross_script: &'a HashMap<String, DomQuery>,
    collection_items: HashMap<SymbolId, DomQuery>,
}

impl CollectionLoopCollector<'_> {
    fn resolve_collection(&self, expr: &Expression<'_>) -> Option<DomQuery> {
        let e = unwrap_parens(expr);
        if let Some(q @ DomQuery::SelectorAll(_)) = extract_dom_query(e, self.scoping) {
            return Some(q);
        }
        if let Expression::Identifier(ident) = e {
            if let Some(rid) = ident.reference_id.get()
                && let Some(sym_id) = self.scoping.get_reference(rid).symbol_id()
            {
                if let Some(q @ DomQuery::SelectorAll(_)) = self.direct.get(&sym_id) {
                    return Some(q.clone());
                }
                return None;
            }
            if self.is_global_scope_shared
                && let Some(q @ DomQuery::SelectorAll(_)) =
                    self.cross_script.get(ident.name.as_str())
            {
                return Some(q.clone());
            }
        }
        None
    }
}

impl<'a> Visit<'a> for CollectionLoopCollector<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee)
            && member.property.name == "forEach"
            && let Some(query) = self.resolve_collection(&member.object)
            && let Some(first_arg) = it.arguments.first().and_then(|a| a.as_expression())
        {
            let first_param = match unwrap_parens(first_arg) {
                Expression::ArrowFunctionExpression(arrow) => arrow.params.items.first(),
                Expression::FunctionExpression(func) => func.params.items.first(),
                _ => None,
            };
            if let Some(param) = first_param
                && let BindingPattern::BindingIdentifier(bind_id) = &param.pattern
                && let Some(sym_id) = bind_id.symbol_id.get()
                && !self.scoping.symbol_is_mutated(sym_id)
            {
                self.collection_items.insert(sym_id, query);
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        if let Some(query) = self.resolve_collection(&it.right)
            && let ForStatementLeft::VariableDeclaration(vd) = &it.left
            && let Some(decl) = vd.declarations.first()
            && let BindingPattern::BindingIdentifier(bind_id) = &decl.id
            && let Some(sym_id) = bind_id.symbol_id.get()
            && !self.scoping.symbol_is_mutated(sym_id)
        {
            self.collection_items.insert(sym_id, query);
        }
        walk::walk_for_of_statement(self, it);
    }
}
