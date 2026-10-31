//! CSS stylesheet and inline `style` parsing via `swc_css_parser` with CSS Syntax 3
//! error recovery.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use swc_common::{
    BytePos, FileName, SourceMap, SourceMapper, Span, Spanned, input::StringInput, sync::Lrc,
};
use swc_css_ast as ast;
use swc_css_codegen::{
    CodeGenerator, CodegenConfig, Emit,
    writer::basic::{BasicCssWriter, BasicCssWriterConfig},
};
use swc_css_parser::{lexer::Lexer, parser::ParserConfig};

use crate::media::MediaQueryList;
use crate::properties::{self, ComputeContext, CssWide, Longhand, PropertyKind};
use crate::selectors::{
    ConvertedSelectorList, Selector, convert_relative_selector_list, convert_selector_list,
    is_supported_complex_selector,
};
use crate::{
    ComputedStyle, Diagnostic, Easing, FontFace, Keyframe, Origin, Severity, Stylesheet, Viewport,
};

static NEXT_ANON_LAYER_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_anonymous_layer_id() -> u64 {
    NEXT_ANON_LAYER_ID.fetch_add(1, Ordering::Relaxed)
}

/// One segment of a cascade layer path (`@layer theme.dark` or an anonymous `@layer`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LayerSegment {
    Named(String),
    Anonymous(u64),
}

pub(crate) type LayerPath = Vec<LayerSegment>;

/// Layer attachment on an `@import` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImportLayer {
    Anonymous(u64),
    Named(Vec<String>),
}

impl ImportLayer {
    pub(crate) fn into_segments(self) -> LayerPath {
        match self {
            Self::Anonymous(id) => vec![LayerSegment::Anonymous(id)],
            Self::Named(names) => names.into_iter().map(LayerSegment::Named).collect(),
        }
    }
}

/// A parsed `@import` rule recorded for `collect.rs`.
#[derive(Debug, Clone)]
pub(crate) struct ImportRule {
    pub(crate) href: String,
    pub(crate) layer: Option<ImportLayer>,
    /// Result of evaluating `supports(...)` on `@import` (`true` if omitted).
    pub(crate) supports_matches: bool,
    pub(crate) media: Option<MediaQueryList>,
    pub(crate) line: u32,
    /// Number of `@layer` registrations recorded in the stylesheet before this `@import`.
    pub(crate) preceding_layer_registrations: usize,
}

/// Records the declaration of a cascade layer in source order so `cascade.rs`
/// can establish document-wide layer ordering before matching rules.
#[derive(Debug, Clone)]
pub(crate) struct LayerRegistration {
    pub(crate) path: LayerPath,
    pub(crate) media: Vec<MediaQueryList>,
}

/// A parsed style rule with compiled selectors, validated declarations, enclosing
/// `@media` conditions, and enclosing `@layer` path.
#[derive(Debug, Clone)]
pub(crate) struct StyleRule {
    pub(crate) selectors: Vec<Selector>,
    pub(crate) declarations: Vec<ParsedDecl>,
    pub(crate) media: Vec<MediaQueryList>,
    pub(crate) layer: Option<LayerPath>,
    /// The selector list as written, for what re-evaluates it at run time;
    /// `None` inside a nesting rule, whose text alone does not say what it
    /// selects.
    pub(crate) selector_text: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedDecl {
    pub(crate) value: DeclValue,
    pub(crate) important: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum DeclValue {
    /// Validated longhand value with no `var()` and no CSS-wide keyword.
    Expanded(Longhand, String),
    /// CSS-wide keyword (`inherit`, `initial`, `unset`, `revert`, `revert-layer`) on a longhand.
    Wide(Longhand, CssWide),
    /// Longhand or shorthand declaration containing `var(...)`, resolved at compute time.
    PendingVar {
        target: Longhand,
        prop_name: String,
        raw_value: String,
    },
    /// Custom property `--name`.
    Custom {
        name: String,
        value: CustomDeclValue,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum CustomDeclValue {
    Tokens(String),
    Wide(CssWide),
}

#[derive(Debug, Clone)]
pub(crate) struct KeyframesRule {
    pub(crate) name: String,
    pub(crate) keyframes: Vec<Keyframe>,
    pub(crate) media: Vec<MediaQueryList>,
}

#[derive(Debug, Clone)]
pub(crate) struct FontFaceRule {
    pub(crate) font_face: FontFace,
    pub(crate) media: Vec<MediaQueryList>,
}

/// Internal parsed representation of a [`Stylesheet`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Sheet {
    pub(crate) imports: Vec<ImportRule>,
    pub(crate) layer_registrations: Vec<LayerRegistration>,
    pub(crate) rules: Vec<StyleRule>,
    pub(crate) keyframes: Vec<KeyframesRule>,
    pub(crate) font_faces: Vec<FontFaceRule>,
}

impl Sheet {
    /// Prepends an `@import`'s `media` query and `layer` path onto every item in an
    /// imported stylesheet.
    pub(crate) fn wrap_imported(
        &mut self,
        import_media: Option<&MediaQueryList>,
        import_layer: Option<&[LayerSegment]>,
    ) {
        if let Some(m) = import_media {
            for reg in &mut self.layer_registrations {
                reg.media.insert(0, m.clone());
            }
            for rule in &mut self.rules {
                rule.media.insert(0, m.clone());
            }
            for kf in &mut self.keyframes {
                kf.media.insert(0, m.clone());
            }
            for ff in &mut self.font_faces {
                ff.media.insert(0, m.clone());
            }
        }

        if let Some(prefix) = import_layer
            && !prefix.is_empty()
        {
            for reg in &mut self.layer_registrations {
                let mut full = prefix.to_vec();
                full.append(&mut reg.path);
                reg.path = full;
            }
            let mut initial_media = Vec::new();
            if let Some(m) = import_media {
                initial_media.push(m.clone());
            }
            self.layer_registrations.insert(
                0,
                LayerRegistration {
                    path: prefix.to_vec(),
                    media: initial_media,
                },
            );
            for rule in &mut self.rules {
                let mut full = prefix.to_vec();
                if let Some(existing) = &mut rule.layer {
                    full.append(existing);
                }
                rule.layer = Some(full);
            }
        }
    }
}

struct ParseCtx<'a> {
    cm: &'a SourceMap,
    source_text: &'a str,
    origin: Origin,
    source_name: &'a str,
    base_url: Option<&'a str>,
    sheet: Sheet,
    diagnostics: Vec<Diagnostic>,
    seen_unknown_props: HashSet<String>,
    seen_unsupported_props: HashSet<String>,
    seen_unsupported_pseudos: HashSet<String>,
    /// True until any rule other than `@charset`, `@import`, or a `@layer` statement
    /// has been encountered at the top level.
    allow_imports: bool,
    /// The source of the style rule being read.
    selector_text: Option<String>,
}

impl<'a> ParseCtx<'a> {
    fn line_of(&self, span: Span) -> u32 {
        if span.is_dummy() {
            1
        } else {
            self.cm.lookup_char_pos(span.lo).line as u32
        }
    }

    fn report(&mut self, severity: Severity, line: Option<u32>, message: impl Into<String>) {
        if self.origin == Origin::UserAgent {
            return;
        }
        self.diagnostics.push(Diagnostic {
            severity,
            source: self.source_name.to_owned(),
            line,
            message: message.into(),
        });
    }

    fn report_selector_diagnostics(&mut self, conv: &ConvertedSelectorList, line: u32) {
        if conv.invalid {
            self.report(Severity::Invalid, Some(line), "invalid selector");
            return;
        }
        for pe in &conv.unsupported_pseudo_elements {
            if self.seen_unsupported_pseudos.insert(pe.clone()) {
                self.report(
                    Severity::Unsupported,
                    Some(line),
                    format!("unsupported pseudo-element `{pe}`"),
                );
            }
        }
        for _ in 0..conv.unsupported_multi_dynamic {
            self.report(
                Severity::Unsupported,
                Some(line),
                "selector with dynamic pseudo-classes on multiple elements is unsupported",
            );
        }
    }
}

/// Maximum block/function nesting depth (`{`, `[`, `(`, `<function-token>`) passed to
/// `swc_css_parser` before excess inner balanced pairs are blanked out.
/// `eldur_css` already rejects property values nested deeper than `MAX_NESTING_DEPTH = 32`,
/// while `swc_css_parser`'s recursive descent overflows a 2 MB thread stack in
/// unoptimized debug builds around ~150 nested frames.
const MAX_SWC_NESTING_DEPTH: usize = 48;

/// Neutralizes balanced `{`..`}`, `[`..`]`, `(`..`)`, and `<function-token>`..`)` pairs
/// nested deeper than [`MAX_SWC_NESTING_DEPTH`] by replacing the opener and closer
/// delimiter bytes with ASCII spaces, preserving byte offsets and line numbers.
/// Returns `Cow::Borrowed(text)` when no nesting exceeds the limit.
fn cap_nesting_depth(text: &str) -> Cow<'_, str> {
    let Ok(len) = u32::try_from(text.len()) else {
        return Cow::Borrowed(text);
    };
    let lexer = Lexer::new(
        StringInput::new(text, BytePos(1), BytePos(1 + len)),
        None,
        ParserConfig::default(),
    );
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut blank_positions: Vec<usize> = Vec::new();

    for ts in lexer {
        let lo = (ts.span.lo.0 - 1) as usize;
        let hi = (ts.span.hi.0 - 1) as usize;
        match ts.token {
            ast::Token::LBrace => {
                stack.push((b'}', hi - 1));
            }
            ast::Token::LBracket => {
                stack.push((b']', hi - 1));
            }
            ast::Token::LParen | ast::Token::Function { .. } => {
                stack.push((b')', hi - 1));
            }
            ast::Token::RBrace | ast::Token::RBracket | ast::Token::RParen => {
                let closer = match ts.token {
                    ast::Token::RBrace => b'}',
                    ast::Token::RBracket => b']',
                    _ => b')',
                };
                if stack.last().is_some_and(|&(c, _)| c == closer)
                    && let Some((_, open_idx)) = stack.pop()
                    && stack.len() >= MAX_SWC_NESTING_DEPTH
                {
                    blank_positions.push(open_idx);
                    blank_positions.push(lo);
                }
            }
            _ => {}
        }
    }

    // Any opener still on `stack` at EOF is genuinely unclosed per CSS Syntax 3,
    // because a closer of a different kind inside it is just a token of its block,
    // as the mismatch check above already treats it. It and all its ancestors run to
    // EOF, so blanking the deep ones cannot change which rules or declarations are valid.
    for &(_, open_idx) in stack.iter().skip(MAX_SWC_NESTING_DEPTH) {
        blank_positions.push(open_idx);
    }

    if blank_positions.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut bytes = text.as_bytes().to_vec();
    for idx in blank_positions {
        bytes[idx] = b' ';
    }
    Cow::Owned(String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned()))
}

/// Parses a full CSS stylesheet into [`Stylesheet`] and recoverable [`Diagnostic`]s.
pub(crate) fn parse_stylesheet(
    text: &str,
    origin: Origin,
    source_name: &str,
    base_url: Option<&str>,
) -> (Stylesheet, Vec<Diagnostic>) {
    let capped = cap_nesting_depth(text);
    let text = capped.as_ref();
    let cm: Lrc<SourceMap> = Lrc::default();
    let fm = cm.new_source_file(FileName::Anon.into(), text.to_owned());
    let config = ParserConfig {
        legacy_nesting: true,
        ..Default::default()
    };
    let mut swc_errors = Vec::new();
    let parsed: Result<Result<ast::Stylesheet, _>, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            swc_css_parser::parse_file(&fm, None, config, &mut swc_errors)
        }));

    let mut ctx = ParseCtx {
        cm: &cm,
        source_text: text,
        origin,
        source_name,
        base_url,
        sheet: Sheet::default(),
        diagnostics: Vec::new(),
        seen_unknown_props: HashSet::new(),
        seen_unsupported_props: HashSet::new(),
        seen_unsupported_pseudos: HashSet::new(),
        allow_imports: true,
        selector_text: None,
    };

    match parsed {
        Ok(Ok(ast_sheet)) => {
            let media_stack = Vec::new();
            let layer_stack = Vec::new();
            for rule in &ast_sheet.rules {
                process_top_or_group_rule(&mut ctx, rule, &media_stack, &layer_stack, true);
            }
        }
        _ => {
            ctx.report(Severity::Invalid, Some(1), "unrecoverable CSS syntax error");
        }
    }

    let stylesheet = Stylesheet {
        origin,
        base_url: base_url.map(str::to_owned),
        source_name: source_name.to_owned(),
        inner: ctx.sheet,
    };
    (stylesheet, ctx.diagnostics)
}

/// Parses an HTML element's `style="..."` attribute as a CSS declaration list.
pub(crate) fn parse_inline_style(
    text: &str,
    source_name: &str,
    origin: Origin,
) -> (Vec<ParsedDecl>, Vec<Diagnostic>) {
    if text.trim().is_empty() {
        return (Vec::new(), Vec::new());
    }
    let capped = cap_nesting_depth(text);
    let text = capped.as_ref();
    let cm: Lrc<SourceMap> = Lrc::default();
    let fm = cm.new_source_file(FileName::Anon.into(), text.to_owned());
    let mut swc_errors = Vec::new();
    let parsed: Result<Result<Vec<ast::DeclarationOrAtRule>, _>, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            swc_css_parser::parse_file(&fm, None, Default::default(), &mut swc_errors)
        }));

    let mut ctx = ParseCtx {
        cm: &cm,
        source_text: text,
        origin,
        source_name,
        base_url: None,
        sheet: Sheet::default(),
        diagnostics: Vec::new(),
        seen_unknown_props: HashSet::new(),
        seen_unsupported_props: HashSet::new(),
        seen_unsupported_pseudos: HashSet::new(),
        allow_imports: false,
        selector_text: None,
    };

    let mut out = Vec::new();
    match parsed {
        Ok(Ok(items)) => {
            for item in &items {
                match item {
                    ast::DeclarationOrAtRule::Declaration(decl) => {
                        process_declaration(&mut ctx, decl, &mut out);
                    }
                    ast::DeclarationOrAtRule::AtRule(at) => {
                        let line = ctx.line_of(at.span);
                        ctx.report(
                            Severity::Invalid,
                            Some(line),
                            "at-rule is not allowed in inline style attribute",
                        );
                    }
                    ast::DeclarationOrAtRule::ListOfComponentValues(locv) => {
                        let line = ctx.line_of(locv.span);
                        ctx.report(Severity::Invalid, Some(line), "invalid declaration");
                    }
                }
            }
        }
        _ => {
            ctx.report(Severity::Invalid, Some(1), "invalid inline style attribute");
        }
    }
    (out, ctx.diagnostics)
}

fn process_top_or_group_rule(
    ctx: &mut ParseCtx<'_>,
    rule: &ast::Rule,
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
    is_top_level: bool,
) {
    match rule {
        ast::Rule::QualifiedRule(qr) => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            process_qualified_rule(ctx, qr, media_stack, layer_stack, None);
        }
        ast::Rule::AtRule(at) => {
            process_at_rule(ctx, at, media_stack, layer_stack, None, is_top_level);
        }
        ast::Rule::ListOfComponentValues(locv) => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let line = ctx.line_of(locv.span);
            ctx.report(Severity::Invalid, Some(line), "invalid CSS rule");
        }
    }
}

fn process_qualified_rule(
    ctx: &mut ParseCtx<'_>,
    qr: &ast::QualifiedRule,
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
    parent_selectors: Option<&[Selector]>,
) {
    let line = ctx.line_of(qr.span);
    let converted = match &qr.prelude {
        ast::QualifiedRulePrelude::SelectorList(sl) => convert_selector_list(sl, parent_selectors),
        ast::QualifiedRulePrelude::RelativeSelectorList(rsl) => match parent_selectors {
            Some(parents) => convert_relative_selector_list(rsl, parents),
            None => ConvertedSelectorList {
                invalid: true,
                ..Default::default()
            },
        },
        ast::QualifiedRulePrelude::ListOfComponentValues(_) => ConvertedSelectorList {
            invalid: true,
            ..Default::default()
        },
    };

    ctx.report_selector_diagnostics(&converted, line);
    if converted.invalid || converted.selectors.is_empty() {
        return;
    }

    let outer_text = ctx.selector_text.take();
    ctx.selector_text = parent_selectors
        .is_none()
        .then(|| {
            ctx.cm
                .span_to_snippet(Span::new(qr.span.lo, qr.block.span.lo))
                .ok()
        })
        .flatten();
    process_style_block_contents(
        ctx,
        &qr.block,
        &converted.selectors,
        media_stack,
        layer_stack,
    );
    ctx.selector_text = outer_text;
}

/// Processes a style rule's `{ ... }` block (or a `@media`/`@supports`/`@layer` block
/// nested inside a style rule), preserving exact source order across declarations
/// and nested rules.
fn process_style_block_contents(
    ctx: &mut ParseCtx<'_>,
    block: &ast::SimpleBlock,
    active_selectors: &[Selector],
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
) {
    // `swc_css_parser` groups declarations before nested rules when building a
    // `StyleBlock`; sorting by `span_lo()` restores exact CSS source order so
    // declarations written after nested rules win the cascade as CSS Nesting 1 requires.
    let mut items: Vec<&ast::ComponentValue> = block.value.iter().collect();
    items.sort_by_key(|cv| cv.span_lo());

    let mut current_decls: Vec<ParsedDecl> = Vec::new();

    let flush_decls = |ctx: &mut ParseCtx<'_>, decls: &mut Vec<ParsedDecl>| {
        if !decls.is_empty() {
            ctx.sheet.rules.push(StyleRule {
                selectors: active_selectors.to_vec(),
                declarations: std::mem::take(decls),
                media: media_stack.to_vec(),
                layer: (!layer_stack.is_empty()).then(|| layer_stack.to_vec()),
                selector_text: ctx.selector_text.clone(),
            });
        }
    };

    for cv in items {
        match cv {
            ast::ComponentValue::Declaration(decl) => {
                process_declaration(ctx, decl, &mut current_decls);
            }
            ast::ComponentValue::QualifiedRule(nested_qr) => {
                flush_decls(ctx, &mut current_decls);
                process_qualified_rule(
                    ctx,
                    nested_qr,
                    media_stack,
                    layer_stack,
                    Some(active_selectors),
                );
            }
            ast::ComponentValue::AtRule(nested_at) => {
                flush_decls(ctx, &mut current_decls);
                process_at_rule(
                    ctx,
                    nested_at,
                    media_stack,
                    layer_stack,
                    Some(active_selectors),
                    false,
                );
            }
            ast::ComponentValue::ListOfComponentValues(locv) => {
                let line = ctx.line_of(locv.span);
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "invalid declaration or nested rule",
                );
            }
            _ => {}
        }
    }

    flush_decls(ctx, &mut current_decls);
}

fn process_at_rule(
    ctx: &mut ParseCtx<'_>,
    at: &ast::AtRule,
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
    parent_selectors: Option<&[Selector]>,
    is_top_level: bool,
) {
    let raw_name = match &at.name {
        ast::AtRuleName::Ident(id) => id.value.to_string(),
        ast::AtRuleName::DashedIdent(di) => format!("--{}", di.value),
    };
    let name_lower = raw_name.to_ascii_lowercase();
    let line = ctx.line_of(at.span);

    // Vendor-prefixed at-rules (`@-webkit-keyframes`, `@-moz-document`, …) are
    // ignored silently per Requirement 1.
    if name_lower.starts_with('-') && !name_lower.starts_with("--") {
        if is_top_level {
            ctx.allow_imports = false;
        }
        return;
    }

    match name_lower.as_str() {
        "charset" => {
            // `@charset` is ignored silently and does not invalidate subsequent `@import`s.
        }
        "import" => {
            if !is_top_level || !ctx.allow_imports || at.block.is_some() {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "misplaced or invalid `@import` rule",
                );
                return;
            }
            let Some(prelude) = &at.prelude else {
                ctx.report(Severity::Invalid, Some(line), "missing `@import` target");
                return;
            };
            let ast::AtRulePrelude::ImportPrelude(imp) = &**prelude else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@import` prelude");
                return;
            };
            if let Some(mut import_rule) = convert_import_rule(ctx.source_text, imp, line) {
                import_rule.preceding_layer_registrations = ctx.sheet.layer_registrations.len();
                ctx.sheet.imports.push(import_rule);
            } else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@import` rule");
            }
        }
        "media" => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let Some(block) = &at.block else {
                ctx.report(Severity::Invalid, Some(line), "`@media` rule missing block");
                return;
            };
            let mql = match at.prelude.as_deref() {
                None => MediaQueryList::parse_str(""),
                Some(ast::AtRulePrelude::MediaPrelude(m)) => MediaQueryList::from_ast(m),
                Some(ast::AtRulePrelude::ListOfComponentValues(locv)) => {
                    MediaQueryList::from_component_values(&locv.children)
                }
                Some(_) => {
                    ctx.report(Severity::Invalid, Some(line), "invalid `@media` prelude");
                    return;
                }
            };
            let mut next_media = media_stack.to_vec();
            next_media.push(mql);
            process_conditional_group_block(ctx, block, &next_media, layer_stack, parent_selectors);
        }
        "supports" => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let Some(block) = &at.block else {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "`@supports` rule missing block",
                );
                return;
            };
            let supported = match at.prelude.as_deref() {
                Some(ast::AtRulePrelude::SupportsPrelude(cond)) => {
                    eval_supports_condition(ctx.source_text, cond)
                }
                _ => {
                    ctx.report(
                        Severity::Invalid,
                        Some(line),
                        "invalid `@supports` condition",
                    );
                    return;
                }
            };
            if supported {
                process_conditional_group_block(
                    ctx,
                    block,
                    media_stack,
                    layer_stack,
                    parent_selectors,
                );
            }
        }
        "layer" => {
            process_layer_at_rule(
                ctx,
                at,
                media_stack,
                layer_stack,
                parent_selectors,
                is_top_level,
                line,
            );
        }
        "keyframes" => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let (Some(prelude), Some(block)) = (at.prelude.as_deref(), &at.block) else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@keyframes` rule");
                return;
            };
            let ast::AtRulePrelude::KeyframesPrelude(kf_name) = prelude else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@keyframes` name");
                return;
            };
            if let Some(kf_rule) = parse_keyframes_rule(ctx, kf_name, block, media_stack) {
                ctx.sheet.keyframes.push(kf_rule);
            } else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@keyframes` rule");
            }
        }
        "font-face" => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let Some(block) = &at.block else {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "`@font-face` rule missing block",
                );
                return;
            };
            if let Some(prelude) = at.prelude.as_deref() {
                match prelude {
                    ast::AtRulePrelude::ListOfComponentValues(locv)
                        if locv.children.iter().all(is_whitespace_cv) => {}
                    _ => {
                        ctx.report(
                            Severity::Invalid,
                            Some(line),
                            "unexpected prelude on `@font-face`",
                        );
                        return;
                    }
                }
            }
            if let Some(ff) = parse_font_face_rule(ctx, block, media_stack) {
                ctx.sheet.font_faces.push(ff);
            } else {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "incomplete or invalid `@font-face` rule",
                );
            }
        }
        "nest" => {
            let (Some(parents), Some(prelude), Some(block)) =
                (parent_selectors, at.prelude.as_deref(), &at.block)
            else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@nest` rule");
                return;
            };
            let ast::AtRulePrelude::NestPrelude(sl) = prelude else {
                ctx.report(Severity::Invalid, Some(line), "invalid `@nest` selector");
                return;
            };
            let converted = convert_selector_list(sl, Some(parents));
            ctx.report_selector_diagnostics(&converted, line);
            if !converted.invalid && !converted.selectors.is_empty() {
                process_style_block_contents(
                    ctx,
                    block,
                    &converted.selectors,
                    media_stack,
                    layer_stack,
                );
            }
        }
        "namespace"
        | "page"
        | "counter-style"
        | "property"
        | "container"
        | "scope"
        | "starting-style"
        | "font-palette-values"
        | "font-feature-values"
        | "color-profile"
        | "view-transition" => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            ctx.report(
                Severity::Unsupported,
                Some(line),
                format!("unsupported `@{name_lower}` rule"),
            );
        }
        _ => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            ctx.report(
                Severity::Invalid,
                Some(line),
                format!("unknown at-rule `@{raw_name}`"),
            );
        }
    }
}

fn process_layer_at_rule(
    ctx: &mut ParseCtx<'_>,
    at: &ast::AtRule,
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
    parent_selectors: Option<&[Selector]>,
    is_top_level: bool,
    line: u32,
) {
    match (&at.prelude, &at.block) {
        // Statement form: `@layer a, b.c;` (does NOT flip `allow_imports` per Cascade 5 §2.2).
        (Some(prelude), None) => match &**prelude {
            ast::AtRulePrelude::LayerPrelude(ast::LayerPrelude::NameList(list)) => {
                if list.name_list.is_empty() {
                    ctx.report(Severity::Invalid, Some(line), "empty `@layer` statement");
                    return;
                }
                let mut paths = Vec::with_capacity(list.name_list.len());
                for ln in &list.name_list {
                    let Some(segs) = convert_layer_name(ln) else {
                        ctx.report(Severity::Invalid, Some(line), "invalid `@layer` name");
                        return;
                    };
                    let mut full = layer_stack.to_vec();
                    full.extend(segs);
                    paths.push(full);
                }
                for path in paths {
                    ctx.sheet.layer_registrations.push(LayerRegistration {
                        path,
                        media: media_stack.to_vec(),
                    });
                }
            }
            ast::AtRulePrelude::LayerPrelude(ast::LayerPrelude::Name(ln)) => {
                let Some(segs) = convert_layer_name(ln) else {
                    ctx.report(Severity::Invalid, Some(line), "invalid `@layer` name");
                    return;
                };
                let mut full = layer_stack.to_vec();
                full.extend(segs);
                ctx.sheet.layer_registrations.push(LayerRegistration {
                    path: full,
                    media: media_stack.to_vec(),
                });
            }
            _ => {
                ctx.report(Severity::Invalid, Some(line), "invalid `@layer` statement");
            }
        },
        // Anonymous layer block: `@layer { ... }`
        (None, Some(block)) => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let mut next_layer = layer_stack.to_vec();
            next_layer.push(LayerSegment::Anonymous(next_anonymous_layer_id()));
            ctx.sheet.layer_registrations.push(LayerRegistration {
                path: next_layer.clone(),
                media: media_stack.to_vec(),
            });
            process_conditional_group_block(ctx, block, media_stack, &next_layer, parent_selectors);
        }
        // Named layer block: `@layer a.b { ... }`
        (Some(prelude), Some(block)) => {
            if is_top_level {
                ctx.allow_imports = false;
            }
            let segs = match &**prelude {
                ast::AtRulePrelude::LayerPrelude(ast::LayerPrelude::Name(ln)) => {
                    convert_layer_name(ln)
                }
                // `swc_css_parser` may parse a single name in `@layer foo {}` as a
                // 1-element `NameList` or `Name`; reject multi-name lists on blocks.
                ast::AtRulePrelude::LayerPrelude(ast::LayerPrelude::NameList(list))
                    if list.name_list.len() == 1 =>
                {
                    convert_layer_name(&list.name_list[0])
                }
                ast::AtRulePrelude::ListOfComponentValues(locv)
                    if locv.children.iter().all(is_whitespace_cv) =>
                {
                    Some(vec![LayerSegment::Anonymous(next_anonymous_layer_id())])
                }
                _ => None,
            };
            let Some(segs) = segs else {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    "invalid `@layer` block prelude",
                );
                return;
            };
            let mut next_layer = layer_stack.to_vec();
            next_layer.extend(segs);
            ctx.sheet.layer_registrations.push(LayerRegistration {
                path: next_layer.clone(),
                media: media_stack.to_vec(),
            });
            process_conditional_group_block(ctx, block, media_stack, &next_layer, parent_selectors);
        }
        (None, None) => {
            ctx.report(Severity::Invalid, Some(line), "empty `@layer` rule");
        }
    }
}

fn convert_layer_name(ln: &ast::LayerName) -> Option<LayerPath> {
    if ln.name.is_empty() || ln.name.iter().any(|id| is_css_wide_ident(&id.value)) {
        return None;
    }
    Some(
        ln.name
            .iter()
            .map(|id| LayerSegment::Named(id.value.to_string()))
            .collect(),
    )
}

fn process_conditional_group_block(
    ctx: &mut ParseCtx<'_>,
    block: &ast::SimpleBlock,
    media_stack: &[MediaQueryList],
    layer_stack: &[LayerSegment],
    parent_selectors: Option<&[Selector]>,
) {
    if let Some(parents) = parent_selectors {
        process_style_block_contents(ctx, block, parents, media_stack, layer_stack);
        return;
    }

    for cv in &block.value {
        match cv {
            ast::ComponentValue::QualifiedRule(qr) => {
                process_qualified_rule(ctx, qr, media_stack, layer_stack, None);
            }
            ast::ComponentValue::AtRule(at) => {
                process_at_rule(ctx, at, media_stack, layer_stack, None, false);
            }
            ast::ComponentValue::ListOfComponentValues(locv) => {
                let line = ctx.line_of(locv.span);
                ctx.report(Severity::Invalid, Some(line), "invalid rule inside block");
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// `@import`, `@supports`, `@keyframes`, `@font-face`
// ---------------------------------------------------------------------------

fn convert_import_rule(
    source_text: &str,
    imp: &ast::ImportPrelude,
    line: u32,
) -> Option<ImportRule> {
    let href = match &*imp.href {
        ast::ImportHref::Str(s) => s.value.to_string(),
        ast::ImportHref::Url(u) => extract_url_node_string(u)?,
    };

    let layer = match imp.layer_name.as_deref() {
        None => None,
        Some(ast::ImportLayerName::Ident(id)) => {
            if !id.value.eq_ignore_ascii_case("layer") {
                return None;
            }
            Some(ImportLayer::Anonymous(next_anonymous_layer_id()))
        }
        Some(ast::ImportLayerName::Function(func)) => {
            if !func.name.as_str().eq_ignore_ascii_case("layer") || func.value.len() != 1 {
                return None;
            }
            let ast::ComponentValue::LayerName(ln) = &func.value[0] else {
                return None;
            };
            if ln.name.is_empty() || ln.name.iter().any(|id| is_css_wide_ident(&id.value)) {
                return None;
            }
            Some(ImportLayer::Named(
                ln.name.iter().map(|id| id.value.to_string()).collect(),
            ))
        }
    };

    let mut supports_matches = true;
    let mut media = None;
    if let Some(conds) = imp.import_conditions.as_deref() {
        if let Some(sup_fn) = conds.supports.as_deref() {
            supports_matches = eval_import_supports_function(source_text, sup_fn);
        }
        if let Some(mql) = conds.media.as_deref() {
            media = Some(MediaQueryList::from_ast(mql));
        }
    }

    Some(ImportRule {
        href,
        layer,
        supports_matches,
        media,
        line,
        preceding_layer_registrations: 0,
    })
}

fn eval_import_supports_function(source_text: &str, func: &ast::Function) -> bool {
    if !func.name.as_str().eq_ignore_ascii_case("supports") {
        return false;
    }
    let non_ws: Vec<&ast::ComponentValue> = func
        .value
        .iter()
        .filter(|cv| !is_whitespace_cv(cv))
        .collect();
    if non_ws.len() != 1 {
        return false;
    }
    match non_ws[0] {
        ast::ComponentValue::SupportsCondition(sc) => eval_supports_condition(source_text, sc),
        ast::ComponentValue::Declaration(decl) => eval_supports_declaration(decl),
        _ => false,
    }
}

fn eval_supports_condition(source_text: &str, cond: &ast::SupportsCondition) -> bool {
    if cond.conditions.is_empty() {
        return false;
    }
    if cond.conditions.len() == 1 {
        return match &cond.conditions[0] {
            ast::SupportsConditionType::Not(n) => {
                // Per CSS Conditional 3 §6.1, `<general-enclosed>` inside `not (...)`
                // must NOT evaluate to true (Kleene Unknown collapses to false at top).
                eval_supports_in_parens_kleene(source_text, &n.condition)
                    .not()
                    .to_bool()
            }
            ast::SupportsConditionType::SupportsInParens(p) => {
                eval_supports_in_parens_kleene(source_text, p).to_bool()
            }
            ast::SupportsConditionType::And(a) => {
                eval_supports_in_parens_kleene(source_text, &a.condition).to_bool()
            }
            ast::SupportsConditionType::Or(o) => {
                eval_supports_in_parens_kleene(source_text, &o.condition).to_bool()
            }
        };
    }

    eval_supports_condition_kleene(source_text, cond).to_bool()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SupportsTruth {
    True,
    False,
    Unknown,
}

impl SupportsTruth {
    fn from_bool(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }

    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    fn and(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    fn or(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    fn to_bool(self) -> bool {
        matches!(self, Self::True)
    }
}

fn eval_supports_condition_kleene(
    source_text: &str,
    cond: &ast::SupportsCondition,
) -> SupportsTruth {
    if cond.conditions.is_empty() {
        return SupportsTruth::Unknown;
    }
    let is_or = cond
        .conditions
        .iter()
        .any(|c| matches!(c, ast::SupportsConditionType::Or(_)));
    if is_or {
        let mut acc = SupportsTruth::False;
        for item in &cond.conditions {
            let val = match item {
                ast::SupportsConditionType::SupportsInParens(p) => {
                    eval_supports_in_parens_kleene(source_text, p)
                }
                ast::SupportsConditionType::Or(o) => {
                    eval_supports_in_parens_kleene(source_text, &o.condition)
                }
                ast::SupportsConditionType::And(a) => {
                    eval_supports_in_parens_kleene(source_text, &a.condition)
                }
                ast::SupportsConditionType::Not(n) => {
                    eval_supports_in_parens_kleene(source_text, &n.condition).not()
                }
            };
            acc = acc.or(val);
        }
        acc
    } else {
        let mut acc = SupportsTruth::True;
        for item in &cond.conditions {
            let val = match item {
                ast::SupportsConditionType::SupportsInParens(p) => {
                    eval_supports_in_parens_kleene(source_text, p)
                }
                ast::SupportsConditionType::And(a) => {
                    eval_supports_in_parens_kleene(source_text, &a.condition)
                }
                ast::SupportsConditionType::Or(o) => {
                    eval_supports_in_parens_kleene(source_text, &o.condition)
                }
                ast::SupportsConditionType::Not(n) => {
                    eval_supports_in_parens_kleene(source_text, &n.condition).not()
                }
            };
            acc = acc.and(val);
        }
        acc
    }
}

fn eval_supports_in_parens_kleene(source_text: &str, p: &ast::SupportsInParens) -> SupportsTruth {
    match p {
        ast::SupportsInParens::SupportsCondition(sc) => {
            eval_supports_condition_kleene(source_text, sc)
        }
        ast::SupportsInParens::Feature(feat) => match feat {
            ast::SupportsFeature::Declaration(decl) => {
                SupportsTruth::from_bool(eval_supports_declaration(decl))
            }
            ast::SupportsFeature::Function(func) => {
                if func.name.as_str().eq_ignore_ascii_case("selector") {
                    SupportsTruth::from_bool(eval_supports_selector_fn(source_text, func))
                } else {
                    SupportsTruth::Unknown
                }
            }
        },
        ast::SupportsInParens::GeneralEnclosed(ge) => match ge {
            // Invalid `selector(...)` arguments fall back to `GeneralEnclosed::Function`
            // in `swc_css_parser`, and must evaluate to definite `False` (so
            // `@supports not selector(:unknown)` is `true` per CSS Conditional 4!).
            ast::GeneralEnclosed::Function(func)
                if func.name.as_str().eq_ignore_ascii_case("selector") =>
            {
                SupportsTruth::False
            }
            _ => SupportsTruth::Unknown,
        },
    }
}

fn eval_supports_declaration(decl: &ast::Declaration) -> bool {
    let prop_name = declaration_property_name(&decl.name);
    let raw_val = serialize_component_values(&decl.value);
    if !has_valid_var_functions_in_cvs(&decl.value)
        || (contains_var_function(&raw_val) && !has_valid_var_syntax(&raw_val))
    {
        return false;
    }
    match properties::kind(&prop_name) {
        PropertyKind::Custom => true,
        PropertyKind::Longhand(_) | PropertyKind::Shorthand => {
            is_valid_property_value(&prop_name, &raw_val)
        }
        PropertyKind::Unsupported | PropertyKind::Unknown => false,
    }
}

fn eval_supports_selector_fn(source_text: &str, func: &ast::Function) -> bool {
    let non_ws: Vec<&ast::ComponentValue> = func
        .value
        .iter()
        .filter(|cv| !is_whitespace_cv(cv))
        .collect();
    if non_ws.len() != 1 {
        return false;
    }
    let ast::ComponentValue::ComplexSelector(cs) = non_ws[0] else {
        return false;
    };

    // Guard against `selector(a, b)` if `swc_css_parser` stopped at `,`: verify
    // only whitespace/comments remain between `cs.span.hi` and the closing `)`.
    if !cs.span.is_dummy() && !func.span.is_dummy() && func.span.hi > BytePos(1) {
        let start = cs.span.hi.0.saturating_sub(1) as usize;
        let end = (func.span.hi.0.saturating_sub(2)) as usize;
        if start <= end
            && let Some(tail) = source_text.get(start..end)
            && tail.contains(',')
        {
            return false;
        }
    }

    is_supported_complex_selector(cs)
}

fn parse_keyframes_rule(
    ctx: &ParseCtx<'_>,
    kf_name: &ast::KeyframesName,
    block: &ast::SimpleBlock,
    media_stack: &[MediaQueryList],
) -> Option<KeyframesRule> {
    let name = match kf_name {
        ast::KeyframesName::CustomIdent(ci) => {
            let s = ci.value.as_str();
            if s.eq_ignore_ascii_case("none") || is_css_wide_ident(s) {
                return None;
            }
            s.to_owned()
        }
        ast::KeyframesName::Str(s) => {
            if s.value.is_empty() {
                return None;
            }
            s.value.to_string()
        }
        ast::KeyframesName::PseudoPrefix(_) | ast::KeyframesName::PseudoFunction(_) => return None,
    };

    let mut keyframes = Vec::new();
    for cv in &block.value {
        let ast::ComponentValue::KeyframeBlock(kb) = cv else {
            continue;
        };
        let mut offsets = Vec::new();
        let mut selector_valid = !kb.prelude.is_empty();
        for sel in &kb.prelude {
            match sel {
                ast::KeyframeSelector::Ident(id) => {
                    if id.value.eq_ignore_ascii_case("from") {
                        offsets.push(0.0f32);
                    } else if id.value.eq_ignore_ascii_case("to") {
                        offsets.push(1.0f32);
                    } else {
                        selector_valid = false;
                    }
                }
                ast::KeyframeSelector::Percentage(pct) => {
                    let v = pct.value.value as f32;
                    if (0.0..=100.0).contains(&v) {
                        offsets.push(v / 100.0);
                    } else {
                        selector_valid = false;
                    }
                }
            }
        }
        if !selector_valid {
            continue;
        }

        let mut declarations = Vec::new();
        let mut easing = None;
        for inner_cv in &kb.block.value {
            let ast::ComponentValue::Declaration(decl) = inner_cv else {
                continue;
            };
            // CSS Animations 1 §3: `!important` declarations inside `@keyframes` are ignored.
            if decl.important.is_some() {
                continue;
            }
            let prop_name = declaration_property_name(&decl.name);
            let val_str = serialize_component_values(&decl.value);
            if !has_valid_var_functions_in_cvs(&decl.value)
                || (contains_var_function(&val_str) && !has_valid_var_syntax(&val_str))
            {
                continue;
            }
            if prop_name == "animation-timing-function" {
                if let Some(parsed_easing) = parse_easing_value(&val_str) {
                    easing = Some(parsed_easing);
                }
            } else {
                declarations.push((prop_name, val_str));
            }
        }

        for offset in offsets {
            keyframes.push(Keyframe {
                offset,
                declarations: declarations.clone(),
                easing,
            });
        }
    }

    let _ = ctx;
    keyframes.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    Some(KeyframesRule {
        name,
        keyframes,
        media: media_stack.to_vec(),
    })
}

fn parse_easing_value(raw: &str) -> Option<Easing> {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "linear" => return Some(Easing::Linear),
        "ease" => return Some(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)),
        "ease-in" => return Some(Easing::CubicBezier(0.42, 0.0, 1.0, 1.0)),
        "ease-out" => return Some(Easing::CubicBezier(0.0, 0.0, 0.58, 1.0)),
        "ease-in-out" => return Some(Easing::CubicBezier(0.42, 0.0, 0.58, 1.0)),
        "step-start" => return Some(Easing::Steps(1, true)),
        "step-end" => return Some(Easing::Steps(1, false)),
        _ => {}
    }

    if let Some(inner) = s
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let nums: Option<Vec<f32>> = inner
            .split(',')
            .map(|p| p.trim().parse::<f32>().ok())
            .collect();
        let nums = nums?;
        if nums.len() == 4
            && nums.iter().all(|v| v.is_finite())
            && (0.0..=1.0).contains(&nums[0])
            && (0.0..=1.0).contains(&nums[2])
        {
            return Some(Easing::CubicBezier(nums[0], nums[1], nums[2], nums[3]));
        }
        return None;
    }

    if let Some(inner) = s
        .strip_prefix("steps(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.is_empty() || parts.len() > 2 {
            return None;
        }
        let count = parts[0].parse::<u32>().ok().filter(|&n| n >= 1)?;
        let jump_start = match parts.get(1).copied() {
            None | Some("end" | "jump-end") => false,
            Some("start" | "jump-start") => true,
            _ => return None,
        };
        return Some(Easing::Steps(count, jump_start));
    }

    None
}

fn parse_font_face_rule(
    ctx: &ParseCtx<'_>,
    block: &ast::SimpleBlock,
    media_stack: &[MediaQueryList],
) -> Option<FontFaceRule> {
    let mut family: Option<String> = None;
    let mut sources: Vec<String> = Vec::new();
    let mut weight: Option<u16> = None;
    let mut italic = false;

    for cv in &block.value {
        let ast::ComponentValue::Declaration(decl) = cv else {
            continue;
        };
        if decl.important.is_some() {
            continue;
        }
        let desc_name = declaration_property_name(&decl.name);
        match desc_name.as_str() {
            "font-family" => {
                if let Some(fam) = extract_font_family(&decl.value) {
                    family = Some(fam);
                }
            }
            "src" => {
                let mut urls = Vec::new();
                collect_urls_in_cvs(&decl.value, &mut urls);
                sources = urls;
            }
            "font-weight" => {
                let val = serialize_component_values(&decl.value);
                if let Some(w) = parse_font_weight_descriptor(&val) {
                    weight = Some(w);
                }
            }
            "font-style" => {
                let val = serialize_component_values(&decl.value);
                let first = val.split_ascii_whitespace().next().unwrap_or("");
                if first.eq_ignore_ascii_case("italic") || first.eq_ignore_ascii_case("oblique") {
                    italic = true;
                } else if first.eq_ignore_ascii_case("normal") {
                    italic = false;
                }
            }
            _ => {}
        }
    }

    let family = family.filter(|f| !f.is_empty())?;
    Some(FontFaceRule {
        font_face: FontFace {
            family,
            sources,
            weight,
            italic,
            base_url: ctx.base_url.map(str::to_owned),
        },
        media: media_stack.to_vec(),
    })
}

fn extract_font_family(cvs: &[ast::ComponentValue]) -> Option<String> {
    let non_ws: Vec<&ast::ComponentValue> = cvs.iter().filter(|c| !is_whitespace_cv(c)).collect();
    if non_ws.is_empty() {
        return None;
    }
    if non_ws.len() == 1
        && let ast::ComponentValue::Str(s) = non_ws[0]
    {
        return Some(s.value.trim().to_owned());
    }
    let raw = serialize_component_values(cvs);
    let trimmed = raw.trim();
    if trimmed.contains(',') {
        return None;
    }
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed);
    let collapsed = unquoted
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

fn collect_urls_in_cvs(cvs: &[ast::ComponentValue], out: &mut Vec<String>) {
    for cv in cvs {
        match cv {
            ast::ComponentValue::Url(u) => {
                if let Some(s) = extract_url_node_string(u) {
                    out.push(s);
                }
            }
            ast::ComponentValue::Function(func)
                if func.name.as_str().eq_ignore_ascii_case("url") =>
            {
                let inner = serialize_component_values(&func.value);
                let trimmed = inner.trim();
                let unquoted = trimmed
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| {
                        trimmed
                            .strip_prefix('\'')
                            .and_then(|s| s.strip_suffix('\''))
                    })
                    .unwrap_or(trimmed);
                if !unquoted.is_empty() {
                    out.push(unquoted.to_owned());
                }
            }
            ast::ComponentValue::PreservedToken(ts) => {
                if let ast::Token::Url { value, .. } = &ts.token {
                    let s = value.trim();
                    if !s.is_empty() {
                        out.push(s.to_owned());
                    }
                }
            }
            ast::ComponentValue::ListOfComponentValues(locv) => {
                collect_urls_in_cvs(&locv.children, out);
            }
            _ => {}
        }
    }
}

fn extract_url_node_string(u: &ast::Url) -> Option<String> {
    let val = u.value.as_deref()?;
    let s = match val {
        ast::UrlValue::Str(st) => st.value.to_string(),
        ast::UrlValue::Raw(raw) => raw.value.trim().to_owned(),
    };
    (!s.is_empty()).then_some(s)
}

fn parse_font_weight_descriptor(raw: &str) -> Option<u16> {
    let first = raw.split_ascii_whitespace().next()?;
    if first.eq_ignore_ascii_case("normal") {
        Some(400)
    } else if first.eq_ignore_ascii_case("bold") {
        Some(700)
    } else {
        let num = first.parse::<f32>().ok()?;
        if (1.0..=1000.0).contains(&num) {
            Some(num.round() as u16)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Declaration Parsing, Serialization & Validation
// ---------------------------------------------------------------------------

fn declaration_property_name(name: &ast::DeclarationName) -> String {
    match name {
        ast::DeclarationName::Ident(id) => id.value.as_str().to_ascii_lowercase(),
        // Custom properties (`--foo`) are case-sensitive per CSS Custom Properties 1 §2.
        // `swc_css_ast::DashedIdent` stores the identifier without the leading `--`.
        ast::DeclarationName::DashedIdent(di) => format!("--{}", di.value),
    }
}

fn process_declaration(ctx: &mut ParseCtx<'_>, decl: &ast::Declaration, out: &mut Vec<ParsedDecl>) {
    let line = ctx.line_of(decl.span);
    let prop_name = declaration_property_name(&decl.name);
    let important = decl.important.is_some();

    match properties::kind(&prop_name) {
        PropertyKind::Unknown => {
            if ctx.seen_unknown_props.insert(prop_name.clone()) {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    format!("unknown property `{prop_name}`"),
                );
            }
        }
        PropertyKind::Unsupported => {
            if ctx.seen_unsupported_props.insert(prop_name.clone()) {
                ctx.report(
                    Severity::Unsupported,
                    Some(line),
                    format!("unsupported property `{prop_name}`"),
                );
            }
        }
        PropertyKind::Custom => {
            let raw_val = serialize_component_values(&decl.value);
            if !has_valid_var_functions_in_cvs(&decl.value)
                || (contains_var_function(&raw_val) && !has_valid_var_syntax(&raw_val))
            {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    format!("invalid `var()` syntax in `{prop_name}: {raw_val}`"),
                );
                return;
            }
            let custom_val = match CssWide::parse(&raw_val) {
                Some(kw) => CustomDeclValue::Wide(kw),
                None => CustomDeclValue::Tokens(raw_val),
            };
            out.push(ParsedDecl {
                value: DeclValue::Custom {
                    name: prop_name,
                    value: custom_val,
                },
                important,
            });
        }
        PropertyKind::Longhand(_) | PropertyKind::Shorthand => {
            let raw_val = serialize_component_values(&decl.value);
            if raw_val.is_empty() {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    format!("empty value for property `{prop_name}`"),
                );
                return;
            }

            if let Some(kw) = CssWide::parse(&raw_val) {
                for l in properties::longhands_of(&prop_name) {
                    out.push(ParsedDecl {
                        value: DeclValue::Wide(l, kw),
                        important,
                    });
                }
                return;
            }

            if !has_valid_var_functions_in_cvs(&decl.value) {
                ctx.report(
                    Severity::Invalid,
                    Some(line),
                    format!("invalid value `{raw_val}` for property `{prop_name}`"),
                );
                return;
            }

            if contains_var_function(&raw_val) {
                if !has_valid_var_syntax(&raw_val) {
                    ctx.report(
                        Severity::Invalid,
                        Some(line),
                        format!("invalid value `{raw_val}` for property `{prop_name}`"),
                    );
                    return;
                }
                for target in properties::longhands_of(&prop_name) {
                    out.push(ParsedDecl {
                        value: DeclValue::PendingVar {
                            target,
                            prop_name: prop_name.clone(),
                            raw_value: raw_val.clone(),
                        },
                        important,
                    });
                }
                return;
            }

            match expand_and_validate(&prop_name, &raw_val) {
                Ok(expanded) => {
                    for (l, v) in expanded {
                        out.push(ParsedDecl {
                            value: DeclValue::Expanded(l, v),
                            important,
                        });
                    }
                }
                Err(_) => {
                    ctx.report(
                        Severity::Invalid,
                        Some(line),
                        format!("invalid value `{raw_val}` for property `{prop_name}`"),
                    );
                }
            }
        }
    }
}

/// Validates any `var(...)` function nodes directly in the AST component value list.
fn has_valid_var_functions_in_cvs(cvs: &[ast::ComponentValue]) -> bool {
    cvs.iter().all(has_valid_var_functions_in_cv)
}

fn has_valid_var_functions_in_cv(cv: &ast::ComponentValue) -> bool {
    match cv {
        ast::ComponentValue::Function(func) => {
            if func.name.as_str().eq_ignore_ascii_case("var") && !is_valid_ast_var_function(func) {
                return false;
            }
            has_valid_var_functions_in_cvs(&func.value)
        }
        ast::ComponentValue::SimpleBlock(sb) => has_valid_var_functions_in_cvs(&sb.value),
        ast::ComponentValue::ListOfComponentValues(locv) => {
            has_valid_var_functions_in_cvs(&locv.children)
        }
        _ => true,
    }
}

/// Per CSS Custom Properties 1 §3: `var() = var( <custom-property-name> [, <declaration-value>? ]? )`.
/// In the AST, the tokens before the first `,` delimiter (ignoring whitespace) must be a single
/// valid `<dashed-ident>` (`--*`).
fn is_valid_ast_var_function(func: &ast::Function) -> bool {
    let mut before_comma = Vec::new();
    for cv in &func.value {
        if is_whitespace_cv(cv) {
            continue;
        }
        let is_comma = matches!(
            cv,
            ast::ComponentValue::Delimiter(d) if d.value == ast::DelimiterValue::Comma
        ) || matches!(
            cv,
            ast::ComponentValue::PreservedToken(ts) if matches!(ts.token, ast::Token::Comma)
        );
        if is_comma {
            break;
        }
        before_comma.push(cv);
    }

    if before_comma.len() != 1 {
        return false;
    }

    match before_comma[0] {
        ast::ComponentValue::DashedIdent(di) => {
            let s = di.value.as_str();
            let full = if s.starts_with("--") {
                s.to_owned()
            } else {
                format!("--{s}")
            };
            is_valid_custom_prop_ident(&full)
        }
        ast::ComponentValue::Ident(id) => is_valid_custom_prop_ident(id.value.as_str()),
        ast::ComponentValue::PreservedToken(ts) => match &ts.token {
            ast::Token::Ident { value, .. } => is_valid_custom_prop_ident(value.as_str()),
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn serialize_component_values(cvs: &[ast::ComponentValue]) -> String {
    if cvs.is_empty() {
        return String::new();
    }
    let mut buf = String::new();
    let locv = ast::ListOfComponentValues {
        span: swc_common::DUMMY_SP,
        children: cvs.to_vec(),
    };
    let writer = BasicCssWriter::new(&mut buf, None, BasicCssWriterConfig::default());
    let mut codegen = CodeGenerator::new(writer, CodegenConfig { minify: false });
    if codegen.emit(&locv).is_err() {
        return String::new();
    }
    buf.trim().to_owned()
}

pub(crate) fn is_valid_property_value(prop_name: &str, raw_val: &str) -> bool {
    let trimmed = raw_val.trim();
    if trimmed.is_empty() {
        return false;
    }
    if CssWide::parse(trimmed).is_some() {
        return true;
    }
    if contains_var_function(trimmed) {
        return has_valid_var_syntax(trimmed);
    }
    expand_and_validate(prop_name, trimmed).is_ok()
}

fn expand_and_validate(prop_name: &str, raw_val: &str) -> Result<Vec<(Longhand, String)>, String> {
    let expanded = properties::expand(prop_name, raw_val)?;
    if expanded.is_empty() {
        return Err("empty expansion".to_owned());
    }
    let default_vp = Viewport::default();
    let probe_ctx = ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: &default_vp,
    };
    let mut probe_style = ComputedStyle::default();
    for (l, val) in &expanded {
        properties::compute(&mut probe_style, *l, val, &probe_ctx)?;
    }
    Ok(expanded)
}

/// Returns true if `s` contains an unquoted `var(` function token.
pub(crate) fn contains_var_function(s: &str) -> bool {
    find_next_var_call(s, 0).is_some()
}

/// Finds the byte index of the next unquoted `var(` call in `s` starting at `from`.
pub(crate) fn find_next_var_call(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    let mut in_quote: Option<u8> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_quote = Some(b);
            i += 1;
            continue;
        }
        if i + 4 <= bytes.len() && bytes[i..i + 4].eq_ignore_ascii_case(b"var(") {
            let preceded_by_ident = i > 0 && is_css_ident_byte(bytes[i - 1]);
            if !preceded_by_ident {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Validates that every `var(...)` call in `s` has balanced parentheses and
/// starts with a `--*` custom property name before `,` or `)`.
pub(crate) fn has_valid_var_syntax(s: &str) -> bool {
    let mut pos = 0usize;
    while let Some(var_idx) = find_next_var_call(s, pos) {
        let args_start = var_idx + 4;
        let Some((first_arg, close_idx)) = parse_var_call_bounds(s, args_start) else {
            return false;
        };
        let var_name = first_arg.trim();
        if !is_valid_custom_prop_ident(var_name) {
            return false;
        }
        // Also validate any nested `var(...)` inside the fallback argument!
        if !has_valid_var_syntax(&s[args_start..close_idx]) {
            return false;
        }
        pos = close_idx + 1;
    }
    true
}

/// Given `s` and `args_start` (immediately after `var(`), finds the matching `)`
/// and splits out the first argument (before any top-level `,`).
pub(crate) fn parse_var_call_bounds(s: &str, args_start: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut i = args_start;
    let mut depth = 1i32;
    let mut in_quote: Option<u8> = None;
    let mut first_comma: Option<usize> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => in_quote = Some(b),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let first_end = first_comma.unwrap_or(i);
                    return Some((&s[args_start..first_end], i));
                }
            }
            b',' if depth == 1 && first_comma.is_none() => {
                first_comma = Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_valid_custom_prop_ident(name: &str) -> bool {
    name.len() >= 3 && name.starts_with("--") && name[2..].bytes().all(is_css_ident_byte)
}

fn is_css_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80
}

fn is_css_wide_ident(s: &str) -> bool {
    CssWide::parse(s).is_some()
}

fn is_whitespace_cv(cv: &ast::ComponentValue) -> bool {
    matches!(
        cv,
        ast::ComponentValue::PreservedToken(ts)
            if matches!(ts.token, ast::Token::WhiteSpace { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules_supports_layers_keyframes_font_face_and_nesting() {
        let css = r#"
            @charset "UTF-8";
            @layer reset, base;
            @import "theme.css" layer(base) screen and (min-width: 600px);

            @-webkit-keyframes ignored-vendor { from { color: red; } }
            @namespace svg url(http://www.w3.org/2000/svg);

            @supports (display: flex) and selector(div:has(> p)) and (not (color: not-a-color)) {
                @layer base {
                    .card {
                        display: flex;
                        color: red;
                        &:hover {
                            color: blue !important;
                        }
                        .title {
                            font-size: 20px;
                        }
                    }
                }
            }

            @supports (unknown-prop: 123) or selector(::selection) {
                .never { display: none; }
            }

            @keyframes fade {
                from { color: black; animation-timing-function: ease-in; }
                50% { color: gray !important; margin-top: 10px; }
                to { color: white; animation-timing-function: steps(4, jump-start); }
            }

            @font-face {
                font-family: "Inter UI";
                src: url("fonts/inter.woff2") format("woff2"), url(fonts/inter.ttf);
                font-weight: 600;
                font-style: italic;
            }
        "#;

        let (sheet, diags) = parse_stylesheet(
            css,
            Origin::Author,
            "app.css",
            Some("https://example.com/css/app.css"),
        );

        // Only `@namespace` produces an Unsupported diagnostic; `@-webkit-keyframes` is silent
        // and the false `@supports` block is skipped without diagnostics.
        assert_eq!(diags.len(), 1, "unexpected diagnostics: {diags:?}");
        assert_eq!(diags[0].severity, Severity::Unsupported);

        assert_eq!(sheet.inner.imports.len(), 1);
        assert_eq!(sheet.inner.imports[0].href, "theme.css");
        assert_eq!(
            sheet.inner.imports[0].layer,
            Some(ImportLayer::Named(vec!["base".to_owned()]))
        );

        // 3 rules from `.card`, `&:hover`, and `.title`.
        assert_eq!(sheet.inner.rules.len(), 3);
        assert_eq!(sheet.inner.keyframes.len(), 1);
        let kf = &sheet.inner.keyframes[0];
        assert_eq!(kf.name, "fade");
        assert_eq!(kf.keyframes.len(), 3);
        assert_eq!(kf.keyframes[0].offset, 0.0);
        assert_eq!(
            kf.keyframes[0].easing,
            Some(Easing::CubicBezier(0.42, 0.0, 1.0, 1.0))
        );
        // `color: gray !important` was ignored inside `@keyframes`, keeping `margin-top: 10px`.
        assert_eq!(
            kf.keyframes[1].declarations,
            vec![("margin-top".to_owned(), "10px".to_owned())]
        );
        assert_eq!(kf.keyframes[2].easing, Some(Easing::Steps(4, true)));

        assert_eq!(sheet.inner.font_faces.len(), 1);
        let ff = &sheet.inner.font_faces[0].font_face;
        assert_eq!(ff.family, "Inter UI");
        assert_eq!(
            ff.sources,
            vec!["fonts/inter.woff2".to_owned(), "fonts/inter.ttf".to_owned()]
        );
        assert_eq!(ff.weight, Some(600));
        assert!(ff.italic);
        assert_eq!(
            ff.base_url.as_deref(),
            Some("https://example.com/css/app.css")
        );
    }

    #[test]
    fn error_recovery_and_diagnostic_deduplication() {
        let css = r#"
            .ok {
                color: green;
                color: definitely-not-a-color;
                not-a-prop: 1px;
                not-a-prop: 2px;
                -webkit-appearance: none;
                -webkit-appearance: button;
            }
            @import "late.css";
            ??bad selector?? { color: red; }
            .after { display: block; }
        "#;

        let (sheet, diags) = parse_stylesheet(css, Origin::Author, "broken.css", None);
        assert_eq!(sheet.inner.rules.len(), 2);
        assert_eq!(sheet.inner.rules[0].declarations.len(), 1);
        assert!(sheet.inner.imports.is_empty());

        // 1 invalid value + 1 deduplicated unknown prop + 1 deduplicated unsupported prop
        // + 1 misplaced @import + 1 bad selector = 5 diagnostics.
        assert_eq!(diags.len(), 5, "diagnostics: {diags:?}");

        // UA origin suppresses all diagnostics.
        let (_ua_sheet, ua_diags) = parse_stylesheet(css, Origin::UserAgent, "ua.css", None);
        assert!(ua_diags.is_empty());
    }
}
