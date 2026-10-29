//! CSS Selectors Level 4 representation, specificity, rule indexing, and DOM matching.
//!
//! `swc_css_ast` selector nodes are converted once at stylesheet parse time into
//! [`Selector`] values so matching against [`h2q_dom::Document`] never touches
//! parser AST types.

use std::collections::HashMap;
use std::ops::Add;

use h2q_dom::{Document, Element, NodeData, NodeId, ns};
#[cfg(test)]
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_css_ast as ast;

use crate::{Direction, PseudoState};

/// Selectors Level 4 §16 specificity `(id, class/attr/pseudo-class, type/pseudo-element)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct Specificity(pub u32, pub u32, pub u32);

impl Add for Specificity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(
            self.0.saturating_add(rhs.0),
            self.1.saturating_add(rhs.1),
            self.2.saturating_add(rhs.2),
        )
    }
}

/// Supported CSS pseudo-elements (`::before`, `::after`, `::placeholder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PseudoElement {
    Before,
    After,
    Placeholder,
}

/// Combinator between adjacent compound selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Combinator {
    /// Whitespace (`A B`).
    Descendant,
    /// `A > B`.
    Child,
    /// `A + B`.
    NextSibling,
    /// `A ~ B`.
    LaterSibling,
}

/// A compiled complex selector (`compounds.len() == combinators.len() + 1`, non-empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selector {
    pub(crate) compounds: Vec<CompoundSelector>,
    pub(crate) combinators: Vec<Combinator>,
    pub(crate) pseudo_element: Option<PseudoElement>,
    pub(crate) specificity: Specificity,
}

/// A relative selector inside `:has(...)`, evaluated left-to-right from the anchor element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelativeSelector {
    pub(crate) leading: Combinator,
    pub(crate) selector: Selector,
}

/// A sequence of simple selectors matching a single element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CompoundSelector {
    pub(crate) type_matcher: TypeMatcher,
    pub(crate) ids: Vec<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) attrs: Vec<AttrSelector>,
    pub(crate) pseudos: Vec<PseudoClass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum TypeMatcher {
    /// `*` or `*|*` or omitted type selector.
    #[default]
    Universal,
    /// `|*` (matches only elements in the empty namespace).
    EmptyNsUniversal,
    /// Tag name selector (`div`, `*|svg`, `|div`).
    Tag {
        /// Lowercased name for HTML elements and `RuleIndex` bucket keys.
        lower: String,
        /// Original case for non-HTML (e.g. SVG `clipPath`) elements.
        raw: String,
        /// True for `|tag` (requires empty namespace).
        empty_ns_only: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaseSensitivity {
    /// Case-insensitive for HTML spec case-insensitive attributes on HTML elements;
    /// case-sensitive otherwise.
    Default,
    /// `i` modifier: ASCII case-insensitive.
    AsciiInsensitive,
    /// `s` modifier: case-sensitive.
    Sensitive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttrOp {
    /// `[attr]`
    Exists,
    /// `[attr=val]`
    Equals(String),
    /// `[attr~=val]`
    Includes(String),
    /// `[attr|=val]`
    DashMatch(String),
    /// `[attr^=val]`
    Prefix(String),
    /// `[attr$=val]`
    Suffix(String),
    /// `[attr*=val]`
    Substring(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttrSelector {
    /// Lowercased name for HTML elements.
    pub(crate) lower_name: String,
    /// Original case name for non-HTML elements.
    pub(crate) raw_name: String,
    /// True for `[*|attr]`, false for `[attr]` / `[|attr]` (no-namespace attributes).
    pub(crate) any_namespace: bool,
    pub(crate) op: AttrOp,
    pub(crate) case: CaseSensitivity,
}

/// `An+B` step and offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NthFormula {
    pub(crate) a: i32,
    pub(crate) b: i32,
}

impl NthFormula {
    /// Returns true when `idx == a * n + b` for some integer `n >= 0` (`idx >= 1`).
    pub(crate) fn matches(self, idx: i32) -> bool {
        if idx < 1 {
            return false;
        }
        if self.a == 0 {
            return idx == self.b;
        }
        let diff = i64::from(idx) - i64::from(self.b);
        let step = i64::from(self.a);
        if diff == 0 {
            return true;
        }
        ((diff > 0) == (step > 0)) && (diff % step == 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PseudoClass {
    Root,
    FirstChild,
    LastChild,
    OnlyChild,
    NthChild(NthFormula, Option<Vec<Selector>>),
    NthLastChild(NthFormula, Option<Vec<Selector>>),
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthOfType(NthFormula),
    NthLastOfType(NthFormula),
    Empty,
    Not(Vec<Selector>),
    Is(Vec<Selector>),
    Where(Vec<Selector>),
    Has(Vec<RelativeSelector>),
    Link,
    AnyLink,
    Checked,
    Disabled,
    Enabled,
    Required,
    Optional,
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Default,
    Indeterminate,
    Lang(Vec<String>),
    /// `:dir(ltr)`, `:dir(rtl)`, or `None` for any other valid ident (never matches).
    Dir(Option<Direction>),
    /// `:hover`, `:active`, `:focus`, `:focus-visible`, `:focus-within`, `:visited`.
    Dynamic(PseudoState),
    /// Valid CSS pseudo-classes that never match in a static document (`:target`, `:defined`, …).
    Never,
}

/// Outcome of converting a selector list on a style rule.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConvertedSelectorList {
    pub(crate) selectors: Vec<Selector>,
    /// Canonical names (e.g. `"::selection"`, `"::-webkit-scrollbar"`) of unsupported
    /// pseudo-elements whose selector branches were ignored.
    pub(crate) unsupported_pseudo_elements: Vec<String>,
    /// Canonical fragments of selectors dropped because they use dynamic pseudo-classes
    /// on two different elements.
    pub(crate) unsupported_multi_dynamic: usize,
    /// True if any selector in an unforgiving list was syntactically invalid
    /// (in which case the entire rule must be dropped with `Severity::Invalid`).
    pub(crate) invalid: bool,
}

/// Evaluation context passed during selector matching.
pub(crate) struct MatchContext<'a> {
    pub(crate) doc: &'a Document,
    /// When computing a `StateVariant`, the single `(element, state)` condition assumed active.
    pub(crate) active_state: Option<(NodeId, PseudoState)>,
}

impl Selector {
    pub(crate) fn without_pseudo_element(&self) -> Self {
        let mut cloned = self.clone();
        if cloned.pseudo_element.take().is_some() {
            cloned.specificity = compute_selector_specificity(&cloned.compounds, None);
        }
        cloned
    }

    /// Computes a 64-bit bloom mask of features that MUST appear among the strict
    /// element ancestors of any matched subject element.
    pub(crate) fn required_ancestor_mask(&self) -> u64 {
        let mut mask = 0u64;
        for (idx, comb) in self.combinators.iter().enumerate() {
            // `compounds[idx]` is guaranteed to match a strict element ancestor of
            // the subject element iff the combinator immediately to its right is
            // `Child` (`>`) or `Descendant` (`' '`).
            if matches!(comb, Combinator::Child | Combinator::Descendant) {
                mask |= compound_required_bloom(&self.compounds[idx]);
            }
        }
        mask
    }

    /// Extracts `(candidate_prefix_selector, state)` pairs for `StyleMap::variants`.
    /// Each returned prefix selector matches candidate condition elements `E` whose
    /// subtree could change style when `E` enters `state`.
    pub(crate) fn dynamic_variants(&self) -> Vec<(Selector, PseudoState)> {
        let mut out = Vec::new();
        let last = self.compounds.len().saturating_sub(1);

        for (idx, compound) in self.compounds.iter().enumerate() {
            let subject_in_subtree = idx == last
                || matches!(
                    self.combinators.get(idx),
                    Some(Combinator::Child | Combinator::Descendant)
                );
            if !subject_in_subtree {
                continue;
            }

            let mut self_states = Vec::new();
            collect_compound_self_dynamic_states(compound, &mut self_states);
            if !self_states.is_empty() {
                let prefix = self.prefix_without_dynamic(idx);
                for st in self_states {
                    out.push((prefix.clone(), st));
                }
            }

            // Also handle nested selectors inside `:is(...)` / `:where(...)` / `:not(...)`
            // where the dynamic pseudo-class sits on an ancestor compound of that argument
            // (for example `.card:hover .mid { .title { ... } }`).
            for pc in &compound.pseudos {
                if let PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) =
                    pc
                {
                    for inner in list {
                        if inner.compounds.len() > 1 {
                            for (inner_prefix, st) in inner.dynamic_variants() {
                                // Only take inner ancestor states; `inner`'s rightmost
                                // compound was already handled by `self_states` above.
                                if inner_prefix.compounds.len() < inner.compounds.len() {
                                    out.push((inner_prefix, st));
                                }
                            }
                        }
                    }
                }
            }
        }

        out
    }

    fn prefix_without_dynamic(&self, up_to_inclusive: usize) -> Selector {
        let compounds: Vec<CompoundSelector> = self.compounds[..=up_to_inclusive]
            .iter()
            .map(strip_compound_dynamic)
            .collect();
        let combinators = self.combinators[..up_to_inclusive].to_vec();
        let specificity = compute_selector_specificity(&compounds, None);
        Selector {
            compounds,
            combinators,
            pseudo_element: None,
            specificity,
        }
    }
}

fn collect_compound_self_dynamic_states(c: &CompoundSelector, out: &mut Vec<PseudoState>) {
    for pc in &c.pseudos {
        match pc {
            PseudoClass::Dynamic(st) => {
                if !out.contains(st) {
                    out.push(*st);
                }
            }
            PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) => {
                for sel in list {
                    if let Some(last) = sel.compounds.last() {
                        collect_compound_self_dynamic_states(last, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn strip_compound_dynamic(c: &CompoundSelector) -> CompoundSelector {
    let mut pseudos = Vec::with_capacity(c.pseudos.len());
    let mut requires_hyperlink = false;
    for pc in &c.pseudos {
        match pc {
            PseudoClass::Dynamic(st) => {
                // `:visited` can only ever match hyperlinks (`<a href>`, `<area href>`),
                // so preserve `:any-link` on the candidate prefix to avoid testing
                // non-link elements for `PseudoState::Visited`.
                if *st == PseudoState::Visited {
                    requires_hyperlink = true;
                }
            }
            PseudoClass::Is(list) => {
                pseudos.push(PseudoClass::Is(
                    list.iter().map(strip_selector_dynamic).collect(),
                ));
            }
            PseudoClass::Where(list) => {
                pseudos.push(PseudoClass::Where(
                    list.iter().map(strip_selector_dynamic).collect(),
                ));
            }
            PseudoClass::Not(list) => {
                // Inside `:not(:hover)`, stripping `:hover` could turn `:not(:hover)`
                // into `:not(*)` (false) and miss candidates! Dropping a `:not(...)`
                // that depends on a dynamic state makes the candidate prefix slightly
                // broader (a safe superset), which the actual subtree recomputation filters.
                if list.iter().any(selector_has_any_dynamic) {
                    continue;
                }
                pseudos.push(PseudoClass::Not(list.clone()));
            }
            other => pseudos.push(other.clone()),
        }
    }
    if requires_hyperlink && !pseudos.contains(&PseudoClass::AnyLink) {
        pseudos.push(PseudoClass::AnyLink);
    }
    CompoundSelector {
        type_matcher: c.type_matcher.clone(),
        ids: c.ids.clone(),
        classes: c.classes.clone(),
        attrs: c.attrs.clone(),
        pseudos,
    }
}

fn strip_selector_dynamic(s: &Selector) -> Selector {
    let compounds: Vec<CompoundSelector> = s.compounds.iter().map(strip_compound_dynamic).collect();
    let specificity = compute_selector_specificity(&compounds, s.pseudo_element);
    Selector {
        compounds,
        combinators: s.combinators.clone(),
        pseudo_element: s.pseudo_element,
        specificity,
    }
}

fn selector_has_any_dynamic(s: &Selector) -> bool {
    s.compounds.iter().any(compound_has_any_dynamic)
}

fn compound_has_any_dynamic(c: &CompoundSelector) -> bool {
    c.pseudos.iter().any(|pc| match pc {
        PseudoClass::Dynamic(_) => true,
        PseudoClass::Is(list)
        | PseudoClass::Where(list)
        | PseudoClass::Not(list)
        | PseudoClass::NthChild(_, Some(list))
        | PseudoClass::NthLastChild(_, Some(list)) => list.iter().any(selector_has_any_dynamic),
        PseudoClass::Has(rels) => rels.iter().any(|r| selector_has_any_dynamic(&r.selector)),
        _ => false,
    })
}

/// Checks Requirement 5: returns true when dynamic pseudo-classes appear on two
/// or more distinct element positions within `sel`.
fn has_multi_element_dynamic_pseudos(sel: &Selector) -> bool {
    let mut dynamic_positions = 0usize;
    for compound in &sel.compounds {
        let mut has_self = false;
        let mut has_other = false;
        for pc in &compound.pseudos {
            match pc {
                PseudoClass::Dynamic(_) => has_self = true,
                PseudoClass::Is(list)
                | PseudoClass::Where(list)
                | PseudoClass::Not(list)
                | PseudoClass::NthChild(_, Some(list))
                | PseudoClass::NthLastChild(_, Some(list)) => {
                    for inner in list {
                        if has_multi_element_dynamic_pseudos(inner) {
                            return true;
                        }
                        if let Some((last, ancestors)) = inner.compounds.split_last() {
                            if compound_has_any_dynamic(last) {
                                has_self = true;
                            }
                            if ancestors.iter().any(compound_has_any_dynamic) {
                                has_other = true;
                            }
                        }
                    }
                }
                PseudoClass::Has(rels) => {
                    for rel in rels {
                        if has_multi_element_dynamic_pseudos(&rel.selector) {
                            return true;
                        }
                        if selector_has_any_dynamic(&rel.selector) {
                            has_other = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if has_self && has_other {
            return true;
        }
        if has_self || has_other {
            dynamic_positions += 1;
        }
    }
    dynamic_positions > 1
}

// ---------------------------------------------------------------------------
// Fast Rule Indexing & 64-bit Ancestor Bloom Filter
// ---------------------------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn bloom_bits(namespace_tag: u8, bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET ^ u64::from(namespace_tag);
    h = h.wrapping_mul(FNV_PRIME);
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    let bit1 = h & 63;
    let bit2 = (h >> 8) & 63;
    (1u64 << bit1) | (1u64 << bit2)
}

/// Bloom filter bits contributed by `elem` itself when it acts as an ancestor.
pub(crate) fn element_self_bloom(elem: &Element) -> u64 {
    let mut mask = if elem.is_html() {
        bloom_bits(1, elem.local_name().as_bytes())
    } else {
        bloom_bits(1, elem.local_name().to_ascii_lowercase().as_bytes())
    };
    if let Some(id) = elem.id() {
        mask |= bloom_bits(2, id.as_bytes());
    }
    for cls in elem.classes() {
        mask |= bloom_bits(3, cls.as_bytes());
    }
    mask
}

fn compound_required_bloom(c: &CompoundSelector) -> u64 {
    let mut mask = 0u64;
    if let TypeMatcher::Tag { lower, .. } = &c.type_matcher {
        mask |= bloom_bits(1, lower.as_bytes());
    }
    for id in &c.ids {
        mask |= bloom_bits(2, id.as_bytes());
    }
    for cls in &c.classes {
        mask |= bloom_bits(3, cls.as_bytes());
    }
    mask
}

/// Precomputes the strict-element-ancestor 64-bit bloom mask for every node in `doc`
/// (indexed by `NodeId::index()`).
pub(crate) fn compute_ancestor_blooms(doc: &Document) -> Vec<u64> {
    let mut blooms = vec![0u64; doc.len()];
    let mut self_masks = vec![0u64; doc.len()];
    for node in doc.descendants(doc.root()) {
        if let Some(elem) = doc.element(node) {
            self_masks[node.index()] = element_self_bloom(elem);
        }
        if let Some(parent) = doc.parent(node) {
            let parent_idx = parent.index();
            blooms[node.index()] = blooms[parent_idx] | self_masks[parent_idx];
        }
    }
    blooms
}

#[derive(Debug, Clone)]
struct IndexedEntry<T> {
    selector: Selector,
    required_ancestor_mask: u64,
    payload: T,
}

/// Indexes selector rules by their rightmost compound's most selective key
/// (`#id` → `.class` → `tag` → `universal`) so each DOM element only tests
/// candidate rules that can match it.
#[derive(Debug, Clone)]
pub(crate) struct RuleIndex<T> {
    by_id: HashMap<String, Vec<IndexedEntry<T>>>,
    by_class: HashMap<String, Vec<IndexedEntry<T>>>,
    by_tag: HashMap<String, Vec<IndexedEntry<T>>>,
    universal: Vec<IndexedEntry<T>>,
}

impl<T> Default for RuleIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> RuleIndex<T> {
    pub(crate) fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            by_class: HashMap::new(),
            by_tag: HashMap::new(),
            universal: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, selector: Selector, payload: T) {
        let required_ancestor_mask = selector.required_ancestor_mask();
        let entry = IndexedEntry {
            selector,
            required_ancestor_mask,
            payload,
        };

        let Some(rightmost) = entry.selector.compounds.last() else {
            self.universal.push(entry);
            return;
        };

        if let Some(id) = rightmost.ids.first() {
            self.by_id.entry(id.clone()).or_default().push(entry);
        } else if let Some(cls) = rightmost.classes.first() {
            self.by_class.entry(cls.clone()).or_default().push(entry);
        } else if let TypeMatcher::Tag { lower, .. } = &rightmost.type_matcher {
            self.by_tag.entry(lower.clone()).or_default().push(entry);
        } else {
            self.universal.push(entry);
        }
    }

    /// Visits every candidate `(selector, payload)` whose rightmost key and
    /// ancestor bloom filter are compatible with `elem`. Each inserted entry
    /// lives in at most one bucket, so no candidate is visited twice.
    pub(crate) fn for_each_candidate<F>(&self, elem: &Element, ancestor_bloom: u64, mut visit: F)
    where
        F: FnMut(&Selector, &T),
    {
        let mut scan = |bucket: &[IndexedEntry<T>]| {
            for entry in bucket {
                if (entry.required_ancestor_mask & ancestor_bloom) == entry.required_ancestor_mask {
                    visit(&entry.selector, &entry.payload);
                }
            }
        };

        if let Some(id) = elem.id()
            && let Some(bucket) = self.by_id.get(id)
        {
            scan(bucket);
        }

        if !self.by_class.is_empty()
            && let Some(class_attr) = elem.attr("class")
        {
            // Track seen class slices on the stack/small vec so `<div class="a a">`
            // never visits the `"a"` bucket twice.
            let mut seen: [&str; 8] = [""; 8];
            let mut seen_len = 0usize;
            let mut extra_seen: Vec<&str> = Vec::new();
            for cls in class_attr.split_ascii_whitespace() {
                let already = if seen_len <= 8 {
                    seen[..seen_len].contains(&cls)
                } else {
                    seen.contains(&cls) || extra_seen.contains(&cls)
                };
                if already {
                    continue;
                }
                if seen_len < 8 {
                    seen[seen_len] = cls;
                    seen_len += 1;
                } else {
                    seen_len += 1;
                    extra_seen.push(cls);
                }
                if let Some(bucket) = self.by_class.get(cls) {
                    scan(bucket);
                }
            }
        }

        if !self.by_tag.is_empty() {
            if elem.is_html() {
                if let Some(bucket) = self.by_tag.get(elem.local_name()) {
                    scan(bucket);
                }
            } else {
                let lower = elem.local_name().to_ascii_lowercase();
                if let Some(bucket) = self.by_tag.get(&lower) {
                    scan(bucket);
                }
            }
        }

        scan(&self.universal);
    }
}

// ---------------------------------------------------------------------------
// Conversion from `swc_css_ast`
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ConvertError {
    Invalid,
    UnsupportedPseudoElement(String),
}

struct ConvertOpts<'a> {
    parent_selectors: Option<&'a [Selector]>,
    allow_pseudo_elements: bool,
    inside_has: bool,
}

/// Converts a top-level (or nested `@media` / style rule) `ast::SelectorList`.
pub(crate) fn convert_selector_list(
    list: &ast::SelectorList,
    parent: Option<&[Selector]>,
) -> ConvertedSelectorList {
    if list.children.is_empty() {
        return ConvertedSelectorList {
            invalid: true,
            ..Default::default()
        };
    }

    let parent_no_pe: Option<Vec<Selector>> = parent.map(|ps| {
        ps.iter()
            .filter(|s| s.pseudo_element.is_none())
            .cloned()
            .collect()
    });

    let opts = ConvertOpts {
        parent_selectors: parent_no_pe.as_deref(),
        allow_pseudo_elements: true,
        inside_has: false,
    };

    let mut out = ConvertedSelectorList::default();
    for cs in &list.children {
        let mut used_nesting = false;
        match convert_complex_selector(cs, &opts, &mut used_nesting) {
            Ok(mut sel) => {
                if let Some(parents) = opts.parent_selectors
                    && !used_nesting
                {
                    sel = prepend_implicit_parent(parents, Combinator::Descendant, sel);
                }
                if has_multi_element_dynamic_pseudos(&sel) {
                    out.unsupported_multi_dynamic += 1;
                    continue;
                }
                out.selectors.push(sel);
            }
            Err(ConvertError::UnsupportedPseudoElement(name)) => {
                out.unsupported_pseudo_elements.push(name);
            }
            Err(ConvertError::Invalid) => {
                return ConvertedSelectorList {
                    invalid: true,
                    ..Default::default()
                };
            }
        }
    }
    out
}

/// Converts a nested style rule's `ast::RelativeSelectorList` against `parent`.
pub(crate) fn convert_relative_selector_list(
    list: &ast::RelativeSelectorList,
    parent: &[Selector],
) -> ConvertedSelectorList {
    if list.children.is_empty() {
        return ConvertedSelectorList {
            invalid: true,
            ..Default::default()
        };
    }

    let parents_no_pe: Vec<Selector> = parent
        .iter()
        .filter(|s| s.pseudo_element.is_none())
        .cloned()
        .collect();

    let opts = ConvertOpts {
        parent_selectors: Some(&parents_no_pe),
        allow_pseudo_elements: true,
        inside_has: false,
    };

    let mut out = ConvertedSelectorList::default();
    for rel in &list.children {
        let leading_comb = match &rel.combinator {
            Some(c) => match convert_combinator(c) {
                Ok(comb) => Some(comb),
                Err(e) => match e {
                    ConvertError::Invalid => {
                        return ConvertedSelectorList {
                            invalid: true,
                            ..Default::default()
                        };
                    }
                    ConvertError::UnsupportedPseudoElement(name) => {
                        out.unsupported_pseudo_elements.push(name);
                        continue;
                    }
                },
            },
            None => None,
        };

        let mut used_nesting = false;
        match convert_complex_selector(&rel.selector, &opts, &mut used_nesting) {
            Ok(mut sel) => {
                if let Some(comb) = leading_comb {
                    sel = prepend_implicit_parent(&parents_no_pe, comb, sel);
                } else if !used_nesting {
                    sel = prepend_implicit_parent(&parents_no_pe, Combinator::Descendant, sel);
                }
                if has_multi_element_dynamic_pseudos(&sel) {
                    out.unsupported_multi_dynamic += 1;
                    continue;
                }
                out.selectors.push(sel);
            }
            Err(ConvertError::UnsupportedPseudoElement(name)) => {
                out.unsupported_pseudo_elements.push(name);
            }
            Err(ConvertError::Invalid) => {
                return ConvertedSelectorList {
                    invalid: true,
                    ..Default::default()
                };
            }
        }
    }
    out
}

/// Evaluates whether a single `<complex-selector>` is valid and supported for
/// `@supports selector(...)`.
pub(crate) fn is_supported_complex_selector(cs: &ast::ComplexSelector) -> bool {
    let opts = ConvertOpts {
        parent_selectors: None,
        allow_pseudo_elements: true,
        inside_has: false,
    };
    let mut used_nesting = false;
    match convert_complex_selector(cs, &opts, &mut used_nesting) {
        Ok(sel) => !has_multi_element_dynamic_pseudos(&sel),
        Err(_) => false,
    }
}

/// Parses a CSS selector list string into a [`ConvertedSelectorList`].
#[cfg(test)]
pub(crate) fn parse_selector_list_str(selector_text: &str) -> ConvertedSelectorList {
    let wrapped = format!("{selector_text} {{}}");
    let cm: Lrc<SourceMap> = Lrc::default();
    let fm = cm.new_source_file(FileName::Anon.into(), wrapped);
    let mut errors = Vec::new();
    let parsed: Result<Result<ast::Stylesheet, _>, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            swc_css_parser::parse_file(&fm, None, Default::default(), &mut errors)
        }));
    let Ok(Ok(sheet)) = parsed else {
        return ConvertedSelectorList {
            invalid: true,
            ..Default::default()
        };
    };
    if sheet.rules.len() != 1 {
        return ConvertedSelectorList {
            invalid: true,
            ..Default::default()
        };
    }
    if let ast::Rule::QualifiedRule(qr) = &sheet.rules[0]
        && let ast::QualifiedRulePrelude::SelectorList(sl) = &qr.prelude
        && errors.is_empty()
    {
        return convert_selector_list(sl, None);
    }
    ConvertedSelectorList {
        invalid: true,
        ..Default::default()
    }
}

fn prepend_implicit_parent(parents: &[Selector], comb: Combinator, mut child: Selector) -> Selector {
    let mut parent_compound = CompoundSelector::default();
    parent_compound.pseudos.push(PseudoClass::Is(parents.to_vec()));
    child.compounds.insert(0, parent_compound);
    child.combinators.insert(0, comb);
    child.specificity = compute_selector_specificity(&child.compounds, child.pseudo_element);
    child
}

fn convert_combinator(c: &ast::Combinator) -> Result<Combinator, ConvertError> {
    match c.value {
        ast::CombinatorValue::Descendant => Ok(Combinator::Descendant),
        ast::CombinatorValue::Child => Ok(Combinator::Child),
        ast::CombinatorValue::NextSibling => Ok(Combinator::NextSibling),
        ast::CombinatorValue::LaterSibling => Ok(Combinator::LaterSibling),
        ast::CombinatorValue::Column => Err(ConvertError::Invalid),
    }
}

fn convert_complex_selector(
    cs: &ast::ComplexSelector,
    opts: &ConvertOpts<'_>,
    used_nesting: &mut bool,
) -> Result<Selector, ConvertError> {
    if cs.children.is_empty() {
        return Err(ConvertError::Invalid);
    }

    let mut compounds = Vec::new();
    let mut combinators = Vec::new();
    let mut pseudo_element: Option<PseudoElement> = None;
    let mut expect_compound = true;
    let total = cs.children.len();

    for (idx, item) in cs.children.iter().enumerate() {
        let is_last = idx + 1 == total;
        match item {
            ast::ComplexSelectorChildren::CompoundSelector(comp) => {
                if !expect_compound {
                    return Err(ConvertError::Invalid);
                }
                let allow_pe_here = opts.allow_pseudo_elements && is_last;
                let (converted, pe) =
                    convert_compound_selector(comp, opts, allow_pe_here, used_nesting)?;
                if pe.is_some() {
                    pseudo_element = pe;
                }
                compounds.push(converted);
                expect_compound = false;
            }
            ast::ComplexSelectorChildren::Combinator(comb) => {
                // Leading combinators in non-relative ComplexSelector (or trailing combinators)
                // are syntax errors.
                if expect_compound || is_last {
                    return Err(ConvertError::Invalid);
                }
                combinators.push(convert_combinator(comb)?);
                expect_compound = true;
            }
        }
    }

    if compounds.is_empty() || compounds.len() != combinators.len() + 1 {
        return Err(ConvertError::Invalid);
    }

    let specificity = compute_selector_specificity(&compounds, pseudo_element);
    Ok(Selector {
        compounds,
        combinators,
        pseudo_element,
        specificity,
    })
}

fn convert_compound_selector(
    comp: &ast::CompoundSelector,
    opts: &ConvertOpts<'_>,
    allow_pseudo_element: bool,
    used_nesting: &mut bool,
) -> Result<(CompoundSelector, Option<PseudoElement>), ConvertError> {
    if comp.nesting_selector.is_none()
        && comp.type_selector.is_none()
        && comp.subclass_selectors.is_empty()
    {
        return Err(ConvertError::Invalid);
    }

    let mut out = CompoundSelector::default();
    let mut found_pe: Option<PseudoElement> = None;

    if comp.nesting_selector.is_some() {
        *used_nesting = true;
        match opts.parent_selectors {
            Some(parents) => out.pseudos.push(PseudoClass::Is(parents.to_vec())),
            None => out.pseudos.push(PseudoClass::Root),
        }
    }

    if let Some(ts) = &comp.type_selector {
        out.type_matcher = convert_type_selector(ts)?;
    }

    for sub in &comp.subclass_selectors {
        // Per Selectors 4 §17, simple/subclass selectors after a pseudo-element
        // (such as `::before.cls`, `::before:hover`, or `::before::after`) are invalid.
        if found_pe.is_some() {
            return Err(ConvertError::Invalid);
        }

        match sub {
            ast::SubclassSelector::Id(id) => {
                out.ids.push(id.text.value.to_string());
            }
            ast::SubclassSelector::Class(cls) => {
                out.classes.push(cls.text.value.to_string());
            }
            ast::SubclassSelector::Attribute(attr) => {
                out.attrs.push(convert_attribute_selector(attr)?);
            }
            ast::SubclassSelector::PseudoClass(pc) => {
                let name_lower = pc.name.value.as_str().to_ascii_lowercase();
                // Legacy single-colon pseudo-elements from Selectors 4 §17.1.
                if pc.children.is_none() {
                    match name_lower.as_str() {
                        "before" => {
                            if !allow_pseudo_element {
                                return Err(ConvertError::Invalid);
                            }
                            found_pe = Some(PseudoElement::Before);
                            continue;
                        }
                        "after" => {
                            if !allow_pseudo_element {
                                return Err(ConvertError::Invalid);
                            }
                            found_pe = Some(PseudoElement::After);
                            continue;
                        }
                        "first-line" | "first-letter" => {
                            if !allow_pseudo_element {
                                return Err(ConvertError::Invalid);
                            }
                            return Err(ConvertError::UnsupportedPseudoElement(format!(
                                "::{name_lower}"
                            )));
                        }
                        _ => {}
                    }
                }
                out.pseudos
                    .push(convert_pseudo_class(pc, &name_lower, opts, used_nesting)?);
            }
            ast::SubclassSelector::PseudoElement(pe) => {
                if !allow_pseudo_element {
                    return Err(ConvertError::Invalid);
                }
                found_pe = Some(convert_pseudo_element(pe)?);
            }
        }
    }

    Ok((out, found_pe))
}

fn convert_type_selector(ts: &ast::TypeSelector) -> Result<TypeMatcher, ConvertError> {
    match ts {
        ast::TypeSelector::Universal(u) => match &u.prefix {
            None => Ok(TypeMatcher::Universal),
            Some(ns_prefix) => match &ns_prefix.namespace {
                None => Ok(TypeMatcher::EmptyNsUniversal),
                Some(ast::Namespace::Any(_)) => Ok(TypeMatcher::Universal),
                // Undeclared named namespace prefix (since `@namespace` is unsupported).
                Some(ast::Namespace::Named(_)) => Err(ConvertError::Invalid),
            },
        },
        ast::TypeSelector::TagName(tag) => {
            let empty_ns_only = match &tag.name.prefix {
                None => false,
                Some(ns_prefix) => match &ns_prefix.namespace {
                    None => true,
                    Some(ast::Namespace::Any(_)) => false,
                    Some(ast::Namespace::Named(_)) => return Err(ConvertError::Invalid),
                },
            };
            let raw = tag.name.value.value.to_string();
            let lower = raw.to_ascii_lowercase();
            Ok(TypeMatcher::Tag {
                lower,
                raw,
                empty_ns_only,
            })
        }
    }
}

fn convert_attribute_selector(attr: &ast::AttributeSelector) -> Result<AttrSelector, ConvertError> {
    let any_namespace = match &attr.name.prefix {
        None => false,
        Some(ns_prefix) => match &ns_prefix.namespace {
            None => false,
            Some(ast::Namespace::Any(_)) => true,
            Some(ast::Namespace::Named(_)) => return Err(ConvertError::Invalid),
        },
    };

    let raw_name = attr.name.value.value.to_string();
    let lower_name = raw_name.to_ascii_lowercase();

    let case = match &attr.modifier {
        None => CaseSensitivity::Default,
        Some(m) => {
            if attr.matcher.is_none() {
                return Err(ConvertError::Invalid);
            }
            match m.value.value.as_str().to_ascii_lowercase().as_str() {
                "i" => CaseSensitivity::AsciiInsensitive,
                "s" => CaseSensitivity::Sensitive,
                _ => return Err(ConvertError::Invalid),
            }
        }
    };

    let op = match (&attr.matcher, &attr.value) {
        (None, None) => AttrOp::Exists,
        (Some(m), Some(v)) => {
            let val = match v {
                ast::AttributeSelectorValue::Str(s) => s.value.to_string(),
                ast::AttributeSelectorValue::Ident(id) => id.value.to_string(),
            };
            match m.value {
                ast::AttributeSelectorMatcherValue::Equals => AttrOp::Equals(val),
                ast::AttributeSelectorMatcherValue::Tilde => AttrOp::Includes(val),
                ast::AttributeSelectorMatcherValue::Bar => AttrOp::DashMatch(val),
                ast::AttributeSelectorMatcherValue::Caret => AttrOp::Prefix(val),
                ast::AttributeSelectorMatcherValue::Dollar => AttrOp::Suffix(val),
                ast::AttributeSelectorMatcherValue::Asterisk => AttrOp::Substring(val),
            }
        }
        _ => return Err(ConvertError::Invalid),
    };

    Ok(AttrSelector {
        lower_name,
        raw_name,
        any_namespace,
        op,
        case,
    })
}

fn convert_pseudo_element(pe: &ast::PseudoElementSelector) -> Result<PseudoElement, ConvertError> {
    let name_lower = pe.name.value.as_str().to_ascii_lowercase();
    if pe.children.is_some() {
        // Functional pseudo-elements (`::part(...)`, `::slotted(...)`, `::cue(...)`,
        // `::highlight(...)`, `::-webkit-*(...)`).
        if name_lower.starts_with('-')
            || matches!(
                name_lower.as_str(),
                "part"
                    | "slotted"
                    | "cue"
                    | "cue-region"
                    | "highlight"
                    | "view-transition-group"
                    | "view-transition-image-pair"
                    | "view-transition-old"
                    | "view-transition-new"
            )
        {
            return Err(ConvertError::UnsupportedPseudoElement(format!(
                "::{name_lower}"
            )));
        }
        return Err(ConvertError::Invalid);
    }

    match name_lower.as_str() {
        "before" => Ok(PseudoElement::Before),
        "after" => Ok(PseudoElement::After),
        "placeholder" => Ok(PseudoElement::Placeholder),
        _ if name_lower.starts_with('-') => Err(ConvertError::UnsupportedPseudoElement(format!(
            "::{name_lower}"
        ))),
        "marker"
        | "selection"
        | "first-line"
        | "first-letter"
        | "backdrop"
        | "file-selector-button"
        | "target-text"
        | "spelling-error"
        | "grammar-error"
        | "details-content"
        | "view-transition" => Err(ConvertError::UnsupportedPseudoElement(format!(
            "::{name_lower}"
        ))),
        _ => Err(ConvertError::Invalid),
    }
}

fn convert_pseudo_class(
    pc: &ast::PseudoClassSelector,
    name_lower: &str,
    opts: &ConvertOpts<'_>,
    used_nesting: &mut bool,
) -> Result<PseudoClass, ConvertError> {
    match &pc.children {
        None => match name_lower {
            "root" | "scope" => Ok(PseudoClass::Root),
            "first-child" => Ok(PseudoClass::FirstChild),
            "last-child" => Ok(PseudoClass::LastChild),
            "only-child" => Ok(PseudoClass::OnlyChild),
            "first-of-type" => Ok(PseudoClass::FirstOfType),
            "last-of-type" => Ok(PseudoClass::LastOfType),
            "only-of-type" => Ok(PseudoClass::OnlyOfType),
            "empty" => Ok(PseudoClass::Empty),
            "link" => Ok(PseudoClass::Link),
            "any-link" => Ok(PseudoClass::AnyLink),
            "checked" => Ok(PseudoClass::Checked),
            "disabled" => Ok(PseudoClass::Disabled),
            "enabled" => Ok(PseudoClass::Enabled),
            "required" => Ok(PseudoClass::Required),
            "optional" => Ok(PseudoClass::Optional),
            "read-only" => Ok(PseudoClass::ReadOnly),
            "read-write" => Ok(PseudoClass::ReadWrite),
            "placeholder-shown" => Ok(PseudoClass::PlaceholderShown),
            "default" => Ok(PseudoClass::Default),
            "indeterminate" => Ok(PseudoClass::Indeterminate),
            "hover" => Ok(PseudoClass::Dynamic(PseudoState::Hover)),
            "active" => Ok(PseudoClass::Dynamic(PseudoState::Active)),
            "focus" => Ok(PseudoClass::Dynamic(PseudoState::Focus)),
            "focus-visible" => Ok(PseudoClass::Dynamic(PseudoState::FocusVisible)),
            "focus-within" => Ok(PseudoClass::Dynamic(PseudoState::FocusWithin)),
            "visited" => Ok(PseudoClass::Dynamic(PseudoState::Visited)),
            "target"
            | "target-within"
            | "local-link"
            | "defined"
            | "fullscreen"
            | "modal"
            | "picture-in-picture"
            | "popover-open"
            | "autofill"
            | "-webkit-autofill"
            | "playing"
            | "paused"
            | "seeking"
            | "buffering"
            | "stalled"
            | "muted"
            | "volume-locked"
            | "open"
            | "closed"
            | "valid"
            | "invalid"
            | "user-valid"
            | "user-invalid"
            | "in-range"
            | "out-of-range"
            | "blank"
            | "current"
            | "past"
            | "future" => Ok(PseudoClass::Never),
            _ => Err(ConvertError::Invalid),
        },
        Some(children) => convert_functional_pseudo_class(name_lower, children, opts, used_nesting),
    }
}

fn convert_functional_pseudo_class(
    name_lower: &str,
    children: &[ast::PseudoClassSelectorChildren],
    opts: &ConvertOpts<'_>,
    used_nesting: &mut bool,
) -> Result<PseudoClass, ConvertError> {
    let inner_opts = ConvertOpts {
        parent_selectors: opts.parent_selectors,
        allow_pseudo_elements: false,
        inside_has: opts.inside_has,
    };

    match name_lower {
        "not" => {
            if children.len() != 1 {
                return Err(ConvertError::Invalid);
            }
            let ast::PseudoClassSelectorChildren::SelectorList(sl) = &children[0] else {
                return Err(ConvertError::Invalid);
            };
            if sl.children.is_empty() {
                return Err(ConvertError::Invalid);
            }
            let mut list = Vec::with_capacity(sl.children.len());
            for cs in &sl.children {
                match convert_complex_selector(cs, &inner_opts, used_nesting) {
                    Ok(s) => list.push(s),
                    // Any invalid or pseudo-element argument inside unforgiving `:not()`
                    // invalidates the whole `:not(...)` selector per Selectors 4.
                    Err(_) => return Err(ConvertError::Invalid),
                }
            }
            Ok(PseudoClass::Not(list))
        }
        "is" | "where" => {
            let mut list = Vec::new();
            match children.first() {
                Some(ast::PseudoClassSelectorChildren::ForgivingSelectorList(fsl))
                    if children.len() == 1 =>
                {
                    for fcs in &fsl.children {
                        if let ast::ForgivingComplexSelector::ComplexSelector(cs) = fcs {
                            let mut branch_nesting = false;
                            if let Ok(s) =
                                convert_complex_selector(cs, &inner_opts, &mut branch_nesting)
                            {
                                *used_nesting |= branch_nesting;
                                list.push(s);
                            }
                        }
                    }
                }
                // Empty `:is()` / `:where()` is valid per forgiving selector list grammar
                // and matches nothing.
                None => {}
                _ => return Err(ConvertError::Invalid),
            }
            if name_lower == "where" {
                Ok(PseudoClass::Where(list))
            } else {
                Ok(PseudoClass::Is(list))
            }
        }
        "has" => {
            // Nested `:has()` inside `:has()` is disallowed by Selectors 4 §4.5.
            if opts.inside_has {
                return Err(ConvertError::Invalid);
            }
            let has_opts = ConvertOpts {
                parent_selectors: opts.parent_selectors,
                allow_pseudo_elements: false,
                inside_has: true,
            };
            if children.len() != 1 {
                return Err(ConvertError::Invalid);
            }
            let ast::PseudoClassSelectorChildren::ForgivingRelativeSelectorList(frsl) =
                &children[0]
            else {
                return Err(ConvertError::Invalid);
            };
            if frsl.children.is_empty() {
                return Err(ConvertError::Invalid);
            }
            let mut list = Vec::new();
            for item in &frsl.children {
                if let ast::ForgivingRelativeSelector::RelativeSelector(rs) = item {
                    let leading = match &rs.combinator {
                        Some(c) => {
                            let Ok(comb) = convert_combinator(c) else {
                                continue;
                            };
                            comb
                        }
                        None => Combinator::Descendant,
                    };
                    let mut branch_nesting = false;
                    if let Ok(selector) =
                        convert_complex_selector(&rs.selector, &has_opts, &mut branch_nesting)
                    {
                        *used_nesting |= branch_nesting;
                        list.push(RelativeSelector { leading, selector });
                    }
                }
            }
            // Note: in modern CSS Selectors 4, `:has()` was switched to unforgiving
            // when all arguments fail or in browsers; if at least one valid relative
            // selector parsed, keep it; if none parsed at all, reject as invalid so
            // `@supports selector(:has(:unknown))` is false!
            if list.is_empty() {
                return Err(ConvertError::Invalid);
            }
            Ok(PseudoClass::Has(list))
        }
        "nth-child" | "nth-last-child" => {
            let (formula, of_list) = convert_nth_with_optional_of(children, &inner_opts, used_nesting)?;
            if name_lower == "nth-child" {
                Ok(PseudoClass::NthChild(formula, of_list))
            } else {
                Ok(PseudoClass::NthLastChild(formula, of_list))
            }
        }
        "nth-of-type" | "nth-last-of-type" => {
            if children.len() != 1 {
                return Err(ConvertError::Invalid);
            }
            let ast::PseudoClassSelectorChildren::AnPlusB(ab) = &children[0] else {
                return Err(ConvertError::Invalid);
            };
            let formula = convert_an_plus_b(ab)?;
            if name_lower == "nth-of-type" {
                Ok(PseudoClass::NthOfType(formula))
            } else {
                Ok(PseudoClass::NthLastOfType(formula))
            }
        }
        "dir" => {
            if children.len() != 1 {
                return Err(ConvertError::Invalid);
            }
            let ast::PseudoClassSelectorChildren::Ident(id) = &children[0] else {
                return Err(ConvertError::Invalid);
            };
            let dir = match id.value.as_str().to_ascii_lowercase().as_str() {
                "ltr" => Some(Direction::Ltr),
                "rtl" => Some(Direction::Rtl),
                // Selectors 4 §12.1: unknown directionality idents are valid syntax
                // that simply match nothing.
                _ => None,
            };
            Ok(PseudoClass::Dir(dir))
        }
        "lang" => {
            let ranges = convert_lang_children(children)?;
            Ok(PseudoClass::Lang(ranges))
        }
        _ => Err(ConvertError::Invalid),
    }
}

fn convert_nth_with_optional_of(
    children: &[ast::PseudoClassSelectorChildren],
    inner_opts: &ConvertOpts<'_>,
    used_nesting: &mut bool,
) -> Result<(NthFormula, Option<Vec<Selector>>), ConvertError> {
    match children {
        [ast::PseudoClassSelectorChildren::AnPlusB(ab)] => Ok((convert_an_plus_b(ab)?, None)),
        [
            ast::PseudoClassSelectorChildren::AnPlusB(ab),
            ast::PseudoClassSelectorChildren::Ident(of_ident),
            ast::PseudoClassSelectorChildren::SelectorList(sl),
        ] => {
            if !of_ident.value.eq_ignore_ascii_case("of") || sl.children.is_empty() {
                return Err(ConvertError::Invalid);
            }
            let formula = convert_an_plus_b(ab)?;
            let mut list = Vec::with_capacity(sl.children.len());
            for cs in &sl.children {
                match convert_complex_selector(cs, inner_opts, used_nesting) {
                    Ok(s) => list.push(s),
                    Err(_) => return Err(ConvertError::Invalid),
                }
            }
            Ok((formula, Some(list)))
        }
        _ => Err(ConvertError::Invalid),
    }
}

fn convert_an_plus_b(ab: &ast::AnPlusB) -> Result<NthFormula, ConvertError> {
    match ab {
        ast::AnPlusB::Ident(id) => match id.value.as_str().to_ascii_lowercase().as_str() {
            "odd" => Ok(NthFormula { a: 2, b: 1 }),
            "even" => Ok(NthFormula { a: 2, b: 0 }),
            _ => Err(ConvertError::Invalid),
        },
        ast::AnPlusB::AnPlusBNotation(n) => Ok(NthFormula {
            a: n.a.unwrap_or(0),
            b: n.b.unwrap_or(0),
        }),
    }
}

fn convert_lang_children(
    children: &[ast::PseudoClassSelectorChildren],
) -> Result<Vec<String>, ConvertError> {
    if children.is_empty() {
        return Err(ConvertError::Invalid);
    }

    // Standard `[Ident|Str, (Delimiter(','), Ident|Str)*]` path from `swc_css_parser`,
    // plus fallback token assembly for unquoted wildcard ranges like `:lang(*-CH)`.
    if children
        .iter()
        .any(|c| matches!(c, ast::PseudoClassSelectorChildren::PreservedToken(_)))
    {
        return convert_lang_preserved_tokens(children);
    }

    let mut ranges = Vec::new();
    let mut expect_value = true;
    for child in children {
        if expect_value {
            let val = match child {
                ast::PseudoClassSelectorChildren::Ident(id) => id.value.to_string(),
                ast::PseudoClassSelectorChildren::Str(s) => s.value.to_string(),
                _ => return Err(ConvertError::Invalid),
            };
            if val.is_empty() {
                return Err(ConvertError::Invalid);
            }
            ranges.push(val.to_ascii_lowercase());
            expect_value = false;
        } else {
            match child {
                ast::PseudoClassSelectorChildren::Delimiter(d)
                    if d.value == ast::DelimiterValue::Comma =>
                {
                    expect_value = true;
                }
                _ => return Err(ConvertError::Invalid),
            }
        }
    }
    if expect_value || ranges.is_empty() {
        return Err(ConvertError::Invalid);
    }
    Ok(ranges)
}

fn convert_lang_preserved_tokens(
    children: &[ast::PseudoClassSelectorChildren],
) -> Result<Vec<String>, ConvertError> {
    let mut ranges = Vec::new();
    let mut current = String::new();
    for child in children {
        let ast::PseudoClassSelectorChildren::PreservedToken(ts) = child else {
            return Err(ConvertError::Invalid);
        };
        match &ts.token {
            ast::Token::WhiteSpace { .. } => {}
            ast::Token::Comma => {
                if current.is_empty() {
                    return Err(ConvertError::Invalid);
                }
                ranges.push(std::mem::take(&mut current).to_ascii_lowercase());
            }
            ast::Token::Delim { value: '*' } => current.push('*'),
            ast::Token::Ident { value, .. } => current.push_str(value.as_str()),
            ast::Token::String { value, .. } if current.is_empty() => {
                if value.is_empty() {
                    return Err(ConvertError::Invalid);
                }
                current.push_str(value.as_str());
            }
            _ => return Err(ConvertError::Invalid),
        }
    }
    if current.is_empty() {
        return Err(ConvertError::Invalid);
    }
    ranges.push(current.to_ascii_lowercase());
    if ranges
        .iter()
        .all(|r| r.split('-').all(|sub| sub == "*" || (!sub.is_empty() && sub.bytes().all(|b| b.is_ascii_alphanumeric()))))
    {
        Ok(ranges)
    } else {
        Err(ConvertError::Invalid)
    }
}

// ---------------------------------------------------------------------------
// Specificity Computation (Selectors Level 4 §16)
// ---------------------------------------------------------------------------

fn compute_selector_specificity(
    compounds: &[CompoundSelector],
    pseudo_element: Option<PseudoElement>,
) -> Specificity {
    let mut spec = compounds
        .iter()
        .fold(Specificity::default(), |acc, c| acc + compute_compound_specificity(c));
    if pseudo_element.is_some() {
        spec = spec + Specificity(0, 0, 1);
    }
    spec
}

fn compute_compound_specificity(c: &CompoundSelector) -> Specificity {
    let a = c.ids.len() as u32;
    let b = (c.classes.len() + c.attrs.len()) as u32;
    let c_type = u32::from(matches!(c.type_matcher, TypeMatcher::Tag { .. }));
    let mut spec = Specificity(a, b, c_type);
    for pc in &c.pseudos {
        spec = spec + compute_pseudo_class_specificity(pc);
    }
    spec
}

fn compute_pseudo_class_specificity(pc: &PseudoClass) -> Specificity {
    match pc {
        PseudoClass::Where(_) => Specificity(0, 0, 0),
        PseudoClass::Is(list) | PseudoClass::Not(list) => list
            .iter()
            .map(|s| s.specificity)
            .max()
            .unwrap_or_default(),
        PseudoClass::Has(rels) => rels
            .iter()
            .map(|r| r.selector.specificity)
            .max()
            .unwrap_or_default(),
        PseudoClass::NthChild(_, Some(list)) | PseudoClass::NthLastChild(_, Some(list)) => {
            Specificity(0, 1, 0)
                + list
                    .iter()
                    .map(|s| s.specificity)
                    .max()
                    .unwrap_or_default()
        }
        _ => Specificity(0, 1, 0),
    }
}

// ---------------------------------------------------------------------------
// DOM Selector Matching
// ---------------------------------------------------------------------------

/// Evaluates whether `sel` (ignoring `sel.pseudo_element`, which selects a
/// sub-target on the matched element) matches `node` in `ctx`.
pub(crate) fn matches_selector(sel: &Selector, node: NodeId, ctx: &MatchContext<'_>) -> bool {
    let Some(elem) = ctx.doc.element(node) else {
        return false;
    };
    let last_idx = sel.compounds.len() - 1;
    if !matches_compound(&sel.compounds[last_idx], node, elem, ctx) {
        return false;
    }
    if last_idx == 0 {
        return true;
    }
    matches_chain(sel, last_idx - 1, node, ctx)
}

fn matches_chain(
    sel: &Selector,
    compound_idx: usize,
    right_node: NodeId,
    ctx: &MatchContext<'_>,
) -> bool {
    let comb = sel.combinators[compound_idx];
    let target_compound = &sel.compounds[compound_idx];

    match comb {
        Combinator::Child => {
            let Some(parent) = ctx.doc.parent(right_node) else {
                return false;
            };
            let Some(parent_elem) = ctx.doc.element(parent) else {
                return false;
            };
            if !matches_compound(target_compound, parent, parent_elem, ctx) {
                return false;
            }
            compound_idx == 0 || matches_chain(sel, compound_idx - 1, parent, ctx)
        }
        Combinator::Descendant => {
            for anc in ctx.doc.ancestors(right_node) {
                let Some(anc_elem) = ctx.doc.element(anc) else {
                    continue;
                };
                if matches_compound(target_compound, anc, anc_elem, ctx)
                    && (compound_idx == 0 || matches_chain(sel, compound_idx - 1, anc, ctx))
                {
                    return true;
                }
            }
            false
        }
        Combinator::NextSibling => {
            let Some(prev) = ctx.doc.prev_element_sibling(right_node) else {
                return false;
            };
            let Some(prev_elem) = ctx.doc.element(prev) else {
                return false;
            };
            if !matches_compound(target_compound, prev, prev_elem, ctx) {
                return false;
            }
            compound_idx == 0 || matches_chain(sel, compound_idx - 1, prev, ctx)
        }
        Combinator::LaterSibling => {
            for prev in ctx.doc.prev_siblings(right_node) {
                let Some(prev_elem) = ctx.doc.element(prev) else {
                    continue;
                };
                if matches_compound(target_compound, prev, prev_elem, ctx)
                    && (compound_idx == 0 || matches_chain(sel, compound_idx - 1, prev, ctx))
                {
                    return true;
                }
            }
            false
        }
    }
}

fn matches_compound(
    c: &CompoundSelector,
    node: NodeId,
    elem: &Element,
    ctx: &MatchContext<'_>,
) -> bool {
    match &c.type_matcher {
        TypeMatcher::Universal => {}
        TypeMatcher::EmptyNsUniversal => {
            if elem.name.ns != ns!() {
                return false;
            }
        }
        TypeMatcher::Tag {
            lower,
            raw,
            empty_ns_only,
        } => {
            if *empty_ns_only && elem.name.ns != ns!() {
                return false;
            }
            if elem.is_html() {
                if elem.local_name() != lower {
                    return false;
                }
            } else if elem.local_name() != raw {
                return false;
            }
        }
    }

    if !c.ids.is_empty() {
        let Some(elem_id) = elem.id() else {
            return false;
        };
        if c.ids.iter().any(|id| id != elem_id) {
            return false;
        }
    }

    for cls in &c.classes {
        if !elem.has_class(cls) {
            return false;
        }
    }

    for attr in &c.attrs {
        if !matches_attribute(attr, elem) {
            return false;
        }
    }

    for pc in &c.pseudos {
        if !matches_pseudo_class(pc, node, elem, ctx) {
            return false;
        }
    }

    true
}

fn matches_attribute(sel: &AttrSelector, elem: &Element) -> bool {
    let is_html = elem.is_html();
    let insensitive = match sel.case {
        CaseSensitivity::AsciiInsensitive => true,
        CaseSensitivity::Sensitive => false,
        CaseSensitivity::Default => is_html && is_html_case_insensitive_attr(&sel.lower_name),
    };

    // An element may in rare cases (e.g. `[*|href]`) have multiple attributes across
    // namespaces; return true if any matching attribute satisfies the operator.
    for attr in &elem.attrs {
        if !sel.any_namespace && attr.name.ns != ns!() {
            continue;
        }
        let name_matches = if is_html {
            &*attr.name.local == sel.lower_name
        } else {
            &*attr.name.local == sel.raw_name
        };
        if !name_matches {
            continue;
        }
        if matches_attr_value(&sel.op, &attr.value, insensitive) {
            return true;
        }
    }
    false
}

fn matches_attr_value(op: &AttrOp, actual: &str, insensitive: bool) -> bool {
    match op {
        AttrOp::Exists => true,
        AttrOp::Equals(expected) => str_eq(actual, expected, insensitive),
        AttrOp::Includes(expected) => {
            if expected.is_empty() || expected.bytes().any(|b| b.is_ascii_whitespace()) {
                return false;
            }
            actual
                .split_ascii_whitespace()
                .any(|tok| str_eq(tok, expected, insensitive))
        }
        AttrOp::DashMatch(expected) => {
            if str_eq(actual, expected, insensitive) {
                return true;
            }
            actual.len() > expected.len()
                && actual.as_bytes().get(expected.len()) == Some(&b'-')
                && actual
                    .get(..expected.len())
                    .is_some_and(|prefix| str_eq(prefix, expected, insensitive))
        }
        AttrOp::Prefix(expected) => {
            !expected.is_empty()
                && actual.len() >= expected.len()
                && actual
                    .get(..expected.len())
                    .is_some_and(|prefix| str_eq(prefix, expected, insensitive))
        }
        AttrOp::Suffix(expected) => {
            !expected.is_empty()
                && actual.len() >= expected.len()
                && actual
                    .get(actual.len() - expected.len()..)
                    .is_some_and(|suffix| str_eq(suffix, expected, insensitive))
        }
        AttrOp::Substring(expected) => {
            if expected.is_empty() {
                return false;
            }
            if !insensitive {
                actual.contains(expected.as_str())
            } else {
                contains_ascii_case_insensitive(actual, expected)
            }
        }
    }
}

fn str_eq(a: &str, b: &str, insensitive: bool) -> bool {
    if insensitive {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// WHATWG HTML Living Standard §4.16.2 ASCII case-insensitive attribute value table.
fn is_html_case_insensitive_attr(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "accept"
            | "accept-charset"
            | "align"
            | "alink"
            | "axis"
            | "bgcolor"
            | "charset"
            | "checked"
            | "clear"
            | "codetype"
            | "color"
            | "compact"
            | "declare"
            | "defer"
            | "dir"
            | "direction"
            | "disabled"
            | "enctype"
            | "face"
            | "frame"
            | "hreflang"
            | "http-equiv"
            | "lang"
            | "language"
            | "link"
            | "media"
            | "method"
            | "multiple"
            | "nohref"
            | "noresize"
            | "noshade"
            | "nowrap"
            | "readonly"
            | "rel"
            | "rev"
            | "rules"
            | "scope"
            | "scrolling"
            | "selected"
            | "shape"
            | "target"
            | "text"
            | "type"
            | "valign"
            | "valuetype"
            | "vlink"
    )
}

fn matches_pseudo_class(
    pc: &PseudoClass,
    node: NodeId,
    elem: &Element,
    ctx: &MatchContext<'_>,
) -> bool {
    let doc = ctx.doc;
    match pc {
        PseudoClass::Root => doc.parent(node) == Some(doc.root()),
        PseudoClass::FirstChild => {
            doc.parent(node).is_some() && doc.prev_element_sibling(node).is_none()
        }
        PseudoClass::LastChild => {
            doc.parent(node).is_some() && doc.next_element_sibling(node).is_none()
        }
        PseudoClass::OnlyChild => {
            doc.parent(node).is_some()
                && doc.prev_element_sibling(node).is_none()
                && doc.next_element_sibling(node).is_none()
        }
        PseudoClass::NthChild(formula, of_list) => {
            if doc.parent(node).is_none() {
                return false;
            }
            if let Some(filter) = of_list {
                if !filter.iter().any(|s| matches_selector(s, node, ctx)) {
                    return false;
                }
                let before = doc
                    .prev_siblings(node)
                    .filter(|&sib| {
                        doc.element(sib).is_some()
                            && filter.iter().any(|s| matches_selector(s, sib, ctx))
                    })
                    .count() as i32;
                formula.matches(before + 1)
            } else {
                let before = doc
                    .prev_siblings(node)
                    .filter(|&sib| doc.element(sib).is_some())
                    .count() as i32;
                formula.matches(before + 1)
            }
        }
        PseudoClass::NthLastChild(formula, of_list) => {
            if doc.parent(node).is_none() {
                return false;
            }
            if let Some(filter) = of_list {
                if !filter.iter().any(|s| matches_selector(s, node, ctx)) {
                    return false;
                }
                let after = doc
                    .next_siblings(node)
                    .filter(|&sib| {
                        doc.element(sib).is_some()
                            && filter.iter().any(|s| matches_selector(s, sib, ctx))
                    })
                    .count() as i32;
                formula.matches(after + 1)
            } else {
                let after = doc
                    .next_siblings(node)
                    .filter(|&sib| doc.element(sib).is_some())
                    .count() as i32;
                formula.matches(after + 1)
            }
        }
        PseudoClass::FirstOfType => {
            doc.parent(node).is_some()
                && !doc
                    .prev_siblings(node)
                    .any(|sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
        }
        PseudoClass::LastOfType => {
            doc.parent(node).is_some()
                && !doc
                    .next_siblings(node)
                    .any(|sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
        }
        PseudoClass::OnlyOfType => {
            doc.parent(node).is_some()
                && !doc
                    .prev_siblings(node)
                    .any(|sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
                && !doc
                    .next_siblings(node)
                    .any(|sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
        }
        PseudoClass::NthOfType(formula) => {
            if doc.parent(node).is_none() {
                return false;
            }
            let before = doc
                .prev_siblings(node)
                .filter(|&sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
                .count() as i32;
            formula.matches(before + 1)
        }
        PseudoClass::NthLastOfType(formula) => {
            if doc.parent(node).is_none() {
                return false;
            }
            let after = doc
                .next_siblings(node)
                .filter(|&sib| doc.element(sib).is_some_and(|e| e.name == elem.name))
                .count() as i32;
            formula.matches(after + 1)
        }
        PseudoClass::Empty => !doc.children(node).any(|child| match &doc.node(child).data {
            NodeData::Element(_) => true,
            NodeData::Text(t) => !t.is_empty(),
            _ => false,
        }),
        PseudoClass::Not(list) => !list.iter().any(|s| matches_selector(s, node, ctx)),
        PseudoClass::Is(list) | PseudoClass::Where(list) => {
            list.iter().any(|s| matches_selector(s, node, ctx))
        }
        PseudoClass::Has(rels) => rels.iter().any(|rel| matches_relative_from(rel, node, ctx)),
        PseudoClass::AnyLink => is_hyperlink(elem),
        PseudoClass::Link => {
            is_hyperlink(elem) && ctx.active_state != Some((node, PseudoState::Visited))
        }
        PseudoClass::Checked => is_checked(elem),
        PseudoClass::Disabled => is_form_control(elem) && is_disabled_control(doc, node, elem),
        PseudoClass::Enabled => is_form_control(elem) && !is_disabled_control(doc, node, elem),
        PseudoClass::Required => is_required_candidate(elem) && elem.has_attr("required"),
        PseudoClass::Optional => is_required_candidate(elem) && !elem.has_attr("required"),
        PseudoClass::ReadWrite => is_read_write(doc, node, elem),
        PseudoClass::ReadOnly => !is_read_write(doc, node, elem),
        PseudoClass::PlaceholderShown => is_placeholder_shown(doc, node, elem),
        PseudoClass::Default => is_default_control(doc, node, elem),
        PseudoClass::Indeterminate => is_indeterminate(doc, node, elem),
        PseudoClass::Lang(ranges) => matches_lang(doc, node, ranges),
        PseudoClass::Dir(expected) => expected.is_some_and(|d| element_direction(doc, node) == d),
        PseudoClass::Dynamic(target_state) => match *target_state {
            PseudoState::Visited => {
                is_hyperlink(elem) && ctx.active_state == Some((node, PseudoState::Visited))
            }
            other => ctx.active_state == Some((node, other)),
        },
        PseudoClass::Never => false,
    }
}

/// Evaluates `:has(...)` left-to-right starting from `anchor`.
fn matches_relative_from(rel: &RelativeSelector, anchor: NodeId, ctx: &MatchContext<'_>) -> bool {
    matches_relative_step(&rel.selector, 0, rel.leading, anchor, ctx)
}

fn matches_relative_step(
    sel: &Selector,
    step: usize,
    comb: Combinator,
    from: NodeId,
    ctx: &MatchContext<'_>,
) -> bool {
    let doc = ctx.doc;
    let compound = &sel.compounds[step];
    let is_last = step + 1 == sel.compounds.len();

    let test_candidate = |cand: NodeId, cand_elem: &Element| -> bool {
        if !matches_compound(compound, cand, cand_elem, ctx) {
            return false;
        }
        if is_last {
            true
        } else {
            matches_relative_step(sel, step + 1, sel.combinators[step], cand, ctx)
        }
    };

    match comb {
        Combinator::Child => {
            for child in doc.element_children(from) {
                if let Some(el) = doc.element(child)
                    && test_candidate(child, el)
                {
                    return true;
                }
            }
            false
        }
        Combinator::Descendant => {
            for desc in doc.descendants(from).skip(1) {
                if let Some(el) = doc.element(desc)
                    && test_candidate(desc, el)
                {
                    return true;
                }
            }
            false
        }
        Combinator::NextSibling => {
            let Some(next) = doc.next_element_sibling(from) else {
                return false;
            };
            doc.element(next)
                .is_some_and(|el| test_candidate(next, el))
        }
        Combinator::LaterSibling => {
            for sib in doc.next_siblings(from) {
                if let Some(el) = doc.element(sib)
                    && test_candidate(sib, el)
                {
                    return true;
                }
            }
            false
        }
    }
}

// ---------------------------------------------------------------------------
// HTML State Helpers (WHATWG HTML §4.16.3)
// ---------------------------------------------------------------------------

fn is_hyperlink(elem: &Element) -> bool {
    elem.is_html() && matches!(elem.local_name(), "a" | "area") && elem.has_attr("href")
}

fn is_checked(elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    match elem.local_name() {
        "input" => {
            if !elem.has_attr("checked") {
                return false;
            }
            match elem.attr("type") {
                None => true,
                Some(t) => t.eq_ignore_ascii_case("checkbox") || t.eq_ignore_ascii_case("radio"),
            }
        }
        "option" => elem.has_attr("selected"),
        _ => false,
    }
}

fn is_form_control(elem: &Element) -> bool {
    elem.is_html()
        && matches!(
            elem.local_name(),
            "button" | "input" | "select" | "textarea" | "optgroup" | "option" | "fieldset"
        )
}

fn is_disabled_control(doc: &Document, node: NodeId, elem: &Element) -> bool {
    if elem.has_attr("disabled") {
        return true;
    }

    if elem.local_name() == "option" {
        for anc in doc.ancestors(node) {
            if let Some(anc_el) = doc.element(anc)
                && anc_el.is_html()
                && anc_el.local_name() == "optgroup"
                && anc_el.has_attr("disabled")
            {
                return true;
            }
        }
    }

    if matches!(
        elem.local_name(),
        "button" | "input" | "select" | "textarea" | "fieldset"
    ) {
        for anc in doc.ancestors(node) {
            let Some(anc_el) = doc.element(anc) else {
                continue;
            };
            if anc_el.is_html() && anc_el.local_name() == "fieldset" && anc_el.has_attr("disabled")
            {
                // Controls inside a disabled `<fieldset>`'s first `<legend>` child
                // remain enabled per WHATWG HTML §4.10.18.5.
                let first_legend = doc.element_children(anc).find(|&c| {
                    doc.element(c)
                        .is_some_and(|ce| ce.is_html() && ce.local_name() == "legend")
                });
                let inside_first_legend =
                    first_legend.is_some_and(|leg| node == leg || doc.ancestors(node).any(|a| a == leg));
                if !inside_first_legend {
                    return true;
                }
            }
        }
    }

    false
}

fn is_required_candidate(elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    match elem.local_name() {
        "select" | "textarea" => true,
        "input" => {
            let ty = elem.attr("type").unwrap_or("text");
            !matches!(
                ty.to_ascii_lowercase().as_str(),
                "hidden" | "button" | "submit" | "reset" | "image"
            )
        }
        _ => false,
    }
}

fn is_read_write(doc: &Document, node: NodeId, elem: &Element) -> bool {
    if elem.is_html() {
        match elem.local_name() {
            "textarea" => {
                return !elem.has_attr("readonly") && !is_disabled_control(doc, node, elem);
            }
            "input" => {
                let ty = elem.attr("type").unwrap_or("text").to_ascii_lowercase();
                let mutable_type = !matches!(
                    ty.as_str(),
                    "hidden"
                        | "button"
                        | "submit"
                        | "reset"
                        | "checkbox"
                        | "radio"
                        | "file"
                        | "image"
                        | "range"
                        | "color"
                );
                return mutable_type
                    && !elem.has_attr("readonly")
                    && !is_disabled_control(doc, node, elem);
            }
            _ => {}
        }
    }

    for cur in std::iter::once(node).chain(doc.ancestors(node)) {
        let Some(el) = doc.element(cur) else {
            continue;
        };
        if !el.is_html() {
            continue;
        }
        if let Some(ce) = el.attr("contenteditable") {
            if ce.is_empty()
                || ce.eq_ignore_ascii_case("true")
                || ce.eq_ignore_ascii_case("plaintext-only")
            {
                return true;
            }
            if ce.eq_ignore_ascii_case("false") {
                return false;
            }
        }
    }
    false
}

fn is_placeholder_shown(doc: &Document, node: NodeId, elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    let Some(ph) = elem.attr("placeholder") else {
        return false;
    };
    if ph.is_empty() {
        return false;
    }
    match elem.local_name() {
        "input" => {
            let ty = elem.attr("type").unwrap_or("text").to_ascii_lowercase();
            let supports_ph = matches!(
                ty.as_str(),
                "text" | "search" | "url" | "tel" | "email" | "password" | "number"
            );
            supports_ph && elem.attr("value").unwrap_or("").is_empty()
        }
        "textarea" => doc.text_content(node).is_empty(),
        _ => false,
    }
}

fn is_submit_button(elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    match elem.local_name() {
        "button" => match elem.attr("type") {
            None => true,
            Some(t) => t.eq_ignore_ascii_case("submit"),
        },
        "input" => elem
            .attr("type")
            .is_some_and(|t| t.eq_ignore_ascii_case("submit") || t.eq_ignore_ascii_case("image")),
        _ => false,
    }
}

fn is_default_control(doc: &Document, node: NodeId, elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    if is_checked(elem) {
        return true;
    }
    if is_submit_button(elem) {
        let form = doc.ancestors(node).find(|&a| {
            doc.element(a)
                .is_some_and(|e| e.is_html() && e.local_name() == "form")
        });
        if let Some(form_id) = form {
            let first_submit = doc
                .descendants(form_id)
                .find(|&d| doc.element(d).is_some_and(is_submit_button));
            return first_submit == Some(node);
        }
    }
    false
}

fn is_indeterminate(doc: &Document, node: NodeId, elem: &Element) -> bool {
    if !elem.is_html() {
        return false;
    }
    match elem.local_name() {
        "progress" => !elem.has_attr("value"),
        "input" => {
            let ty = elem.attr("type").unwrap_or("text");
            if ty.eq_ignore_ascii_case("checkbox") {
                return elem.has_attr("indeterminate");
            }
            if ty.eq_ignore_ascii_case("radio") {
                let Some(group_name) = elem.attr("name").filter(|n| !n.is_empty()) else {
                    return false;
                };
                let scope = doc
                    .ancestors(node)
                    .find(|&a| {
                        doc.element(a)
                            .is_some_and(|e| e.is_html() && e.local_name() == "form")
                    })
                    .unwrap_or_else(|| doc.root());
                let any_checked = doc.descendants(scope).any(|d| {
                    doc.element(d).is_some_and(|e| {
                        e.is_html()
                            && e.local_name() == "input"
                            && e.attr("type")
                                .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
                            && e.attr("name") == Some(group_name)
                            && e.has_attr("checked")
                    })
                });
                return !any_checked;
            }
            false
        }
        _ => false,
    }
}

fn matches_lang(doc: &Document, node: NodeId, ranges: &[String]) -> bool {
    let mut found_lang: Option<&str> = None;
    for cur in std::iter::once(node).chain(doc.ancestors(node)) {
        let Some(el) = doc.element(cur) else {
            continue;
        };
        if let Some(l) = el
            .attrs
            .iter()
            .find(|a| &*a.name.local == "lang")
            .map(|a| a.value.as_str())
        {
            found_lang = Some(l.trim());
            break;
        }
    }
    let Some(elem_lang) = found_lang.filter(|l| !l.is_empty()) else {
        return false;
    };
    let lang_lower = elem_lang.to_ascii_lowercase();
    let lang_subs: Vec<&str> = lang_lower.split('-').collect();
    ranges
        .iter()
        .any(|range| rfc4647_extended_filter(&lang_subs, range))
}

fn rfc4647_extended_filter(lang_subs: &[&str], range_lower: &str) -> bool {
    let range_subs: Vec<&str> = range_lower.split('-').collect();
    if range_subs.is_empty() || lang_subs.is_empty() {
        return false;
    }
    if range_subs[0] != "*" && range_subs[0] != lang_subs[0] {
        return false;
    }
    let mut r_idx = 1usize;
    let mut l_idx = 1usize;
    while r_idx < range_subs.len() {
        if range_subs[r_idx] == "*" {
            r_idx += 1;
        } else if l_idx >= lang_subs.len() {
            return false;
        } else if range_subs[r_idx] == lang_subs[l_idx] {
            r_idx += 1;
            l_idx += 1;
        } else if lang_subs[l_idx].len() == 1 {
            return false;
        } else {
            l_idx += 1;
        }
    }
    true
}

fn element_direction(doc: &Document, node: NodeId) -> Direction {
    for cur in std::iter::once(node).chain(doc.ancestors(node)) {
        let Some(el) = doc.element(cur) else {
            continue;
        };
        if let Some(dir) = el.attr("dir") {
            if dir.eq_ignore_ascii_case("ltr") {
                return Direction::Ltr;
            }
            if dir.eq_ignore_ascii_case("rtl") {
                return Direction::Rtl;
            }
            if dir.eq_ignore_ascii_case("auto") {
                return auto_direction(doc, cur);
            }
        } else if el.is_html() && el.local_name() == "bdi" {
            return auto_direction(doc, cur);
        }
    }
    Direction::Ltr
}

fn auto_direction(doc: &Document, node: NodeId) -> Direction {
    let text = doc.text_content(node);
    for ch in text.chars() {
        if is_strong_rtl_char(ch) {
            return Direction::Rtl;
        }
        if ch.is_alphabetic() {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

fn is_strong_rtl_char(ch: char) -> bool {
    let u = u32::from(ch);
    (0x0590..=0x08FF).contains(&u)
        || (0xFB1D..=0xFDFF).contains(&u)
        || (0xFE70..=0xFEFF).contains(&u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(s: &str) -> Selector {
        let res = parse_selector_list_str(s);
        assert!(!res.invalid, "expected valid selector: {s}");
        assert_eq!(res.selectors.len(), 1, "expected 1 selector: {s}");
        res.selectors.into_iter().next().expect("checked len == 1")
    }

    fn matched_ids(doc: &Document, selector_css: &str) -> Vec<String> {
        let list = parse_selector_list_str(selector_css);
        assert!(!list.invalid, "selector should be valid: {selector_css}");
        let ctx = MatchContext {
            doc,
            active_state: None,
        };
        let mut out = Vec::new();
        for node in doc.descendants(doc.root()) {
            let Some(el) = doc.element(node) else {
                continue;
            };
            if list.selectors.iter().any(|s| matches_selector(s, node, &ctx))
                && let Some(id) = el.id()
            {
                out.push(id.to_owned());
            }
        }
        out
    }

    #[test]
    fn specificity_table_per_selectors_4() {
        let cases: &[(&str, Specificity)] = &[
            ("*", Specificity(0, 0, 0)),
            ("li", Specificity(0, 0, 1)),
            ("ul li", Specificity(0, 0, 2)),
            ("ul ol+li", Specificity(0, 0, 3)),
            ("h1 + *[rel=up]", Specificity(0, 1, 1)),
            ("ul ol li.red", Specificity(0, 1, 3)),
            ("li.red.level", Specificity(0, 2, 1)),
            ("#x34y", Specificity(1, 0, 0)),
            ("#s12:not(FOO)", Specificity(1, 0, 1)),
            (":where(#id, .cls, div)", Specificity(0, 0, 0)),
            ("div:where(#id.cls)", Specificity(0, 0, 1)),
            (":is(#id, .cls)", Specificity(1, 0, 0)),
            (":is(.a, .b.c)", Specificity(0, 2, 0)),
            (":not(.a, #id.b)", Specificity(1, 1, 0)),
            ("div:has(> #hero, .sub)", Specificity(1, 0, 1)),
            ("li:nth-child(2n+1)", Specificity(0, 1, 1)),
            ("li:nth-child(2n+1 of .featured, #top)", Specificity(1, 1, 1)),
            ("p::before", Specificity(0, 0, 2)),
            ("p:after", Specificity(0, 0, 2)),
            ("input.field::placeholder", Specificity(0, 1, 2)),
        ];

        for &(css, expected) in cases {
            let sel = parse_one(css);
            assert_eq!(sel.specificity, expected, "specificity mismatch for `{css}`");
        }
    }

    #[test]
    fn combinators_and_basic_selectors() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html><body>
              <div id="wrap" class="card primary">
                <h2 id="h">Title</h2>
                <p id="p1" class="lead">First</p>
                <!-- comment between siblings -->
                <p id="p2" class="body">Second <span id="sp">inner</span></p>
                <span id="s1">Tail</span>
              </div>
            </body></html>"#,
        );

        assert_eq!(matched_ids(&doc, "DIV#wrap.card.primary"), vec!["wrap"]);
        assert_eq!(matched_ids(&doc, "#wrap > p"), vec!["p1", "p2"]);
        assert_eq!(matched_ids(&doc, "#wrap span"), vec!["sp", "s1"]);
        assert_eq!(matched_ids(&doc, "#wrap > span"), vec!["s1"]);
        assert_eq!(matched_ids(&doc, "h2 + p"), vec!["p1"]);
        assert_eq!(matched_ids(&doc, "#p1 + p"), vec!["p2"]);
        assert_eq!(matched_ids(&doc, "h2 ~ p"), vec!["p1", "p2"]);
        assert_eq!(matched_ids(&doc, "h2 ~ span"), vec!["s1"]);
    }

    #[test]
    fn attribute_selectors_and_case_flags() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html><body>
              <input id="i1" type="TEXT" data-tags="alpha beta-1" data-url="https://example.org/app.qml" lang="en-US">
              <input id="i2" type="checkbox" data-tags="alphabet" data-empty="">
            </body></html>"#,
        );

        assert_eq!(matched_ids(&doc, "[data-empty]"), vec!["i2"]);
        // `type` is case-insensitive on HTML elements by default, unless `s` flag is given.
        assert_eq!(matched_ids(&doc, r#"input[type="text"]"#), vec!["i1"]);
        assert!(matched_ids(&doc, r#"input[type="text" s]"#).is_empty());
        assert_eq!(matched_ids(&doc, r#"input[type="TEXT" s]"#), vec!["i1"]);
        // Custom `data-*` is case-sensitive by default, case-insensitive with `i`.
        assert!(matched_ids(&doc, r#"[data-tags~="ALPHA"]"#).is_empty());
        assert_eq!(matched_ids(&doc, r#"[data-tags~="ALPHA" i]"#), vec!["i1"]);
        assert!(matched_ids(&doc, r#"[data-tags~=""]"#).is_empty());
        assert!(matched_ids(&doc, r#"[data-tags~="alpha beta-1"]"#).is_empty());
        assert_eq!(matched_ids(&doc, r#"[lang|="en"]"#), vec!["i1"]);
        assert_eq!(matched_ids(&doc, r#"[data-url^="https://"]"#), vec!["i1"]);
        assert!(matched_ids(&doc, r#"[data-url^=""]"#).is_empty());
        assert_eq!(matched_ids(&doc, r#"[data-url$=".qml"]"#), vec!["i1"]);
        assert!(matched_ids(&doc, r#"[data-url$=""]"#).is_empty());
        assert_eq!(matched_ids(&doc, r#"[data-url*="EXAMPLE" i]"#), vec!["i1"]);
        assert!(matched_ids(&doc, r#"[data-url*=""]"#).is_empty());
    }

    #[test]
    fn nth_child_and_structural_pseudos() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html id="root"><body>
              <ul id="list">
                <li id="c1" class="hot">1</li>
                <li id="c2">2</li>
                <li id="c3" class="hot">3</li>
                <li id="c4" class="hot">4</li>
                <li id="c5">5</li>
              </ul>
              <div id="box">
                <span id="only-span">x</span>
                <p id="p-a">a</p>
                <p id="p-b">b</p>
              </div>
              <div id="solo"><em id="only-child"></em></div>
              <div id="empty-comment"><!-- comment --></div>
              <div id="non-empty-ws"> </div>
            </body></html>"#,
        );

        assert_eq!(matched_ids(&doc, ":root"), vec!["root"]);
        assert_eq!(matched_ids(&doc, "#list > :first-child"), vec!["c1"]);
        assert_eq!(matched_ids(&doc, "#list > :last-child"), vec!["c5"]);
        assert_eq!(matched_ids(&doc, "#solo > :only-child"), vec!["only-child"]);
        assert_eq!(
            matched_ids(&doc, "#list > :nth-child(odd)"),
            vec!["c1", "c3", "c5"]
        );
        assert_eq!(
            matched_ids(&doc, "#list > :nth-child(even)"),
            vec!["c2", "c4"]
        );
        assert_eq!(
            matched_ids(&doc, "#list > :nth-child(-n+3)"),
            vec!["c1", "c2", "c3"]
        );
        assert_eq!(
            matched_ids(&doc, "#list > :nth-last-child(2)"),
            vec!["c4"]
        );
        assert_eq!(
            matched_ids(&doc, "#list > :nth-child(2 of .hot)"),
            vec!["c3"]
        );
        assert_eq!(
            matched_ids(&doc, "#list > :nth-last-child(1 of .hot)"),
            vec!["c4"]
        );
        assert_eq!(matched_ids(&doc, "#box > :only-of-type"), vec!["only-span"]);
        assert_eq!(matched_ids(&doc, "#box > p:first-of-type"), vec!["p-a"]);
        assert_eq!(matched_ids(&doc, "#box > p:last-of-type"), vec!["p-b"]);
        assert_eq!(matched_ids(&doc, "#box > p:nth-of-type(2)"), vec!["p-b"]);
        assert_eq!(
            matched_ids(&doc, "#box > p:nth-last-of-type(2)"),
            vec!["p-a"]
        );
        assert_eq!(
            matched_ids(&doc, "div:empty"),
            vec!["empty-comment"]
        );
    }

    #[test]
    fn logical_and_relational_has_pseudos() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html><body>
              <section id="s1"><div id="d1"><img id="img1" src="a.png"></div></section>
              <section id="s2"><p id="p2">No image</p></section>
              <h3 id="h3-a">Heading A</h3>
              <p id="after-h3" class="warning">Warn</p>
              <h3 id="h3-b">Heading B</h3>
              <div id="div-b">Info</div>
              <p id="later-warn" class="warning">Later</p>
            </body></html>"#,
        );

        assert_eq!(matched_ids(&doc, "section:has(img)"), vec!["s1"]);
        assert!(matched_ids(&doc, "section:has(> img)").is_empty());
        assert_eq!(matched_ids(&doc, "div:has(> img)"), vec!["d1"]);
        assert_eq!(matched_ids(&doc, "h3:has(+ p.warning)"), vec!["h3-a"]);
        assert_eq!(
            matched_ids(&doc, "h3:has(~ p.warning)"),
            vec!["h3-a", "h3-b"]
        );
        assert_eq!(
            matched_ids(&doc, "section:not(:has(img))"),
            vec!["s2"]
        );
        assert_eq!(
            matched_ids(&doc, ":is(section, invalid..syntax!!):where(#s2)"),
            vec!["s2"]
        );
    }

    #[test]
    fn form_link_lang_and_dir_pseudos() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html lang="en-GB"><body>
              <a id="link1" href="/home">Home</a>
              <a id="anchor-no-href">Anchor</a>
              <form id="f">
                <input id="chk" type="checkbox" checked>
                <input id="req" type="email" required placeholder="Email">
                <input id="filled" type="text" placeholder="Name" value="Ada" readonly>
                <select id="sel"><option id="opt1">A</option><option id="opt2" selected>B</option></select>
                <fieldset id="fs" disabled>
                  <legend><input id="in-legend" type="text"></legend>
                  <input id="in-fs" type="text">
                </fieldset>
                <button id="btn-submit">Send</button>
                <button id="btn-second" type="submit">Send 2</button>
                <progress id="prog"></progress>
              </form>
              <div id="rtl-box" dir="rtl" lang="ar-EG"><span id="rtl-child">مرحبا</span></div>
            </body></html>"#,
        );

        assert_eq!(matched_ids(&doc, "a:link"), vec!["link1"]);
        assert_eq!(matched_ids(&doc, ":any-link"), vec!["link1"]);
        assert_eq!(matched_ids(&doc, ":checked"), vec!["chk", "opt2"]);
        assert_eq!(matched_ids(&doc, "input:disabled"), vec!["in-fs"]);
        assert_eq!(
            matched_ids(&doc, "input:enabled"),
            vec!["chk", "req", "filled", "in-legend"]
        );
        assert_eq!(matched_ids(&doc, ":required"), vec!["req"]);
        assert_eq!(matched_ids(&doc, "input:placeholder-shown"), vec!["req"]);
        assert_eq!(
            matched_ids(&doc, "input:read-write"),
            vec!["req", "in-legend"]
        );
        assert!(matched_ids(&doc, "#filled:read-only").contains(&"filled".to_owned()));
        assert_eq!(
            matched_ids(&doc, ":default"),
            vec!["chk", "opt2", "btn-submit"]
        );
        assert_eq!(matched_ids(&doc, ":indeterminate"), vec!["prog"]);
        assert_eq!(
            matched_ids(&doc, "#rtl-child:lang(ar)"),
            vec!["rtl-child"]
        );
        assert_eq!(matched_ids(&doc, "#link1:lang(en)"), vec!["link1"]);
        assert_eq!(
            matched_ids(&doc, "#rtl-child:dir(rtl)"),
            vec!["rtl-child"]
        );
        assert_eq!(matched_ids(&doc, "#link1:dir(ltr)"), vec!["link1"]);
    }

    #[test]
    fn pseudo_elements_dynamic_rejection_and_rule_index() {
        let res = parse_selector_list_str("a::before, b:after, input::placeholder, li::marker, ::-webkit-scrollbar");
        assert!(!res.invalid);
        assert_eq!(res.selectors.len(), 3);
        assert_eq!(res.selectors[0].pseudo_element, Some(PseudoElement::Before));
        assert_eq!(res.selectors[1].pseudo_element, Some(PseudoElement::After));
        assert_eq!(
            res.selectors[2].pseudo_element,
            Some(PseudoElement::Placeholder)
        );
        assert_eq!(
            res.unsupported_pseudo_elements,
            vec!["::marker".to_owned(), "::-webkit-scrollbar".to_owned()]
        );

        let multi_dyn = parse_selector_list_str(".a:hover .b:focus, .ok:hover .child");
        assert!(!multi_dyn.invalid);
        assert_eq!(multi_dyn.unsupported_multi_dynamic, 1);
        assert_eq!(multi_dyn.selectors.len(), 1);

        let bad = parse_selector_list_str(".valid, :totally-unknown-pseudo");
        assert!(bad.invalid);

        // Verify RuleIndex + ancestor bloom filter including `A + B > C` where `A` is
        // a sibling of `B` (NOT an ancestor of `C`).
        let doc = Document::parse(
            r#"<!doctype html><html><body>
              <div class="uncle"></div>
              <div class="parent"><span id="target" class="hit hit"></span></div>
            </body></html>"#,
        );
        let blooms = compute_ancestor_blooms(&doc);
        let mut index = RuleIndex::new();
        index.insert(parse_one(".uncle + .parent > span.hit"), 1u32);
        index.insert(parse_one(".nonexistent-ancestor span.hit"), 2u32);

        let target = doc.element_by_id("target").expect("target exists");
        let target_el = doc.element(target).expect("target is element");
        let ctx = MatchContext {
            doc: &doc,
            active_state: None,
        };
        let mut hits = Vec::new();
        index.for_each_candidate(target_el, blooms[target.index()], |sel, &id| {
            if matches_selector(sel, target, &ctx) {
                hits.push(id);
            }
        });
        // `.nonexistent-ancestor` is filtered out by bloom filter, `.uncle + .parent > span.hit`
        // passes bloom filter AND matches exactly once despite `class="hit hit"`.
        assert_eq!(hits, vec![1u32]);
    }
}
