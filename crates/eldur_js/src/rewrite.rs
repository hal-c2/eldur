use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentOperator, AssignmentTarget, BindingIdentifier, BindingPattern, Declaration,
    ExportDefaultDeclarationKind, Expression, ImportDeclarationSpecifier, ObjectPropertyKind,
    Program, PropertyKind, Statement, UnaryOperator, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping, SymbolId};
use oxc_span::GetSpan;
use oxc_syntax::operator::BinaryOperator;
use oxc_syntax::scope::ScopeFlags;

use crate::analyze::collect_unresolvable_module_diagnostics;
use crate::lower::{
    AstLowerer, SharedRewriteState, TdzBinding, TdzKind, is_elidable_dom_declarator,
    sanitize_ident, static_property_key_name,
};
use crate::named::QML_GLOBAL_NAMES;
use crate::parse::{
    GlobalDeclTracker, ParsedScript, collect_binding_identifiers, collect_hoisted_var_names,
    collect_unsupported_diagnostics, parse_script,
};
use crate::resolve::{
    ScriptQueryBindings, build_cross_script_query_map, collect_window_property_writes,
    extract_string_literal, is_global_document, is_global_ident, is_global_window,
    known_dom_event_from_on_prop, normalize_handler_event, resolve_module_specifier, unwrap_parens,
};
use crate::{
    Analysis, Diagnostic, DomQuery, DomResolver, ElementKey, EventBinding, JsOutput, QmlTarget,
    ScriptInput, ScriptKind, Severity,
};

struct ValidScript<'a> {
    idx: usize,
    input: &'a ScriptInput,
    parsed: ParsedScript<'a>,
}

enum LoadEntry {
    Listener(String),
    WindowOnload,
}

#[derive(Default, Clone)]
struct ModuleExports {
    /// Maps exported name (`"foo"`, `"default"`) -> `(top_level_ident, deferred_to_init, is_mutable)`.
    entries: HashMap<String, (String, bool, bool)>,
    ordered_keys: Vec<String>,
}

impl ModuleExports {
    fn insert(
        &mut self,
        exported: impl Into<String>,
        local_ident: impl Into<String>,
        deferred: bool,
        is_mutable: bool,
    ) {
        let key = exported.into();
        if !self.entries.contains_key(&key) {
            self.ordered_keys.push(key.clone());
        }
        self.entries
            .insert(key, (local_ident.into(), deferred, is_mutable));
    }
}

/// Output buffers accumulated across scripts, kept separate from [`SharedRewriteState`]
/// so `AstLowerer` can borrow `&mut SharedRewriteState` while emission methods mutate buffers.
#[derive(Default)]
struct PageBuffers {
    top_decls: Vec<String>,
    /// Tracks `(top_decls_index, init_stmts_len_when_declared)` for classic top-level `var` / `function`
    /// bindings so cross-script redeclarations follow browser "later wins" semantics without emitting
    /// duplicate declarations in `page.js`.
    var_func_slots: HashMap<String, (usize, usize)>,
    /// Top-level `const`/`var` identifiers whose pure literal values are already available at module load,
    /// with what is known of each value.
    pure_top_vars: HashMap<String, PureKind>,
    handler_fns: Vec<String>,
    init_stmts: Vec<String>,
    dcl_stmts: Vec<String>,
    load_entries: Vec<LoadEntry>,
    window_onload_has_slot: bool,
    window_onload_value: Option<String>,
    /// Emitting a lazy module: nothing it declares exists until it runs, so
    /// its bindings are assigned in its run function, never set at load.
    in_lazy_module: bool,
    /// Emitting a module run at load: a `DOMContentLoaded` or `load` listener
    /// it adds is heard only if its code got that far.
    in_eager_module: bool,
    /// The run functions of modules run at load, emitted after every
    /// declaration they assign.
    module_fns: Vec<String>,
}

struct PageBuilder<'a> {
    scripts: &'a [ScriptInput],
    valid_map: HashMap<usize, usize>,
    valid_scripts: &'a [ValidScript<'a>],
    cross_script: &'a HashMap<String, crate::DomQuery>,
    resolver: &'a dyn DomResolver,
    shared: SharedRewriteState,
    out: PageBuffers,
    claimed_top_names: HashSet<String>,
    emitted_modules: HashSet<usize>,
    visiting_modules: HashSet<usize>,
    /// The modules being emitted, outermost first: a back-edge to one of them
    /// closes a cycle through every module after it.
    visiting_stack: Vec<usize>,
    /// Module index -> the cycle it is part of. A module that throws
    /// fails every module of its cycle, as a browser's module map records.
    cycle_groups: HashMap<usize, usize>,
    /// Module index -> the statement in its run function's `catch`
    /// replaced, once the cycles are known, by marking the rest of its cycle
    /// failed too.
    cycle_failure_markers: HashMap<usize, String>,
    /// Per module of a cycle run at load, in the order their run functions end:
    /// the line where its run marks it done, which `finish` makes the cycle's
    /// once every group is known.
    cycle_done_markers: Vec<(usize, String)>,
    module_exports: HashMap<usize, ModuleExports>,
    /// Module index -> the top-level function returning its namespace, for
    /// `import()` of a module the page inlines.
    module_namespaces: HashMap<usize, String>,
    /// Modules only an `import()` reaches, or only modules that are themselves
    /// lazy: they run on their first import, not at load.
    lazy_modules: HashSet<usize>,
    /// Module index -> the function that runs it once and records, on
    /// itself, whether it ran or what it threw: a lazy module's on its first
    /// import, one run at load's at its place and by each module importing
    /// it. A module run at load has one only when it has code to run, is in
    /// a cycle, or an `import()` of it comes first.
    module_runs: HashMap<usize, String>,
    /// `(module, export)` -> the name a module in a cycle uses for an export of
    /// one still being emitted, replaced by the export's own name once known.
    cycle_placeholders: HashMap<(usize, String), String>,
    diagnostics: Vec<Diagnostic>,
}

/// Answers only the queries the page cannot change. Everything downstream asks
/// the resolver, and an unresolved query already goes through the runtime, so
/// this one place decides.
struct StaticOnly<'a> {
    inner: &'a dyn DomResolver,
    live: &'a [DomQuery],
    /// Names the page's scripts give to something other than the element of
    /// that QML id: written into a script, the id would mean the page's thing.
    shadowed: HashSet<String>,
}

impl DomResolver for StaticOnly<'_> {
    fn text_has_generated_parts(&self) -> bool {
        self.inner.text_has_generated_parts()
    }

    fn resolve(&self, query: &DomQuery) -> Vec<QmlTarget> {
        if self.live.contains(query) {
            return Vec::new();
        }
        let targets = self.inner.resolve(query);
        if targets.iter().any(|t| self.shadowed.contains(&t.qml_id)) {
            return Vec::new();
        }
        targets
    }

    fn element(&self, key: ElementKey) -> Option<QmlTarget> {
        self.inner.element(key)
    }

    fn original(&self, query: &DomQuery) -> Vec<QmlTarget> {
        self.inner.original(query)
    }
}

pub(crate) fn rewrite(
    scripts: &[ScriptInput],
    analysis: &Analysis,
    resolver: &dyn DomResolver,
) -> JsOutput {
    let resolver = &StaticOnly {
        inner: resolver,
        // Adoption replaces the item an id names, so no lookup can stay a literal.
        live: if analysis.adopts_for_inline {
            &analysis.queries
        } else {
            &analysis.live_queries
        },
        shadowed: HashSet::new(),
    };
    let allocator = Allocator::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut decl_tracker = GlobalDeclTracker::new();
    let mut valid_scripts: Vec<ValidScript<'_>> = Vec::new();

    for (idx, input) in scripts.iter().enumerate() {
        let parsed = match parse_script(&allocator, input) {
            Ok(p) => p,
            Err(diags) => {
                diagnostics.extend(diags);
                continue;
            }
        };

        if let Err(diags) = decl_tracker.check_and_register(input, &parsed.program) {
            diagnostics.extend(diags);
            continue;
        }

        if input.kind.is_module() {
            collect_unresolvable_module_diagnostics(
                input,
                &parsed.program,
                scripts,
                &mut diagnostics,
            );
        }

        let unsupported =
            collect_unsupported_diagnostics(input, &parsed.program, &parsed.scoping, scripts);
        diagnostics.extend(unsupported);

        if let ScriptKind::HandlerAttribute { element, .. } = &input.kind
            && resolver.element(*element).is_none()
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Unsupported,
                source: input.name.clone(),
                line: Some(input.line_offset.max(1)),
                message: format!(
                    "handler attribute target ElementKey({}) could not be resolved",
                    element.0
                ),
            });
            continue;
        }

        valid_scripts.push(ValidScript { idx, input, parsed });
    }

    let valid_refs: Vec<(usize, &ScriptInput, &ParsedScript<'_>)> = valid_scripts
        .iter()
        .map(|v| (v.idx, v.input, &v.parsed))
        .collect();
    let cross_script = build_cross_script_query_map(&valid_refs);
    let resolver = &StaticOnly {
        inner: resolver,
        live: &[],
        shadowed: names_shadowing_ids(&valid_scripts, &cross_script, resolver),
    };

    let mut shared = SharedRewriteState {
        reads_attributes: analysis.reads_attributes,
        keeps_cascade: analysis.needs_cascade(),
        ..SharedRewriteState::default()
    };
    let mut claimed_top_names = HashSet::new();
    // Reserve `__eldur_init` and all user identifiers so generated helper names never collide.
    shared.used_names.insert("__eldur_init".to_string());
    claimed_top_names.insert("__eldur_init".to_string());
    claimed_top_names.insert("Eldur".to_string());
    for v in &valid_scripts {
        collect_program_declared_names(&v.parsed.program, &mut shared.used_names);
        for sym_id in v.parsed.scoping.symbol_ids() {
            shared
                .used_names
                .insert(v.parsed.scoping.symbol_name(sym_id).to_string());
        }
        if matches!(v.input.kind, ScriptKind::Classic) {
            collect_program_declared_names(&v.parsed.program, &mut claimed_top_names);
            for name in v.parsed.scoping.root_unresolved_references().keys() {
                claimed_top_names.insert(name.to_string());
            }
        }
    }
    // A classic script's own top-level binding stays that binding: page.js
    // declares it once for every script. So does a name QML's global object
    // has; a browser name it lacks reaches the window property or throws.
    let mut declared_globals: HashSet<&str> = QML_GLOBAL_NAMES.iter().copied().collect();
    for v in &valid_scripts {
        collect_window_property_writes(
            &v.parsed.program,
            &v.parsed.scoping,
            &mut shared.window_globals,
        );
        if matches!(v.input.kind, ScriptKind::Classic) {
            let scoping = &v.parsed.scoping;
            declared_globals.extend(
                scoping
                    .iter_bindings_in(scoping.root_scope_id())
                    .map(|sym| scoping.symbol_name(sym)),
            );
        }
    }
    shared
        .window_globals
        .retain(|name| !declared_globals.contains(name.as_str()));

    // Pre-scan `.on<event> = ...` assignments across all valid scripts to find which
    // `(qml_id, event)` pairs cannot be hoisted as a single static top-level binding.
    prescan_on_prop_assignments(
        &valid_scripts,
        &cross_script,
        resolver,
        analysis.dispatch_by_path,
        &mut shared,
    );

    let mut valid_map = HashMap::with_capacity(valid_scripts.len());
    for (pos, v) in valid_scripts.iter().enumerate() {
        valid_map.insert(v.idx, pos);
    }

    let mut builder = PageBuilder {
        scripts,
        valid_map,
        valid_scripts: &valid_scripts,
        cross_script: &cross_script,
        resolver,
        shared,
        out: PageBuffers::default(),
        claimed_top_names,
        emitted_modules: HashSet::new(),
        visiting_modules: HashSet::new(),
        visiting_stack: Vec::new(),
        cycle_groups: HashMap::new(),
        cycle_failure_markers: HashMap::new(),
        cycle_done_markers: Vec::new(),
        module_exports: HashMap::new(),
        module_namespaces: HashMap::new(),
        lazy_modules: lazy_modules(&valid_scripts, scripts),
        module_runs: HashMap::new(),
        cycle_placeholders: HashMap::new(),
        diagnostics,
    };

    for (pos, v) in valid_scripts.iter().enumerate() {
        match &v.input.kind {
            ScriptKind::Classic => builder.process_classic_or_module(pos, None),
            ScriptKind::Module => {
                builder.ensure_module_emitted(v.idx);
                builder.run_module_script(v.idx);
            }
            // A module the converter read runs when a module imports it, not
            // at a place on the page.
            ScriptKind::ImportedModule => {}
            ScriptKind::HandlerAttribute { element, event } => {
                builder.process_handler_attribute(v, *element, event);
            }
        }
    }
    // The lazy modules some `import()` names, and those their `import()`s name
    // in turn. One nothing names is not emitted: it would never run.
    loop {
        let mut wanted: Vec<usize> = builder
            .module_namespaces
            .keys()
            .copied()
            .filter(|idx| !builder.emitted_modules.contains(idx))
            .filter(|idx| builder.lazy_modules.contains(idx))
            .collect();
        if wanted.is_empty() {
            break;
        }
        wanted.sort_unstable();
        for idx in wanted {
            builder.ensure_module_emitted(idx);
        }
    }

    builder.finish()
}

/// The modules the converter read that run lazily: those no module script
/// reaches through static imports.
fn lazy_modules(valid_scripts: &[ValidScript<'_>], scripts: &[ScriptInput]) -> HashSet<usize> {
    let by_idx: HashMap<usize, &ValidScript<'_>> =
        valid_scripts.iter().map(|v| (v.idx, v)).collect();
    let mut eager: HashSet<usize> = HashSet::new();
    let mut work: Vec<usize> = valid_scripts
        .iter()
        .filter(|v| matches!(v.input.kind, ScriptKind::Module))
        .map(|v| v.idx)
        .collect();
    while let Some(idx) = work.pop() {
        if !eager.insert(idx) {
            continue;
        }
        let Some(v) = by_idx.get(&idx) else {
            continue;
        };
        for spec in static_module_specifiers(&v.parsed.program) {
            if let Some(dep) = resolve_module_specifier(v.input, spec, scripts) {
                work.push(dep);
            }
        }
    }
    valid_scripts
        .iter()
        .filter(|v| matches!(v.input.kind, ScriptKind::ImportedModule) && !eager.contains(&v.idx))
        .map(|v| v.idx)
        .collect()
}

fn static_module_specifiers<'p>(
    program: &'p oxc_ast::ast::Program<'_>,
) -> impl Iterator<Item = &'p str> {
    program.body.iter().filter_map(|stmt| match stmt {
        Statement::ImportDeclaration(imp) => Some(imp.source.value.as_str()),
        Statement::ExportAllDeclaration(exp) => Some(exp.source.value.as_str()),
        Statement::ExportFromDeclaration(exp) => Some(exp.source.value.as_str()),
        _ => None,
    })
}

impl<'a> PageBuilder<'a> {
    fn ensure_module_emitted(&mut self, script_idx: usize) {
        if self.emitted_modules.contains(&script_idx) || self.visiting_modules.contains(&script_idx)
        {
            return;
        }
        let Some(&pos) = self.valid_map.get(&script_idx) else {
            return;
        };
        self.visiting_modules.insert(script_idx);
        self.visiting_stack.push(script_idx);
        let mut exports = ModuleExports::default();
        if self.lazy_modules.contains(&script_idx) {
            // Its top-level statements become a function run on the first
            // import, once; an error it threw is thrown again to every later
            // import, as a browser's module map keeps it.
            let run = self.module_run(script_idx);
            let marker = self.unique_page_name(&format!("__eldur_cycle_failed_m{script_idx}_"));
            self.cycle_failure_markers
                .insert(script_idx, marker.clone());
            let outer = std::mem::take(&mut self.out.init_stmts);
            let outer_lazy = std::mem::replace(&mut self.out.in_lazy_module, true);
            let decls_from = self.out.top_decls.len();
            self.process_classic_or_module(pos, Some(&mut exports));
            self.out.in_lazy_module = outer_lazy;
            // Another module of its cycle, emitted above it, reads these
            // bindings: `var`, so QML does not take that for a use before
            // the declaration. Each name is the module's own.
            // One that is lexical holds the sentinel until its declaration
            // runs.
            let sentinel = self.shared.tdz.as_ref().map(|h| h.sentinel.clone());
            for decl in &mut self.out.top_decls[decls_from..] {
                let Some(list) = decl.strip_prefix("let ").and_then(|d| d.strip_suffix(';')) else {
                    continue;
                };
                let names: Vec<&str> = list.split(", ").collect();
                if !names.iter().all(|n| is_bare_binding_name(n)) {
                    continue;
                }
                let vars: Vec<String> = names
                    .iter()
                    .map(|n| match &sentinel {
                        Some(s) if self.shared.tdz_names.contains(*n) => format!("{n} = {s}"),
                        _ => (*n).to_string(),
                    })
                    .collect();
                *decl = format!("var {};", vars.join(", "));
            }
            let body = std::mem::replace(&mut self.out.init_stmts, outer);
            debug_assert_no_top_declaration(&body);
            let mut lines = vec![
                format!("if ({run}.state === 2) throw {run}.error;"),
                format!("if ({run}.state) return;"),
                format!("{run}.state = 1;"),
            ];
            if !body.is_empty() {
                lines.push(format!(
                    "try {{\n{}\n}} catch (err) {{\n    {run}.state = 2;\n    {run}.error = err;\n    {marker}\n    throw err;\n}}",
                    indent_block(&body.join("\n"), 4)
                ));
            }
            self.out.top_decls.push(format!(
                "function {run}() {{\n{}\n}}",
                indent_block(&lines.join("\n"), 4)
            ));
        } else {
            // What it throws stops it and every module importing it, not the
            // scripts after it, and the module is never run again: each later
            // importer and `import()` gets that error. One its importer never
            // reached runs at its own script or `import()`. Its statements are
            // collected in place, so a binding still counts as run at load
            // only when nothing runs ahead of it.
            let start = self.out.init_stmts.len();
            let outer_eager = std::mem::replace(&mut self.out.in_eager_module, true);
            self.process_classic_or_module(pos, Some(&mut exports));
            self.out.in_eager_module = outer_eager;
            let body = self.out.init_stmts.split_off(start);
            debug_assert_no_top_declaration(&body);
            if !body.is_empty()
                || self.module_runs.contains_key(&script_idx)
                || self.cycle_groups.contains_key(&script_idx)
            {
                let run = self.module_run(script_idx);
                let mut lines = vec![
                    format!("if ({run}.state === 2) throw {run}.error;"),
                    format!("if ({run}.state) return;"),
                ];
                if !body.is_empty() {
                    let marker =
                        self.unique_page_name(&format!("__eldur_cycle_failed_m{script_idx}_"));
                    self.cycle_failure_markers
                        .insert(script_idx, marker.clone());
                    // Running: a module of its cycle importing it goes on.
                    lines.push(format!("{run}.state = 3;"));
                    lines.push(format!(
                        "try {{\n{}\n}} catch (err) {{\n    {run}.state = 2;\n    {run}.error = err;\n    {marker}\n    throw err;\n}}",
                        indent_block(&body.join("\n"), 4)
                    ));
                }
                if self.cycle_groups.contains_key(&script_idx) {
                    let marker =
                        self.unique_page_name(&format!("__eldur_cycle_done_m{script_idx}_"));
                    self.cycle_done_markers.push((script_idx, marker.clone()));
                    lines.push(marker);
                } else {
                    lines.push(format!("{run}.state = 1;"));
                }
                self.out.module_fns.push(format!(
                    "function {run}() {{\n{}\n}}",
                    indent_block(&lines.join("\n"), 4)
                ));
            }
        }
        self.visiting_stack.pop();
        self.visiting_modules.remove(&script_idx);
        self.emitted_modules.insert(script_idx);
        self.module_exports.insert(script_idx, exports);
        // Imported lazily by a script before it: the name was handed out then.
        self.declare_module_namespace(script_idx);
    }

    /// What `stmt` takes from `dep`, a module further up a cycle, under
    /// placeholder names. A namespace of such a module is not supported.
    fn add_cycle_exports(
        &mut self,
        importer: &ScriptInput,
        stmt: &Statement<'_>,
        dep: usize,
        cycle_exports: &mut HashMap<usize, ModuleExports>,
    ) {
        let mut names: Vec<String> = Vec::new();
        let mut namespace_span = None;
        match stmt {
            Statement::ImportDeclaration(imp) => {
                for spec in imp.specifiers.iter().flatten() {
                    match spec {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => {
                            names.push(named.imported.name().to_string());
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {
                            names.push("default".to_string());
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                            namespace_span = Some(ns.span);
                        }
                    }
                }
            }
            Statement::ExportFromDeclaration(exp) => {
                names.extend(exp.specifiers.iter().map(|s| s.local.name().to_string()));
            }
            Statement::ExportAllDeclaration(exp) if exp.exported.is_some() => {
                namespace_span = Some(exp.span);
            }
            _ => {}
        }
        if let Some(span) = namespace_span {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Unsupported,
                source: importer.name.clone(),
                line: Some(crate::parse::line_for_span(importer, span)),
                message: "the namespace of a module importing this one is not supported: it is empty here".to_string(),
            });
        }
        let lazy = self.lazy_modules.contains(&dep);
        for name in names {
            let placeholder = self.cycle_placeholder(dep, &name);
            if lazy {
                self.shared.add_tdz_name(&placeholder);
            }
            cycle_exports
                .entry(dep)
                .or_default()
                .insert(name, placeholder, true, true);
        }
    }

    /// A module script runs its module, unless an importer already has, and
    /// reports what its graph threw, as a browser does for each such script;
    /// the page goes on.
    fn run_module_script(&mut self, script_idx: usize) {
        if self.lazy_modules.contains(&script_idx) {
            return;
        }
        if let Some(run) = self.module_runs.get(&script_idx) {
            self.out.init_stmts.push(format!(
                "try {{\n    {run}();\n}} catch (err) {{\n    Eldur.reportError(err);\n}}"
            ));
        }
    }

    fn module_run(&mut self, script_idx: usize) -> String {
        if let Some(run) = self.module_runs.get(&script_idx) {
            return run.clone();
        }
        let run = self
            .shared
            .alloc_fn_name(&format!("__eldur_run_m{script_idx}"));
        self.module_runs.insert(script_idx, run.clone());
        run
    }

    /// The name a module uses for `name`, exported by `dep`, which is being
    /// emitted further up a cycle and has no export map yet.
    fn cycle_placeholder(&mut self, dep: usize, name: &str) -> String {
        let key = (dep, name.to_string());
        if let Some(found) = self.cycle_placeholders.get(&key) {
            return found.clone();
        }
        let n = self.cycle_placeholders.len();
        let placeholder = self.unique_page_name(&format!("__eldur_cycle{n}_"));
        self.cycle_placeholders.insert(key, placeholder.clone());
        placeholder
    }

    /// A name `finish` can replace throughout page.js: no other name has it,
    /// and no script's text holds it, inside a string or a comment either.
    fn unique_page_name(&mut self, base: &str) -> String {
        let mut n = 0usize;
        loop {
            let candidate = if n == 0 {
                base.to_string()
            } else {
                format!("{base}{n}_")
            };
            n += 1;
            if self.scripts.iter().any(|s| s.source.contains(&candidate)) {
                continue;
            }
            if self.shared.used_names.insert(candidate.clone()) {
                return candidate;
            }
        }
    }

    /// `dep`, a module being emitted, is imported by the module emitted last:
    /// every module from `dep` on is one cycle.
    fn join_cycle(&mut self, dep: usize) {
        let Some(at) = self.visiting_stack.iter().position(|m| *m == dep) else {
            return;
        };
        let members: Vec<usize> = self.visiting_stack[at..].to_vec();
        let merged: HashSet<usize> = members
            .iter()
            .filter_map(|m| self.cycle_groups.get(m).copied())
            .collect();
        let group = merged.iter().copied().min().unwrap_or(dep);
        for g in self.cycle_groups.values_mut() {
            if merged.contains(g) {
                *g = group;
            }
        }
        for m in members {
            self.cycle_groups.insert(m, group);
        }
    }

    /// `dep`, already emitted, is imported by the module emitted last. When
    /// `dep` is of a cycle a module of which is still being emitted, the
    /// importer waits on that module too, so it is of the cycle, from the
    /// module still being emitted on: a browser fails it with the cycle. A
    /// `dep` that finished outside any running cycle stays apart.
    fn join_running_cycle(&mut self, dep: usize) {
        let Some(&group) = self.cycle_groups.get(&dep) else {
            return;
        };
        let running = self
            .visiting_stack
            .iter()
            .copied()
            .find(|m| self.cycle_groups.get(m) == Some(&group));
        if let Some(m) = running {
            self.join_cycle(m);
        }
    }

    /// The page modules `v` imports with `import("literal")`, mapped from
    /// specifier to the expression the call becomes. The function that returns
    /// the module's namespace is declared once the module is emitted. The module is
    /// not emitted here: `import()` never runs a module ahead of its importer,
    /// so each one stays at its own place on the page and its namespace
    /// function is declared once it has been emitted there.
    fn dynamic_import_namespaces(&mut self, v: &ValidScript<'a>) -> HashMap<String, String> {
        let mut found = DynamicImports {
            importer: v.input,
            scripts: self.scripts,
            found: Vec::new(),
        };
        found.visit_program(&v.parsed.program);
        let mut namespaces = HashMap::new();
        for (spec, dep_idx, span) in found.found {
            if namespaces.contains_key(&spec) {
                continue;
            }
            if !self.valid_map.contains_key(&dep_idx) {
                // Not converted at all, which its own diagnostics say.
                self.diagnostics.push(Diagnostic {
                    severity: Severity::Unsupported,
                    source: v.input.name.clone(),
                    line: Some(crate::parse::line_for_span(v.input, span)),
                    message: format!(
                        "dynamic import('{spec}') of a module that could not be converted cannot load it: the promise rejects as for a missing module"
                    ),
                });
                continue;
            }
            let name = match self.module_namespaces.get(&dep_idx) {
                Some(name) => name.clone(),
                None => {
                    let name = self.shared.alloc_fn_name("__eldur_module_namespace");
                    self.module_namespaces.insert(dep_idx, name.clone());
                    if self.emitted_modules.contains(&dep_idx) {
                        self.declare_module_namespace(dep_idx);
                    }
                    name
                }
            };
            // A lazy module loads after the code that imported it has run, and
            // its microtasks; one already loaded resolves to its namespace.
            // A module run at load resolves at once when it has run; one
            // still to come gets a run function, which the promise runs if
            // nothing else has by then.
            let call = if self.lazy_modules.contains(&dep_idx) {
                format!("Eldur.importModule({name})")
            } else if self.emitted_modules.contains(&dep_idx)
                && !self.module_runs.contains_key(&dep_idx)
            {
                format!("Eldur.importEager(null, {name})")
            } else {
                let run = self.module_run(dep_idx);
                format!("Eldur.importEager({run}, {name})")
            };
            namespaces.insert(spec, call);
        }
        namespaces
    }

    /// Declares the function an `import()` of `script_idx` calls. A function
    /// declaration is hoisted and its getters read the bindings only when
    /// called, so where it lands in page.js does not matter.
    fn declare_module_namespace(&mut self, script_idx: usize) {
        let (Some(name), Some(exports)) = (
            self.module_namespaces.get(&script_idx),
            self.module_exports.get(&script_idx),
        ) else {
            return;
        };
        let run = self
            .module_runs
            .get(&script_idx)
            .map(|run| format!("{run}(); "))
            .unwrap_or_default();
        self.out.top_decls.push(format!(
            "function {name}() {{ {run}return {name}.value || ({name}.value = {}); }}",
            // Made once the module has run to its end: every binding is
            // initialized. Its run comes first each time, so a module that
            // failed, even after this was made, throws what it threw.
            format_live_namespace_object(exports, None)
        ));
    }

    fn process_handler_attribute(
        &mut self,
        v: &ValidScript<'a>,
        element: crate::ElementKey,
        event: &str,
    ) {
        let Some(target) = self.resolver.element(element) else {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Unsupported,
                source: v.input.name.clone(),
                line: Some(v.input.line_offset.max(1)),
                message: format!(
                    "handler attribute target ElementKey({}) could not be resolved",
                    element.0
                ),
            });
            return;
        };

        let dom_event = normalize_handler_event(event);
        let safe_ev = sanitize_ident(dom_event);
        let fn_name = self
            .shared
            .alloc_fn_name(&format!("__eldur_on_{safe_ev}_{}", target.qml_id));

        let dynamic_imports = self.dynamic_import_namespaces(v);
        let scoping = &v.parsed.scoping;
        let bindings =
            ScriptQueryBindings::build(v.input, &v.parsed.program, scoping, self.cross_script);
        let final_span = v.parsed.program.body.last().map(GetSpan::span);
        let mut lowerer = AstLowerer::new_handler(
            &v.input.source,
            scoping,
            &bindings,
            self.resolver,
            &mut self.shared,
            target.clone(),
            final_span,
        );
        lowerer.dynamic_imports = dynamic_imports;
        lowerer.visit_program(&v.parsed.program);
        let body_rewritten = lowerer.editor.finish();
        let trimmed = body_rewritten.trim();

        let fn_code = if trimmed.is_empty() {
            format!("function {fn_name}(event) {{}}")
        } else {
            format!(
                "function {fn_name}(event) {{\n{}\n}}",
                indent_block(trimmed, 4)
            )
        };
        self.out.handler_fns.push(fn_code);
        self.shared.bindings.push(EventBinding {
            target,
            event: dom_event.to_string(),
            function: fn_name,
        });
    }

    fn process_classic_or_module(
        &mut self,
        valid_pos: usize,
        mut mod_exports: Option<&mut ModuleExports>,
    ) {
        let v = &self.valid_scripts[valid_pos];
        let is_classic = matches!(v.input.kind, ScriptKind::Classic);
        let scoping = &v.parsed.scoping;
        let mut bindings =
            ScriptQueryBindings::build(v.input, &v.parsed.program, scoping, self.cross_script);
        if !is_classic && self.lazy_modules.contains(&v.idx) {
            // Its top-level element lookups run when it does, and a read
            // before then throws: each is a variable, not the element.
            bindings.forget(scoping.iter_bindings_in(scoping.root_scope_id()));
        }

        let mut cycle_exports: HashMap<usize, ModuleExports> = HashMap::new();
        let mut unresolved_static = false;
        // Each module it imports is run once, however many imports name it.
        let mut called: HashSet<usize> = HashSet::new();
        // Pre-inline any imported local modules in source order before lowering statements.
        if !is_classic {
            for stmt in &v.parsed.program.body {
                let spec_opt = match stmt {
                    Statement::ImportDeclaration(imp) => Some(imp.source.value.as_str()),
                    Statement::ExportAllDeclaration(exp) => Some(exp.source.value.as_str()),
                    Statement::ExportFromDeclaration(exp) => Some(exp.source.value.as_str()),
                    _ => None,
                };
                let Some(spec) = spec_opt else {
                    continue;
                };
                let Some(dep_idx) = resolve_module_specifier(v.input, spec, self.scripts) else {
                    unresolved_static = true;
                    continue;
                };
                {
                    if self.visiting_modules.contains(&dep_idx) {
                        // Further up a cycle: it runs after this module, and
                        // this one reads its bindings by placeholder names.
                        if let Statement::ExportAllDeclaration(exp) = stmt
                            && exp.exported.is_none()
                        {
                            self.diagnostics.push(Diagnostic {
                                severity: Severity::Unsupported,
                                source: v.input.name.clone(),
                                line: Some(crate::parse::line_for_span(v.input, exp.span)),
                                message: format!(
                                    "export * from '{spec}', a module importing this one, is not supported: its names are not re-exported"
                                ),
                            });
                        }
                        self.add_cycle_exports(v.input, stmt, dep_idx, &mut cycle_exports);
                        self.join_cycle(dep_idx);
                        // Run on its own, as the target of an `import()` or
                        // its own script, this module still runs the far
                        // side of the cycle first.
                        let run = self.module_run(dep_idx);
                        if called.insert(dep_idx) {
                            self.out.init_stmts.push(format!("{run}();"));
                        }
                    } else {
                        self.ensure_module_emitted(dep_idx);
                        self.join_running_cycle(dep_idx);
                        if let Some(run) = self.module_runs.get(&dep_idx)
                            && called.insert(dep_idx)
                        {
                            self.out.init_stmts.push(format!("{run}();"));
                        }
                    }
                }
            }
        }
        // A module of its graph could not be fetched, so none of it runs and
        // `import()` of it rejects, as a browser's does. An eager module has
        // said so in a diagnostic.
        if unresolved_static && self.lazy_modules.contains(&v.idx) {
            self.out.init_stmts.insert(
                0,
                format!(
                    "throw new TypeError(\"Failed to fetch dynamically imported module: \" + {:?});",
                    v.input.name
                ),
            );
        }
        // Only while this module is emitted; its own map comes when it is done.
        for (dep, exports) in &cycle_exports {
            self.module_exports.insert(*dep, exports.clone());
        }

        let dynamic_imports = self.dynamic_import_namespaces(v);
        // A lazy module runs after the page has loaded: a listener it adds for
        // `DOMContentLoaded` or `load` never hears it, as in a browser.
        let is_lazy = self.lazy_modules.contains(&v.idx);

        let scripts = self.scripts;
        let resolver = self.resolver;
        let module_exports = &self.module_exports;
        let out = &mut self.out;

        let (symbol_renames, elided_dom_symbols) = if is_classic {
            (HashMap::new(), HashMap::new())
        } else {
            prepare_module_symbol_renames(
                v,
                scoping,
                &bindings,
                resolver,
                scripts,
                module_exports,
                &mut self.claimed_top_names,
                &mut self.shared,
            )
        };

        let mut lowerer = AstLowerer::new(
            &v.input.source,
            scoping,
            &bindings,
            resolver,
            &mut self.shared,
        );
        if is_lazy && !is_classic {
            lowerer.tdz =
                lazy_module_tdz_bindings(v, &bindings, resolver, &symbol_renames, lowerer.shared);
        }
        lowerer.symbol_renames = symbol_renames;
        lowerer.dynamic_imports = dynamic_imports;
        lowerer.after_load = is_lazy;

        // An import's bindings exist before any of its module runs. A lazy
        // module copies some of them in its run function, so those copies
        // come first there; at load they are declared with their values.
        let body: Vec<&Statement<'_>> = if is_lazy {
            let (imports, rest): (Vec<_>, Vec<_>) = v
                .parsed
                .program
                .body
                .iter()
                .partition(|s| matches!(s, Statement::ImportDeclaration(_)));
            imports.into_iter().chain(rest).collect()
        } else {
            v.parsed.program.body.iter().collect()
        };
        for stmt in body {
            match stmt {
                Statement::EmptyStatement(_) => {}

                Statement::FunctionDeclaration(func) => {
                    lowerer.visit_function(func, ScopeFlags::Function);
                    let fn_str = lowerer
                        .editor
                        .extract_and_consume(func.span)
                        .trim()
                        .to_string();
                    if let Some(id) = &func.id {
                        let emitted_name = lowerer.bound_name(id).to_string();
                        out.emit_top_function_decl(is_classic, &emitted_name, fn_str);
                    } else {
                        out.top_decls.push(fn_str);
                    }
                }

                Statement::ClassDeclaration(cls) => {
                    out.emit_class_decl(
                        cls,
                        &mut lowerer,
                        scoping,
                        mod_exports.as_deref_mut(),
                        false,
                    );
                }

                Statement::VariableDeclaration(vd) => {
                    out.emit_top_var_decl(
                        vd,
                        is_classic,
                        &mut lowerer,
                        scoping,
                        &bindings,
                        resolver,
                        None,
                    );
                }

                Statement::ImportDeclaration(imp) => {
                    let spec = imp.source.value.as_str();
                    if let Some(dep_idx) = resolve_module_specifier(v.input, spec, scripts)
                        && let Some(dep_exports) = module_exports.get(&dep_idx)
                    {
                        out.emit_import_bindings(
                            dep_exports,
                            imp.specifiers.as_ref().map(|vec| vec.as_slice()),
                            &lowerer,
                        );
                    }
                }

                Statement::ExportDeclaration(exp) => match &exp.declaration {
                    Declaration::FunctionDeclaration(func) => {
                        lowerer.visit_function(func, ScopeFlags::Function);
                        let fn_str = lowerer
                            .editor
                            .extract_and_consume(func.span)
                            .trim()
                            .to_string();
                        if let Some(id) = &func.id {
                            let orig_name = id.name.to_string();
                            let emitted_name = lowerer.bound_name(id).to_string();
                            let is_mutated = id
                                .symbol_id
                                .get()
                                .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                            out.emit_top_function_decl(false, &emitted_name, fn_str);
                            if let Some(me) = mod_exports.as_deref_mut() {
                                me.insert(orig_name, emitted_name, false, is_mutated);
                            }
                        } else {
                            out.top_decls.push(fn_str);
                        }
                    }
                    Declaration::ClassDeclaration(cls) => {
                        out.emit_class_decl(
                            cls,
                            &mut lowerer,
                            scoping,
                            mod_exports.as_deref_mut(),
                            true,
                        );
                    }
                    Declaration::VariableDeclaration(vd) => {
                        out.emit_top_var_decl(
                            vd,
                            false,
                            &mut lowerer,
                            scoping,
                            &bindings,
                            resolver,
                            mod_exports.as_deref_mut(),
                        );
                    }
                    _ => {}
                },

                Statement::ExportNamedDeclaration(exp) => {
                    if let Some(me) = mod_exports.as_deref_mut() {
                        for spec in &exp.specifiers {
                            let local_orig = spec.local.name().as_str();
                            let exported = spec.exported.name().to_string();
                            let sym_id =
                                scoping.get_binding(scoping.root_scope_id(), local_orig.into());
                            if let Some(sid) = sym_id
                                && let Some(qml_id) = elided_dom_symbols.get(&sid)
                            {
                                me.insert(exported, qml_id.clone(), false, false);
                                continue;
                            }
                            let emitted_local = sym_id
                                .and_then(|sid| lowerer.symbol_renames.get(&sid))
                                .cloned()
                                .unwrap_or_else(|| local_orig.to_string());
                            let is_mutated =
                                sym_id.is_some_and(|sid| scoping.symbol_is_mutated(sid));
                            let deferred = !out.pure_top_vars.contains_key(&emitted_local);
                            me.insert(exported, emitted_local, deferred, is_mutated);
                        }
                    }
                }

                Statement::ExportFromDeclaration(exp) => {
                    if let Some(dep_idx) =
                        resolve_module_specifier(v.input, exp.source.value.as_str(), scripts)
                        && let Some(dep_exports) = module_exports.get(&dep_idx)
                        && let Some(me) = mod_exports.as_deref_mut()
                    {
                        for spec in &exp.specifiers {
                            let imported = spec.local.name().as_str();
                            let exported = spec.exported.name().to_string();
                            if let Some((target_ident, deferred, is_mutable)) =
                                dep_exports.entries.get(imported)
                            {
                                me.insert(exported, target_ident.clone(), *deferred, *is_mutable);
                            }
                        }
                    }
                }

                Statement::ExportAllDeclaration(exp) => {
                    if let Some(dep_idx) =
                        resolve_module_specifier(v.input, exp.source.value.as_str(), scripts)
                        && let Some(dep_exports) = module_exports.get(&dep_idx)
                    {
                        if let Some(alias_name) = &exp.exported {
                            let ns_ident = lowerer
                                .shared
                                .alloc_fn_name(&format!("__eldur_ns_{}", v.idx));
                            // Getters, so the namespace reads the live bindings.
                            let obj_lit =
                                format_live_namespace_object(dep_exports, Some(&*lowerer.shared));
                            out.top_decls.push(format!("const {ns_ident} = {obj_lit};"));
                            if let Some(me) = mod_exports.as_deref_mut() {
                                me.insert(alias_name.name().to_string(), ns_ident, false, false);
                            }
                        } else if let Some(me) = mod_exports.as_deref_mut() {
                            for key in &dep_exports.ordered_keys {
                                if key != "default"
                                    && let Some((target_ident, deferred, is_mutable)) =
                                        dep_exports.entries.get(key)
                                {
                                    me.insert(
                                        key.clone(),
                                        target_ident.clone(),
                                        *deferred,
                                        *is_mutable,
                                    );
                                }
                            }
                        }
                    }
                }

                Statement::ExportDefaultDeclaration(exp) => {
                    out.emit_export_default(
                        v.idx,
                        &exp.declaration,
                        &mut lowerer,
                        scoping,
                        mod_exports.as_deref_mut(),
                    );
                }

                Statement::ExpressionStatement(es) => {
                    if !is_lazy
                        && out.try_handle_special_top_expr(&es.expression, &mut lowerer, scoping)
                    {
                        continue;
                    }
                    lowerer.visit_statement(stmt);
                    let rendered = lowerer.editor.extract_and_consume(stmt.span());
                    let trimmed = rendered.trim();
                    if !trimmed.is_empty() {
                        out.init_stmts.push(ensure_trailing_semicolon(trimmed));
                    }
                }

                _ => {
                    lowerer.visit_statement(stmt);
                    for name in lowerer.hoisted_top_vars.drain(..) {
                        out.pure_top_vars.remove(&name);
                        if !is_classic || !out.var_func_slots.contains_key(&name) {
                            let slot_idx = out.top_decls.len();
                            out.top_decls.push(format!("var {name};"));
                            if is_classic {
                                out.var_func_slots
                                    .insert(name, (slot_idx, out.init_stmts.len()));
                            }
                        }
                    }
                    let rendered = lowerer.editor.extract_and_consume(stmt.span());
                    let trimmed = rendered.trim();
                    if !trimmed.is_empty() {
                        out.init_stmts.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    fn finish(self) -> JsOutput {
        let mut sections: Vec<String> = Vec::new();
        sections.push(".import \"eldur_runtime.js\" as Eldur".to_string());

        if !self.out.top_decls.is_empty() {
            sections.push(self.out.top_decls.join("\n"));
        }
        if !self.shared.generated_decls.is_empty() {
            sections.push(self.shared.generated_decls.join("\n"));
        }
        if !self.out.handler_fns.is_empty() {
            sections.push(self.out.handler_fns.join("\n\n"));
        }
        if !self.out.module_fns.is_empty() {
            sections.push(self.out.module_fns.join("\n\n"));
        }

        debug_assert_no_top_declaration(&self.out.init_stmts);
        let mut all_init_lines: Vec<String> = Vec::new();
        all_init_lines.extend(self.out.init_stmts);
        all_init_lines.extend(self.out.dcl_stmts);
        for entry in self.out.load_entries {
            match entry {
                LoadEntry::Listener(code) => all_init_lines.push(code),
                LoadEntry::WindowOnload => {
                    if let Some(code) = &self.out.window_onload_value {
                        all_init_lines.push(code.clone());
                    }
                }
            }
        }

        let init_function = if all_init_lines.is_empty() {
            None
        } else {
            let mut body_parts = Vec::with_capacity(all_init_lines.len() + 1);
            body_parts.push(
                "    Eldur.init(rootItem || (typeof root !== \"undefined\" ? root : null));"
                    .to_string(),
            );
            for stmt in &all_init_lines {
                body_parts.push(indent_block(stmt, 4));
            }
            sections.push(format!(
                "function __eldur_init(rootItem) {{\n{}\n}}",
                body_parts.join("\n")
            ));
            Some("__eldur_init".to_string())
        };

        let mut page_js = sections.join("\n\n");
        page_js.push('\n');
        let mut diagnostics = self.diagnostics;
        let mut placeholders: Vec<(&(usize, String), &String)> =
            self.cycle_placeholders.iter().collect();
        placeholders.sort_by(|a, b| a.1.cmp(b.1));
        for ((dep, name), placeholder) in placeholders {
            let ident =
                cycle_export_ident(&self.module_exports, &self.cycle_placeholders, *dep, name);
            let ident = ident.unwrap_or_else(|| {
                let source = self.scripts[*dep].name.clone();
                diagnostics.push(Diagnostic {
                    severity: Severity::Unsupported,
                    source,
                    line: None,
                    message: format!("a module in a cycle imports '{name}', which this module does not export: it reads undefined"),
                });
                "undefined".to_string()
            });
            page_js = page_js.replace(placeholder.as_str(), &ident);
        }

        // A cycle shares one result, settled when its root, the module whose
        // run ends last, ends: each other module that ran to its end waits in
        // state 4, which runs nothing more but settles no `import()` yet.
        for (i, (idx, marker)) in self.cycle_done_markers.iter().enumerate() {
            let group = self.cycle_groups.get(idx);
            let run = &self.module_runs[idx];
            let ends_later = |m: &usize| self.cycle_groups.get(m) == group;
            let line = if self.cycle_done_markers[i + 1..]
                .iter()
                .any(|(m, _)| ends_later(m))
            {
                format!("{run}.state = 4;")
            } else {
                std::iter::once(run)
                    .chain(
                        self.cycle_done_markers[..i]
                            .iter()
                            .filter(|(m, _)| ends_later(m))
                            .map(|(m, _)| &self.module_runs[m]),
                    )
                    .map(|r| format!("{r}.state = 1;"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            page_js = page_js.replace(marker.as_str(), &line);
        }

        for (idx, marker) in &self.cycle_failure_markers {
            let others: Vec<String> = match self.cycle_groups.get(idx) {
                Some(group) => {
                    let mut members: Vec<usize> = self
                        .cycle_groups
                        .iter()
                        .filter(|(m, g)| *g == group && *m != idx)
                        .map(|(m, _)| *m)
                        .collect();
                    members.sort_unstable();
                    members
                        .into_iter()
                        .filter_map(|m| self.module_runs.get(&m))
                        .map(|run| format!("{run}.state = 2; {run}.error = err;"))
                        .collect()
                }
                None => Vec::new(),
            };
            if others.is_empty() {
                // Not in a cycle: the marker's line goes.
                page_js = page_js
                    .lines()
                    .filter(|line| line.trim() != marker)
                    .map(|line| format!("{line}\n"))
                    .collect();
            } else {
                page_js = page_js.replace(marker.as_str(), &others.join(" "));
            }
        }

        JsOutput {
            redispatching_slots: self.shared.redispatching_slots,
            page_js,
            init_function,
            bindings: self.shared.bindings,
            diagnostics,
        }
    }
}

/// The name `dep`'s export `name` has in page.js, following re-exports that
/// were themselves placeholders.
fn cycle_export_ident(
    module_exports: &HashMap<usize, ModuleExports>,
    placeholders: &HashMap<(usize, String), String>,
    dep: usize,
    name: &str,
) -> Option<String> {
    let mut ident = module_exports.get(&dep)?.entries.get(name)?.0.clone();
    for _ in 0..placeholders.len() {
        let Some((key, _)) = placeholders.iter().find(|(_, p)| **p == ident) else {
            return Some(ident);
        };
        ident = module_exports.get(&key.0)?.entries.get(&key.1)?.0.clone();
    }
    None
}

impl PageBuffers {
    /// Whether a statement emitted now runs at load with nothing before it,
    /// so a pure binding can be declared with its value.
    fn at_load(&self) -> bool {
        !self.in_lazy_module && self.init_stmts.is_empty()
    }

    fn emit_top_function_decl(&mut self, is_classic: bool, name: &str, fn_code: String) {
        if is_classic && let Some(&(slot_idx, prev_init_len)) = self.var_func_slots.get(name) {
            if self.init_stmts.len() == prev_init_len {
                self.top_decls[slot_idx] = fn_code;
                return;
            }
            // Intervening top-level statements ran before this script redefined `function name`,
            // so convert the original slot to a mutable `var` and update `name` at this point
            // in `__eldur_init()` to preserve browser script-by-script execution order.
            if self.top_decls[slot_idx].starts_with("function ") {
                self.top_decls[slot_idx] = format!("var {name} = {};", self.top_decls[slot_idx]);
            }
            self.init_stmts.push(format!("{name} = {fn_code};"));
            return;
        }
        let slot_idx = self.top_decls.len();
        self.top_decls.push(fn_code);
        self.pure_top_vars
            .insert(name.to_string(), PureKind::Function);
        if is_classic {
            self.var_func_slots
                .insert(name.to_string(), (slot_idx, self.init_stmts.len()));
        }
    }

    fn emit_class_decl<'b>(
        &mut self,
        cls: &'b oxc_ast::ast::Class<'b>,
        lowerer: &mut AstLowerer<'b, '_>,
        scoping: &'b Scoping,
        mod_exports: Option<&mut ModuleExports>,
        is_exported: bool,
    ) {
        let all_static_pure = cls.body.body.iter().all(|el| match el {
            oxc_ast::ast::ClassElement::PropertyDefinition(p) if p.r#static => {
                p.value.as_ref().is_none_or(|val| {
                    is_pure_top_level_init(
                        val,
                        scoping,
                        &self.pure_top_vars,
                        &lowerer.symbol_renames,
                    )
                })
            }
            _ => true,
        });

        lowerer.visit_class(cls);
        let cls_str = lowerer
            .editor
            .extract_and_consume(cls.span)
            .trim()
            .to_string();
        let deferred = self.in_lazy_module && cls.id.is_some();
        if deferred && let Some(id) = &cls.id {
            let emitted_name = lowerer.bound_name(id).to_string();
            self.top_decls.push(format!("let {emitted_name};"));
            self.init_stmts.push(format!("{emitted_name} = {cls_str};"));
        } else {
            self.top_decls.push(cls_str);
        }

        if let Some(static_stmts) = lowerer
            .post_class_static_inits
            .remove(&(cls.span.start, cls.span.end))
        {
            if all_static_pure && self.at_load() {
                self.top_decls.extend(static_stmts);
            } else {
                self.init_stmts.extend(static_stmts);
            }
        }

        if let Some(id) = &cls.id {
            let emitted_name = lowerer.bound_name(id).to_string();
            if all_static_pure && self.at_load() {
                self.pure_top_vars
                    .insert(emitted_name.clone(), PureKind::Function);
            }
            if is_exported && let Some(me) = mod_exports {
                let orig_name = id.name.to_string();
                let is_mutated = id
                    .symbol_id
                    .get()
                    .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                me.insert(orig_name, emitted_name, deferred, is_mutated);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_top_var_decl<'b>(
        &mut self,
        vd: &'b VariableDeclaration<'b>,
        is_classic: bool,
        lowerer: &mut AstLowerer<'b, '_>,
        scoping: &'b Scoping,
        bindings: &ScriptQueryBindings<'b>,
        resolver: &dyn DomResolver,
        mut mod_exports: Option<&mut ModuleExports>,
    ) {
        let is_mutable_kind = vd.kind != VariableDeclarationKind::Const;
        for decl in &vd.declarations {
            // Check if this declarator is a single-target statically resolved DOM binding.
            if is_elidable_dom_declarator(decl, bindings, resolver) {
                if let BindingPattern::BindingIdentifier(bind_id) = &decl.id
                    && let Some(sym_id) = bind_id.symbol_id.get()
                    && let Some(query) = bindings.symbol_queries.get(&sym_id)
                    && let Some(target) =
                        crate::lower::resolve_query_to_single_target(query, resolver)
                    && let Some(me) = mod_exports.as_deref_mut()
                {
                    me.insert(bind_id.name.to_string(), target.qml_id, false, false);
                }
                continue;
            }

            let mut bound_idents = Vec::new();
            collect_binding_identifiers(&decl.id, &mut bound_idents);

            let Some(init) = &decl.init else {
                // Uninitialized declaration (`let x;` / `var x;`).
                if vd.kind == VariableDeclarationKind::Var {
                    for ident in &bound_idents {
                        let orig_name = ident.name.to_string();
                        let emitted_name = lowerer.bound_name(ident).to_string();
                        if !self.var_func_slots.contains_key(&emitted_name) {
                            let slot_idx = self.top_decls.len();
                            self.top_decls.push(format!("var {emitted_name};"));
                            self.var_func_slots
                                .insert(emitted_name.clone(), (slot_idx, self.init_stmts.len()));
                        }
                        if let Some(me) = mod_exports.as_deref_mut() {
                            me.insert(orig_name, emitted_name, true, true);
                        }
                    }
                } else {
                    lowerer.visit_binding_pattern(&decl.id);
                    let pat_str = lowerer.editor.extract_and_consume(decl.id.span());
                    self.top_decls.push(format!("let {pat_str};"));
                    if !self.at_load() {
                        for ident in &bound_idents {
                            let emitted_name = lowerer.bound_name(ident).to_string();
                            self.pure_top_vars.remove(&emitted_name);
                            self.init_stmts.push(format!("{emitted_name} = undefined;"));
                        }
                    }
                    if let Some(me) = mod_exports.as_deref_mut() {
                        for ident in &bound_idents {
                            let orig_name = ident.name.to_string();
                            let emitted_name = lowerer.bound_name(ident).to_string();
                            me.insert(orig_name, emitted_name, true, true);
                        }
                    }
                }
                continue;
            };

            let has_obj_rest =
                matches!(&decl.id, BindingPattern::ObjectPattern(obj) if obj.rest.is_some());
            let is_destructured_var = vd.kind == VariableDeclarationKind::Var
                && !matches!(&decl.id, BindingPattern::BindingIdentifier(_));
            // A lazy module's bindings get their values when it runs. One whose
            // value could throw while it is worked out is worked out where the
            // module or script runs, so the throw stops that module only.
            let init_kind =
                pure_init_kind(init, scoping, &self.pure_top_vars, &lowerer.symbol_renames);
            let is_pure = !self.in_lazy_module
                && !has_obj_rest
                && !is_destructured_var
                && pattern_defaults_are_pure(
                    &decl.id,
                    scoping,
                    &self.pure_top_vars,
                    &lowerer.symbol_renames,
                )
                && init_kind.is_some_and(|k| pattern_cannot_throw(&decl.id, k));

            if is_pure {
                let init_kind = init_kind.unwrap_or(PureKind::Unknown);
                // A destructured name's value is not followed.
                let bound_kind = if matches!(&decl.id, BindingPattern::BindingIdentifier(_)) {
                    init_kind
                } else {
                    PureKind::Unknown
                };
                lowerer.visit_binding_pattern(&decl.id);
                let pat_str = lowerer.editor.extract_and_consume(decl.id.span());
                lowerer.visit_expression(init);
                let init_str = lowerer.editor.extract_and_consume(init.span());

                if vd.kind == VariableDeclarationKind::Var
                    && let BindingPattern::BindingIdentifier(bind_id) = &decl.id
                {
                    let orig_name = bind_id.name.to_string();
                    let emitted_name = lowerer.bound_name(bind_id).to_string();
                    if let Some(&(slot_idx, prev_init_len)) = self.var_func_slots.get(&emitted_name)
                    {
                        if self.init_stmts.len() == prev_init_len {
                            self.top_decls[slot_idx] = format!("var {emitted_name} = {init_str};");
                        } else {
                            self.pure_top_vars.remove(&emitted_name);
                            self.init_stmts
                                .push(format!("{emitted_name} = {init_str};"));
                        }
                    } else {
                        let slot_idx = self.top_decls.len();
                        self.top_decls
                            .push(format!("var {emitted_name} = {init_str};"));
                        if self.at_load() {
                            self.pure_top_vars.insert(emitted_name.clone(), bound_kind);
                        }
                        self.var_func_slots
                            .insert(emitted_name.clone(), (slot_idx, self.init_stmts.len()));
                    }
                    if let Some(me) = mod_exports.as_deref_mut() {
                        me.insert(orig_name, emitted_name, !self.at_load(), true);
                    }
                } else {
                    let kw = match vd.kind {
                        VariableDeclarationKind::Var => "var",
                        VariableDeclarationKind::Const => "const",
                        VariableDeclarationKind::Let
                        | VariableDeclarationKind::Using
                        | VariableDeclarationKind::AwaitUsing => "let",
                    };
                    self.top_decls.push(format!("{kw} {pat_str} = {init_str};"));
                    if vd.kind == VariableDeclarationKind::Const && self.at_load() {
                        for ident in &bound_idents {
                            self.pure_top_vars
                                .insert(lowerer.bound_name(ident).to_string(), bound_kind);
                        }
                    }
                    if let Some(me) = mod_exports.as_deref_mut() {
                        for ident in &bound_idents {
                            let orig_name = ident.name.to_string();
                            let emitted_name = lowerer.bound_name(ident).to_string();
                            me.insert(orig_name, emitted_name, false, is_mutable_kind);
                        }
                    }
                }
                continue;
            }

            // Non-pure initializer (touches DOM, calls functions, references deferred bindings,
            // or uses object rest): hoist declaration to top level and assign in `__eldur_init()`.
            if vd.kind == VariableDeclarationKind::Var {
                for ident in &bound_idents {
                    let emitted_name = lowerer.bound_name(ident).to_string();
                    self.pure_top_vars.remove(&emitted_name);
                    if !self.var_func_slots.contains_key(&emitted_name) {
                        let slot_idx = self.top_decls.len();
                        self.top_decls.push(format!("var {emitted_name};"));
                        self.var_func_slots
                            .insert(emitted_name, (slot_idx, self.init_stmts.len()));
                    }
                }
            } else if !bound_idents.is_empty() {
                let names = bound_idents
                    .iter()
                    .map(|id| lowerer.bound_name(id))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.top_decls.push(format!("let {names};"));
            }

            if let Some(me) = mod_exports.as_deref_mut() {
                for ident in &bound_idents {
                    let orig_name = ident.name.to_string();
                    let emitted_name = lowerer.bound_name(ident).to_string();
                    me.insert(orig_name, emitted_name, true, is_mutable_kind);
                }
            }

            lowerer.visit_expression(init);
            let init_str = lowerer.editor.extract_and_consume(init.span());

            if let BindingPattern::ObjectPattern(obj) = &decl.id {
                if let Some(rest) = &obj.rest
                    && let BindingPattern::BindingIdentifier(rest_ident) = &rest.argument
                {
                    let mut excluded_keys = Vec::new();
                    let mut regular_props = Vec::new();
                    let mut ok = true;
                    for prop in &obj.properties {
                        lowerer.visit_binding_property(prop);
                        if prop.computed {
                            ok = false;
                            break;
                        }
                        let Some(k) = static_property_key_name(&prop.key) else {
                            ok = false;
                            break;
                        };
                        excluded_keys.push(format!("{k:?}"));
                        regular_props.push(lowerer.editor.extract_and_consume(prop.span));
                    }
                    if ok {
                        let rest_name = lowerer.bound_name(rest_ident).to_string();
                        if regular_props.is_empty() {
                            self.init_stmts
                                .push(format!("{rest_name} = Eldur.objectRest({init_str}, []);"));
                        } else {
                            let tmp = format!("__eldur_obj_{}", lowerer.shared.fresh_id());
                            self.init_stmts.push(format!(
                                "{{ const {tmp} = {init_str}; ({{ {} }} = {tmp}); {rest_name} = Eldur.objectRest({tmp}, [{}]); }}",
                                regular_props.join(", "),
                                excluded_keys.join(", ")
                            ));
                        }
                        continue;
                    }
                }
                lowerer.visit_binding_pattern(&decl.id);
                let pat_str = lowerer.editor.extract_and_consume(decl.id.span());
                self.init_stmts.push(format!("({pat_str} = {init_str});"));
            } else {
                lowerer.visit_binding_pattern(&decl.id);
                let pat_str = lowerer.editor.extract_and_consume(decl.id.span());
                self.init_stmts.push(format!("{pat_str} = {init_str};"));
            }
        }
        let _ = is_classic;
    }

    fn emit_export_default<'b>(
        &mut self,
        script_idx: usize,
        decl_kind: &'b ExportDefaultDeclarationKind<'b>,
        lowerer: &mut AstLowerer<'b, '_>,
        scoping: &'b Scoping,
        mod_exports: Option<&mut ModuleExports>,
    ) {
        match decl_kind {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                lowerer.visit_function(func, ScopeFlags::Function);
                let fn_str = lowerer
                    .editor
                    .extract_and_consume(func.span)
                    .trim()
                    .to_string();
                let (name, is_mutated) = if let Some(id) = &func.id {
                    let n = lowerer.bound_name(id).to_string();
                    let m = id
                        .symbol_id
                        .get()
                        .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                    self.emit_top_function_decl(false, &n, fn_str);
                    (n, m)
                } else {
                    let def_name = lowerer
                        .shared
                        .alloc_fn_name(&format!("__eldur_default_{script_idx}"));
                    self.top_decls.push(format!("const {def_name} = {fn_str};"));
                    self.pure_top_vars
                        .insert(def_name.clone(), PureKind::Function);
                    (def_name, false)
                };
                if let Some(me) = mod_exports {
                    me.insert("default", name, false, is_mutated);
                }
            }
            ExportDefaultDeclarationKind::ClassDeclaration(cls) => {
                let all_static_pure = cls.body.body.iter().all(|el| match el {
                    oxc_ast::ast::ClassElement::PropertyDefinition(p) if p.r#static => {
                        p.value.as_ref().is_none_or(|val| {
                            is_pure_top_level_init(
                                val,
                                scoping,
                                &self.pure_top_vars,
                                &lowerer.symbol_renames,
                            )
                        })
                    }
                    _ => true,
                });
                lowerer.visit_class(cls);
                let cls_str = lowerer
                    .editor
                    .extract_and_consume(cls.span)
                    .trim()
                    .to_string();
                // In a lazy module the class exists once the module runs, as
                // a named class declaration there does.
                let lazy = self.in_lazy_module;
                let (name, is_mutated) = if let Some(id) = &cls.id {
                    let n = lowerer.bound_name(id).to_string();
                    if lazy {
                        self.top_decls.push(format!("let {n};"));
                        self.init_stmts.push(format!("{n} = {cls_str};"));
                    } else {
                        self.top_decls.push(cls_str);
                    }
                    let m = id
                        .symbol_id
                        .get()
                        .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                    (n, m)
                } else {
                    let def_name = lowerer
                        .shared
                        .alloc_fn_name(&format!("__eldur_default_{script_idx}"));
                    if lazy {
                        lowerer.shared.add_tdz_name(&def_name);
                        self.top_decls.push(format!("let {def_name};"));
                        self.init_stmts.push(format!("{def_name} = {cls_str};"));
                    } else {
                        self.top_decls
                            .push(format!("const {def_name} = {cls_str};"));
                    }
                    (def_name, false)
                };
                if all_static_pure && self.at_load() {
                    self.pure_top_vars.insert(name.clone(), PureKind::Function);
                }
                if let Some(static_stmts) = lowerer
                    .post_class_static_inits
                    .remove(&(cls.span.start, cls.span.end))
                {
                    if all_static_pure && self.at_load() {
                        self.top_decls.extend(static_stmts);
                    } else {
                        self.init_stmts.extend(static_stmts);
                    }
                }
                if let Some(me) = mod_exports {
                    me.insert("default", name, lazy, is_mutated);
                }
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {}
            _ => {
                if let Some(expr) = decl_kind.as_expression() {
                    let kind =
                        pure_init_kind(expr, scoping, &self.pure_top_vars, &lowerer.symbol_renames);
                    let is_pure = kind.is_some();
                    lowerer.visit_expression(expr);
                    let expr_str = lowerer.editor.extract_and_consume(expr.span());
                    let def_name = lowerer
                        .shared
                        .alloc_fn_name(&format!("__eldur_default_{script_idx}"));
                    if is_pure && self.at_load() {
                        self.top_decls
                            .push(format!("const {def_name} = {expr_str};"));
                        self.pure_top_vars
                            .insert(def_name.clone(), kind.unwrap_or(PureKind::Unknown));
                        if let Some(me) = mod_exports {
                            me.insert("default", def_name, false, false);
                        }
                    } else {
                        if self.in_lazy_module {
                            lowerer.shared.add_tdz_name(&def_name);
                        }
                        self.top_decls.push(format!("let {def_name};"));
                        self.init_stmts.push(format!("{def_name} = {expr_str};"));
                        if let Some(me) = mod_exports {
                            me.insert("default", def_name, true, false);
                        }
                    }
                }
            }
        }
    }

    fn emit_import_bindings(
        &mut self,
        dep_exports: &ModuleExports,
        specifiers: Option<&[ImportDeclarationSpecifier<'_>]>,
        lowerer: &AstLowerer<'_, '_>,
    ) {
        let Some(specs) = specifiers else {
            return;
        };

        for spec in specs {
            match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    let imported = named.imported.name().as_str();
                    let local = lowerer.bound_name(&named.local);
                    if let Some((target_ident, deferred, _)) = dep_exports.entries.get(imported)
                        && target_ident != local
                    {
                        self.emit_alias_binding(local, target_ident, *deferred, &*lowerer.shared);
                    }
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    let local = lowerer.bound_name(&def.local);
                    if let Some((target_ident, deferred, _)) = dep_exports.entries.get("default")
                        && target_ident != local
                    {
                        self.emit_alias_binding(local, target_ident, *deferred, &*lowerer.shared);
                    }
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                    let local = lowerer.bound_name(&ns.local);
                    // Getters, so the namespace reads the live bindings.
                    let obj_lit = format_live_namespace_object(dep_exports, Some(&*lowerer.shared));
                    self.top_decls.push(format!("const {local} = {obj_lit};"));
                }
            }
        }
    }

    fn emit_alias_binding(
        &mut self,
        local: &str,
        target_ident: &str,
        deferred: bool,
        shared: &SharedRewriteState,
    ) {
        if deferred || !self.at_load() {
            // A copy of a binding that may be uninitialized throws as a read
            // of it does, rather than keep the sentinel.
            let read = shared
                .tdz_read(target_ident, local)
                .unwrap_or_else(|| target_ident.to_string());
            self.top_decls.push(format!("let {local};"));
            self.init_stmts.push(format!("{local} = {read};"));
        } else {
            self.top_decls
                .push(format!("const {local} = {target_ident};"));
            let kind = self
                .pure_top_vars
                .get(target_ident)
                .copied()
                .unwrap_or(PureKind::Unknown);
            self.pure_top_vars.insert(local.to_string(), kind);
        }
    }

    fn try_handle_special_top_expr<'b>(
        &mut self,
        expr: &'b Expression<'b>,
        lowerer: &mut AstLowerer<'b, '_>,
        scoping: &'b Scoping,
    ) -> bool {
        let e = unwrap_parens(expr);

        // 1. Top-level `addEventListener` on `document`, `window`, or a statically resolved `QmlTarget`
        if let Expression::CallExpression(call) = e
            && let Expression::StaticMemberExpression(member) = unwrap_parens(&call.callee)
            && member.property.name == "addEventListener"
            && let Some(ev) = call
                .arguments
                .first()
                .and_then(|a| a.as_expression())
                .and_then(extract_string_literal)
            && let Some(cb) = call.arguments.get(1).and_then(|a| a.as_expression())
        {
            let is_doc = is_global_document(&member.object, scoping);
            let is_win = !is_doc && is_global_window(&member.object, scoping);
            if (is_doc || is_win) && matches!(ev.as_str(), "DOMContentLoaded" | "load") {
                let target_js = if is_doc {
                    "Eldur.document"
                } else {
                    "Eldur.window"
                };
                let cb_str = lowerer.rewrite_listener_callback_expr(cb, None);
                let mut invoke = format_lifecycle_invocation(cb, &cb_str, &ev, target_js);
                if self.in_eager_module {
                    // Its module can throw first, or never run at all.
                    let reached = lowerer.shared.alloc_fn_name("__eldur_reached");
                    self.top_decls.push(format!("var {reached} = false;"));
                    self.init_stmts.push(format!("{reached} = true;"));
                    invoke = format!("if ({reached}) {{\n{}\n}}", indent_block(&invoke, 4));
                }
                if ev == "DOMContentLoaded" {
                    self.dcl_stmts.push(invoke);
                } else {
                    self.load_entries.push(LoadEntry::Listener(invoke));
                }
                return true;
            }

            if let Some(target) = lowerer.resolve_single_target(&member.object)
                && !lowerer
                    .shared
                    .dynamic_listener_events
                    .contains(&(target.qml_id.clone(), ev.clone()))
                && is_hoistable_callback_expr(cb)
                && options_arg_is_pure_or_absent(call.arguments.get(2))
            {
                let is_once = extract_once_option(call.arguments.get(2));
                let cb_str = lowerer.rewrite_listener_callback_expr(cb, Some(target.clone()));
                self.emit_static_listener_fn(lowerer.shared, target, &ev, cb, &cb_str, is_once);
                return true;
            }
        }

        // 2. Top-level `window.onload = rhs` or single static `el.on<event> = rhs`
        if let Expression::AssignmentExpression(assign) = e
            && assign.operator == AssignmentOperator::Assign
        {
            let member_pair = match &assign.left {
                AssignmentTarget::StaticMemberExpression(m) => {
                    Some((&m.object, m.property.name.as_str().to_string()))
                }
                AssignmentTarget::ComputedMemberExpression(c) => {
                    extract_string_literal(&c.expression).map(|p| (&c.object, p))
                }
                _ => None,
            };

            if let Some((obj, prop)) = member_pair
                && let Some(ev) = known_dom_event_from_on_prop(&prop)
            {
                if is_global_window(obj, scoping) && ev == "load" {
                    if is_null_or_undefined(&assign.right, scoping) {
                        self.window_onload_value = None;
                    } else {
                        let rhs_str = lowerer.rewrite_listener_callback_expr(&assign.right, None);
                        let invoke = format_lifecycle_invocation(
                            &assign.right,
                            &rhs_str,
                            "load",
                            "Eldur.window",
                        );
                        if !self.window_onload_has_slot {
                            self.window_onload_has_slot = true;
                            self.load_entries.push(LoadEntry::WindowOnload);
                        }
                        self.window_onload_value = Some(invoke);
                    }
                    return true;
                }

                if let Some(target) = lowerer.resolve_single_target(obj)
                    && !lowerer
                        .shared
                        .dynamic_on_props
                        .contains(&(target.qml_id.clone(), ev.to_string()))
                    && is_hoistable_callback_expr(&assign.right)
                {
                    let cb_str =
                        lowerer.rewrite_listener_callback_expr(&assign.right, Some(target.clone()));
                    self.emit_static_listener_fn(
                        lowerer.shared,
                        target,
                        ev,
                        &assign.right,
                        &cb_str,
                        false,
                    );
                    return true;
                }
            }
        }

        false
    }

    fn emit_static_listener_fn(
        &mut self,
        shared: &mut SharedRewriteState,
        target: QmlTarget,
        event: &str,
        cb_ast: &Expression<'_>,
        cb_str: &str,
        is_once: bool,
    ) {
        let safe_ev = sanitize_ident(event);
        let fn_name = shared.alloc_fn_name(&format!("__eldur_on_{safe_ev}_{}", target.qml_id));

        let call_stmt = if matches!(unwrap_parens(cb_ast), Expression::Identifier(_)) {
            format!("{cb_str}.call(Eldur.wrap({}), event);", target.qml_id)
        } else {
            format!("({cb_str})(event);")
        };

        let body = if is_once {
            let uid = shared.fresh_id();
            let flag = shared.alloc_fn_name(&format!("__eldur_once_{uid}"));
            shared.generated_decls.push(format!("var {flag} = false;"));
            format!(
                "    if ({flag}) {{\n        return;\n    }}\n    {flag} = true;\n{}",
                indent_block(&call_stmt, 4)
            )
        } else {
            indent_block(&call_stmt, 4)
        };

        self.handler_fns
            .push(format!("function {fn_name}(event) {{\n{body}\n}}"));
        shared.bindings.push(EventBinding {
            target,
            event: event.to_string(),
            function: fn_name,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_module_symbol_renames<'a>(
    v: &ValidScript<'a>,
    scoping: &Scoping,
    bindings: &ScriptQueryBindings<'a>,
    resolver: &dyn DomResolver,
    scripts: &[ScriptInput],
    module_exports: &HashMap<usize, ModuleExports>,
    claimed_top_names: &mut HashSet<String>,
    shared: &mut SharedRewriteState,
) -> (HashMap<SymbolId, String>, HashMap<SymbolId, String>) {
    let mut symbol_renames: HashMap<SymbolId, String> = HashMap::new();
    let mut elided_dom_symbols: HashMap<SymbolId, String> = HashMap::new();
    let mut handled_symbols: HashSet<SymbolId> = HashSet::new();

    let mut unaliased_named_exports: HashSet<&str> = HashSet::new();
    for stmt in &v.parsed.program.body {
        if let Statement::ExportNamedDeclaration(exp) = stmt {
            for spec in &exp.specifiers {
                if spec.local.name() == spec.exported.name() {
                    unaliased_named_exports.insert(spec.local.name().as_str());
                }
            }
        }
    }

    fn alloc_binding(
        script_idx: usize,
        bind_id: &BindingIdentifier<'_>,
        is_exported: bool,
        handled_symbols: &mut HashSet<SymbolId>,
        claimed_top_names: &mut HashSet<String>,
        shared: &mut SharedRewriteState,
        symbol_renames: &mut HashMap<SymbolId, String>,
    ) {
        let Some(sym_id) = bind_id.symbol_id.get() else {
            return;
        };
        if !handled_symbols.insert(sym_id) {
            return;
        }
        let orig = bind_id.name.as_str();
        if is_exported && !claimed_top_names.contains(orig) {
            claimed_top_names.insert(orig.to_string());
            shared.used_names.insert(orig.to_string());
        } else {
            let new_name = shared.alloc_fn_name(&format!("__eldur_m{script_idx}_{orig}"));
            claimed_top_names.insert(new_name.clone());
            symbol_renames.insert(sym_id, new_name);
        }
    }

    for stmt in &v.parsed.program.body {
        match stmt {
            Statement::ImportDeclaration(imp) => {
                let spec = imp.source.value.as_str();
                let Some(dep_idx) = resolve_module_specifier(v.input, spec, scripts) else {
                    continue;
                };
                let Some(dep_exports) = module_exports.get(&dep_idx) else {
                    continue;
                };
                for s in imp.specifiers.iter().flatten() {
                    match s {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => {
                            let imported = named.imported.name().as_str();
                            let local_id = &named.local;
                            let local = local_id.name.as_str();
                            if let Some(sym_id) = local_id.symbol_id.get() {
                                handled_symbols.insert(sym_id);
                            }
                            if let Some((target_ident, _deferred, is_mutable)) =
                                dep_exports.entries.get(imported)
                            {
                                if target_ident == local {
                                    claimed_top_names.insert(local.to_string());
                                } else if !*is_mutable && !claimed_top_names.contains(local) {
                                    claimed_top_names.insert(local.to_string());
                                    shared.used_names.insert(local.to_string());
                                } else if let Some(sym_id) = local_id.symbol_id.get() {
                                    symbol_renames.insert(sym_id, target_ident.clone());
                                }
                            }
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                            let local_id = &def.local;
                            let local = local_id.name.as_str();
                            if let Some(sym_id) = local_id.symbol_id.get() {
                                handled_symbols.insert(sym_id);
                            }
                            if let Some((target_ident, _deferred, is_mutable)) =
                                dep_exports.entries.get("default")
                            {
                                if target_ident == local {
                                    claimed_top_names.insert(local.to_string());
                                } else if !*is_mutable && !claimed_top_names.contains(local) {
                                    claimed_top_names.insert(local.to_string());
                                    shared.used_names.insert(local.to_string());
                                } else if let Some(sym_id) = local_id.symbol_id.get() {
                                    symbol_renames.insert(sym_id, target_ident.clone());
                                }
                            }
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                            let local_id = &ns.local;
                            let local = local_id.name.as_str();
                            if let Some(sym_id) = local_id.symbol_id.get() {
                                handled_symbols.insert(sym_id);
                            }
                            if !claimed_top_names.contains(local) {
                                claimed_top_names.insert(local.to_string());
                                shared.used_names.insert(local.to_string());
                            } else {
                                let new_name =
                                    shared.alloc_fn_name(&format!("__eldur_m{}_{local}", v.idx));
                                claimed_top_names.insert(new_name.clone());
                                if let Some(sym_id) = local_id.symbol_id.get() {
                                    symbol_renames.insert(sym_id, new_name);
                                }
                            }
                        }
                    }
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    let is_exp = unaliased_named_exports.contains(id.name.as_str());
                    alloc_binding(
                        v.idx,
                        id,
                        is_exp,
                        &mut handled_symbols,
                        claimed_top_names,
                        shared,
                        &mut symbol_renames,
                    );
                }
            }
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    let is_exp = unaliased_named_exports.contains(id.name.as_str());
                    alloc_binding(
                        v.idx,
                        id,
                        is_exp,
                        &mut handled_symbols,
                        claimed_top_names,
                        shared,
                        &mut symbol_renames,
                    );
                }
            }
            Statement::VariableDeclaration(vd) => {
                for decl in &vd.declarations {
                    if is_elidable_dom_declarator(decl, bindings, resolver) {
                        if let BindingPattern::BindingIdentifier(bind_id) = &decl.id
                            && let Some(sym_id) = bind_id.symbol_id.get()
                            && let Some(query) = bindings.symbol_queries.get(&sym_id)
                            && let Some(target) =
                                crate::lower::resolve_query_to_single_target(query, resolver)
                        {
                            handled_symbols.insert(sym_id);
                            elided_dom_symbols.insert(sym_id, target.qml_id);
                        }
                        continue;
                    }
                    let mut idents = Vec::new();
                    collect_binding_identifiers(&decl.id, &mut idents);
                    for id in idents {
                        let is_exp = unaliased_named_exports.contains(id.name.as_str());
                        alloc_binding(
                            v.idx,
                            id,
                            is_exp,
                            &mut handled_symbols,
                            claimed_top_names,
                            shared,
                            &mut symbol_renames,
                        );
                    }
                }
            }
            Statement::ExportDeclaration(exp) => match &exp.declaration {
                Declaration::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        alloc_binding(
                            v.idx,
                            id,
                            true,
                            &mut handled_symbols,
                            claimed_top_names,
                            shared,
                            &mut symbol_renames,
                        );
                    }
                }
                Declaration::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        alloc_binding(
                            v.idx,
                            id,
                            true,
                            &mut handled_symbols,
                            claimed_top_names,
                            shared,
                            &mut symbol_renames,
                        );
                    }
                }
                Declaration::VariableDeclaration(vd) => {
                    for decl in &vd.declarations {
                        if is_elidable_dom_declarator(decl, bindings, resolver) {
                            if let BindingPattern::BindingIdentifier(bind_id) = &decl.id
                                && let Some(sym_id) = bind_id.symbol_id.get()
                                && let Some(query) = bindings.symbol_queries.get(&sym_id)
                                && let Some(target) =
                                    crate::lower::resolve_query_to_single_target(query, resolver)
                            {
                                handled_symbols.insert(sym_id);
                                elided_dom_symbols.insert(sym_id, target.qml_id);
                            }
                            continue;
                        }
                        let mut idents = Vec::new();
                        collect_binding_identifiers(&decl.id, &mut idents);
                        for id in idents {
                            alloc_binding(
                                v.idx,
                                id,
                                true,
                                &mut handled_symbols,
                                claimed_top_names,
                                shared,
                                &mut symbol_renames,
                            );
                        }
                    }
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(exp) => match &exp.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        alloc_binding(
                            v.idx,
                            id,
                            true,
                            &mut handled_symbols,
                            claimed_top_names,
                            shared,
                            &mut symbol_renames,
                        );
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        alloc_binding(
                            v.idx,
                            id,
                            true,
                            &mut handled_symbols,
                            claimed_top_names,
                            shared,
                            &mut symbol_renames,
                        );
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_scope_id(sym_id) == scoping.root_scope_id()
            && handled_symbols.insert(sym_id)
        {
            let orig = scoping.symbol_name(sym_id);
            let is_exp = unaliased_named_exports.contains(orig);
            if is_exp && !claimed_top_names.contains(orig) {
                claimed_top_names.insert(orig.to_string());
                shared.used_names.insert(orig.to_string());
            } else {
                let new_name = shared.alloc_fn_name(&format!("__eldur_m{}_{orig}", v.idx));
                claimed_top_names.insert(new_name.clone());
                symbol_renames.insert(sym_id, new_name);
            }
        }
    }

    (symbol_renames, elided_dom_symbols)
}

/// Every name a script binds, in any scope, save one bound only to the element
/// of that same id.
fn names_shadowing_ids(
    scripts: &[ValidScript<'_>],
    cross_script: &HashMap<String, DomQuery>,
    resolver: &dyn DomResolver,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for v in scripts {
        let scoping = &v.parsed.scoping;
        let bindings =
            ScriptQueryBindings::build(v.input, &v.parsed.program, scoping, cross_script);
        for sym_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(sym_id);
            let is_the_element = bindings.symbol_queries.get(&sym_id).is_some_and(|query| {
                crate::lower::resolve_query_to_single_target(query, resolver)
                    .is_some_and(|t| t.qml_id == name)
            });
            if !is_the_element {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn collect_program_declared_names(program: &Program<'_>, out: &mut HashSet<String>) {
    for stmt in &program.body {
        match stmt {
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    out.insert(id.name.to_string());
                }
            }
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    out.insert(id.name.to_string());
                }
            }
            Statement::VariableDeclaration(vd) => {
                for decl in &vd.declarations {
                    let mut idents = Vec::new();
                    collect_binding_identifiers(&decl.id, &mut idents);
                    for id in idents {
                        out.insert(id.name.to_string());
                    }
                }
            }
            Statement::ImportDeclaration(imp) => {
                for spec in imp.specifiers.iter().flatten() {
                    match spec {
                        ImportDeclarationSpecifier::ImportSpecifier(s) => {
                            out.insert(s.local.name.to_string());
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                            out.insert(s.local.name.to_string());
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                            out.insert(s.local.name.to_string());
                        }
                    }
                }
            }
            Statement::ExportDeclaration(exp) => match &exp.declaration {
                Declaration::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        out.insert(id.name.to_string());
                    }
                }
                Declaration::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        out.insert(id.name.to_string());
                    }
                }
                Declaration::VariableDeclaration(vd) => {
                    for decl in &vd.declarations {
                        let mut idents = Vec::new();
                        collect_binding_identifiers(&decl.id, &mut idents);
                        for id in idents {
                            out.insert(id.name.to_string());
                        }
                    }
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(exp) => match &exp.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        out.insert(id.name.to_string());
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        out.insert(id.name.to_string());
                    }
                }
                _ => {}
            },
            _ => {
                collect_hoisted_var_names(stmt, out);
            }
        }
    }
}

fn prescan_on_prop_assignments(
    valid_scripts: &[ValidScript<'_>],
    cross_script: &HashMap<String, crate::DomQuery>,
    resolver: &dyn DomResolver,
    dispatch_by_path: bool,
    shared: &mut SharedRewriteState,
) {
    let mut total_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut top_static_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut total_listener_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut top_static_listener_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut dynamic_listener_ops: HashSet<(String, String)> = HashSet::new();
    let mut targets_with_dispatch: HashSet<String> = HashSet::new();

    for v in valid_scripts {
        let scoping = &v.parsed.scoping;
        let bindings =
            ScriptQueryBindings::build(v.input, &v.parsed.program, scoping, cross_script);
        let root_this = match &v.input.kind {
            ScriptKind::HandlerAttribute { element, .. } => resolver.element(*element),
            _ => None,
        };

        // Count top-level simple `el.on<ev> = fn` and `el.addEventListener(ev, fn)` statements in classic/module scripts.
        if matches!(
            v.input.kind,
            ScriptKind::Classic | ScriptKind::Module | ScriptKind::ImportedModule
        ) {
            for stmt in &v.parsed.program.body {
                if let Statement::ExpressionStatement(es) = stmt {
                    let expr = unwrap_parens(&es.expression);
                    if let Expression::AssignmentExpression(assign) = expr
                        && assign.operator == AssignmentOperator::Assign
                        && is_hoistable_callback_expr(&assign.right)
                    {
                        let member_pair = match &assign.left {
                            AssignmentTarget::StaticMemberExpression(m) => {
                                Some((&m.object, m.property.name.as_str().to_string()))
                            }
                            AssignmentTarget::ComputedMemberExpression(c) => {
                                extract_string_literal(&c.expression).map(|p| (&c.object, p))
                            }
                            _ => None,
                        };
                        if let Some((obj, prop)) = member_pair
                            && let Some(ev) = known_dom_event_from_on_prop(&prop)
                            && let Some(target) =
                                resolve_target_for_prescan(obj, scoping, &bindings, resolver, None)
                        {
                            *top_static_counts
                                .entry((target.qml_id.clone(), ev.to_string()))
                                .or_insert(0) += 1;
                        }
                    }

                    if let Expression::CallExpression(call) = expr
                        && let Expression::StaticMemberExpression(member) =
                            unwrap_parens(&call.callee)
                        && member.property.name == "addEventListener"
                        && let Some(ev) = call
                            .arguments
                            .first()
                            .and_then(|a| a.as_expression())
                            .and_then(extract_string_literal)
                        && let Some(cb) = call.arguments.get(1).and_then(|a| a.as_expression())
                        && is_hoistable_callback_expr(cb)
                        && options_arg_is_pure_or_absent(call.arguments.get(2))
                        && let Some(target) = resolve_target_for_prescan(
                            &member.object,
                            scoping,
                            &bindings,
                            resolver,
                            None,
                        )
                    {
                        *top_static_listener_counts
                            .entry((target.qml_id, ev))
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        let mut scanner = OnPropScanner {
            scoping,
            bindings: &bindings,
            resolver,
            this_stack: vec![root_this],
            counts: &mut total_counts,
            listener_counts: &mut total_listener_counts,
            dynamic_listener_ops: &mut dynamic_listener_ops,
            targets_with_dispatch: &mut targets_with_dispatch,
        };
        scanner.visit_program(&v.parsed.program);
    }

    for (key, &total) in &total_counts {
        let top_single = top_static_counts.get(key).copied().unwrap_or(0);
        let needs_dynamic_event_path = dispatch_by_path
            || total_listener_counts.contains_key(key)
            || dynamic_listener_ops.contains(key)
            || targets_with_dispatch.contains(&key.0);
        if total != 1 || top_single != 1 || needs_dynamic_event_path {
            shared.dynamic_on_props.insert(key.clone());
        }
        if needs_dynamic_event_path {
            shared.dynamic_listener_events.insert(key.clone());
        }
    }

    for (key, total) in total_listener_counts {
        let top_single = top_static_listener_counts.get(&key).copied().unwrap_or(0);
        if dispatch_by_path
            || total != 1
            || top_single != 1
            || total_counts.contains_key(&key)
            || dynamic_listener_ops.contains(&key)
            || targets_with_dispatch.contains(&key.0)
        {
            shared.dynamic_listener_events.insert(key);
        }
    }
}

fn resolve_target_for_prescan(
    expr: &Expression<'_>,
    scoping: &Scoping,
    bindings: &ScriptQueryBindings<'_>,
    resolver: &dyn DomResolver,
    current_this: Option<&QmlTarget>,
) -> Option<QmlTarget> {
    let e = unwrap_parens(expr);
    if matches!(e, Expression::ThisExpression(_)) {
        return current_this.cloned();
    }
    if let Some(query) = bindings.resolve_expr(e) {
        return crate::lower::resolve_query_to_single_target(&query, resolver);
    }
    if let Expression::StaticMemberExpression(member) = e
        && is_global_document(&member.object, scoping)
    {
        match member.property.name.as_str() {
            "body" => {
                return resolver
                    .resolve(&crate::DomQuery::Selector("body".to_string()))
                    .into_iter()
                    .next();
            }
            "documentElement" => {
                return resolver
                    .resolve(&crate::DomQuery::Selector("html".to_string()))
                    .into_iter()
                    .next();
            }
            _ => {}
        }
    }
    None
}

struct OnPropScanner<'a, 'b> {
    scoping: &'b Scoping,
    bindings: &'b ScriptQueryBindings<'a>,
    resolver: &'b dyn DomResolver,
    this_stack: Vec<Option<QmlTarget>>,
    counts: &'b mut HashMap<(String, String), usize>,
    listener_counts: &'b mut HashMap<(String, String), usize>,
    dynamic_listener_ops: &'b mut HashSet<(String, String)>,
    targets_with_dispatch: &'b mut HashSet<String>,
}

impl OnPropScanner<'_, '_> {
    fn resolve_target(&self, expr: &Expression<'_>) -> Option<QmlTarget> {
        resolve_target_for_prescan(
            expr,
            self.scoping,
            self.bindings,
            self.resolver,
            self.this_stack.last().and_then(Option::as_ref),
        )
    }
}

impl<'a> Visit<'a> for OnPropScanner<'_, '_> {
    fn visit_function(&mut self, it: &oxc_ast::ast::Function<'a>, flags: ScopeFlags) {
        self.this_stack.push(None);
        walk::walk_function(self, it, flags);
        self.this_stack.pop();
    }

    fn visit_assignment_expression(&mut self, it: &oxc_ast::ast::AssignmentExpression<'a>) {
        let member_pair = match &it.left {
            AssignmentTarget::StaticMemberExpression(m) => {
                Some((&m.object, m.property.name.as_str().to_string()))
            }
            AssignmentTarget::ComputedMemberExpression(c) => {
                extract_string_literal(&c.expression).map(|p| (&c.object, p))
            }
            _ => None,
        };
        if let Some((obj, prop)) = member_pair
            && let Some(ev) = known_dom_event_from_on_prop(&prop)
            && let Some(target) = self.resolve_target(obj)
        {
            *self
                .counts
                .entry((target.qml_id.clone(), ev.to_string()))
                .or_insert(0) += 1;
            let bound_this = Some(target);
            if let Expression::FunctionExpression(func) = unwrap_parens(&it.right) {
                self.this_stack.push(bound_this);
                walk::walk_function(self, func, ScopeFlags::Function);
                self.this_stack.pop();
                return;
            }
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &oxc_ast::ast::CallExpression<'a>) {
        if let Expression::StaticMemberExpression(member) = unwrap_parens(&it.callee)
            && let Some(target) = self.resolve_target(&member.object)
        {
            let method = member.property.name.as_str();
            match method {
                "addEventListener" => {
                    if let Some(ev) = it
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(extract_string_literal)
                    {
                        *self
                            .listener_counts
                            .entry((target.qml_id.clone(), ev))
                            .or_insert(0) += 1;
                    } else {
                        self.targets_with_dispatch.insert(target.qml_id.clone());
                    }
                    if let Some(cb) = it.arguments.get(1).and_then(|a| a.as_expression())
                        && let Expression::FunctionExpression(func) = unwrap_parens(cb)
                    {
                        if let Some(first) = it.arguments.first() {
                            self.visit_argument(first);
                        }
                        self.this_stack.push(Some(target));
                        walk::walk_function(self, func, ScopeFlags::Function);
                        self.this_stack.pop();
                        for extra in it.arguments.iter().skip(2) {
                            self.visit_argument(extra);
                        }
                        return;
                    }
                }
                "removeEventListener" => {
                    if let Some(ev) = it
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(extract_string_literal)
                    {
                        self.dynamic_listener_ops.insert((target.qml_id, ev));
                    } else {
                        self.targets_with_dispatch.insert(target.qml_id);
                    }
                }
                "dispatchEvent" => {
                    self.targets_with_dispatch.insert(target.qml_id);
                }
                "focus" => {
                    self.dynamic_listener_ops
                        .insert((target.qml_id.clone(), "focus".to_string()));
                    self.dynamic_listener_ops
                        .insert((target.qml_id, "focusin".to_string()));
                }
                "blur" => {
                    self.dynamic_listener_ops
                        .insert((target.qml_id.clone(), "blur".to_string()));
                    self.dynamic_listener_ops
                        .insert((target.qml_id, "focusout".to_string()));
                }
                "click" => {
                    self.dynamic_listener_ops
                        .insert((target.qml_id, "click".to_string()));
                }
                _ => {}
            }
        }
        walk::walk_call_expression(self, it);
    }
}

fn is_hoistable_callback_expr(expr: &Expression<'_>) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::Identifier(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
    )
}

fn is_null_or_undefined(expr: &Expression<'_>, scoping: &Scoping) -> bool {
    let e = unwrap_parens(expr);
    matches!(e, Expression::NullLiteral(_)) || is_global_ident(e, "undefined", scoping)
}

fn options_arg_is_pure_or_absent(arg: Option<&Argument<'_>>) -> bool {
    let Some(arg) = arg else {
        return true;
    };
    let Some(expr) = arg.as_expression() else {
        return false;
    };
    match unwrap_parens(expr) {
        Expression::BooleanLiteral(_) => true,
        Expression::ObjectExpression(obj) => obj.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(prop) => {
                !prop.computed
                    && matches!(unwrap_parens(&prop.value), Expression::BooleanLiteral(_))
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        _ => false,
    }
}

fn extract_once_option(arg: Option<&Argument<'_>>) -> bool {
    let Some(expr) = arg.and_then(|a| a.as_expression()) else {
        return false;
    };
    let Expression::ObjectExpression(obj) = unwrap_parens(expr) else {
        return false;
    };
    obj.properties.iter().any(|p| match p {
        ObjectPropertyKind::ObjectProperty(prop) => {
            !prop.computed
                && static_property_key_name(&prop.key).as_deref() == Some("once")
                && matches!(unwrap_parens(&prop.value), Expression::BooleanLiteral(b) if b.value)
        }
        ObjectPropertyKind::SpreadProperty(_) => false,
    })
}

fn format_lifecycle_invocation(
    cb_ast: &Expression<'_>,
    cb_str: &str,
    event_name: &str,
    target_js: &str,
) -> String {
    if matches!(unwrap_parens(cb_ast), Expression::Identifier(_)) {
        format!("{cb_str}(Eldur.event({event_name:?}, {target_js}, null));")
    } else {
        format!("({cb_str})(Eldur.event({event_name:?}, {target_js}, null));")
    }
}

/// Whether `name`, from a declaration this rewriter emitted, is a name alone
/// rather than one with an initializer. Unlike [`is_valid_js_ident`], any
/// identifier a source may use counts, `café` among them.
fn is_bare_binding_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c == '$' || c.is_alphabetic())
        && chars.all(|c| {
            c == '_' || c == '$' || c == '\u{200c}' || c == '\u{200d}' || c.is_alphanumeric()
        })
}

/// What a lazy module reads that can be uninitialized: its own `let`,
/// `const` and `class` bindings, whose page names it adds to
/// `shared.tdz_names`, and its imports, which also cannot be written.
fn lazy_module_tdz_bindings(
    v: &ValidScript<'_>,
    bindings: &ScriptQueryBindings<'_>,
    resolver: &dyn DomResolver,
    symbol_renames: &HashMap<SymbolId, String>,
    shared: &mut SharedRewriteState,
) -> HashMap<SymbolId, TdzBinding> {
    let mut tdz = HashMap::new();
    let emitted = |id: &BindingIdentifier<'_>| -> Option<(SymbolId, String)> {
        let sym_id = id.symbol_id.get()?;
        let name = symbol_renames
            .get(&sym_id)
            .cloned()
            .unwrap_or_else(|| id.name.to_string());
        Some((sym_id, name))
    };
    let mut own =
        |id: &BindingIdentifier<'_>, kind: TdzKind, end: u32, shared: &mut SharedRewriteState| {
            if let Some((sym_id, name)) = emitted(id) {
                shared.add_tdz_name(&name);
                tdz.insert(
                    sym_id,
                    TdzBinding {
                        kind,
                        declared_at: Some(end),
                    },
                );
            }
        };
    for stmt in &v.parsed.program.body {
        let vd = match stmt {
            Statement::VariableDeclaration(vd) => Some(&**vd),
            Statement::ExportDeclaration(exp) => match &exp.declaration {
                Declaration::VariableDeclaration(vd) => Some(&**vd),
                Declaration::ClassDeclaration(cls) => {
                    if let Some(id) = &cls.id {
                        own(id, TdzKind::Let, cls.span.end, shared);
                    }
                    None
                }
                _ => None,
            },
            Statement::ClassDeclaration(cls) => {
                if let Some(id) = &cls.id {
                    own(id, TdzKind::Let, cls.span.end, shared);
                }
                None
            }
            Statement::ExportDefaultDeclaration(exp) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(cls) = &exp.declaration
                    && let Some(id) = &cls.id
                {
                    own(id, TdzKind::Let, cls.span.end, shared);
                }
                None
            }
            _ => None,
        };
        let Some(vd) = vd else {
            continue;
        };
        if vd.kind == VariableDeclarationKind::Var {
            continue;
        }
        let kind = if vd.kind == VariableDeclarationKind::Const {
            TdzKind::Const
        } else {
            TdzKind::Let
        };
        for decl in &vd.declarations {
            // A DOM element it names is no binding at all.
            if is_elidable_dom_declarator(decl, bindings, resolver) {
                continue;
            }
            let mut idents = Vec::new();
            collect_binding_identifiers(&decl.id, &mut idents);
            for id in idents {
                own(id, kind, vd.span.end, shared);
            }
        }
    }
    for stmt in &v.parsed.program.body {
        let Statement::ImportDeclaration(imp) = stmt else {
            continue;
        };
        for spec in imp.specifiers.iter().flatten() {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => &def.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => &ns.local,
            };
            let Some((sym_id, _)) = emitted(local) else {
                continue;
            };
            // A write to any import throws. A live one, read under the
            // exporter's own name, is checked as the exporter's binding is
            // (`tdz_read` leaves alone a name that is never uninitialized);
            // a copy is checked where it is copied.
            shared.tdz_helpers();
            tdz.insert(
                sym_id,
                TdzBinding {
                    kind: TdzKind::Import,
                    declared_at: None,
                },
            );
        }
    }
    tdz
}

/// A module namespace for `import()`: getters read the live bindings, as a
/// namespace's do, and the keys are sorted, as a namespace's are.
/// `checked` reads a binding that may be uninitialized through its check.
fn format_live_namespace_object(
    exports: &ModuleExports,
    checked: Option<&SharedRewriteState>,
) -> String {
    let mut keys: Vec<&String> = exports.ordered_keys.iter().collect();
    keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let fields: Vec<String> = keys
        .into_iter()
        .filter_map(|key| {
            let (target_ident, _, _) = exports.entries.get(key)?;
            let read = checked
                .and_then(|shared| shared.tdz_read(target_ident, key))
                .unwrap_or_else(|| target_ident.clone());
            Some(if is_valid_js_ident(key) {
                format!("get {key}() {{ return {read}; }}")
            } else {
                format!("get {key:?}() {{ return {read}; }}")
            })
        })
        .collect();
    if fields.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", fields.join(", "))
    }
}

/// Every `import("literal")` in a script that names a page module.
struct DynamicImports<'s> {
    importer: &'s ScriptInput,
    scripts: &'s [ScriptInput],
    found: Vec<(String, usize, oxc_span::Span)>,
}

impl<'a> Visit<'a> for DynamicImports<'_> {
    fn visit_import_expression(&mut self, it: &oxc_ast::ast::ImportExpression<'a>) {
        if let Some(idx) =
            crate::parse::dynamic_import_of_page_module(self.importer, it, self.scripts)
            && let Some(spec) = extract_string_literal(&it.source)
        {
            self.found.push((spec, idx, it.span));
        }
        walk::walk_import_expression(self, it);
    }
}

fn is_valid_js_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn pattern_defaults_are_pure(
    pat: &BindingPattern<'_>,
    scoping: &Scoping,
    pure_vars: &HashMap<String, PureKind>,
    symbol_renames: &HashMap<SymbolId, String>,
) -> bool {
    match pat {
        BindingPattern::BindingIdentifier(_) => true,
        BindingPattern::AssignmentPattern(assign) => {
            pattern_defaults_are_pure(&assign.left, scoping, pure_vars, symbol_renames)
                && is_pure_top_level_init(&assign.right, scoping, pure_vars, symbol_renames)
        }
        BindingPattern::ObjectPattern(obj) => {
            obj.properties.iter().all(|p| {
                !p.computed
                    && pattern_defaults_are_pure(&p.value, scoping, pure_vars, symbol_renames)
            }) && obj.rest.as_ref().is_none_or(|r| {
                pattern_defaults_are_pure(&r.argument, scoping, pure_vars, symbol_renames)
            })
        }
        BindingPattern::ArrayPattern(arr) => {
            arr.elements
                .iter()
                .flatten()
                .all(|el| pattern_defaults_are_pure(el, scoping, pure_vars, symbol_renames))
                && arr.rest.as_ref().is_none_or(|r| {
                    pattern_defaults_are_pure(&r.argument, scoping, pure_vars, symbol_renames)
                })
        }
    }
}

/// What is known of a value worked out at load, so an operator that converts
/// it is hoisted only where the conversion cannot throw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PureKind {
    Number,
    BigInt,
    String,
    Boolean,
    /// `null` or `undefined`.
    Nullish,
    Array,
    /// A regular expression or an object literal with only data properties:
    /// converting it to a primitive runs its own code, which may throw.
    Object,
    /// A function or class: as an object, and its own properties may be
    /// getters, a static one or `caller` of a strict function, which may throw.
    Function,
    /// Read without converting it, as another binding holds it.
    Unknown,
}

impl PureKind {
    fn is_primitive(self) -> bool {
        matches!(
            self,
            Self::Number | Self::BigInt | Self::String | Self::Boolean | Self::Nullish
        )
    }

    fn merge(self, other: Self) -> Self {
        if self == other { self } else { Self::Unknown }
    }

    /// What a numeric operator gives for primitive operands of these kinds:
    /// `None` when it throws, as for a BigInt mixed with a Number.
    fn numeric(self, other: Self) -> Option<Self> {
        match (self == Self::BigInt, other == Self::BigInt) {
            (false, false) => Some(Self::Number),
            (true, true) => Some(Self::BigInt),
            _ => None,
        }
    }
}

fn is_pure_top_level_init(
    expr: &Expression<'_>,
    scoping: &Scoping,
    pure_vars: &HashMap<String, PureKind>,
    symbol_renames: &HashMap<SymbolId, String>,
) -> bool {
    pure_init_kind(expr, scoping, pure_vars, symbol_renames).is_some()
}

/// Whether binding `kind`'s value to `pat` cannot throw: an object pattern
/// needs a value whose properties are all data, a primitive that is not
/// nullish, an array or an object literal, and an array pattern an array; each
/// of their names is a plain name, so nothing nested is read.
fn pattern_cannot_throw(pat: &BindingPattern<'_>, kind: PureKind) -> bool {
    let plain = |p: &BindingPattern<'_>| match p {
        BindingPattern::BindingIdentifier(_) => true,
        BindingPattern::AssignmentPattern(a) => {
            matches!(a.left, BindingPattern::BindingIdentifier(_))
        }
        _ => false,
    };
    match pat {
        BindingPattern::BindingIdentifier(_) => true,
        BindingPattern::AssignmentPattern(_) => false,
        BindingPattern::ObjectPattern(obj) => {
            !matches!(
                kind,
                PureKind::Nullish | PureKind::Function | PureKind::Unknown
            ) && obj.rest.is_none()
                && obj.properties.iter().all(|p| plain(&p.value))
        }
        BindingPattern::ArrayPattern(arr) => {
            kind == PureKind::Array
                && arr.elements.iter().flatten().all(plain)
                && arr.rest.as_ref().is_none_or(|r| plain(&r.argument))
        }
    }
}

/// What `expr` gives, when working it out at load has no effect and cannot
/// throw; `None` otherwise.
fn pure_init_kind(
    expr: &Expression<'_>,
    scoping: &Scoping,
    pure_vars: &HashMap<String, PureKind>,
    symbol_renames: &HashMap<SymbolId, String>,
) -> Option<PureKind> {
    let kind = |e: &Expression<'_>| pure_init_kind(e, scoping, pure_vars, symbol_renames);
    let primitive = |e: &Expression<'_>| kind(e).filter(|k| k.is_primitive());
    match unwrap_parens(expr) {
        Expression::BooleanLiteral(_) => Some(PureKind::Boolean),
        Expression::NullLiteral(_) => Some(PureKind::Nullish),
        Expression::NumericLiteral(_) => Some(PureKind::Number),
        Expression::StringLiteral(_) => Some(PureKind::String),
        Expression::BigIntLiteral(_) => Some(PureKind::BigInt),
        Expression::RegExpLiteral(_) => Some(PureKind::Object),
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
            Some(PureKind::Function)
        }

        // Each value is converted to a string.
        Expression::TemplateLiteral(tpl) => {
            for e in &tpl.expressions {
                primitive(e)?;
            }
            Some(PureKind::String)
        }

        Expression::Identifier(id) => {
            if id.is_global_reference(scoping) {
                match id.name.as_str() {
                    "undefined" => Some(PureKind::Nullish),
                    "NaN" | "Infinity" => Some(PureKind::Number),
                    name => pure_vars.get(name).copied(),
                }
            } else {
                let resolved_name = id
                    .reference_id
                    .get()
                    .and_then(|rid| scoping.get_reference(rid).symbol_id())
                    .and_then(|sid| symbol_renames.get(&sid))
                    .map_or(id.name.as_str(), String::as_str);
                pure_vars.get(resolved_name).copied()
            }
        }

        Expression::UnaryExpression(u) => match u.operator {
            UnaryOperator::Delete => None,
            UnaryOperator::LogicalNot => kind(&u.argument).map(|_| PureKind::Boolean),
            UnaryOperator::Typeof => kind(&u.argument).map(|_| PureKind::String),
            UnaryOperator::Void => kind(&u.argument).map(|_| PureKind::Nullish),
            // `+` of a BigInt throws.
            UnaryOperator::UnaryPlus => primitive(&u.argument)
                .filter(|k| *k != PureKind::BigInt)
                .map(|_| PureKind::Number),
            UnaryOperator::UnaryNegation | UnaryOperator::BitwiseNot => {
                primitive(&u.argument).map(|k| k.numeric(k).unwrap_or(PureKind::Number))
            }
        },

        Expression::BinaryExpression(b) => {
            use BinaryOperator as Op;
            match b.operator {
                // Each throws for a right side that is not an object, and
                // may run code of the object's.
                Op::In | Op::Instanceof => None,
                Op::StrictEquality | Op::StrictInequality => {
                    kind(&b.left)?;
                    kind(&b.right)?;
                    Some(PureKind::Boolean)
                }
                Op::Equality
                | Op::Inequality
                | Op::LessThan
                | Op::LessEqualThan
                | Op::GreaterThan
                | Op::GreaterEqualThan => {
                    primitive(&b.left)?;
                    primitive(&b.right)?;
                    Some(PureKind::Boolean)
                }
                Op::Addition => {
                    let (l, r) = (primitive(&b.left)?, primitive(&b.right)?);
                    if l == PureKind::String || r == PureKind::String {
                        Some(PureKind::String)
                    } else {
                        l.numeric(r)
                    }
                }
                // A BigInt can be divided by zero, raised to a negative
                // power, shifted out of range or shifted unsigned, each of
                // which throws.
                Op::Division
                | Op::Remainder
                | Op::Exponential
                | Op::ShiftLeft
                | Op::ShiftRight
                | Op::ShiftRightZeroFill => primitive(&b.left)?
                    .numeric(primitive(&b.right)?)
                    .filter(|k| *k == PureKind::Number),
                Op::Subtraction
                | Op::Multiplication
                | Op::BitwiseOR
                | Op::BitwiseXOR
                | Op::BitwiseAnd => primitive(&b.left)?.numeric(primitive(&b.right)?),
            }
        }

        Expression::LogicalExpression(l) => Some(kind(&l.left)?.merge(kind(&l.right)?)),

        Expression::ConditionalExpression(c) => {
            kind(&c.test)?;
            Some(kind(&c.consequent)?.merge(kind(&c.alternate)?))
        }

        Expression::SequenceExpression(seq) => {
            let mut last = PureKind::Nullish;
            for e in &seq.expressions {
                last = kind(e)?;
            }
            Some(last)
        }

        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                match el {
                    // Spreading a value that is not iterable throws.
                    oxc_ast::ast::ArrayExpressionElement::SpreadElement(sp) => {
                        if !matches!(kind(&sp.argument)?, PureKind::Array | PureKind::String) {
                            return None;
                        }
                    }
                    oxc_ast::ast::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        kind(el.as_expression()?)?;
                    }
                }
            }
            Some(PureKind::Array)
        }

        Expression::ObjectExpression(obj) => {
            for p in &obj.properties {
                match p {
                    ObjectPropertyKind::ObjectProperty(prop) => {
                        // `__proto__: v` sets the prototype, whose getters a
                        // destructuring would run.
                        if prop.kind != PropertyKind::Init
                            || prop.computed
                            || (!prop.shorthand
                                && prop.key.static_name().as_deref() == Some("__proto__"))
                        {
                            return None;
                        }
                        kind(&prop.value)?;
                    }
                    // A value read without converting it may have getters,
                    // which spreading runs.
                    ObjectPropertyKind::SpreadProperty(sp) => {
                        if kind(&sp.argument)? == PureKind::Unknown {
                            return None;
                        }
                    }
                }
            }
            Some(PureKind::Object)
        }

        _ => None,
    }
}

/// The keyword of a declaration `stmt` starts with, if it is one.
fn top_declaration_keyword(stmt: &str) -> Option<&'static str> {
    const KEYWORDS: [&str; 9] = [
        "var",
        "let",
        "const",
        "function",
        "async function",
        "class",
        "using",
        "await using",
        "enum",
    ];
    let stmt = stmt.trim_start();
    KEYWORDS.into_iter().find(|k| {
        stmt.strip_prefix(k).is_some_and(|after| {
            let rest = after.trim_start();
            let spaced = rest.len() < after.len();
            // `function*` or `function *g`; not `using *= 3`.
            if k.ends_with("function") && rest.starts_with('*') {
                return true;
            }
            // `let x`, `const {` or `let [`; not `let = 1`, `var_x = 1` or
            // `using [0] = 1`.
            let pattern = matches!(*k, "var" | "let" | "const");
            spaced
                && rest.chars().next().is_some_and(|c| {
                    c == '$' || c == '_' || c.is_alphabetic() || (pattern && (c == '{' || c == '['))
                })
        })
    })
}

/// Statements in `init_stmts` run inside `__eldur_init` or a module's run
/// function, so a declaration there would be local to that function and its
/// name missing everywhere else. Every binding is declared in `top_decls` and
/// only assigned here.
fn debug_assert_no_top_declaration(stmts: &[String]) {
    if cfg!(debug_assertions)
        && let Some((stmt, kw)) = stmts
            .iter()
            .find_map(|s| top_declaration_keyword(s).map(|kw| (s, kw)))
    {
        panic!(
            "a `{kw}` declaration reached the statements run at load, where it would be local:\n{stmt}"
        );
    }
}

fn ensure_trailing_semicolon(s: &str) -> String {
    if s.ends_with(';') || s.ends_with('}') {
        s.to_string()
    } else {
        format!("{s};")
    }
}

fn indent_block(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod top_declaration_tests {
    use super::top_declaration_keyword;

    #[test]
    fn declarations_are_told_from_statements_that_look_like_them() {
        for (stmt, kw) in [
            ("var x = 1;", "var"),
            ("let x;", "let"),
            ("let [a] = b;", "let"),
            ("const {a} = b;", "const"),
            ("function f() {}", "function"),
            ("function* g() {}", "function"),
            ("function *g() {}", "function"),
            ("async function f() {}", "async function"),
            ("class C {}", "class"),
            ("using r = open();", "using"),
            ("await using r = open();", "await using"),
            ("  const $x = 1;", "const"),
        ] {
            assert_eq!(top_declaration_keyword(stmt), Some(kw), "{stmt}");
        }
        for stmt in [
            "x = 1;",
            "var_x = 1;",
            "letter = 2;",
            "let = 1;",
            "classes.push(1);",
            "using(r);",
            "(function () {})();",
            "for (let i = 0; i < 2; i++) {}",
            "if (a) { const b = 1; }",
            "__eldur_run_m0();",
            "using *= 3;",
            "using [0] = 1;",
            "class *= 2;",
            "let *x;",
        ] {
            assert_eq!(top_declaration_keyword(stmt), None, "{stmt}");
        }
    }
}
