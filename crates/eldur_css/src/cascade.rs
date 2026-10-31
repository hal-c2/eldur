//! CSS Cascade Level 5, custom properties & `var()` substitution, inheritance,
//! computed-value fix-ups (CSS Display 3 §2.7), pseudo-elements, and state variants.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use eldur_dom::{Document, Element, NodeId};

use crate::media::MediaQueryList;
use crate::parse::{
    CustomDeclValue, DeclValue, LayerSegment, ParsedDecl, contains_var_function,
    find_next_var_call, parse_inline_style, parse_var_call_bounds,
};
use crate::properties::{self, ComputeContext, CssWide, Longhand, PropertyKind};
use crate::selectors::{
    DynamicFacts, MatchContext, PseudoElement, RuleIndex, Selector, Specificity,
    compute_ancestor_blooms, matches_selector,
};
use crate::{
    ComputedStyle, Diagnostic, Display, ElementStyle, Float, FontFace, Keyframe, Origin, Position,
    PseudoState, StateVariant, StyleMap, Stylesheet, Viewport, ua,
};

const MAX_VAR_EXPANSION_BYTES: usize = 65_536;

/// Known CSS property names probed via `properties::longhands_of` so every
/// longhand implemented by `properties.rs` participates in inheritance and
/// `initial` resets without matching on `Longhand` enum variants.
const PROBED_CSS_PROPERTIES: &[&str] = &[
    "display",
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "z-index",
    "float",
    "box-sizing",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "aspect-ratio",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-width",
    "border-style",
    "border-color",
    "border-radius",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "outline",
    "outline-width",
    "outline-style",
    "outline-color",
    "outline-offset",
    "flex",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "flex-direction",
    "flex-wrap",
    "flex-flow",
    "justify-content",
    "align-items",
    "align-self",
    "align-content",
    "justify-items",
    "justify-self",
    "gap",
    "row-gap",
    "column-gap",
    "order",
    "grid-template-columns",
    "grid-template-rows",
    "grid-template-areas",
    "grid-auto-flow",
    "grid-auto-columns",
    "grid-auto-rows",
    "grid-column",
    "grid-column-start",
    "grid-column-end",
    "grid-row",
    "grid-row-start",
    "grid-row-end",
    "background",
    "background-color",
    "background-image",
    "background-size",
    "background-position",
    "background-repeat",
    "box-shadow",
    "opacity",
    "overflow",
    "overflow-x",
    "overflow-y",
    "transform",
    "transform-origin",
    "object-fit",
    "filter",
    "color",
    "font",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-variant",
    "line-height",
    "letter-spacing",
    "word-spacing",
    "text-align",
    "text-indent",
    "text-transform",
    "white-space",
    "word-break",
    "overflow-wrap",
    "word-wrap",
    "text-shadow",
    "direction",
    "visibility",
    "cursor",
    "pointer-events",
    "list-style",
    "list-style-type",
    "list-style-position",
    "list-style-image",
    "user-select",
    "text-decoration",
    "text-decoration-line",
    "text-decoration-color",
    "text-decoration-style",
    "text-overflow",
    "vertical-align",
    "transition",
    "animation",
    "content",
    "counter-reset",
    "counter-increment",
    "counter-set",
    "border-collapse",
    "border-spacing",
    "caption-side",
    "table-layout",
];

fn statically_known_longhands() -> &'static [Longhand] {
    static KNOWN: OnceLock<Vec<Longhand>> = OnceLock::new();
    KNOWN.get_or_init(|| {
        let mut set = BTreeSet::new();
        for &name in PROBED_CSS_PROPERTIES {
            for l in properties::longhands_of(name) {
                set.insert(l);
            }
        }
        set.into_iter().collect()
    })
}

// ---------------------------------------------------------------------------
// Cascade Layer Ordering (CSS Cascade Level 5 §6.4.3)
// ---------------------------------------------------------------------------

/// Lexicographic layer sort key `[idx_0, idx_1, ..., u32::MAX]`.
///
/// Using `u32::MAX` as the terminator means:
/// - Sublayer `A.B` (`[0, 0, MAX]`) sorts BEFORE direct rules in `A` (`[0, MAX]`).
/// - Earlier layer `A` (`[0, MAX]`) sorts BEFORE later layer `C` (`[1, MAX]`).
/// - Every layered rule (`[i, ..., MAX]` with `i < MAX`) sorts BEFORE unlayered
///   rules (`[MAX]`).
///
/// For `!important` declarations, comparing keys in reverse order inverts all three
/// relationships automatically as required by CSS Cascade 5 §6.4.3.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LayerOrderKey(Vec<u32>);

impl LayerOrderKey {
    fn unlayered() -> Self {
        Self(vec![u32::MAX])
    }
}

#[derive(Default)]
struct LayerTreeNode {
    index_by_segment: HashMap<LayerSegment, u32>,
    children: Vec<LayerTreeNode>,
}

impl LayerTreeNode {
    fn register_path(&mut self, path: &[LayerSegment]) {
        let Some((first, rest)) = path.split_first() else {
            return;
        };
        let idx = match self.index_by_segment.get(first).copied() {
            Some(existing) => existing,
            None => {
                let next_idx = self.children.len() as u32;
                self.index_by_segment.insert(first.clone(), next_idx);
                self.children.push(Self::default());
                next_idx
            }
        };
        if !rest.is_empty() {
            self.children[idx as usize].register_path(rest);
        }
    }

    fn key_for_path(&self, path: Option<&[LayerSegment]>) -> LayerOrderKey {
        let Some(segs) = path.filter(|p| !p.is_empty()) else {
            return LayerOrderKey::unlayered();
        };
        let mut out = Vec::with_capacity(segs.len() + 1);
        let mut cur = self;
        for seg in segs {
            let Some(&idx) = cur.index_by_segment.get(seg) else {
                break;
            };
            out.push(idx);
            cur = &cur.children[idx as usize];
        }
        out.push(u32::MAX);
        LayerOrderKey(out)
    }
}

// ---------------------------------------------------------------------------
// Cascade Precedence Keys & Indexed Rules
// ---------------------------------------------------------------------------

/// Origin + importance tier (CSS Cascade 5 §6.1):
/// `UaNormal < AuthorNormal < AuthorImportant < UaImportant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OriginTier {
    UaNormal = 0,
    AuthorNormal = 1,
    AuthorImportant = 2,
    UaImportant = 3,
}

impl OriginTier {
    fn of(origin: Origin, important: bool) -> Self {
        match (origin, important) {
            (Origin::UserAgent, false) => Self::UaNormal,
            (Origin::Author, false) => Self::AuthorNormal,
            (Origin::Author, true) => Self::AuthorImportant,
            (Origin::UserAgent, true) => Self::UaImportant,
        }
    }

    fn origin(self) -> Origin {
        match self {
            Self::UaNormal | Self::UaImportant => Origin::UserAgent,
            Self::AuthorNormal | Self::AuthorImportant => Origin::Author,
        }
    }

    fn is_important(self) -> bool {
        matches!(self, Self::AuthorImportant | Self::UaImportant)
    }
}

/// Identifies a cascade layer bucket for `revert-layer` rollback.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LayerBucket {
    tier: OriginTier,
    is_inline_style: bool,
    layer_key: LayerOrderKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CascadeKey {
    tier: OriginTier,
    is_inline_style: bool,
    layer_key: LayerOrderKey,
    specificity: Specificity,
    order: u32,
}

impl CascadeKey {
    fn bucket(&self) -> LayerBucket {
        LayerBucket {
            tier: self.tier,
            is_inline_style: self.is_inline_style,
            layer_key: self.layer_key.clone(),
        }
    }
}

impl Ord for CascadeKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tier
            .cmp(&other.tier)
            .then_with(|| self.is_inline_style.cmp(&other.is_inline_style))
            .then_with(|| {
                if self.tier.is_important() {
                    // `!important` declarations invert cascade layer precedence!
                    other.layer_key.cmp(&self.layer_key)
                } else {
                    self.layer_key.cmp(&other.layer_key)
                }
            })
            .then_with(|| self.specificity.cmp(&other.specificity))
            .then_with(|| self.order.cmp(&other.order))
    }
}

impl PartialOrd for CascadeKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone)]
struct IndexedRule<'a> {
    origin: Origin,
    layer_key: LayerOrderKey,
    specificity: Specificity,
    base_order: u32,
    declarations: &'a [ParsedDecl],
}

pub(crate) struct CascadeEngine<'a> {
    pub(crate) doc: &'a Document,
    viewport: &'a Viewport,
    initial_style: ComputedStyle,
    ancestor_blooms: Vec<u64>,
    /// Bloom bits of every class a condition may add (`conditional_styles`),
    /// so ancestors' assumed classes pass the filter. 0 in the base cascade.
    pub(crate) dynamic_bloom: u64,
    /// Per node (by index; empty in the base cascade): classes a condition
    /// may add to it, whose rule buckets must be visited too.
    pub(crate) dynamic_classes: Vec<Vec<String>>,
    element_rules: RuleIndex<IndexedRule<'a>>,
    pseudo_rules: RuleIndex<(PseudoElement, IndexedRule<'a>)>,
    variant_prefixes: RuleIndex<PseudoState>,
    inline_styles: Vec<Vec<ParsedDecl>>,
    /// All active longhands sorted by `(properties::priority(l), l)`.
    ordered_longhands: Vec<Longhand>,
    /// Subset of `ordered_longhands` where `properties::is_inherited(l)` is true.
    inherited_longhands: Vec<Longhand>,
}

/// Everything the cascade needs before visiting elements.
pub(crate) struct Prepared<'a> {
    pub(crate) engine: CascadeEngine<'a>,
    diagnostics: Vec<Diagnostic>,
    keyframes: BTreeMap<String, Vec<Keyframe>>,
    font_faces: Vec<FontFace>,
}

/// The base cascade's per-element results.
pub(crate) struct BasePass {
    pub(crate) styles: Vec<Option<ElementStyle>>,
    pub(crate) hidden: Vec<Option<ElementStyle>>,
    element_cascades: Vec<Option<ElementCascade>>,
    pub(crate) root_font_size: f32,
    variant_candidates: BTreeSet<(NodeId, PseudoState)>,
}

/// Runs the full CSS cascade, inheritance, computed-value fix-ups, pseudo-elements,
/// and dynamic state variants over `doc`.
pub(crate) fn compute_styles(
    doc: &Document,
    sheets: &[Stylesheet],
    viewport: &Viewport,
) -> StyleMap {
    let Prepared {
        engine,
        diagnostics,
        keyframes,
        font_faces,
    } = prepare(doc, sheets, viewport);
    let BasePass {
        styles,
        hidden,
        element_cascades,
        root_font_size,
        variant_candidates,
    } = engine.base_pass();

    // 5. Dynamic state variants (`StyleMap::variants`).
    let mut variants = Vec::new();
    for (cond_node, state) in variant_candidates {
        // An element without a box in the base style (or under a `display: none`
        // ancestor) cannot be hovered/focused/activated.
        if styles
            .get(cond_node.index())
            .and_then(Option::as_ref)
            .is_none()
        {
            continue;
        }
        let facts = DynamicFacts::single_state(cond_node, state);
        let mut variant_root_fs = root_font_size;
        let mut changed: Vec<(NodeId, Box<ComputedStyle>)> = Vec::new();
        for child in doc.element_children(doc.root()) {
            engine.compute_variant_subtree(
                child,
                &facts,
                None,
                None,
                &mut variant_root_fs,
                &styles,
                &mut changed,
            );
        }

        if !changed.is_empty() {
            variants.push(StateVariant {
                node: cond_node,
                state,
                changed,
            });
        }
    }

    StyleMap {
        styles,
        hidden,
        parent_elements: engine.parent_elements(),
        element_cascades,
        root_font_size,
        viewport: viewport.clone(),
        variants,
        keyframes,
        font_faces,
        diagnostics,
    }
}

/// Parses inline styles, orders layers and indexes the rules of `sheets`.
pub(crate) fn prepare<'a>(
    doc: &'a Document,
    sheets: &'a [Stylesheet],
    viewport: &'a Viewport,
) -> Prepared<'a> {
    let ua_sheet = ua::stylesheet();
    let mut longhand_set: BTreeSet<Longhand> =
        statically_known_longhands().iter().copied().collect();

    // 1. Parse inline `style` attributes once up front in document order, collecting
    //    their diagnostics and discovering any referenced longhands.
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut inline_styles: Vec<Vec<ParsedDecl>> = vec![Vec::new(); doc.len()];
    for node in doc.descendants(doc.root()) {
        let Some(elem) = doc.element(node) else {
            continue;
        };
        if let Some(style_attr) = elem.attr("style")
            && !style_attr.trim().is_empty()
        {
            let source_label = match elem.id() {
                Some(id) => format!("style attribute of <{} id={id}>", elem.local_name()),
                None => format!("style attribute of <{}>", elem.local_name()),
            };
            let (decls, diags) = parse_inline_style(style_attr, &source_label, Origin::Author);
            for d in &decls {
                record_decl_longhand(d, &mut longhand_set);
            }
            inline_styles[node.index()] = decls;
            diagnostics.extend(diags);
        }
    }

    // 2. Build the document-wide author `@layer` tree from all active layer
    //    registrations and rules in stylesheet order.
    let mut author_layers = LayerTreeNode::default();
    for sheet in sheets {
        if sheet.origin == Origin::Author {
            for reg in &sheet.inner.layer_registrations {
                if media_all_match(&reg.media, viewport) {
                    author_layers.register_path(&reg.path);
                }
            }
            for rule in &sheet.inner.rules {
                if media_all_match(&rule.media, viewport)
                    && let Some(path) = &rule.layer
                {
                    author_layers.register_path(path);
                }
            }
        }
    }

    // 3. Index active rules from the UA stylesheet and author stylesheets, and
    //    collect active `@keyframes` and `@font-face` rules.
    let mut element_rules: RuleIndex<IndexedRule<'_>> = RuleIndex::new();
    let mut pseudo_rules: RuleIndex<(PseudoElement, IndexedRule<'_>)> = RuleIndex::new();
    let mut variant_prefixes: RuleIndex<PseudoState> = RuleIndex::new();
    let mut keyframes: BTreeMap<String, Vec<Keyframe>> = BTreeMap::new();
    let mut font_faces: Vec<FontFace> = Vec::new();
    let mut next_order: u32 = 0;

    let all_sheets = std::iter::once(ua_sheet).chain(sheets.iter());
    for sheet in all_sheets {
        for rule in &sheet.inner.rules {
            if !media_all_match(&rule.media, viewport) {
                continue;
            }
            for d in &rule.declarations {
                record_decl_longhand(d, &mut longhand_set);
            }

            let layer_key = if sheet.origin == Origin::Author {
                author_layers.key_for_path(rule.layer.as_deref())
            } else {
                LayerOrderKey::unlayered()
            };

            let decl_count = (rule.declarations.len() as u32).max(1);
            for sel in &rule.selectors {
                let base_order = next_order;
                next_order = next_order.saturating_add(decl_count);

                let indexed = IndexedRule {
                    origin: sheet.origin,
                    layer_key: layer_key.clone(),
                    specificity: sel.specificity,
                    base_order,
                    declarations: &rule.declarations,
                };

                if let Some(pe) = sel.pseudo_element {
                    pseudo_rules.insert(sel.without_pseudo_element(), (pe, indexed));
                } else {
                    element_rules.insert(sel.clone(), indexed);
                }

                // Seed dynamic state variant candidates from author rules.
                if sheet.origin == Origin::Author {
                    for (prefix_sel, st) in sel.dynamic_variants() {
                        variant_prefixes.insert(prefix_sel, st);
                    }
                }
            }
        }

        if sheet.origin == Origin::Author {
            for kf in &sheet.inner.keyframes {
                if media_all_match(&kf.media, viewport) {
                    keyframes.insert(kf.name.clone(), kf.keyframes.clone());
                }
            }
            for ff in &sheet.inner.font_faces {
                if media_all_match(&ff.media, viewport) {
                    font_faces.push(ff.font_face.clone());
                }
            }
        }
    }

    let mut ordered_longhands: Vec<Longhand> = longhand_set.into_iter().collect();
    ordered_longhands.sort_by_key(|&l| (properties::priority(l), l));
    let inherited_longhands: Vec<Longhand> = ordered_longhands
        .iter()
        .copied()
        .filter(|&l| properties::is_inherited(l))
        .collect();

    let engine = CascadeEngine {
        doc,
        viewport,
        initial_style: ComputedStyle::default(),
        ancestor_blooms: compute_ancestor_blooms(doc),
        dynamic_bloom: 0,
        dynamic_classes: Vec::new(),
        element_rules,
        pseudo_rules,
        variant_prefixes,
        inline_styles,
        ordered_longhands,
        inherited_longhands,
    };

    Prepared {
        engine,
        diagnostics,
        keyframes,
        font_faces,
    }
}

impl CascadeEngine<'_> {
    fn extra_classes(&self, node: NodeId) -> &[String] {
        self.dynamic_classes
            .get(node.index())
            .map_or(&[], Vec::as_slice)
    }

    /// Visits the selector of every element rule that could match `node`,
    /// counting the classes conditions may add.
    pub(crate) fn for_each_element_selector(
        &self,
        node: NodeId,
        elem: &Element,
        mut visit: impl FnMut(&Selector),
    ) {
        let bloom = self.ancestor_blooms[node.index()] | self.dynamic_bloom;
        self.element_rules.for_each_candidate_with(
            elem,
            bloom,
            self.extra_classes(node),
            |sel, _| {
                visit(sel);
            },
        );
    }

    fn parent_elements(&self) -> Vec<Option<NodeId>> {
        let mut parent_elements = vec![None; self.doc.len()];
        for node in self.doc.descendants(self.doc.root()) {
            parent_elements[node.index()] = self.doc.parent_element(node);
        }
        parent_elements
    }

    /// The cascade of the document as loaded, in pre-order.
    pub(crate) fn base_pass(&self) -> BasePass {
        let doc = self.doc;
        let mut styles: Vec<Option<ElementStyle>> = vec![None; doc.len()];
        let mut hidden: Vec<Option<ElementStyle>> = vec![None; doc.len()];
        let mut element_cascades: Vec<Option<ElementCascade>> = vec![None; doc.len()];
        let mut root_font_size: f32 = self.initial_style.font_size;
        let mut variant_candidates: BTreeSet<(NodeId, PseudoState)> = BTreeSet::new();

        for child in doc.element_children(doc.root()) {
            self.compute_base_subtree(
                child,
                None,
                None,
                &mut root_font_size,
                &mut styles,
                &mut hidden,
                &mut element_cascades,
                &mut variant_candidates,
            );
        }
        BasePass {
            styles,
            hidden,
            element_cascades,
            root_font_size,
            variant_candidates,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ElementCascade {
    /// Winning action from the full base cascade for each longhand that had
    /// at least one matching declaration on this element.
    base_actions: BTreeMap<Longhand, ResolvedAction>,
    /// Winning action from `Origin::UserAgent` declarations on this element,
    /// used when a keyframe rolls back author + animation origins via `revert`.
    ua_actions: BTreeMap<Longhand, ResolvedAction>,
}

fn parent_element_of(map: &StyleMap, node: NodeId) -> Option<NodeId> {
    map.parent_elements.get(node.index()).copied().flatten()
}

fn enclosing_box_display_of(map: &StyleMap, node: NodeId) -> Option<Display> {
    let mut cur = parent_element_of(map, node);
    while let Some(pid) = cur {
        let pstyle = map.style(pid)?;
        if pstyle.display != Display::Contents {
            return Some(pstyle.display);
        }
        cur = parent_element_of(map, pid);
    }
    None
}

fn element_cascade_of(map: &StyleMap, node: NodeId) -> Option<&ElementCascade> {
    map.element_cascades
        .get(node.index())
        .and_then(Option::as_ref)
}

/// Applies a [`Keyframe`]'s declarations on top of `node`'s base [`ComputedStyle`].
pub(crate) fn keyframe_style(
    map: &StyleMap,
    node: NodeId,
    keyframe: &Keyframe,
) -> Option<ComputedStyle> {
    let base = map.style(node)?;
    let parent_id = parent_element_of(map, node);
    let is_root = parent_id.is_none();
    let enclosing_box_display = enclosing_box_display_of(map, node);

    if keyframe.declarations.is_empty() {
        let mut style = base.clone();
        apply_box_fixups(&mut style, is_root, enclosing_box_display);
        return Some(style);
    }

    let parent_style = parent_id.and_then(|pid| map.style(pid));
    let elem_cascade = element_cascade_of(map, node);

    let mut style = base.clone();
    let initial = ComputedStyle::default();

    // First update any custom properties declared inside the keyframe.
    for (name, raw_val) in &keyframe.declarations {
        if properties::kind(name) == PropertyKind::Custom {
            if let Some(kw) = CssWide::parse(raw_val) {
                match kw {
                    CssWide::Initial => {
                        style.custom_properties.remove(name);
                    }
                    CssWide::Inherit | CssWide::Unset | CssWide::Revert => {
                        if let Some(v) = parent_style.and_then(|p| p.custom_properties.get(name)) {
                            style.custom_properties.insert(name.clone(), v.clone());
                        } else {
                            style.custom_properties.remove(name);
                        }
                    }
                    CssWide::RevertLayer => {
                        if let Some(v) = base.custom_properties.get(name) {
                            style.custom_properties.insert(name.clone(), v.clone());
                        } else {
                            style.custom_properties.remove(name);
                        }
                    }
                }
            } else if let Some(resolved) = substitute_from_map(raw_val, &style.custom_properties) {
                style.custom_properties.insert(name.clone(), resolved);
            } else {
                style.custom_properties.remove(name);
            }
        }
    }

    // Collect winning `(Longhand, ResolvedAction)` from the keyframe declarations in order.
    let mut winners: BTreeMap<Longhand, ResolvedAction> = BTreeMap::new();
    for (name, raw_val) in &keyframe.declarations {
        match properties::kind(name) {
            PropertyKind::Longhand(_) | PropertyKind::Shorthand => {
                let effective = if contains_var_function(raw_val) {
                    let Some(sub) = substitute_from_map(raw_val, &style.custom_properties) else {
                        for l in properties::longhands_of(name) {
                            winners.insert(l, ResolvedAction::Unset);
                        }
                        continue;
                    };
                    sub
                } else {
                    raw_val.clone()
                };

                if let Some(kw) = CssWide::parse(&effective) {
                    for l in properties::longhands_of(name) {
                        let act = match kw {
                            CssWide::Initial => ResolvedAction::Initial,
                            CssWide::Inherit => ResolvedAction::Inherit,
                            CssWide::Unset => ResolvedAction::Unset,
                            // CSS Cascade 5 §7.3.4 & §7.3.5 Note: `revert` in `@keyframes`
                            // collapses the animation and author origins, rolling back to UA.
                            CssWide::Revert => elem_cascade
                                .and_then(|c| c.ua_actions.get(&l).cloned())
                                .unwrap_or(ResolvedAction::Unset),
                            // CSS Cascade 5 §7.3.5: the animation origin forms its own cascade
                            // layer for `revert-layer`, rolling back to the element's base cascade.
                            CssWide::RevertLayer => elem_cascade
                                .and_then(|c| c.base_actions.get(&l).cloned())
                                .unwrap_or(ResolvedAction::Unset),
                        };
                        winners.insert(l, act);
                    }
                } else if let Ok(expanded) = properties::expand(name, &effective) {
                    for (l, val) in expanded {
                        winners.insert(l, ResolvedAction::Value(val));
                    }
                }
            }
            _ => {}
        }
    }

    let mut combined: BTreeMap<Longhand, ResolvedAction> = elem_cascade
        .map(|c| c.base_actions.clone())
        .unwrap_or_default();
    for (l, act) in winners {
        combined.insert(l, act);
    }

    let mut ordered: Vec<(Longhand, ResolvedAction)> = combined.into_iter().collect();
    ordered.sort_by_key(|(l, _)| (properties::priority(*l), *l));

    let mut effective_root_fs = if map.root_font_size > 0.0 {
        map.root_font_size
    } else {
        16.0
    };
    let mut root_fs_updated = !is_root;

    for (l, action) in ordered {
        if !root_fs_updated && properties::priority(l) > 0 {
            effective_root_fs = style.font_size;
            root_fs_updated = true;
        }
        let ctx = ComputeContext {
            parent: parent_style,
            root_font_size: effective_root_fs,
            viewport: &map.viewport,
        };
        apply_resolved_action(&mut style, l, action, parent_style, &initial, &ctx);
    }

    apply_box_fixups(&mut style, is_root, enclosing_box_display);

    Some(style)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedAction {
    Initial,
    Inherit,
    Unset,
    Value(String),
}

impl<'a> CascadeEngine<'a> {
    #[allow(clippy::too_many_arguments)]
    fn compute_base_subtree(
        &self,
        node: NodeId,
        parent_node: Option<NodeId>,
        enclosing_box_display: Option<Display>,
        root_font_size: &mut f32,
        styles: &mut [Option<ElementStyle>],
        hidden: &mut [Option<ElementStyle>],
        element_cascades: &mut [Option<ElementCascade>],
        variant_candidates: &mut BTreeSet<(NodeId, PseudoState)>,
    ) {
        let Some(elem) = self.doc.element(node) else {
            return;
        };

        let is_root = self.doc.parent(node) == Some(self.doc.root());
        let match_ctx = MatchContext {
            doc: self.doc,
            dynamic: None,
        };

        let mut elem_cascade = ElementCascade::default();
        let elem_style = {
            let parent_style = parent_node
                .and_then(|pid| {
                    styles[pid.index()]
                        .as_ref()
                        .or(hidden[pid.index()].as_ref())
                })
                .map(|es| &es.style);
            self.compute_single_element(
                node,
                elem,
                is_root,
                parent_style,
                enclosing_box_display,
                root_font_size,
                &match_ctx,
                Some(&mut elem_cascade),
            )
        };

        // An element with `display: none` and its entire subtree have no box
        // (`None` in `styles`) but keep their computed values in `hidden`.
        let parent_hidden = parent_node.is_some_and(|pid| hidden[pid.index()].is_some());
        if parent_hidden || elem_style.style.display == Display::None {
            styles[node.index()] = None;
            element_cascades[node.index()] = None;
            let child_box_display = Some(elem_style.style.display);
            hidden[node.index()] = Some(elem_style);
            for child in self.doc.element_children(node) {
                self.compute_base_subtree(
                    child,
                    Some(node),
                    child_box_display,
                    root_font_size,
                    styles,
                    hidden,
                    element_cascades,
                    variant_candidates,
                );
            }
            return;
        }

        // Check whether `node` is a candidate condition element for any dynamic state variant.
        let bloom = self.ancestor_blooms[node.index()];
        self.variant_prefixes
            .for_each_candidate(elem, bloom, |prefix_sel, &state| {
                if matches_selector(prefix_sel, node, &match_ctx) {
                    variant_candidates.insert((node, state));
                }
            });

        let child_box_display = if elem_style.style.display == Display::Contents {
            enclosing_box_display
        } else {
            Some(elem_style.style.display)
        };

        styles[node.index()] = Some(elem_style);
        element_cascades[node.index()] = Some(elem_cascade);

        for child in self.doc.element_children(node) {
            self.compute_base_subtree(
                child,
                Some(node),
                child_box_display,
                root_font_size,
                styles,
                hidden,
                element_cascades,
                variant_candidates,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_variant_subtree(
        &self,
        node: NodeId,
        facts: &DynamicFacts,
        parent_style: Option<&ComputedStyle>,
        enclosing_box_display: Option<Display>,
        root_font_size: &mut f32,
        base_styles: &[Option<ElementStyle>],
        changed: &mut Vec<(NodeId, Box<ComputedStyle>)>,
    ) {
        let Some(elem) = self.doc.element(node) else {
            return;
        };

        let is_root = self.doc.parent(node) == Some(self.doc.root());
        let match_ctx = MatchContext {
            doc: self.doc,
            dynamic: Some(facts),
        };

        let variant_elem_style = self.compute_single_element(
            node,
            elem,
            is_root,
            parent_style,
            enclosing_box_display,
            root_font_size,
            &match_ctx,
            None,
        );

        let base_computed = base_styles
            .get(node.index())
            .and_then(Option::as_ref)
            .map(|es| &es.style);

        match base_computed {
            Some(base_s) => {
                if &variant_elem_style.style != base_s {
                    changed.push((node, Box::new(variant_elem_style.style.clone())));
                }
            }
            None => {
                // Element had `display: none` in base style; record it if it became visible!
                if variant_elem_style.style.display != Display::None {
                    changed.push((node, Box::new(variant_elem_style.style.clone())));
                }
            }
        }

        if variant_elem_style.style.display == Display::None {
            return;
        }

        let child_box_display = if variant_elem_style.style.display == Display::Contents {
            enclosing_box_display
        } else {
            Some(variant_elem_style.style.display)
        };

        for child in self.doc.element_children(node) {
            self.compute_variant_subtree(
                child,
                facts,
                Some(&variant_elem_style.style),
                child_box_display,
                root_font_size,
                base_styles,
                changed,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_single_element(
        &self,
        node: NodeId,
        elem: &Element,
        is_root: bool,
        parent_style: Option<&ComputedStyle>,
        enclosing_box_display: Option<Display>,
        root_font_size: &mut f32,
        match_ctx: &MatchContext<'_>,
        cascade_out: Option<&mut ElementCascade>,
    ) -> ElementStyle {
        let bloom = self.ancestor_blooms[node.index()] | self.dynamic_bloom;
        let extra_classes = self.extra_classes(node);
        let mut matched_decls: Vec<(CascadeKey, &DeclValue)> = Vec::new();

        self.element_rules
            .for_each_candidate_with(elem, bloom, extra_classes, |sel, rule| {
                if matches_selector(sel, node, match_ctx) {
                    for (offset, decl) in rule.declarations.iter().enumerate() {
                        let key = CascadeKey {
                            tier: OriginTier::of(rule.origin, decl.important),
                            is_inline_style: false,
                            layer_key: rule.layer_key.clone(),
                            specificity: rule.specificity,
                            order: rule.base_order.saturating_add(offset as u32),
                        };
                        matched_decls.push((key, &decl.value));
                    }
                }
            });

        let inline = &self.inline_styles[node.index()];
        let inline_base_order = u32::MAX - (inline.len() as u32) - 1;
        for (offset, decl) in inline.iter().enumerate() {
            let key = CascadeKey {
                tier: OriginTier::of(Origin::Author, decl.important),
                is_inline_style: true,
                layer_key: LayerOrderKey::unlayered(),
                specificity: Specificity::default(),
                order: inline_base_order.saturating_add(offset as u32),
            };
            matched_decls.push((key, &decl.value));
        }

        let mut style = self.resolve_declarations_to_style(
            &matched_decls,
            is_root,
            parent_style,
            enclosing_box_display,
            root_font_size,
            cascade_out,
        );

        if style.display == Display::None {
            return ElementStyle {
                style,
                before: None,
                after: None,
                placeholder: None,
                marker: None,
            };
        }

        // Pseudo-elements (`::before`, `::after`, `::placeholder`, `::marker`).
        let mut before_decls: Vec<(CascadeKey, &DeclValue)> = Vec::new();
        let mut after_decls: Vec<(CascadeKey, &DeclValue)> = Vec::new();
        let mut placeholder_decls: Vec<(CascadeKey, &DeclValue)> = Vec::new();
        let mut marker_decls: Vec<(CascadeKey, &DeclValue)> = Vec::new();
        let supports_placeholder =
            elem.is_html() && matches!(elem.local_name(), "input" | "textarea");

        self.pseudo_rules
            .for_each_candidate_with(elem, bloom, extra_classes, |sel, (pe, rule)| {
                if *pe == PseudoElement::Placeholder && !supports_placeholder {
                    return;
                }
                if matches_selector(sel, node, match_ctx) {
                    let target_vec = match pe {
                        PseudoElement::Before => &mut before_decls,
                        PseudoElement::After => &mut after_decls,
                        PseudoElement::Placeholder => &mut placeholder_decls,
                        PseudoElement::Marker => &mut marker_decls,
                    };
                    for (offset, decl) in rule.declarations.iter().enumerate() {
                        let key = CascadeKey {
                            tier: OriginTier::of(rule.origin, decl.important),
                            is_inline_style: false,
                            layer_key: rule.layer_key.clone(),
                            specificity: rule.specificity,
                            order: rule.base_order.saturating_add(offset as u32),
                        };
                        target_vec.push((key, &decl.value));
                    }
                }
            });

        let pseudo_enclosing = if style.display == Display::Contents {
            enclosing_box_display
        } else {
            Some(style.display)
        };

        let before = if before_decls.is_empty() {
            None
        } else {
            let mut fs = *root_font_size;
            let ps = self.resolve_declarations_to_style(
                &before_decls,
                false,
                Some(&style),
                pseudo_enclosing,
                &mut fs,
                None,
            );
            (ps.content.is_some() && ps.display != Display::None).then(|| Box::new(ps))
        };

        let after = if after_decls.is_empty() {
            None
        } else {
            let mut fs = *root_font_size;
            let ps = self.resolve_declarations_to_style(
                &after_decls,
                false,
                Some(&style),
                pseudo_enclosing,
                &mut fs,
                None,
            );
            (ps.content.is_some() && ps.display != Display::None).then(|| Box::new(ps))
        };

        let placeholder = if placeholder_decls.is_empty() {
            None
        } else {
            let mut fs = *root_font_size;
            let ps = self.resolve_declarations_to_style(
                &placeholder_decls,
                false,
                Some(&style),
                None,
                &mut fs,
                None,
            );
            (ps.display != Display::None).then(|| Box::new(ps))
        };

        let marker = if marker_decls.is_empty() || style.display != Display::ListItem {
            None
        } else {
            let mut fs = *root_font_size;
            Some(Box::new(self.resolve_declarations_to_style(
                &marker_decls,
                false,
                Some(&style),
                pseudo_enclosing,
                &mut fs,
                None,
            )))
        };

        // Ensure `style.content` on regular elements doesn't leak onto non-pseudo elements
        // unless set directly, keeping `style` intact as computed.
        let _ = &mut style;

        ElementStyle {
            style,
            before,
            after,
            placeholder,
            marker,
        }
    }

    fn resolve_declarations_to_style(
        &self,
        matched_decls: &[(CascadeKey, &DeclValue)],
        is_root: bool,
        parent_style: Option<&ComputedStyle>,
        enclosing_box_display: Option<Display>,
        root_font_size: &mut f32,
        mut cascade_out: Option<&mut ElementCascade>,
    ) -> ComputedStyle {
        let mut style = self.initial_style.clone();

        // 1. Inherit custom properties from parent, then cascade & resolve `--*` on this element.
        if let Some(p) = parent_style {
            style.custom_properties = p.custom_properties.clone();
        }

        let mut custom_candidates: HashMap<&str, Vec<(CascadeKey, &CustomDeclValue)>> =
            HashMap::new();
        let mut longhand_candidates: HashMap<Longhand, Vec<(CascadeKey, LonghandCandidate<'_>)>> =
            HashMap::new();

        for (key, decl_val) in matched_decls {
            match decl_val {
                DeclValue::Custom { name, value } => {
                    custom_candidates
                        .entry(name.as_str())
                        .or_default()
                        .push((key.clone(), value));
                }
                DeclValue::Expanded(l, val) => {
                    longhand_candidates
                        .entry(*l)
                        .or_default()
                        .push((key.clone(), LonghandCandidate::Expanded(val.as_str())));
                }
                DeclValue::Wide(l, kw) => {
                    longhand_candidates
                        .entry(*l)
                        .or_default()
                        .push((key.clone(), LonghandCandidate::Wide(*kw)));
                }
                DeclValue::PendingVar {
                    target,
                    prop_name,
                    raw_value,
                } => {
                    longhand_candidates.entry(*target).or_default().push((
                        key.clone(),
                        LonghandCandidate::PendingVar {
                            prop_name: prop_name.as_str(),
                            raw_value: raw_value.as_str(),
                        },
                    ));
                }
            }
        }

        if !custom_candidates.is_empty() {
            resolve_element_custom_properties(
                &mut style.custom_properties,
                parent_style.map(|p| &p.custom_properties),
                custom_candidates,
            );
        }

        // 2. Pre-copy all inherited active longhands from `parent_style`.
        if let Some(p) = parent_style {
            for &l in &self.inherited_longhands {
                properties::copy(&mut style, p, l);
            }
        }

        // 3. Compute winning longhands in `properties::priority` order.
        let mut root_fs_updated = !is_root;
        for &l in &self.ordered_longhands {
            if !root_fs_updated && properties::priority(l) > 0 {
                *root_font_size = style.font_size;
                root_fs_updated = true;
            }

            if let Some(mut cands) = longhand_candidates.remove(&l) {
                cands.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
                let action = select_winning_longhand_action(l, &cands, &style.custom_properties);
                if let Some(casc) = cascade_out.as_deref_mut() {
                    casc.base_actions.insert(l, action.clone());
                    let ua_cands: Vec<(CascadeKey, LonghandCandidate<'_>)> = cands
                        .iter()
                        .filter(|(k, _)| k.tier.origin() == Origin::UserAgent)
                        .cloned()
                        .collect();
                    if !ua_cands.is_empty() {
                        let ua_action =
                            select_winning_longhand_action(l, &ua_cands, &style.custom_properties);
                        casc.ua_actions.insert(l, ua_action);
                    }
                }
                let compute_ctx = ComputeContext {
                    parent: parent_style,
                    root_font_size: *root_font_size,
                    viewport: self.viewport,
                };
                apply_resolved_action(
                    &mut style,
                    l,
                    action,
                    parent_style,
                    &self.initial_style,
                    &compute_ctx,
                );
            }
        }
        if !root_fs_updated {
            *root_font_size = style.font_size;
        }

        // 4. CSS Display 3 §2.7 & CSS 2.2 §9.7 computed-value fix-ups.
        apply_box_fixups(&mut style, is_root, enclosing_box_display);

        style
    }
}

#[derive(Clone, Copy)]
enum LonghandCandidate<'a> {
    Expanded(&'a str),
    Wide(CssWide),
    PendingVar {
        prop_name: &'a str,
        raw_value: &'a str,
    },
}

fn select_winning_longhand_action(
    target: Longhand,
    sorted_cands: &[(CascadeKey, LonghandCandidate<'_>)],
    custom_props: &BTreeMap<String, String>,
) -> ResolvedAction {
    let mut idx = sorted_cands.len();
    while idx > 0 {
        idx -= 1;
        let (key, cand) = &sorted_cands[idx];
        match *cand {
            LonghandCandidate::Expanded(val) => {
                return ResolvedAction::Value(val.to_owned());
            }
            LonghandCandidate::Wide(kw) => match kw {
                CssWide::Initial => return ResolvedAction::Initial,
                CssWide::Inherit => return ResolvedAction::Inherit,
                CssWide::Unset => return ResolvedAction::Unset,
                CssWide::Revert => {
                    if key.tier.origin() == Origin::UserAgent {
                        return ResolvedAction::Unset;
                    }
                    // Roll back to highest-precedence UA-origin declaration.
                    while idx > 0 && sorted_cands[idx - 1].0.tier.origin() != Origin::UserAgent {
                        idx -= 1;
                    }
                }
                CssWide::RevertLayer => {
                    let cur_bucket = key.bucket();
                    while idx > 0 && sorted_cands[idx - 1].0.bucket() == cur_bucket {
                        idx -= 1;
                    }
                }
            },
            LonghandCandidate::PendingVar {
                prop_name,
                raw_value,
            } => {
                let Some(substituted) = substitute_from_map(raw_value, custom_props) else {
                    return ResolvedAction::Unset;
                };
                let trimmed = substituted.trim();
                if trimmed.is_empty() {
                    return ResolvedAction::Unset;
                }
                if let Some(kw) = CssWide::parse(trimmed) {
                    match kw {
                        CssWide::Initial => return ResolvedAction::Initial,
                        CssWide::Inherit => return ResolvedAction::Inherit,
                        CssWide::Unset => return ResolvedAction::Unset,
                        CssWide::Revert => {
                            if key.tier.origin() == Origin::UserAgent {
                                return ResolvedAction::Unset;
                            }
                            while idx > 0
                                && sorted_cands[idx - 1].0.tier.origin() != Origin::UserAgent
                            {
                                idx -= 1;
                            }
                            continue;
                        }
                        CssWide::RevertLayer => {
                            let cur_bucket = key.bucket();
                            while idx > 0 && sorted_cands[idx - 1].0.bucket() == cur_bucket {
                                idx -= 1;
                            }
                            continue;
                        }
                    }
                }
                match properties::expand(prop_name, trimmed) {
                    Ok(pairs) => {
                        if let Some((_, val)) = pairs.into_iter().find(|(lh, _)| *lh == target) {
                            return ResolvedAction::Value(val);
                        }
                        return ResolvedAction::Unset;
                    }
                    Err(_) => return ResolvedAction::Unset,
                }
            }
        }
    }
    ResolvedAction::Unset
}

fn apply_resolved_action(
    style: &mut ComputedStyle,
    l: Longhand,
    action: ResolvedAction,
    parent_style: Option<&ComputedStyle>,
    initial_style: &ComputedStyle,
    compute_ctx: &ComputeContext<'_>,
) {
    let apply_unset = |dst: &mut ComputedStyle| {
        if properties::is_inherited(l)
            && let Some(p) = parent_style
        {
            properties::copy(dst, p, l);
        } else {
            properties::copy(dst, initial_style, l);
        }
    };

    match action {
        ResolvedAction::Initial => properties::copy(style, initial_style, l),
        ResolvedAction::Inherit => {
            if let Some(p) = parent_style {
                properties::copy(style, p, l);
            } else {
                properties::copy(style, initial_style, l);
            }
        }
        ResolvedAction::Unset => apply_unset(style),
        ResolvedAction::Value(val) => {
            let _ = (compute_ctx.root_font_size, compute_ctx.viewport);
            if properties::compute(style, l, &val, compute_ctx).is_err() {
                apply_unset(style);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Custom Properties Cascade, Cycle Elimination & `var()` Substitution
// ---------------------------------------------------------------------------

fn resolve_element_custom_properties(
    target_map: &mut BTreeMap<String, String>,
    inherited_map: Option<&BTreeMap<String, String>>,
    candidates: HashMap<&str, Vec<(CascadeKey, &CustomDeclValue)>>,
) {
    let mut pending_tokens: HashMap<String, String> = HashMap::new();

    for (name, mut cands) in candidates {
        cands.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
        let winner = select_winning_custom_decl(&cands);
        match winner {
            None | Some(CustomDeclValue::Wide(CssWide::Inherit | CssWide::Unset)) => {
                if let Some(inh_val) = inherited_map.and_then(|m| m.get(name)) {
                    target_map.insert(name.to_owned(), inh_val.clone());
                } else {
                    target_map.remove(name);
                }
            }
            Some(CustomDeclValue::Wide(CssWide::Initial)) => {
                // `initial` on a custom property sets it to the guaranteed-invalid value.
                target_map.remove(name);
            }
            Some(CustomDeclValue::Wide(CssWide::Revert | CssWide::RevertLayer)) => {
                target_map.remove(name);
            }
            Some(CustomDeclValue::Tokens(raw)) => {
                if !contains_var_function(raw) {
                    target_map.insert(name.to_owned(), raw.clone());
                } else {
                    // Remove any inherited value while pending so guaranteed-invalid
                    // does not accidentally fall back to parent's value instead of `unset`!
                    target_map.remove(name);
                    pending_tokens.insert(name.to_owned(), raw.clone());
                }
            }
        }
    }

    if pending_tokens.is_empty() {
        return;
    }

    // Build directed `var()` dependency edges across `pending_tokens` (including
    // references inside fallback arguments, per CSS Custom Properties 1 §2.1).
    let mut edges: HashMap<String, Vec<String>> = HashMap::with_capacity(pending_tokens.len());
    for (prop, raw) in &pending_tokens {
        let mut refs = Vec::new();
        collect_all_var_references(raw, &mut refs);
        edges.insert(prop.clone(), refs);
    }

    // Mark every custom property that lies on a directed cycle (`prop ->+ prop`)
    // as guaranteed-invalid before substituting `var()`s.
    let mut cycled: HashSet<String> = HashSet::new();
    for start in pending_tokens.keys() {
        if can_reach_self(start, &edges) {
            cycled.insert(start.clone());
        }
    }
    for bad in &cycled {
        pending_tokens.remove(bad);
        target_map.remove(bad);
    }

    // `pending_tokens` is now a DAG; resolve with memoization.
    let mut memo: HashMap<String, Option<String>> = HashMap::with_capacity(pending_tokens.len());
    let keys: Vec<String> = pending_tokens.keys().cloned().collect();
    for k in keys {
        if let Some(val) = resolve_dag_var(&k, &pending_tokens, target_map, &mut memo) {
            target_map.insert(k, val);
        } else {
            target_map.remove(&k);
        }
    }
}

fn select_winning_custom_decl<'a>(
    sorted_cands: &[(CascadeKey, &'a CustomDeclValue)],
) -> Option<&'a CustomDeclValue> {
    let mut idx = sorted_cands.len();
    while idx > 0 {
        idx -= 1;
        let (key, val) = &sorted_cands[idx];
        match val {
            CustomDeclValue::Wide(CssWide::Revert) => {
                if key.tier.origin() == Origin::UserAgent {
                    return None;
                }
                while idx > 0 && sorted_cands[idx - 1].0.tier.origin() != Origin::UserAgent {
                    idx -= 1;
                }
            }
            CustomDeclValue::Wide(CssWide::RevertLayer) => {
                let cur_bucket = key.bucket();
                while idx > 0 && sorted_cands[idx - 1].0.bucket() == cur_bucket {
                    idx -= 1;
                }
            }
            other => return Some(other),
        }
    }
    None
}

fn collect_all_var_references(s: &str, out: &mut Vec<String>) {
    let mut pos = 0usize;
    while let Some(var_idx) = find_next_var_call(s, pos) {
        let args_start = var_idx + 4;
        let Some((first_arg, close_idx)) = parse_var_call_bounds(s, args_start) else {
            break;
        };
        let name = first_arg.trim();
        if name.starts_with("--") && !out.iter().any(|existing| existing == name) {
            out.push(name.to_owned());
        }
        // Inspect fallback arguments recursively as well.
        let after_first = args_start + first_arg.len();
        if after_first < close_idx {
            collect_all_var_references(&s[after_first + 1..close_idx], out);
        }
        pos = close_idx + 1;
    }
}

fn can_reach_self(start: &str, edges: &HashMap<String, Vec<String>>) -> bool {
    let Some(initial_neighbors) = edges.get(start) else {
        return false;
    };
    let mut visited: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = initial_neighbors.iter().map(String::as_str).collect();
    while let Some(cur) = stack.pop() {
        if cur == start {
            return true;
        }
        if !visited.insert(cur) {
            continue;
        }
        if let Some(next_list) = edges.get(cur) {
            for n in next_list {
                stack.push(n.as_str());
            }
        }
    }
    false
}

fn resolve_dag_var(
    name: &str,
    pending: &HashMap<String, String>,
    resolved_base: &BTreeMap<String, String>,
    memo: &mut HashMap<String, Option<String>>,
) -> Option<String> {
    if let Some(cached) = memo.get(name) {
        return cached.clone();
    }
    if let Some(raw) = pending.get(name) {
        let res = substitute_with_lookup(raw, &mut |dep| {
            resolve_dag_var(dep, pending, resolved_base, memo)
        });
        // Check whether a CSS-wide keyword resulted from `var()` substitution on a custom prop.
        let final_res = match res {
            Some(s) if CssWide::parse(&s) == Some(CssWide::Initial) => None,
            other => other,
        };
        memo.insert(name.to_owned(), final_res.clone());
        return final_res;
    }
    resolved_base.get(name).cloned()
}

fn substitute_from_map(raw: &str, custom_props: &BTreeMap<String, String>) -> Option<String> {
    substitute_with_lookup(raw, &mut |name| custom_props.get(name).cloned())
}

fn substitute_with_lookup<F>(raw: &str, lookup: &mut F) -> Option<String>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::with_capacity(raw.len());
    let mut pos = 0usize;

    while let Some(var_idx) = find_next_var_call(raw, pos) {
        out.push_str(&raw[pos..var_idx]);
        if out.len() > MAX_VAR_EXPANSION_BYTES {
            return None;
        }

        let args_start = var_idx + 4;
        let (first_arg, close_idx) = parse_var_call_bounds(raw, args_start)?;
        let var_name = first_arg.trim();
        let after_first = args_start + first_arg.len();
        let fallback_slice = (after_first < close_idx).then(|| &raw[after_first + 1..close_idx]);

        if let Some(val) = lookup(var_name) {
            push_token_safe(&mut out, &val);
        } else {
            let fb = fallback_slice?;
            let resolved_fb = substitute_with_lookup(fb, lookup)?;
            push_token_safe(&mut out, resolved_fb.trim());
        }

        if out.len() > MAX_VAR_EXPANSION_BYTES {
            return None;
        }
        pos = close_idx + 1;
    }

    out.push_str(&raw[pos..]);
    if out.len() > MAX_VAR_EXPANSION_BYTES {
        return None;
    }
    Some(out)
}

/// Appends substituted token text, inserting an ASCII space only when adjacent
/// identifier/number/dimension bytes would otherwise merge into a single token.
fn push_token_safe(dst: &mut String, piece: &str) {
    if piece.is_empty() {
        return;
    }
    if let (Some(&last_b), Some(&first_b)) = (dst.as_bytes().last(), piece.as_bytes().first())
        && is_token_join_hazard(last_b)
        && is_token_join_hazard(first_b)
    {
        dst.push(' ');
    }
    dst.push_str(piece);
}

fn is_token_join_hazard(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'%' | b'#') || b >= 0x80
}

// ---------------------------------------------------------------------------
// Computed-Value Box Fix-ups (CSS Display Level 3 §2.7 & CSS 2.2 §9.7)
// ---------------------------------------------------------------------------

fn apply_box_fixups(
    style: &mut ComputedStyle,
    is_root: bool,
    enclosing_box_display: Option<Display>,
) {
    if style.display == Display::None {
        return;
    }

    let is_abs_or_fixed = matches!(style.position, Position::Absolute | Position::Fixed);
    if is_abs_or_fixed {
        style.float = Float::None;
    }

    let is_flex_or_grid_item = matches!(
        enclosing_box_display,
        Some(Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid)
    );

    if is_root || is_abs_or_fixed || style.float != Float::None || is_flex_or_grid_item {
        style.blockified_inline = matches!(style.display, Display::Inline | Display::InlineBlock);
        style.display = blockify_display(style.display, is_root);
    }
}

fn blockify_display(d: Display, is_root: bool) -> Display {
    match d {
        Display::None => Display::None,
        // `display: contents` blockifies to `block` on the root element, but remains
        // `contents` on non-root elements (their children become the flex/grid items).
        Display::Contents => {
            if is_root {
                Display::Block
            } else {
                Display::Contents
            }
        }
        Display::InlineFlex => Display::Flex,
        Display::InlineGrid => Display::Grid,
        Display::InlineTable => Display::Table,
        Display::Inline
        | Display::InlineBlock
        | Display::TableRowGroup
        | Display::TableHeaderGroup
        | Display::TableFooterGroup
        | Display::TableRow
        | Display::TableCell
        | Display::TableColumnGroup
        | Display::TableColumn
        | Display::TableCaption => Display::Block,
        Display::Block
        | Display::Flex
        | Display::Grid
        | Display::FlowRoot
        | Display::ListItem
        | Display::Table => d,
    }
}

fn media_all_match(media_stack: &[MediaQueryList], viewport: &Viewport) -> bool {
    media_stack.iter().all(|m| m.matches(viewport))
}

fn record_decl_longhand(decl: &ParsedDecl, out: &mut BTreeSet<Longhand>) {
    match &decl.value {
        DeclValue::Expanded(l, _) | DeclValue::Wide(l, _) => {
            out.insert(*l);
        }
        DeclValue::PendingVar { target, .. } => {
            out.insert(*target);
        }
        DeclValue::Custom { .. } => {}
    }
}

/// The page's cascade layers as numbers: a higher one wins among normal
/// declarations.
pub(crate) struct LayerRanks {
    tree: LayerTreeNode,
    keys: Vec<LayerOrderKey>,
}

impl LayerRanks {
    pub(crate) fn rank(&self, path: &[LayerSegment]) -> Option<u32> {
        let key = self.tree.key_for_path(Some(path));
        self.keys
            .iter()
            .position(|k| *k == key)
            .and_then(|at| u32::try_from(at).ok())
    }
}

pub(crate) fn layer_ranks(sheets: &[Stylesheet], viewport: &Viewport) -> LayerRanks {
    let mut tree = LayerTreeNode::default();
    let author = || sheets.iter().filter(|s| s.origin == Origin::Author);
    for sheet in author() {
        for reg in &sheet.inner.layer_registrations {
            if media_all_match(&reg.media, viewport) {
                tree.register_path(&reg.path);
            }
        }
        for rule in &sheet.inner.rules {
            if media_all_match(&rule.media, viewport)
                && let Some(path) = &rule.layer
            {
                tree.register_path(path);
            }
        }
    }
    let mut keys: Vec<LayerOrderKey> = author()
        .flat_map(|s| s.inner.rules.iter())
        .filter_map(|r| r.layer.as_deref())
        .map(|path| tree.key_for_path(Some(path)))
        .collect();
    keys.sort();
    keys.dedup();
    LayerRanks { tree, keys }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LengthAuto, Rgba};

    #[test]
    fn blockification_rules_per_css_display_3() {
        let mut s = ComputedStyle {
            display: Display::Inline,
            position: Position::Absolute,
            float: Float::Left,
            ..ComputedStyle::default()
        };
        apply_box_fixups(&mut s, false, None);
        assert_eq!(s.display, Display::Block);
        assert_eq!(s.float, Float::None);

        let mut flex_child = ComputedStyle {
            display: Display::InlineFlex,
            ..ComputedStyle::default()
        };
        apply_box_fixups(&mut flex_child, false, Some(Display::Flex));
        assert_eq!(flex_child.display, Display::Flex);

        let mut floated = ComputedStyle {
            display: Display::Inline,
            float: Float::Right,
            ..ComputedStyle::default()
        };
        apply_box_fixups(&mut floated, false, None);
        assert_eq!(floated.display, Display::Block);
        assert_eq!(floated.float, Float::Right);

        let mut root_contents = ComputedStyle {
            display: Display::Contents,
            ..ComputedStyle::default()
        };
        apply_box_fixups(&mut root_contents, true, None);
        assert_eq!(root_contents.display, Display::Block);
    }

    #[test]
    fn ua_defaults_cascade_layers_vars_and_state_variants() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html>
              <head><title>Hidden Title</title></head>
              <body>
                <h1 id="h1">Title</h1>
                <div id="hidden-div" hidden><span id="inside-hidden">x</span></div>
                <div id="card" class="card">
                  <span id="title" class="title">Card Title</span>
                </div>
                <button id="btn" class="btn" style="margin-top: 12px">Go</button>
              </body>
            </html>"#,
        );

        let css = r#"
            @layer theme, components;
            @layer components {
                .btn { color: rgb(10, 20, 30); margin-top: 4px; }
            }
            @layer theme {
                .btn { color: rgb(200, 0, 0) !important; }
            }
            :root {
                --brand: rgb(0, 128, 255);
                --bad-a: var(--bad-b, red);
                --bad-b: var(--bad-a, blue);
            }
            .card {
                display: flex;
                color: var(--brand);
                margin-top: var(--bad-a);
            }
            .card:hover .title {
                color: rgb(255, 100, 0);
            }
            .btn:hover {
                margin-top: 24px;
            }
        "#;

        let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "test.css", None);
        assert!(diags.is_empty(), "unexpected parse diags: {diags:?}");
        let vp = Viewport::default();
        let map = compute_styles(&doc, &[sheet], &vp);
        assert!(
            map.diagnostics.is_empty(),
            "unexpected cascade diags: {:?}",
            map.diagnostics
        );

        // UA defaults: body margin-top == 8px, h1 font-size == 2em == 32px, hidden == None.
        let body = doc.body().expect("body");
        assert_eq!(
            map.style(body).expect("body style").margin.top,
            LengthAuto::Length(crate::Length::px(8.0))
        );
        let h1 = doc.element_by_id("h1").expect("h1");
        assert_eq!(map.style(h1).expect("h1 style").font_size, 32.0);

        let hidden_div = doc.element_by_id("hidden-div").expect("hidden-div");
        let inside_hidden = doc.element_by_id("inside-hidden").expect("inside-hidden");
        assert!(map.get(hidden_div).is_none());
        assert!(map.get(inside_hidden).is_none());

        // `.card` has `--brand` resolved, `--bad-a` cycle -> unset (0px), and flex child `.title`
        // blockified from inline -> block!
        let card = doc.element_by_id("card").expect("card");
        let title = doc.element_by_id("title").expect("title");
        assert_eq!(
            map.style(card).expect("card style").color,
            Rgba::new(0, 128, 255, 255)
        );
        assert_eq!(
            map.style(card).expect("card style").margin.top,
            LengthAuto::Length(crate::Length::px(0.0))
        );
        assert_eq!(
            map.style(title).expect("title style").display,
            Display::Block
        );

        // `@layer theme` was declared BEFORE `components`, so its `!important` beats `components`,
        // and inline `style="margin-top: 12px"` beats normal `.btn { margin-top: 4px }` AND
        // even beats `.btn:hover { margin-top: 24px }` (normal selector vs inline style)!
        let btn = doc.element_by_id("btn").expect("btn");
        assert_eq!(
            map.style(btn).expect("btn style").color,
            Rgba::new(200, 0, 0, 255)
        );
        assert_eq!(
            map.style(btn).expect("btn style").margin.top,
            LengthAuto::Length(crate::Length::px(12.0))
        );

        // State variants: `.card:hover` changes `#title`'s color!
        let card_variants: Vec<&StateVariant> = map.variants_of(card).collect();
        assert_eq!(card_variants.len(), 1);
        assert_eq!(card_variants[0].state, PseudoState::Hover);
        assert_eq!(card_variants[0].changed.len(), 1);
        assert_eq!(card_variants[0].changed[0].0, title);
        assert_eq!(
            card_variants[0].changed[0].1.color,
            Rgba::new(255, 100, 0, 255)
        );
    }

    #[test]
    fn keyframe_style_box_fixups_positioned_floated_root_flex_and_grid_items() {
        let doc = Document::parse(
            r#"<!doctype html><html id="root"><body>
                <div id="abs-el"></div>
                <div id="fixed-el"></div>
                <div id="float-el"></div>
                <div id="flex-parent" style="display: flex"><span id="flex-item"></span></div>
                <div id="grid-parent">
                    <span id="grid-item"></span>
                    <div id="contents-wrap"><span id="contents-grid-item"></span></div>
                </div>
            </body></html>"#,
        );
        let css = r#"
            @keyframes inline-kf {
                to { display: inline; }
            }
            @keyframes abs-kf {
                to { position: absolute; }
            }
        "#;
        let (sheet, _) = Stylesheet::parse(css, Origin::Author, "kf_box.css", None);
        let mut map = compute_styles(&doc, &[sheet], &Viewport::default());
        let kf = map.keyframes["inline-kf"][0].clone();
        let abs_kf_rule = map.keyframes["abs-kf"][0].clone();

        let root = doc.element_by_id("root").expect("root");
        let abs_el = doc.element_by_id("abs-el").expect("abs-el");
        let fixed_el = doc.element_by_id("fixed-el").expect("fixed-el");
        let float_el = doc.element_by_id("float-el").expect("float-el");
        let flex_item = doc.element_by_id("flex-item").expect("flex-item");
        let grid_parent = doc.element_by_id("grid-parent").expect("grid-parent");
        let grid_item = doc.element_by_id("grid-item").expect("grid-item");
        let contents_wrap = doc.element_by_id("contents-wrap").expect("contents-wrap");
        let contents_grid_item = doc
            .element_by_id("contents-grid-item")
            .expect("contents-grid-item");

        // Configure position/float/grid states supported on ComputedStyle in Part A.
        let abs_base = &mut map.styles[abs_el.index()]
            .as_mut()
            .expect("element style")
            .style;
        abs_base.position = Position::Absolute;
        abs_base.float = Float::Left;

        let fixed_base = &mut map.styles[fixed_el.index()]
            .as_mut()
            .expect("element style")
            .style;
        fixed_base.position = Position::Fixed;
        fixed_base.float = Float::Left;

        let float_base = &mut map.styles[float_el.index()]
            .as_mut()
            .expect("element style")
            .style;
        float_base.position = Position::Static;
        float_base.float = Float::Left;

        map.styles[grid_parent.index()]
            .as_mut()
            .expect("element style")
            .style
            .display = Display::Grid;
        map.styles[contents_wrap.index()]
            .as_mut()
            .expect("element style")
            .style
            .display = Display::Contents;

        // 1. position: absolute + float: left + keyframe display: inline -> blockified, float: none
        let abs_kf = map.keyframe_style(abs_el, &kf).expect("abs kf");
        assert_eq!(abs_kf.display, Display::Block);
        assert_eq!(abs_kf.float, Float::None);

        // 2. position: fixed + float: left + keyframe display: inline -> blockified, float: none
        let fixed_kf = map.keyframe_style(fixed_el, &kf).expect("fixed kf");
        assert_eq!(fixed_kf.display, Display::Block);
        assert_eq!(fixed_kf.float, Float::None);

        // 3. float: left (static) + keyframe display: inline -> blockified, float stays left
        let float_kf = map.keyframe_style(float_el, &kf).expect("float kf");
        assert_eq!(float_kf.display, Display::Block);
        assert_eq!(float_kf.float, Float::Left);

        // 3b. float: left (static) + keyframe position: absolute -> float: none (CSS 2.1 §9.7)
        let float_abs_kf = map
            .keyframe_style(float_el, &abs_kf_rule)
            .expect("float abs kf");
        assert_eq!(float_abs_kf.position, Position::Absolute);
        assert_eq!(float_abs_kf.float, Float::None);

        // 4. Root element + keyframe display: inline -> blockified
        let root_kf = map.keyframe_style(root, &kf).expect("root kf");
        assert_eq!(root_kf.display, Display::Block);

        // 5. Flex item + keyframe display: inline -> blockified
        let flex_kf = map.keyframe_style(flex_item, &kf).expect("flex kf");
        assert_eq!(flex_kf.display, Display::Block);

        // 6. Grid item (direct and via display: contents) + keyframe display: inline -> blockified
        let grid_kf = map.keyframe_style(grid_item, &kf).expect("grid kf");
        assert_eq!(grid_kf.display, Display::Block);
        let contents_grid_kf = map
            .keyframe_style(contents_grid_item, &kf)
            .expect("contents grid kf");
        assert_eq!(contents_grid_kf.display, Display::Block);
    }

    #[test]
    fn keyframe_style_independent_maps_and_clones_do_not_cross_contaminate() {
        let doc_a =
            Document::parse(r#"<!doctype html><html><body><h1 id="t">A</h1></body></html>"#);
        let doc_b =
            Document::parse(r#"<!doctype html><html><body><span id="t">B</span></body></html>"#);
        let target_a = doc_a.element_by_id("t").expect("target_a");
        let target_b = doc_b.element_by_id("t").expect("target_b");
        assert_eq!(target_a.index(), target_b.index());

        let css_a = r#"
            @layer base {
                #t { color: rgb(10, 20, 30); }
            }
            #t {
                display: inline;
                color: rgb(100, 110, 120);
                font-size: 20px;
                margin-top: 2em;
            }
            @keyframes kf-revert {
                to { display: revert; font-size: revert; }
            }
            @keyframes kf-revert-layer {
                to { color: revert-layer; font-size: revert-layer; }
            }
        "#;
        let css_b = r#"
            @layer base {
                #t { color: rgb(40, 50, 60); }
            }
            #t {
                display: block;
                color: rgb(200, 210, 220);
                font-size: 12px;
                margin-top: 3em;
            }
            @keyframes kf-revert {
                to { display: revert; font-size: revert; }
            }
            @keyframes kf-revert-layer {
                to { color: revert-layer; font-size: revert-layer; }
            }
        "#;

        let (sheet_a, _) = Stylesheet::parse(css_a, Origin::Author, "a.css", None);
        let (sheet_b, _) = Stylesheet::parse(css_b, Origin::Author, "b.css", None);

        let map_a = compute_styles(&doc_a, &[sheet_a], &Viewport::default());
        let map_b = compute_styles(&doc_b, &[sheet_b], &Viewport::default());

        assert_eq!(map_a.parent_elements.len(), doc_a.len());
        assert_eq!(map_a.element_cascades.len(), doc_a.len());
        assert_eq!(map_b.parent_elements.len(), doc_b.len());
        assert_eq!(map_b.element_cascades.len(), doc_b.len());

        let kf_rev_a = map_a.keyframes["kf-revert"][0].clone();
        let kf_layer_a = map_a.keyframes["kf-revert-layer"][0].clone();
        let kf_rev_b = map_b.keyframes["kf-revert"][0].clone();
        let kf_layer_b = map_b.keyframes["kf-revert-layer"][0].clone();

        // Interleaved queries across map_a and map_b:
        let a_rev_1 = map_a.keyframe_style(target_a, &kf_rev_a).expect("a_rev_1");
        let b_rev_1 = map_b.keyframe_style(target_b, &kf_rev_b).expect("b_rev_1");
        let a_layer_1 = map_a
            .keyframe_style(target_a, &kf_layer_a)
            .expect("a_layer_1");
        let b_layer_1 = map_b
            .keyframe_style(target_b, &kf_layer_b)
            .expect("b_layer_1");

        // h1 UA has display: block and font-size: 2em (32px); author margin-top: 2em -> 64px
        assert_eq!(a_rev_1.display, Display::Block);
        assert_eq!(a_rev_1.font_size, 32.0);
        assert_eq!(
            a_rev_1.margin.top,
            crate::LengthAuto::Length(crate::Length::px(64.0))
        );
        // span has no UA display/font-size rules (unset -> inline, 16px); margin-top: 3em -> 48px
        assert_eq!(b_rev_1.display, Display::Inline);
        assert_eq!(b_rev_1.font_size, 16.0);
        assert_eq!(
            b_rev_1.margin.top,
            crate::LengthAuto::Length(crate::Length::px(48.0))
        );

        // revert-layer in keyframes rolls back to each map's own base cascade winner
        assert_eq!(a_layer_1.color, Rgba::new(100, 110, 120, 255));
        assert_eq!(a_layer_1.font_size, 20.0);
        assert_eq!(
            a_layer_1.margin.top,
            crate::LengthAuto::Length(crate::Length::px(40.0))
        );
        assert_eq!(b_layer_1.color, Rgba::new(200, 210, 220, 255));
        assert_eq!(b_layer_1.font_size, 12.0);
        assert_eq!(
            b_layer_1.margin.top,
            crate::LengthAuto::Length(crate::Length::px(36.0))
        );

        // Clone map_a, drop the original map_a, and interleave with map_b again.
        let map_a_clone = map_a.clone();
        drop(map_a);

        let b_rev_2 = map_b.keyframe_style(target_b, &kf_rev_b).expect("b_rev_2");
        let a_clone_rev = map_a_clone
            .keyframe_style(target_a, &kf_rev_a)
            .expect("a_clone_rev");
        let a_clone_layer = map_a_clone
            .keyframe_style(target_a, &kf_layer_a)
            .expect("a_clone_layer");

        assert_eq!(b_rev_2, b_rev_1);
        assert_eq!(a_clone_rev, a_rev_1);
        assert_eq!(a_clone_layer, a_layer_1);
    }
}
