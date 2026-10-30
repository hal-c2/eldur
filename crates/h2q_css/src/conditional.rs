//! Conditional styles (`docs/design/dynamic-styles.md` §3): the real cascade
//! under every combination of the script-toggled classes and pseudo-states
//! each element depends on.
//!
//! A condition on `n` can only be read by selectors matched at `n`, its
//! subtree, its following siblings and their subtrees (combinators look up
//! and back, never down or forward, except `:has()` and `:nth-*(of)`). Which
//! of the conditions in scope an element really depends on is found by
//! running its candidate rules with the matcher recording every assumed fact
//! it reads, under "all hold", "none holds" and "as in the document". Its
//! parent's conditions are added, because they flow in through inheritance.
//! The element is then cascaded once per combination, and conditions whose
//! flip never changes its computed style are dropped.

use std::collections::BTreeSet;

use h2q_dom::{Document, Element, NodeId};

use crate::cascade::{self, BasePass, CascadeEngine};
use crate::selectors::{
    DynamicFact, DynamicFacts, MatchContext, Selector, TypeMatcher, class_bloom, matches_selector,
    visit_compounds,
};
use crate::{
    ComputedStyle, Condition, ConditionalStyles, Diagnostic, Display, ElementConditional, Severity,
    Stylesheet, Viewport,
};

/// Conditions combined per element (64 cascades); past it, a diagnostic.
const MAX_CONDITIONS: usize = 6;

const SOURCE: &str = "dynamic styles";

pub(crate) fn conditional_styles(
    doc: &Document,
    sheets: &[Stylesheet],
    viewport: &Viewport,
    conditions: &[Condition],
) -> ConditionalStyles {
    let mut diagnostics = Vec::new();
    let mut prepared = cascade::prepare(doc, sheets, viewport);
    let base = prepared.engine.base_pass();

    // Conditions on non-elements and repeats take no part.
    let mut usable = vec![false; conditions.len()];
    for (i, cond) in conditions.iter().enumerate() {
        let node = condition_node(cond);
        if node.index() >= doc.len() || doc.element(node).is_none() {
            diagnostics.push(diagnostic(format!(
                "dynamic-condition: condition {i} is not on an element; ignored"
            )));
        } else if u16::try_from(i).is_err() {
            // `depends_on` holds `u16` indices.
            diagnostics.push(diagnostic(format!(
                "dynamic-condition: condition {i} is past the first 65536; ignored"
            )));
        } else {
            usable[i] = !conditions[..i].contains(cond);
        }
    }
    let active: Vec<usize> = (0..conditions.len()).filter(|&i| usable[i]).collect();

    let engine = &mut prepared.engine;
    engine.dynamic_classes = vec![Vec::new(); doc.len()];
    for &i in &active {
        if let Condition::Class { node, class } = &conditions[i] {
            engine.dynamic_bloom |= class_bloom(class);
            engine.dynamic_classes[node.index()].push(class.clone());
        }
    }
    let engine = &prepared.engine;

    let mut in_scope: Vec<Vec<usize>> = vec![Vec::new(); doc.len()];
    // Elements outside every scope that `:has()` could still reach a
    // condition from: probed only to report it.
    let mut reaches = vec![false; doc.len()];
    for &i in &active {
        let node = condition_node(&conditions[i]);
        for n in scope_of(doc, node) {
            in_scope[n.index()].push(i);
        }
        for a in std::iter::once(node).chain(doc.ancestors(node)) {
            reaches[a.index()] = true;
            for sib in doc.prev_siblings(a) {
                reaches[sib.index()] = true;
            }
        }
    }

    let probes = [Some(true), Some(false), None].map(|holds| {
        let mut facts = DynamicFacts::recording();
        for &i in &active {
            set_fact(&mut facts, &conditions[i], holds, i);
        }
        facts
    });

    let mut results: Vec<Option<ElementConditional>> = vec![None; doc.len()];
    let mut reported_unsupported = vec![false; conditions.len()];
    let pass = Pass {
        doc,
        engine,
        base: &base,
        conditions,
    };

    for node in doc.descendants(doc.root()) {
        let Some(elem) = doc.element(node) else {
            continue;
        };
        let parent_entry = doc
            .parent_element(node)
            .and_then(|p| results[p.index()].as_ref());
        if in_scope[node.index()].is_empty() && parent_entry.is_none() && !reaches[node.index()] {
            continue;
        }

        let mut read = BTreeSet::new();
        for facts in &probes {
            let ctx = MatchContext {
                doc,
                dynamic: Some(facts),
            };
            engine.for_each_element_selector(node, elem, |sel| {
                matches_selector(sel, node, &ctx);
            });
            read.extend(facts.take_touched());
        }

        let mut depends: Vec<usize> = parent_entry
            .map(|e| e.depends_on.iter().map(|&c| usize::from(c)).collect())
            .unwrap_or_default();
        let mut capped = false;
        for c in read {
            if !in_scope[node.index()].contains(&c) {
                if !reported_unsupported[c] {
                    reported_unsupported[c] = true;
                    diagnostics.push(diagnostic(format!(
                        "dynamic-unsupported-selector: a selector of {} reads {} of {}, \
                         which only `:has()` or `:nth-*(of)` can reach; not converted",
                        label(elem),
                        describe(&conditions[c]),
                        label(
                            doc.element(condition_node(&conditions[c]))
                                .expect("element")
                        ),
                    )));
                }
            } else if !depends.contains(&c) {
                if depends.len() < MAX_CONDITIONS {
                    depends.push(c);
                } else {
                    capped = true;
                }
            }
        }
        if capped {
            diagnostics.push(diagnostic(format!(
                "dynamic-combinations: {} depends on more than {MAX_CONDITIONS} conditions; \
                 only {MAX_CONDITIONS} are combined, the rest keep their document value",
                label(elem),
            )));
        }
        if depends.is_empty() {
            continue;
        }
        depends.sort_unstable();

        let by_mask: Vec<ComputedStyle> = (0..1usize << depends.len())
            .map(|mask| pass.style_under(node, elem, &depends, mask, &results))
            .collect();
        if let Some(entry) = drop_constant_bits(node, &depends, by_mask) {
            results[node.index()] = Some(entry);
        }
    }

    ConditionalStyles {
        conditions: conditions.to_vec(),
        elements: results.into_iter().flatten().collect(),
        diagnostics,
    }
}

struct Pass<'p, 'a> {
    doc: &'a Document,
    engine: &'p CascadeEngine<'a>,
    base: &'p BasePass,
    conditions: &'p [Condition],
}

impl Pass<'_, '_> {
    /// `node`'s style when, of `depends`, exactly the bits of `mask` hold
    /// (other conditions keep their document value).
    fn style_under(
        &self,
        node: NodeId,
        elem: &Element,
        depends: &[usize],
        mask: usize,
        results: &[Option<ElementConditional>],
    ) -> ComputedStyle {
        let doc = self.doc;
        let mut facts = DynamicFacts::default();
        for (bit, &c) in depends.iter().enumerate() {
            set_fact(
                &mut facts,
                &self.conditions[c],
                Some(mask >> bit & 1 == 1),
                c,
            );
        }
        let ctx = MatchContext {
            doc,
            dynamic: Some(&facts),
        };

        let parent = doc.parent_element(node);
        let parent_style = parent.map(|p| self.ancestor_style(p, depends, mask, results));
        let mut enclosing = None;
        let mut cur = parent;
        while let Some(a) = cur {
            let display = self.ancestor_style(a, depends, mask, results).display;
            if display != Display::Contents {
                enclosing = Some(display);
                break;
            }
            cur = doc.parent_element(a);
        }
        // `rem` follows the root element's font size in this combination,
        // when that is known from conditions in `depends`.
        let mut root_font_size = doc
            .element_children(doc.root())
            .find(|&r| doc.element(r).is_some())
            .and_then(|r| {
                let entry = results[r.index()].as_ref()?;
                let m = project(entry, depends, mask)?;
                Some(entry.by_mask[m].font_size)
            })
            .unwrap_or(self.base.root_font_size);
        let is_root = doc.parent(node) == Some(doc.root());
        self.engine
            .compute_single_element(
                node,
                elem,
                is_root,
                parent_style,
                enclosing,
                &mut root_font_size,
                &ctx,
                None,
            )
            .style
    }

    fn ancestor_style<'r>(
        &'r self,
        node: NodeId,
        depends: &[usize],
        mask: usize,
        results: &'r [Option<ElementConditional>],
    ) -> &'r ComputedStyle {
        if let Some(entry) = results[node.index()].as_ref()
            && let Some(m) = project(entry, depends, mask)
        {
            return &entry.by_mask[m];
        }
        let i = node.index();
        &self.base.styles[i]
            .as_ref()
            .or(self.base.hidden[i].as_ref())
            .expect("every element has a computed style")
            .style
    }
}

/// `entry`'s mask for the combination `mask` of `depends`, when every
/// condition `entry` depends on is among `depends`.
fn project(entry: &ElementConditional, depends: &[usize], mask: usize) -> Option<usize> {
    let mut m = 0;
    for (bit, &c) in entry.depends_on.iter().enumerate() {
        let pos = depends.iter().position(|&d| d == usize::from(c))?;
        if mask >> pos & 1 == 1 {
            m |= 1 << bit;
        }
    }
    Some(m)
}

/// Drops the conditions whose flip never changes the style; `None` when
/// none is left.
fn drop_constant_bits(
    node: NodeId,
    depends: &[usize],
    by_mask: Vec<ComputedStyle>,
) -> Option<ElementConditional> {
    let keep: Vec<usize> = (0..depends.len())
        .filter(|&bit| {
            (0..by_mask.len())
                .filter(|m| m >> bit & 1 == 0)
                .any(|m| by_mask[m] != by_mask[m | 1 << bit])
        })
        .collect();
    if keep.is_empty() {
        return None;
    }
    let by_mask = if keep.len() == depends.len() {
        by_mask.into_iter().map(Box::new).collect()
    } else {
        (0..1usize << keep.len())
            .map(|small| {
                let full = keep
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| small >> i & 1 == 1)
                    .fold(0, |m, (_, &bit)| m | 1 << bit);
                Box::new(by_mask[full].clone())
            })
            .collect()
    };
    Some(ElementConditional {
        node,
        depends_on: keep
            .iter()
            .map(|&bit| u16::try_from(depends[bit]).expect("condition index fits u16"))
            .collect(),
        by_mask,
    })
}

fn set_fact(facts: &mut DynamicFacts, cond: &Condition, holds: Option<bool>, tag: usize) {
    match cond {
        Condition::Class { node, class } => {
            facts.set(*node, DynamicFact::Class(class.clone()), holds, tag);
        }
        Condition::State { node, state } => {
            facts.set(*node, DynamicFact::State(*state), holds, tag);
        }
    }
}

fn condition_node(cond: &Condition) -> NodeId {
    match cond {
        Condition::Class { node, .. } | Condition::State { node, .. } => *node,
    }
}

/// `node`, its subtree, its following siblings and their subtrees.
fn scope_of(doc: &Document, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    doc.descendants(node).chain(
        doc.next_siblings(node)
            .flat_map(move |sib| doc.descendants(sib)),
    )
}

fn describe(cond: &Condition) -> String {
    match cond {
        Condition::Class { class, .. } => format!("class `{class}`"),
        Condition::State { state, .. } => format!("state {state:?}"),
    }
}

fn label(elem: &Element) -> String {
    match (elem.id(), elem.attr("class")) {
        (Some(id), _) => format!("<{} id={id}>", elem.local_name()),
        (None, Some(cls)) if !cls.trim().is_empty() => {
            format!("<{} class=\"{}\">", elem.local_name(), cls.trim())
        }
        _ => format!("<{}>", elem.local_name()),
    }
}

fn diagnostic(message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Unsupported,
        source: SOURCE.to_owned(),
        line: None,
        message,
    }
}

fn all_selectors(sheets: &[Stylesheet]) -> impl Iterator<Item = &Selector> {
    sheets
        .iter()
        .flat_map(|s| &s.inner.rules)
        .flat_map(|r| &r.selectors)
}

pub(crate) fn styled_classes(sheets: &[Stylesheet]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for sel in all_selectors(sheets) {
        visit_compounds(sel, &mut |c| out.extend(c.classes.iter().cloned()));
    }
    out
}

pub(crate) fn class_candidates(doc: &Document, sheets: &[Stylesheet], class: &str) -> Vec<NodeId> {
    let selectors: Vec<Selector> = all_selectors(sheets)
        .filter(|sel| {
            let mut mentions = false;
            visit_compounds(sel, &mut |c| {
                mentions |= c.classes.iter().any(|k| k == class)
            });
            mentions
        })
        .map(Selector::without_pseudo_element)
        .collect();
    if selectors.is_empty() {
        return Vec::new();
    }
    let facts_for = |node: NodeId, holds: bool| {
        let mut facts = DynamicFacts::default();
        facts.set(node, DynamicFact::Class(class.to_owned()), Some(holds), 0);
        facts
    };
    doc.descendants(doc.root())
        .filter(|&node| doc.element(node).is_some())
        .filter(|&node| {
            let (on, off) = (facts_for(node, true), facts_for(node, false));
            let on = MatchContext {
                doc,
                dynamic: Some(&on),
            };
            let off = MatchContext {
                doc,
                dynamic: Some(&off),
            };
            scope_of(doc, node)
                .filter(|&s| doc.element(s).is_some())
                .any(|subject| {
                    selectors.iter().any(|sel| {
                        matches_selector(sel, subject, &on) != matches_selector(sel, subject, &off)
                    })
                })
        })
        .collect()
}

pub(crate) fn open_class_candidates(
    sheets: &[Stylesheet],
    tag: &str,
    known: &[String],
) -> BTreeSet<String> {
    let tag = tag.to_ascii_lowercase();
    let mut out = BTreeSet::new();
    for sel in all_selectors(sheets) {
        visit_compounds(sel, &mut |c| {
            let tag_named = match &c.type_matcher {
                TypeMatcher::Tag { lower, .. } if *lower == tag => true,
                TypeMatcher::Tag { .. } => return,
                TypeMatcher::Universal | TypeMatcher::EmptyNsUniversal => false,
            };
            if tag_named || c.classes.iter().any(|k| known.contains(k)) {
                out.extend(c.classes.iter().filter(|k| !known.contains(k)).cloned());
            }
        });
    }
    out
}
