//! CSS for html-2-qml: parsing (swc_css_parser), selector matching against
//! an [`h2q_dom::Document`], the cascade, inheritance and computed values.
//!
//! The public API in this file is the contract the converter is written
//! against. Add to it freely; change or remove items only with the lead's
//! agreement.

pub mod values;

pub use values::*;

use std::collections::BTreeMap;

use h2q_dom::{Document, NodeId};

/// Where a style sheet came from (CSS Cascade 4 §6.2). User style sheets
/// are not modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    UserAgent,
    Author,
}

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Valid CSS this converter cannot express in QML (or only roughly).
    Unsupported,
    /// Invalid CSS a browser would drop too.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Where the style sheet came from: a file path/URL, `<style>#2`, or
    /// `style attribute of <div id=x>`.
    pub source: String,
    /// 1-based line within that source, when known.
    pub line: Option<u32>,
    pub message: String,
}

/// A parsed style sheet, ready for matching. Opaque: build it with
/// [`Stylesheet::parse`].
#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub origin: Origin,
    /// Base for resolving relative `url()`s in this sheet.
    pub base_url: Option<String>,
    /// Name used in diagnostics.
    pub source_name: String,
    pub(crate) inner: parse::Sheet,
}

impl Stylesheet {
    /// Parses CSS text with CSS Syntax 3 error recovery: invalid
    /// declarations and rules are dropped (and reported), never the sheet.
    #[must_use]
    pub fn parse(
        text: &str,
        origin: Origin,
        source_name: &str,
        base_url: Option<&str>,
    ) -> (Self, Vec<Diagnostic>) {
        parse::parse_stylesheet(text, origin, source_name, base_url)
    }
}

/// The browser's default style sheet: the CSS from the WHATWG HTML
/// "Rendering" section (§15), trimmed to what the converter can use.
#[must_use]
pub fn user_agent_stylesheet() -> &'static Stylesheet {
    ua::stylesheet()
}

/// The environment media queries and viewport units are evaluated in.
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub prefers_dark: bool,
    pub prefers_reduced_motion: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            prefers_dark: false,
            prefers_reduced_motion: false,
        }
    }
}

/// Fetches a linked style sheet's text by its (unresolved) `href` and the
/// document's base URL. `None` when it cannot be read; that is reported.
pub trait StylesheetLoader {
    fn load(&mut self, href: &str) -> Option<LoadedSheet>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSheet {
    pub text: String,
    /// Resolved URL or path, used as the sheet's base and source name.
    pub url: String,
}

/// Collects the author style sheets of `doc` in document order: `<style>`
/// elements and `<link rel="stylesheet" href>` (through `loader`), honouring
/// their `media` attribute against `viewport`, following `@import`
/// (through `loader`, cycles broken). `style` attributes are not sheets;
/// [`compute_styles`] reads them.
#[must_use]
pub fn collect_stylesheets(
    doc: &Document,
    loader: &mut dyn StylesheetLoader,
    viewport: &Viewport,
) -> (Vec<Stylesheet>, Vec<Diagnostic>) {
    collect::collect_stylesheets(doc, loader, viewport)
}

/// Dynamic pseudo-classes the converter can turn into QML states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PseudoState {
    Hover,
    Active,
    Focus,
    FocusVisible,
    FocusWithin,
    Checked,
    Disabled,
    Visited,
}

/// The styles of `node`'s subtree when `node` is in `state`, as far as they
/// differ from the base styles.
#[derive(Debug, Clone, PartialEq)]
pub struct StateVariant {
    /// Element whose state is the condition (may be an ancestor of the
    /// styled element: `.card:hover .title`).
    pub node: NodeId,
    pub state: PseudoState,
    /// Every element in `node`'s subtree (including `node`) whose computed
    /// style differs in that state, with its full style in that state.
    pub changed: Vec<(NodeId, Box<ComputedStyle>)>,
}

/// All computed styles of one element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementStyle {
    pub style: ComputedStyle,
    /// `::before` / `::after`, only when their `content` is not `none`.
    pub before: Option<Box<ComputedStyle>>,
    pub after: Option<Box<ComputedStyle>>,
    /// `::marker` content for list items is derived by the converter from
    /// `list_style_*`; `::placeholder` styles inputs' placeholder text.
    pub placeholder: Option<Box<ComputedStyle>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// 0.0 ..= 1.0
    pub offset: f32,
    /// Declarations at this keyframe as computed-style deltas: property
    /// name (CSS longhand) and the computed style with them applied on top
    /// of the animated element's base style is produced by
    /// [`StyleMap::keyframe_style`]. Here they are kept as text.
    pub declarations: Vec<(String, String)>,
    pub easing: Option<Easing>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub family: String,
    /// `url()`s in `src`, in order, unresolved.
    pub sources: Vec<String>,
    pub weight: Option<u16>,
    pub italic: bool,
    pub base_url: Option<String>,
}

/// The result of the cascade over a whole document.
#[derive(Debug, Clone, Default)]
pub struct StyleMap {
    /// Indexed by `NodeId::index()`; `None` for non-elements and for
    /// elements under a `display: none` ancestor (they have no box).
    pub(crate) styles: Vec<Option<ElementStyle>>,
    pub(crate) parent_elements: Vec<Option<NodeId>>,
    pub(crate) element_cascades: Vec<Option<cascade::ElementCascade>>,
    pub(crate) root_font_size: f32,
    pub(crate) viewport: Viewport,
    pub variants: Vec<StateVariant>,
    pub keyframes: BTreeMap<String, Vec<Keyframe>>,
    pub font_faces: Vec<FontFace>,
    pub diagnostics: Vec<Diagnostic>,
}

impl StyleMap {
    /// Computed styles of an element (`None` for text, comments, and
    /// elements that are or are inside `display: none`).
    #[must_use]
    pub fn get(&self, node: NodeId) -> Option<&ElementStyle> {
        self.styles.get(node.index()).and_then(Option::as_ref)
    }

    #[must_use]
    pub fn style(&self, node: NodeId) -> Option<&ComputedStyle> {
        self.get(node).map(|s| &s.style)
    }

    /// Variants whose condition element is `node`.
    pub fn variants_of(&self, node: NodeId) -> impl Iterator<Item = &StateVariant> {
        self.variants.iter().filter(move |v| v.node == node)
    }

    /// The computed style of `node` with a keyframe's declarations applied.
    #[must_use]
    pub fn keyframe_style(&self, node: NodeId, keyframe: &Keyframe) -> Option<ComputedStyle> {
        cascade::keyframe_style(self, node, keyframe)
    }
}

/// Runs the cascade: the UA sheet, then `sheets` in order, then each
/// element's `style` attribute; `!important` reversed per origin; ties by
/// specificity then order. Then inheritance and computed values.
#[must_use]
pub fn compute_styles(doc: &Document, sheets: &[Stylesheet], viewport: &Viewport) -> StyleMap {
    cascade::compute_styles(doc, sheets, viewport)
}

/// Elements matching `selector` in document order, as `querySelectorAll`
/// would return them for a page in its initial state (no hover/focus/active).
/// `None` when the selector does not parse.
#[must_use]
pub fn query_selector_all(doc: &Document, selector: &str) -> Option<Vec<NodeId>> {
    let list = selectors::parse_selector_list_str(selector);
    if list.invalid {
        return None;
    }
    let ctx = selectors::MatchContext {
        doc,
        active_state: None,
    };
    let mut out = Vec::new();
    for node in doc.descendants(doc.root()) {
        if doc.element(node).is_some()
            && list
                .selectors
                .iter()
                .any(|sel| selectors::matches_selector(sel, node, &ctx))
        {
            out.push(node);
        }
    }
    Some(out)
}

/// Whether `node` matches `selector` (initial state). `None` when it does not parse.
#[must_use]
pub fn element_matches(doc: &Document, node: NodeId, selector: &str) -> Option<bool> {
    let list = selectors::parse_selector_list_str(selector);
    if list.invalid {
        return None;
    }
    let ctx = selectors::MatchContext {
        doc,
        active_state: None,
    };
    Some(
        list.selectors
            .iter()
            .any(|sel| selectors::matches_selector(sel, node, &ctx)),
    )
}

mod cascade;
mod collect;
mod media;
mod parse;
mod properties;
mod selectors;
mod ua;
