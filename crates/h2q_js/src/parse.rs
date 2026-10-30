use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BigIntLiteral, BindingIdentifier, BindingPattern, CallExpression,
    Class, ClassElement, Expression, ForOfStatement, Function, NewExpression, Program, RegExpFlags,
    RegExpLiteral, Statement, VariableDeclaration, VariableDeclarationKind, WithStatement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{IsGlobalReference, Scoping, SemanticBuilder};
use oxc_span::{SourceType, Span};
use oxc_syntax::scope::{ScopeFlags, ScopeId};

use crate::resolve::{extract_string_literal, is_global_document, is_global_window, unwrap_parens};
use crate::{Diagnostic, ScriptInput, ScriptKind, Severity};

pub(crate) struct ParsedScript<'a> {
    pub program: Program<'a>,
    pub scoping: Scoping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopDeclKind {
    VarOrFunc,
    Lexical,
}

pub(crate) fn source_type_for(kind: &ScriptKind) -> SourceType {
    match kind {
        ScriptKind::Classic => SourceType::script(),
        ScriptKind::Module => SourceType::mjs(),
        // HTML event handler attributes execute inside an implicit function body,
        // so top-level `return false` is valid syntax.
        ScriptKind::HandlerAttribute { .. } => SourceType::cjs(),
    }
}

pub(crate) fn line_for_offset(source: &str, line_offset: u32, byte_offset: usize) -> u32 {
    let clamped = byte_offset.min(source.len());
    let newlines = source.as_bytes()[..clamped]
        .iter()
        .fold(0u32, |acc, &b| acc.saturating_add(u32::from(b == b'\n')));
    line_offset.max(1).saturating_add(newlines)
}

pub(crate) fn line_for_span(input: &ScriptInput, span: Span) -> u32 {
    line_for_offset(&input.source, input.line_offset, span.start as usize)
}

pub(crate) fn parse_script<'a>(
    allocator: &'a Allocator,
    input: &'a ScriptInput,
) -> Result<ParsedScript<'a>, Vec<Diagnostic>> {
    let to_diag = |message: &str, first_offset: Option<u32>| Diagnostic {
        severity: Severity::Error,
        source: input.name.clone(),
        line: first_offset
            .map(|off| line_for_offset(&input.source, input.line_offset, off as usize))
            .or_else(|| Some(input.line_offset.max(1))),
        message: message.to_string(),
    };

    let source_type = source_type_for(&input.kind);
    let ret = Parser::new(allocator, &input.source, source_type).parse();
    if ret.panicked || !ret.diagnostics.is_empty() {
        let mut diags: Vec<Diagnostic> = ret
            .diagnostics
            .iter()
            .map(|d| to_diag(&d.message, d.labels.first().map(|l| l.offset())))
            .collect();
        if diags.is_empty() {
            diags.push(Diagnostic {
                severity: Severity::Error,
                source: input.name.clone(),
                line: Some(input.line_offset.max(1)),
                message: "JavaScript parser aborted on input".to_string(),
            });
        }
        return Err(diags);
    }

    let sem_ret = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .build(&ret.program);
    if !sem_ret.diagnostics.is_empty() {
        let diags = sem_ret
            .diagnostics
            .iter()
            .map(|d| to_diag(&d.message, d.labels.first().map(|l| l.offset())))
            .collect();
        return Err(diags);
    }

    let scoping = sem_ret.semantic.into_scoping();
    Ok(ParsedScript {
        program: ret.program,
        scoping,
    })
}

/// Checks browser global scope lexical redeclaration rules across classic `<script>` tags.
///
/// In browsers, redeclaring a top-level `let`/`const`/`class` binding in a subsequent
/// classic script (or after an earlier `var`/`function` of the same name) is a `SyntaxError`
/// that aborts the offending script before any of its bindings or statements take effect.
pub(crate) struct GlobalDeclTracker {
    decls: HashMap<String, TopDeclKind>,
}

impl GlobalDeclTracker {
    pub(crate) fn new() -> Self {
        Self {
            decls: HashMap::new(),
        }
    }

    pub(crate) fn check_and_register(
        &mut self,
        input: &ScriptInput,
        program: &Program<'_>,
    ) -> Result<(), Vec<Diagnostic>> {
        if !matches!(input.kind, ScriptKind::Classic) {
            return Ok(());
        }

        let mut script_decls: Vec<(String, TopDeclKind, Span)> = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::VariableDeclaration(vd) => {
                    let kind = match vd.kind {
                        VariableDeclarationKind::Var => TopDeclKind::VarOrFunc,
                        VariableDeclarationKind::Let
                        | VariableDeclarationKind::Const
                        | VariableDeclarationKind::Using
                        | VariableDeclarationKind::AwaitUsing => TopDeclKind::Lexical,
                    };
                    for decl in &vd.declarations {
                        let mut idents = Vec::new();
                        collect_binding_identifiers(&decl.id, &mut idents);
                        for ident in idents {
                            script_decls.push((ident.name.to_string(), kind, ident.span));
                        }
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    if let Some(id) = &func.id {
                        script_decls.push((id.name.to_string(), TopDeclKind::VarOrFunc, id.span));
                    }
                }
                Statement::ClassDeclaration(cls) => {
                    if let Some(id) = &cls.id {
                        script_decls.push((id.name.to_string(), TopDeclKind::Lexical, id.span));
                    }
                }
                _ => {
                    let mut collector = HoistedVarCollector {
                        out: &mut script_decls,
                    };
                    collector.visit_statement(stmt);
                }
            }
        }

        let mut errors = Vec::new();
        for (name, kind, span) in &script_decls {
            if let Some(&prev) = self.decls.get(name.as_str())
                && (prev == TopDeclKind::Lexical || *kind == TopDeclKind::Lexical)
            {
                errors.push(Diagnostic {
                    severity: Severity::Error,
                    source: input.name.clone(),
                    line: Some(line_for_span(input, *span)),
                    message: format!("Identifier '{name}' has already been declared"),
                });
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        for (name, kind, _) in script_decls {
            self.decls.insert(name, kind);
        }
        Ok(())
    }
}

struct HoistedVarCollector<'a> {
    out: &'a mut Vec<(String, TopDeclKind, Span)>,
}

impl<'a> Visit<'a> for HoistedVarCollector<'_> {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.kind == VariableDeclarationKind::Var {
            for decl in &it.declarations {
                let mut idents = Vec::new();
                collect_binding_identifiers(&decl.id, &mut idents);
                for ident in idents {
                    self.out
                        .push((ident.name.to_string(), TopDeclKind::VarOrFunc, ident.span));
                }
            }
        }
        walk::walk_variable_declaration(self, it);
    }

    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {
        // Do not descend into function bodies; `var` inside a function is function-scoped.
    }

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {
        // Do not descend into arrow function bodies.
    }

    fn visit_class(&mut self, _it: &Class<'a>) {
        // Do not descend into class bodies.
    }
}

pub(crate) fn collect_binding_identifiers<'a, 'b>(
    pattern: &'b BindingPattern<'a>,
    out: &mut Vec<&'b BindingIdentifier<'a>>,
) {
    match pattern {
        BindingPattern::BindingIdentifier(id) => out.push(id),
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                collect_binding_identifiers(&prop.value, out);
            }
            if let Some(rest) = &obj.rest {
                collect_binding_identifiers(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(arr) => {
            for elem in arr.elements.iter().flatten() {
                collect_binding_identifiers(elem, out);
            }
            if let Some(rest) = &arr.rest {
                collect_binding_identifiers(&rest.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(assign) => {
            collect_binding_identifiers(&assign.left, out);
        }
    }
}

/// Checks whether an `async` function body contains constructs (`super` or
/// unshadowed `arguments`) that cannot be lowered into an inner `function*()`.
pub(crate) fn async_body_has_unlowerable_constructs(
    func_scope_id: Option<ScopeId>,
    scoping: &Scoping,
    body_fn: impl FnOnce(&mut AsyncHazardFinder<'_>),
) -> bool {
    let mut finder = AsyncHazardFinder {
        scoping,
        func_scope_id,
        nested_non_arrow_depth: 0,
        has_hazard: false,
    };
    body_fn(&mut finder);
    finder.has_hazard
}

pub(crate) struct AsyncHazardFinder<'a> {
    scoping: &'a Scoping,
    func_scope_id: Option<ScopeId>,
    nested_non_arrow_depth: u32,
    has_hazard: bool,
}

impl<'a> Visit<'a> for AsyncHazardFinder<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.nested_non_arrow_depth += 1;
        walk::walk_function(self, it, flags);
        self.nested_non_arrow_depth -= 1;
    }

    fn visit_super(&mut self, _it: &oxc_ast::ast::Super) {
        if self.nested_non_arrow_depth == 0 {
            self.has_hazard = true;
        }
    }

    fn visit_identifier_reference(&mut self, it: &oxc_ast::ast::IdentifierReference<'a>) {
        if self.nested_non_arrow_depth == 0 && it.name == "arguments" {
            let shadowed_in_async = it
                .reference_id
                .get()
                .and_then(|rid| self.scoping.get_reference(rid).symbol_id())
                .is_some_and(|sid| {
                    let decl_scope = self.scoping.symbol_scope_id(sid);
                    self.func_scope_id.is_some_and(|fs| {
                        decl_scope == fs || self.scoping.scope_is_descendant_of(decl_scope, fs)
                    })
                });
            if !shadowed_in_async {
                self.has_hazard = true;
            }
        }
    }
}

/// Scans a parsed script for JS engine or browser features that QML 6.11
/// does not support (or only stubs) and returns [`Severity::Unsupported`] diagnostics.
pub(crate) fn collect_unsupported_diagnostics(
    input: &ScriptInput,
    program: &Program<'_>,
    scoping: &Scoping,
) -> Vec<Diagnostic> {
    let mut checker = UnsupportedChecker {
        input,
        scoping,
        async_depth: 0,
        diagnostics: Vec::new(),
    };
    checker.visit_program(program);
    checker.diagnostics
}

struct UnsupportedChecker<'a> {
    input: &'a ScriptInput,
    scoping: &'a Scoping,
    async_depth: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> UnsupportedChecker<'a> {
    fn report(&mut self, span: Span, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Unsupported,
            source: self.input.name.clone(),
            line: Some(line_for_span(self.input, span)),
            message: msg.into(),
        });
    }

    fn check_regex_pattern_and_flags(&mut self, span: Span, pattern: &str, flags_str: &str) {
        if flags_str.contains('s') || flags_str.contains('d') || flags_str.contains('v') {
            self.report(
                span,
                format!("RegExp flags '{flags_str}' use features unsupported by QML's JS engine"),
            );
        }
        if let Some(reason) = detect_unsupported_regex_syntax(pattern) {
            self.report(span, reason);
        }
    }
}

impl<'a> Visit<'a> for UnsupportedChecker<'_> {
    fn visit_big_int_literal(&mut self, it: &BigIntLiteral<'a>) {
        self.report(it.span, "BigInt is not supported by QML's JS engine");
    }

    fn visit_reg_exp_literal(&mut self, it: &RegExpLiteral<'a>) {
        let mut flags = String::new();
        if it.regex.flags.contains(RegExpFlags::S) {
            flags.push('s');
        }
        if it.regex.flags.contains(RegExpFlags::D) {
            flags.push('d');
        }
        if it.regex.flags.contains(RegExpFlags::V) {
            flags.push('v');
        }
        self.check_regex_pattern_and_flags(it.span, it.regex.pattern.text.as_str(), &flags);
    }

    fn visit_with_statement(&mut self, it: &WithStatement<'a>) {
        self.report(it.span, "'with' statement is not supported in QML");
        walk::walk_with_statement(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        if it.r#await {
            self.report(
                it.span,
                "'for await...of' is not supported by QML's JS engine",
            );
        }
        walk::walk_for_of_statement(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.r#async && it.generator {
            self.report(
                it.span,
                "async generator functions are not supported by QML's JS engine",
            );
        } else if it.r#async
            && let Some(body) = &it.body
            && async_body_has_unlowerable_constructs(it.scope_id.get(), self.scoping, |finder| {
                finder.visit_function_body(body);
            })
        {
            self.report(
                it.span,
                "async function using 'super' or 'arguments' cannot be lowered for QML",
            );
        }

        let prev = self.async_depth;
        if it.r#async {
            self.async_depth += 1;
        } else {
            self.async_depth = 0;
        }
        walk::walk_function(self, it, flags);
        self.async_depth = prev;
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if it.r#async
            && async_body_has_unlowerable_constructs(it.scope_id.get(), self.scoping, |finder| {
                finder.visit_arrow_function_body(&it.body);
            })
        {
            self.report(
                it.span,
                "async arrow function using 'super' or 'arguments' cannot be lowered for QML",
            );
        }

        let prev = self.async_depth;
        if it.r#async {
            self.async_depth += 1;
        } else {
            self.async_depth = 0;
        }
        walk::walk_arrow_function_expression(self, it);
        self.async_depth = prev;
    }

    fn visit_await_expression(&mut self, it: &oxc_ast::ast::AwaitExpression<'a>) {
        if self.async_depth == 0 {
            self.report(it.span, "top-level 'await' is not supported in QML scripts");
        }
        walk::walk_await_expression(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        for elem in &it.body.body {
            match elem {
                ClassElement::StaticBlock(sb) => {
                    self.report(
                        sb.span,
                        "class static initialization blocks are not supported by QML's JS engine",
                    );
                }
                ClassElement::AccessorProperty(acc) => {
                    self.report(
                        acc.span,
                        "class 'accessor' fields are not supported by QML's JS engine",
                    );
                }
                ClassElement::PropertyDefinition(prop) => {
                    if matches!(prop.key, oxc_ast::ast::PropertyKey::PrivateIdentifier(_)) {
                        self.report(
                            prop.span,
                            "private class fields are not supported by QML's JS engine",
                        );
                    } else if prop.computed {
                        self.report(
                            prop.span,
                            "computed class fields cannot be lowered for QML's JS engine",
                        );
                    }
                }
                ClassElement::MethodDefinition(method) => {
                    if matches!(method.key, oxc_ast::ast::PropertyKey::PrivateIdentifier(_)) {
                        self.report(
                            method.span,
                            "private class methods are not supported by QML's JS engine",
                        );
                    }
                }
                ClassElement::TSIndexSignature(_) => {}
            }
        }
        walk::walk_class(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &oxc_ast::ast::IdentifierReference<'a>) {
        if !it.is_global_reference(self.scoping) {
            return;
        }
        match it.name.as_str() {
            "BigInt" => self.report(it.span, "BigInt is not supported by QML's JS engine"),
            "Intl" => self.report(it.span, "Intl is not supported by QML's JS engine"),
            "MutationObserver" => {
                self.report(it.span, "MutationObserver is not supported in QML");
            }
            "IntersectionObserver" => {
                self.report(it.span, "IntersectionObserver is not supported in QML");
            }
            "location" => self.report(it.span, "'location' is only stubbed in QML"),
            "history" => self.report(it.span, "'history' is only stubbed in QML"),
            "navigator" => self.report(it.span, "'navigator' is only stubbed in QML"),
            _ => {}
        }
    }

    fn visit_static_member_expression(&mut self, it: &oxc_ast::ast::StaticMemberExpression<'a>) {
        if is_global_window(&it.object, self.scoping) {
            match it.property.name.as_str() {
                "location" => self.report(it.span, "'window.location' is only stubbed in QML"),
                "history" => self.report(it.span, "'window.history' is only stubbed in QML"),
                "navigator" => self.report(it.span, "'window.navigator' is only stubbed in QML"),
                "Intl" => self.report(it.span, "Intl is not supported by QML's JS engine"),
                _ => {}
            }
            // Skip walking `it.object` as bare `window` if handled, but still walk other children.
            return;
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &oxc_ast::ast::AssignmentExpression<'a>) {
        if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &it.left
            && member.property.name == "cssText"
            && let Expression::StaticMemberExpression(inner) = unwrap_parens(&member.object)
            && inner.property.name == "style"
        {
            self.report(
                it.span,
                "style.cssText assignment cannot be mapped statically in QML",
            );
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let callee = unwrap_parens(&it.callee);
        match callee {
            Expression::Identifier(id) if id.is_global_reference(self.scoping) => {
                match id.name.as_str() {
                    "eval" => {
                        self.report(it.span, "'eval' bypasses static DOM rewriting in QML");
                    }
                    "Function" => {
                        self.report(
                            it.span,
                            "'Function' constructor bypasses static DOM rewriting",
                        );
                    }
                    "confirm" => {
                        self.report(it.span, "synchronous 'confirm()' cannot block in QML");
                    }
                    "prompt" => {
                        self.report(it.span, "synchronous 'prompt()' cannot block in QML");
                    }
                    "RegExp" => {
                        check_regexp_ctor_args(self, it.span, &it.arguments);
                    }
                    _ => {}
                }
            }
            Expression::StaticMemberExpression(member) => {
                if is_global_document(&member.object, self.scoping)
                    && (member.property.name == "write" || member.property.name == "writeln")
                {
                    self.report(it.span, "document.write() is not supported in QML");
                } else if is_global_window(&member.object, self.scoping) {
                    match member.property.name.as_str() {
                        "eval" => {
                            self.report(it.span, "'eval' bypasses static DOM rewriting in QML");
                        }
                        "confirm" => {
                            self.report(it.span, "synchronous 'confirm()' cannot block in QML");
                        }
                        "prompt" => {
                            self.report(it.span, "synchronous 'prompt()' cannot block in QML");
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        let callee = unwrap_parens(&it.callee);
        if let Expression::Identifier(id) = callee
            && id.is_global_reference(self.scoping)
        {
            match id.name.as_str() {
                "Function" => {
                    self.report(
                        it.span,
                        "'new Function' bypasses static DOM rewriting in QML",
                    );
                }
                "RegExp" => {
                    check_regexp_ctor_args(self, it.span, &it.arguments);
                }
                _ => {}
            }
        }
        walk::walk_new_expression(self, it);
    }
}

fn check_regexp_ctor_args(
    checker: &mut UnsupportedChecker<'_>,
    span: Span,
    args: &[oxc_ast::ast::Argument<'_>],
) {
    let pat = args
        .first()
        .and_then(|a| a.as_expression())
        .and_then(extract_string_literal)
        .unwrap_or_default();
    let flags = args
        .get(1)
        .and_then(|a| a.as_expression())
        .and_then(extract_string_literal)
        .unwrap_or_default();
    if !pat.is_empty() || !flags.is_empty() {
        checker.check_regex_pattern_and_flags(span, &pat, &flags);
    }
}

fn detect_unsupported_regex_syntax(pattern: &str) -> Option<&'static str> {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    let mut in_char_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                // Skip escaped character.
                i += 2;
                continue;
            }
            b'[' if !in_char_class => {
                in_char_class = true;
            }
            b']' if in_char_class => {
                in_char_class = false;
            }
            b'(' if !in_char_class
                && bytes.get(i + 1) == Some(&b'?')
                && bytes.get(i + 2) == Some(&b'<') =>
            {
                return match bytes.get(i + 3) {
                    Some(b'=' | b'!') => {
                        Some("RegExp lookbehind assertions are not supported by QML's JS engine")
                    }
                    _ => Some("RegExp named capture groups are not supported by QML's JS engine"),
                };
            }
            _ => {}
        }
        i += 1;
    }
    None
}
