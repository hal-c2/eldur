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
    AstLowerer, SharedRewriteState, is_elidable_dom_declarator, sanitize_ident,
    static_property_key_name,
};
use crate::parse::{
    GlobalDeclTracker, ParsedScript, collect_binding_identifiers, collect_hoisted_var_names,
    collect_unsupported_diagnostics, parse_script,
};
use crate::resolve::{
    ScriptQueryBindings, build_cross_script_query_map, extract_string_literal, is_global_document,
    is_global_ident, is_global_window, known_dom_event_from_on_prop, normalize_handler_event,
    resolve_module_specifier, unwrap_parens,
};
use crate::{
    Analysis, Diagnostic, DomResolver, EventBinding, JsOutput, QmlTarget, ScriptInput, ScriptKind,
    Severity,
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
    /// Top-level `const`/`var` identifiers whose pure literal values are already available at module load.
    pure_top_vars: HashSet<String>,
    handler_fns: Vec<String>,
    init_stmts: Vec<String>,
    dcl_stmts: Vec<String>,
    load_entries: Vec<LoadEntry>,
    window_onload_has_slot: bool,
    window_onload_value: Option<String>,
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
    module_exports: HashMap<usize, ModuleExports>,
    diagnostics: Vec<Diagnostic>,
}

pub(crate) fn rewrite(
    scripts: &[ScriptInput],
    _analysis: &Analysis,
    resolver: &dyn DomResolver,
) -> JsOutput {
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

        if matches!(input.kind, ScriptKind::Module) {
            collect_unresolvable_module_diagnostics(
                input,
                &parsed.program,
                scripts,
                &mut diagnostics,
            );
        }

        let unsupported = collect_unsupported_diagnostics(input, &parsed.program, &parsed.scoping);
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

    let mut shared = SharedRewriteState::default();
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

    // Pre-scan `.on<event> = ...` assignments across all valid scripts to find which
    // `(qml_id, event)` pairs cannot be hoisted as a single static top-level binding.
    prescan_on_prop_assignments(&valid_scripts, &cross_script, resolver, &mut shared);

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
        module_exports: HashMap::new(),
        diagnostics,
    };

    for (pos, v) in valid_scripts.iter().enumerate() {
        match &v.input.kind {
            ScriptKind::Classic => builder.process_classic_or_module(pos, None),
            ScriptKind::Module => builder.ensure_module_emitted(v.idx),
            ScriptKind::HandlerAttribute { element, event } => {
                builder.process_handler_attribute(v, *element, event);
            }
        }
    }

    builder.finish()
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
        let mut exports = ModuleExports::default();
        self.process_classic_or_module(pos, Some(&mut exports));
        self.visiting_modules.remove(&script_idx);
        self.emitted_modules.insert(script_idx);
        self.module_exports.insert(script_idx, exports);
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
        let bindings =
            ScriptQueryBindings::build(v.input, &v.parsed.program, scoping, self.cross_script);

        // Pre-inline any imported local modules in source order before lowering statements.
        if !is_classic {
            for stmt in &v.parsed.program.body {
                let spec_opt = match stmt {
                    Statement::ImportDeclaration(imp) => Some(imp.source.value.as_str()),
                    Statement::ExportAllDeclaration(exp) => Some(exp.source.value.as_str()),
                    Statement::ExportFromDeclaration(exp) => Some(exp.source.value.as_str()),
                    _ => None,
                };
                if let Some(spec) = spec_opt
                    && let Some(dep_idx) =
                        resolve_module_specifier(&v.input.name, spec, self.scripts)
                {
                    if self.visiting_modules.contains(&dep_idx) {
                        self.diagnostics.push(Diagnostic {
                            severity: Severity::Unsupported,
                            source: v.input.name.clone(),
                            line: Some(v.input.line_offset.max(1)),
                            message: format!("circular module import '{spec}' is not supported"),
                        });
                    } else {
                        self.ensure_module_emitted(dep_idx);
                    }
                }
            }
        }

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
        lowerer.symbol_renames = symbol_renames;

        for stmt in &v.parsed.program.body {
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
                    if let Some(dep_idx) = resolve_module_specifier(&v.input.name, spec, scripts)
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
                            let deferred = !out.pure_top_vars.contains(&emitted_local);
                            me.insert(exported, emitted_local, deferred, is_mutated);
                        }
                    }
                }

                Statement::ExportFromDeclaration(exp) => {
                    if let Some(dep_idx) =
                        resolve_module_specifier(&v.input.name, exp.source.value.as_str(), scripts)
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
                        resolve_module_specifier(&v.input.name, exp.source.value.as_str(), scripts)
                        && let Some(dep_exports) = module_exports.get(&dep_idx)
                    {
                        if let Some(alias_name) = &exp.exported {
                            let ns_ident = lowerer
                                .shared
                                .alloc_fn_name(&format!("__eldur_ns_{}", v.idx));
                            let (obj_lit, any_deferred) = format_namespace_object(dep_exports);
                            if any_deferred {
                                out.top_decls.push(format!("let {ns_ident};"));
                                out.init_stmts.push(format!("{ns_ident} = {obj_lit};"));
                            } else {
                                out.top_decls.push(format!("const {ns_ident} = {obj_lit};"));
                            }
                            if let Some(me) = mod_exports.as_deref_mut() {
                                me.insert(
                                    alias_name.name().to_string(),
                                    ns_ident,
                                    any_deferred,
                                    false,
                                );
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
                    if out.try_handle_special_top_expr(&es.expression, &mut lowerer, scoping) {
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

        JsOutput {
            redispatching_slots: self.shared.redispatching_slots,
            page_js,
            init_function,
            bindings: self.shared.bindings,
            diagnostics: self.diagnostics,
        }
    }
}

impl PageBuffers {
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
        self.pure_top_vars.insert(name.to_string());
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
        self.top_decls.push(cls_str);

        if let Some(static_stmts) = lowerer
            .post_class_static_inits
            .remove(&(cls.span.start, cls.span.end))
        {
            if all_static_pure && self.init_stmts.is_empty() {
                self.top_decls.extend(static_stmts);
            } else {
                self.init_stmts.extend(static_stmts);
            }
        }

        if let Some(id) = &cls.id {
            let emitted_name = lowerer.bound_name(id).to_string();
            if all_static_pure && self.init_stmts.is_empty() {
                self.pure_top_vars.insert(emitted_name.clone());
            }
            if is_exported && let Some(me) = mod_exports {
                let orig_name = id.name.to_string();
                let is_mutated = id
                    .symbol_id
                    .get()
                    .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                me.insert(orig_name, emitted_name, false, is_mutated);
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
                    if !self.init_stmts.is_empty() {
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
            let is_pure = !has_obj_rest
                && !is_destructured_var
                && pattern_defaults_are_pure(
                    &decl.id,
                    scoping,
                    &self.pure_top_vars,
                    &lowerer.symbol_renames,
                )
                && is_pure_top_level_init(
                    init,
                    scoping,
                    &self.pure_top_vars,
                    &lowerer.symbol_renames,
                );

            if is_pure {
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
                        if self.init_stmts.is_empty() {
                            self.pure_top_vars.insert(emitted_name.clone());
                        }
                        self.var_func_slots
                            .insert(emitted_name.clone(), (slot_idx, self.init_stmts.len()));
                    }
                    if let Some(me) = mod_exports.as_deref_mut() {
                        me.insert(orig_name, emitted_name, !self.init_stmts.is_empty(), true);
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
                    if vd.kind == VariableDeclarationKind::Const && self.init_stmts.is_empty() {
                        for ident in &bound_idents {
                            self.pure_top_vars
                                .insert(lowerer.bound_name(ident).to_string());
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
                    self.pure_top_vars.insert(def_name.clone());
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
                let (name, is_mutated) = if let Some(id) = &cls.id {
                    self.top_decls.push(cls_str);
                    let n = lowerer.bound_name(id).to_string();
                    let m = id
                        .symbol_id
                        .get()
                        .is_some_and(|sid| scoping.symbol_is_mutated(sid));
                    (n, m)
                } else {
                    let def_name = lowerer
                        .shared
                        .alloc_fn_name(&format!("__eldur_default_{script_idx}"));
                    self.top_decls
                        .push(format!("const {def_name} = {cls_str};"));
                    (def_name, false)
                };
                if all_static_pure && self.init_stmts.is_empty() {
                    self.pure_top_vars.insert(name.clone());
                }
                if let Some(static_stmts) = lowerer
                    .post_class_static_inits
                    .remove(&(cls.span.start, cls.span.end))
                {
                    if all_static_pure && self.init_stmts.is_empty() {
                        self.top_decls.extend(static_stmts);
                    } else {
                        self.init_stmts.extend(static_stmts);
                    }
                }
                if let Some(me) = mod_exports {
                    me.insert("default", name, false, is_mutated);
                }
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {}
            _ => {
                if let Some(expr) = decl_kind.as_expression() {
                    let is_pure = is_pure_top_level_init(
                        expr,
                        scoping,
                        &self.pure_top_vars,
                        &lowerer.symbol_renames,
                    );
                    lowerer.visit_expression(expr);
                    let expr_str = lowerer.editor.extract_and_consume(expr.span());
                    let def_name = lowerer
                        .shared
                        .alloc_fn_name(&format!("__eldur_default_{script_idx}"));
                    if is_pure && self.init_stmts.is_empty() {
                        self.top_decls
                            .push(format!("const {def_name} = {expr_str};"));
                        self.pure_top_vars.insert(def_name.clone());
                        if let Some(me) = mod_exports {
                            me.insert("default", def_name, false, false);
                        }
                    } else {
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
                        self.emit_alias_binding(local, target_ident, *deferred);
                    }
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    let local = lowerer.bound_name(&def.local);
                    if let Some((target_ident, deferred, _)) = dep_exports.entries.get("default")
                        && target_ident != local
                    {
                        self.emit_alias_binding(local, target_ident, *deferred);
                    }
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                    let local = lowerer.bound_name(&ns.local);
                    let (obj_lit, any_deferred) = format_namespace_object(dep_exports);
                    if any_deferred || !self.init_stmts.is_empty() {
                        self.top_decls.push(format!("let {local};"));
                        self.init_stmts.push(format!("{local} = {obj_lit};"));
                    } else {
                        self.top_decls.push(format!("const {local} = {obj_lit};"));
                    }
                }
            }
        }
    }

    fn emit_alias_binding(&mut self, local: &str, target_ident: &str, deferred: bool) {
        if deferred || !self.init_stmts.is_empty() {
            self.top_decls.push(format!("let {local};"));
            self.init_stmts.push(format!("{local} = {target_ident};"));
        } else {
            self.top_decls
                .push(format!("const {local} = {target_ident};"));
            self.pure_top_vars.insert(local.to_string());
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
                let invoke = format_lifecycle_invocation(cb, &cb_str, &ev, target_js);
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
            format!("{cb_str}.call({}, event);", target.qml_id)
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
                let Some(dep_idx) = resolve_module_specifier(&v.input.name, spec, scripts) else {
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
        if matches!(v.input.kind, ScriptKind::Classic | ScriptKind::Module) {
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
        let needs_dynamic_event_path = total_listener_counts.contains_key(key)
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
        if total != 1
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

fn format_namespace_object(exports: &ModuleExports) -> (String, bool) {
    let mut fields = Vec::with_capacity(exports.ordered_keys.len());
    let mut any_deferred = false;
    for key in &exports.ordered_keys {
        if let Some((target_ident, deferred, _)) = exports.entries.get(key) {
            if *deferred {
                any_deferred = true;
            }
            if is_valid_js_ident(key) {
                fields.push(format!("{key}: {target_ident}"));
            } else {
                fields.push(format!("{key:?}: {target_ident}"));
            }
        }
    }
    if fields.is_empty() {
        ("{}".to_string(), any_deferred)
    } else {
        (format!("{{ {} }}", fields.join(", ")), any_deferred)
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
    pure_vars: &HashSet<String>,
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

fn is_pure_top_level_init(
    expr: &Expression<'_>,
    scoping: &Scoping,
    pure_vars: &HashSet<String>,
    symbol_renames: &HashMap<SymbolId, String>,
) -> bool {
    match unwrap_parens(expr) {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_) => true,

        Expression::TemplateLiteral(tpl) => tpl
            .expressions
            .iter()
            .all(|e| is_pure_top_level_init(e, scoping, pure_vars, symbol_renames)),

        Expression::Identifier(id) => {
            if id.is_global_reference(scoping) {
                matches!(id.name.as_str(), "undefined" | "NaN" | "Infinity")
                    || pure_vars.contains(id.name.as_str())
            } else {
                let resolved_name = id
                    .reference_id
                    .get()
                    .and_then(|rid| scoping.get_reference(rid).symbol_id())
                    .and_then(|sid| symbol_renames.get(&sid))
                    .map_or(id.name.as_str(), String::as_str);
                pure_vars.contains(resolved_name)
            }
        }

        Expression::UnaryExpression(u) => {
            u.operator != UnaryOperator::Delete
                && is_pure_top_level_init(&u.argument, scoping, pure_vars, symbol_renames)
        }

        Expression::BinaryExpression(b) => {
            !matches!(b.operator, BinaryOperator::In | BinaryOperator::Instanceof)
                && is_pure_top_level_init(&b.left, scoping, pure_vars, symbol_renames)
                && is_pure_top_level_init(&b.right, scoping, pure_vars, symbol_renames)
        }

        Expression::LogicalExpression(l) => {
            is_pure_top_level_init(&l.left, scoping, pure_vars, symbol_renames)
                && is_pure_top_level_init(&l.right, scoping, pure_vars, symbol_renames)
        }

        Expression::ConditionalExpression(c) => {
            is_pure_top_level_init(&c.test, scoping, pure_vars, symbol_renames)
                && is_pure_top_level_init(&c.consequent, scoping, pure_vars, symbol_renames)
                && is_pure_top_level_init(&c.alternate, scoping, pure_vars, symbol_renames)
        }

        Expression::SequenceExpression(seq) => seq
            .expressions
            .iter()
            .all(|e| is_pure_top_level_init(e, scoping, pure_vars, symbol_renames)),

        Expression::ArrayExpression(arr) => arr.elements.iter().all(|el| match el {
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(sp) => {
                is_pure_top_level_init(&sp.argument, scoping, pure_vars, symbol_renames)
            }
            oxc_ast::ast::ArrayExpressionElement::Elision(_) => true,
            _ => el
                .as_expression()
                .is_some_and(|e| is_pure_top_level_init(e, scoping, pure_vars, symbol_renames)),
        }),

        Expression::ObjectExpression(obj) => obj.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(prop) => {
                prop.kind == PropertyKind::Init
                    && !prop.computed
                    && is_pure_top_level_init(&prop.value, scoping, pure_vars, symbol_renames)
            }
            ObjectPropertyKind::SpreadProperty(sp) => {
                is_pure_top_level_init(&sp.argument, scoping, pure_vars, symbol_renames)
            }
        }),

        _ => false,
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
