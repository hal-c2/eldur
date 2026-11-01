//! Conditional styles (`docs/design/dynamic-styles.md` §4, §13 item 4):
//! classes scripts toggle and `:hover`, `:focus`, `:active`, `:checked`
//! and `:disabled` become bindings on the static elements' properties.
//!
//! The page is emitted once for each combination ("world") of flipped
//! conditions some element depends on, with every element styled as
//! [`eldur_css::conditional_styles`] has it there, and each world's tree is
//! compared with the document's. A property that differs becomes a nested
//! ternary over the conditions. One emission path decides every form, so a
//! hover colour is written the way a static colour is.
//!
//! The worlds must share one tree shape. Variable lists (transforms,
//! shadows) are padded to a common shape, an element that gains paint or
//! padding somewhere is painted everywhere ("sticky"), and `display: none`
//! is `visible: false` on a laid-out item. What still changes shape keeps
//! its document form and is reported (`dynamic-form`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use eldur_css::{
    ComputedStyle, Condition, ConditionalStyles, Display, LengthPercentage, PseudoState, Rgba,
    Shadow, Structural, StyleMap, Stylesheet, TransformFn,
};
use eldur_dom::{Document, Element, NodeId};
use eldur_js::ElementRef;
use eldur_qml::{Member, Object, Value};

use super::controls::{Control, classify};
use super::ids::Ids;
use super::units::{ROOT_ID, fmt_number};
use super::{Emitter, Form, Target};
use crate::Options;
use crate::boxes::{self, describe, node_line};
use crate::diag::Diagnostics;
use crate::images::ImageAsset;
use crate::scripts::Scripts;

/// More worlds than this are left out (each is a full emission).
const MAX_WORLDS: usize = 512;
/// A container scripts change with more element children than this keeps
/// its children's document positions (each position is a condition).
const MAX_POSITIONED: usize = 32;

const MARKER: &str = "\u{1}eldur:";

/// The marker comment [`Emitter`] puts on `node`'s outermost object.
pub(super) fn marker(node: NodeId) -> String {
    format!("{MARKER}{}", node.index())
}

fn marked(obj: &Object) -> Option<usize> {
    obj.members.iter().find_map(|m| match m {
        Member::Comment(c) => c.strip_prefix(MARKER).and_then(|n| n.parse().ok()),
        _ => None,
    })
}

pub(crate) struct Inputs<'a> {
    pub doc: &'a Document,
    pub styles: &'a StyleMap,
    pub sheets: &'a [Stylesheet],
    pub options: &'a Options,
    pub wanted: &'a HashSet<NodeId>,
    pub images: &'a BTreeMap<NodeId, ImageAsset>,
    pub background_images: &'a BTreeMap<String, String>,
    pub runtime_dom: bool,
    /// The page's scripts, when it has any.
    pub scripts: Option<&'a Scripts>,
    /// Set when this is the probe document of the templates.
    pub probe: Option<&'a Probe>,
}

/// What a probe emission (`super::templates`) adds: class conditions on
/// the probe elements, the ids the page already uses, and the elements
/// whose conditions can reach a template (the probes and their ancestors).
pub(crate) struct Probe {
    /// Class, and whether the probe has it, for each probe element.
    pub classes: BTreeMap<NodeId, BTreeMap<String, bool>>,
    pub reserved: BTreeSet<String>,
    pub relevant: HashSet<NodeId>,
    /// Every probe element: each is a child its container gains.
    pub nodes: HashSet<NodeId>,
    /// The probes that are an item of their own when inline: those inside
    /// a created element or in `<body>` (dynamic-styles.md §6.8).
    pub inline_items: HashSet<NodeId>,
}

/// What a probe emission leaves for the templates. Its bindings read each
/// condition through [`token`], which each template replaces with its own
/// expression for it.
#[derive(Default)]
pub(crate) struct ProbeOut {
    /// Each condition, and whether it holds in the probe document.
    pub conditions: Vec<(Condition, bool)>,
    /// The expression for a condition inside the template of its element.
    pub locals: BTreeMap<u16, String>,
    /// Elements whose form differs under some condition.
    pub reshaped: BTreeSet<NodeId>,
}

const TOKEN_START: char = '\u{2}';
const TOKEN_END: char = '\u{3}';

/// The stand-in for condition `c` in a probe emission's bindings.
fn token(c: u16) -> String {
    format!("{TOKEN_START}{c}{TOKEN_END}")
}

/// `expr` with each [`token`] replaced by `with` of its condition.
pub(crate) fn replace_tokens(expr: &str, mut with: impl FnMut(u16) -> String) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut rest = expr;
    while let Some(start) = rest.find(TOKEN_START) {
        out.push_str(&rest[..start]);
        let after = &rest[start + TOKEN_START.len_utf8()..];
        let Some(end) = after.find(TOKEN_END) else {
            break;
        };
        match after[..end].parse() {
            Ok(c) => out.push_str(&with(c)),
            Err(_) => out.push_str(&rest[start..start + TOKEN_START.len_utf8() + end + 1]),
        }
        rest = &after[end + TOKEN_END.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// The object of each element, by node index, in a probe emission's tree.
pub(crate) fn element_paths(root: &Object) -> HashMap<usize, Vec<usize>> {
    let mut paths = HashMap::new();
    marker_paths(root, &mut Vec::new(), &mut paths);
    paths
        .into_iter()
        .map(|(n, p)| {
            let children = p
                .into_iter()
                .map(|s| match s {
                    Step::Child(i) => i,
                    Step::Prop(_) | Step::Item(_) => unreachable!("markers sit on children"),
                })
                .collect();
            (n, children)
        })
        .collect()
}

/// The element a marked object is of.
pub(crate) fn marked_node(obj: &Object) -> Option<usize> {
    marked(obj)
}

pub(crate) fn remove_markers(obj: &mut Object) {
    strip_markers(obj);
}

pub(crate) struct Emission {
    pub root: Object,
    pub targets: BTreeMap<NodeId, Target>,
    pub forms: BTreeMap<NodeId, Form>,
    pub imports: BTreeSet<&'static str>,
    /// Some `Behavior` waits for `root.eldurAnimate`, which the page's init
    /// handler must declare and set once the scripts have run.
    pub gated: bool,
    /// Set for a probe emission; its tree keeps the markers.
    pub probe: ProbeOut,
    /// Inline elements and options scripts reach through `eldurInline`.
    pub inline_covered: std::collections::BTreeSet<NodeId>,
}

/// The QML tree of the page, with conditional bindings when
/// `options.state_variants` is on and some condition changes a style.
pub(crate) fn emit(cx: &Inputs<'_>, diags: &mut Diagnostics) -> Emission {
    let conds = if cx.options.state_variants {
        conditions(cx, diags)
    } else {
        Vec::new()
    };
    if conds.is_empty() {
        return plain(cx, diags);
    }
    let list: Vec<Condition> = conds.iter().map(|c| c.cond.clone()).collect();
    let found = eldur_css::conditional_styles(cx.doc, cx.sheets, &cx.options.viewport, &list);
    for d in &found.diagnostics {
        diags.push(crate::pipeline::css_diagnostic(d));
    }
    if found.elements.is_empty() {
        return plain(cx, diags);
    }
    Dynamic {
        cx,
        conds,
        found,
        diags,
    }
    .run()
}

fn plain(cx: &Inputs<'_>, diags: &mut Diagnostics) -> Emission {
    let tree = boxes::build_with(
        cx.doc,
        cx.styles,
        Some(cx.wanted),
        cx.probe.map(|p| &p.inline_items),
        diags,
    );
    let body = cx.doc.body().and_then(|b| cx.styles.style(b));
    let mut emitter = Emitter::new(cx.doc, cx.styles, cx.options, diags, cx.wanted);
    emitter.images = cx.images.clone();
    emitter.background_images = cx.background_images.clone();
    emitter.runtime_dom = cx.runtime_dom;
    if let Some(scripts) = cx.scripts {
        emitter.open_elements = scripts.analysis.open_elements;
        emitter.hides_items = scripts.analysis.touches_inline_style;
        emitter.cascade_rules = scripts.analysis.needs_cascade();
        // Rows a script makes are sized by the same solver, table or no table.
        emitter.has_table = scripts.analysis.open_elements
            || !scripts.live_tables.is_empty()
            || scripts.analysis.created.iter().any(|c| {
                matches!(
                    c.tag.as_str(),
                    "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
                )
            });
        emitter.gains.clone_from(&scripts.gains.nodes);
        emitter.live_tables.clone_from(&scripts.live_tables);
        // A read needs an element to read: an empty one is not left out.
        emitter.gains_any_empty = scripts.gains.any_empty || scripts.analysis.reads_computed_style;
    }
    emitter.by_path = cx.scripts.is_some_and(|s| s.analysis.dispatch_by_path);
    emitter.script_attrs = cx
        .scripts
        .is_some_and(|s| s.analysis.reads_attributes || s.analysis.needs_cascade());
    if let Some(probe) = cx.probe {
        emitter.markers = true;
        emitter.no_carry = true;
        emitter.inline_items = probe.inline_items.clone();
        for id in &probe.reserved {
            emitter.ids.reserve(id);
        }
    }
    let root = emitter.document(tree.as_ref(), body);
    Emission {
        root,
        targets: std::mem::take(&mut emitter.targets),
        forms: std::mem::take(&mut emitter.forms),
        imports: std::mem::take(&mut emitter.imports),
        gated: false,
        probe: ProbeOut::default(),
        inline_covered: std::mem::take(&mut emitter.inline_covered),
    }
}

/// A condition and whether it holds in the document.
struct Cond {
    cond: Condition,
    initial: bool,
}

fn node_of(c: &Condition) -> NodeId {
    match c {
        Condition::Class { node, .. }
        | Condition::State { node, .. }
        | Condition::Structural { node, .. } => *node,
    }
}

/// The conditions of the page: states its selectors use where QML has the
/// state, and classes its scripts write.
fn conditions(cx: &Inputs<'_>, diags: &mut Diagnostics) -> Vec<Cond> {
    let doc = cx.doc;
    let mut out: BTreeMap<Condition, bool> = BTreeMap::new();
    let disabled_writes: HashSet<NodeId> = cx
        .scripts
        .map(|s| {
            s.analysis
                .writes
                .iter()
                .filter(|(_, prop)| prop == "disabled")
                .flat_map(|(q, _)| s.matches(q).iter().copied())
                .collect()
        })
        .unwrap_or_default();
    for v in &cx.styles.variants {
        let control = classify(doc, v.node);
        let element = doc.element(v.node);
        let has = |name: &str| element.is_some_and(|e| e.has_attr(name));
        let line = node_line(doc, v.node);
        let initial = match v.state {
            PseudoState::Hover | PseudoState::Active | PseudoState::Visited => false,
            PseudoState::Focus | PseudoState::FocusVisible => {
                if control.is_none() && !has("tabindex") {
                    diags.info(
                        "css",
                        line,
                        format!(
                            "`:focus` styles of {} are left out: only form controls and what has a `tabindex` take focus here",
                            describe(doc, v.node)
                        ),
                    );
                    continue;
                }
                false
            }
            PseudoState::FocusWithin => {
                if focusable_within(doc, v.node).is_empty() {
                    continue;
                }
                false
            }
            PseudoState::PlaceholderShown => {
                if !matches!(control, Some(Control::Text | Control::TextArea))
                    || !has("placeholder")
                {
                    continue;
                }
                eldur_css::state_in_document(doc, v.node, v.state)
            }
            PseudoState::Valid | PseudoState::Invalid => {
                if element
                    .and_then(|e| broken_constraint("", e, control))
                    .is_none()
                {
                    continue;
                }
                eldur_css::state_in_document(doc, v.node, v.state)
            }
            PseudoState::Checked => {
                if !matches!(control, Some(Control::CheckBox | Control::Radio)) {
                    continue;
                }
                has("checked")
            }
            PseudoState::Disabled => {
                // Otherwise the document's value is all there is.
                if control.is_none() || !disabled_writes.contains(&v.node) {
                    continue;
                }
                has("disabled")
            }
        };
        if v.state == PseudoState::Visited {
            // Links are never visited in a converted page.
            continue;
        }
        out.insert(
            Condition::State {
                node: v.node,
                state: v.state,
            },
            initial,
        );
    }
    if let Some(scripts) = cx.scripts {
        class_conditions(cx, scripts, &mut out, diags);
    }
    structural_conditions(cx, &mut out, diags);
    if let Some(probe) = cx.probe {
        for (&node, classes) in &probe.classes {
            for (class, &initial) in classes {
                out.insert(
                    Condition::Class {
                        node,
                        class: class.clone(),
                    },
                    initial,
                );
            }
        }
        out.retain(|c, _| probe.relevant.contains(&node_of(c)));
    }
    out.into_iter()
        .map(|(cond, initial)| Cond { cond, initial })
        .collect()
}

/// Classes scripts write on static elements (dynamic-styles §5.1): a
/// literal query's elements, or every element the class would restyle
/// when the target is not known.
fn class_conditions(
    cx: &Inputs<'_>,
    scripts: &Scripts,
    out: &mut BTreeMap<Condition, bool>,
    diags: &mut Diagnostics,
) {
    let doc = cx.doc;
    let styled = eldur_css::styled_classes(cx.sheets);
    let own = |node: NodeId| -> Vec<String> {
        doc.element(node)
            .map(|e| e.classes().map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let add = |node: NodeId, class: &str, out: &mut BTreeMap<Condition, bool>| {
        if !styled.contains(class) {
            return;
        }
        let initial = own(node).iter().any(|c| c == class);
        out.insert(
            Condition::Class {
                node,
                class: class.to_owned(),
            },
            initial,
        );
    };
    for write in &scripts.analysis.class_writes {
        let nodes: Vec<NodeId> = match &write.target {
            ElementRef::Query(query) => scripts.matches(query).to_vec(),
            ElementRef::Unknown => {
                let mut nodes = BTreeSet::new();
                for name in &write.names {
                    for node in eldur_css::class_candidates(doc, cx.sheets, name) {
                        add(node, name, out);
                        nodes.insert(node);
                    }
                }
                if write.open && write.names.is_empty() {
                    diags.info(
                        "js",
                        write.line,
                        "a class written on an element the script finds at run time, with a computed name, is not followed".to_owned(),
                    );
                }
                nodes.into_iter().collect()
            }
            // Templates (S7).
            ElementRef::Created(_) => continue,
        };
        for node in nodes {
            for name in &write.names {
                add(node, name, out);
            }
            let classes = own(node);
            if write.assigns {
                for class in &classes {
                    add(node, class, out);
                }
            }
            if write.open {
                let tag = doc
                    .element(node)
                    .map(|e| e.local_name().to_ascii_lowercase())
                    .unwrap_or_default();
                let mut known = classes.clone();
                known.extend(write.names.iter().cloned());
                for class in eldur_css::open_class_candidates(cx.sheets, &tag, &known) {
                    add(node, &class, out);
                }
            }
        }
    }
}

/// The places of the children of containers scripts insert into or remove
/// from (dynamic-styles §5.5), for the structural pseudo-classes the
/// sheets use. Other containers keep the document's positions.
fn structural_conditions(
    cx: &Inputs<'_>,
    out: &mut BTreeMap<Condition, bool>,
    diags: &mut Diagnostics,
) {
    let tests = eldur_css::structural_pseudos(cx.sheets);
    if tests.is_empty() {
        return;
    }
    let doc = cx.doc;
    let mut containers: BTreeSet<NodeId> = BTreeSet::new();
    if let Some(scripts) = cx.scripts {
        let analysis = &scripts.analysis;
        let parents = analysis
            .created
            .iter()
            .flat_map(|e| &e.parents)
            .chain(&analysis.mutated_parents);
        for target in parents {
            if let ElementRef::Query(query) = target {
                containers.extend(scripts.matches(query).iter().copied());
            }
        }
        for target in &analysis.mutated_siblings {
            if let ElementRef::Query(query) = target {
                containers.extend(
                    scripts
                        .matches(query)
                        .iter()
                        .filter_map(|&n| doc.parent_element(n)),
                );
            }
        }
    }
    if let Some(probe) = cx.probe {
        containers.extend(probe.nodes.iter().filter_map(|&n| doc.parent_element(n)));
    }
    for container in containers {
        let kids: Vec<NodeId> = doc.element_children(container).collect();
        if kids.len() > MAX_POSITIONED {
            if cx.probe.is_none() {
                diags.warning(
                    "css",
                    node_line(doc, container),
                    format!(
                        "{} has more than {MAX_POSITIONED} children; scripts changing them do not restyle `:first-child`, `:nth-child()` and the like (dynamic-structural)",
                        describe(doc, container)
                    ),
                );
            }
            continue;
        }
        for kid in kids {
            let (place, type_place) = crate::metadata::position(doc, kid);
            for &test in &tests {
                let (index, count) = if test.of_type() { type_place } else { place };
                out.insert(
                    Condition::Structural { node: kid, test },
                    test.matches(index, count),
                );
            }
        }
    }
}

/// The QML test of `test` on the element with id `id`, which declares
/// [`crate::metadata::declare_position`]'s properties.
pub(crate) fn structural_expr(test: Structural, id: &str) -> String {
    let (index, count) = if test.of_type() {
        (
            format!("{id}.eldurTypeIndex"),
            format!("{id}.eldurTypeCount"),
        )
    } else {
        (format!("{id}.eldurIndex"), format!("{id}.eldurCount"))
    };
    let from_end = format!("({count} - {index} + 1)");
    match test {
        Structural::FirstChild | Structural::FirstOfType => format!("{index} === 1"),
        Structural::LastChild | Structural::LastOfType => format!("{index} === {count}"),
        Structural::OnlyChild | Structural::OnlyOfType => format!("{count} === 1"),
        Structural::NthChild { a, b } | Structural::NthOfType { a, b } => nth(a, b, &index),
        Structural::NthLastChild { a, b } | Structural::NthLastOfType { a, b } => {
            nth(a, b, &from_end)
        }
    }
}

/// Whether `x` is `a`n+`b` for some n >= 0.
fn nth(a: i32, b: i32, x: &str) -> String {
    let offset = match b {
        0 => x.to_owned(),
        b if b > 0 => format!("({x} - {b})"),
        b => format!("({x} + {})", -i64::from(b)),
    };
    match a {
        0 => format!("{x} === {b}"),
        1 => format!("{x} >= {b}"),
        -1 => format!("{x} <= {b}"),
        a if a > 0 && b <= 1 => format!("{offset} % {a} === 0"),
        a if a > 0 => format!("({x} >= {b} && {offset} % {a} === 0)"),
        a => format!("({x} <= {b} && ({b} - {x}) % {} === 0)", -i64::from(a)),
    }
}

/// How the sheets write `test`.
pub(crate) fn structural_name(test: Structural) -> String {
    let formula = |a: i32, b: i32| match (a, b) {
        (0, b) => b.to_string(),
        (a, 0) => format!("{a}n"),
        (a, b) if b > 0 => format!("{a}n+{b}"),
        (a, b) => format!("{a}n{b}"),
    };
    match test {
        Structural::FirstChild => ":first-child".to_owned(),
        Structural::LastChild => ":last-child".to_owned(),
        Structural::OnlyChild => ":only-child".to_owned(),
        Structural::FirstOfType => ":first-of-type".to_owned(),
        Structural::LastOfType => ":last-of-type".to_owned(),
        Structural::OnlyOfType => ":only-of-type".to_owned(),
        Structural::NthChild { a, b } => format!(":nth-child({})", formula(a, b)),
        Structural::NthLastChild { a, b } => format!(":nth-last-child({})", formula(a, b)),
        Structural::NthOfType { a, b } => format!(":nth-of-type({})", formula(a, b)),
        Structural::NthLastOfType { a, b } => format!(":nth-last-of-type({})", formula(a, b)),
    }
}

/// One step from an object to a nested one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Step {
    /// The child at this member index.
    Child(usize),
    /// The object value of this property.
    Prop(String),
    /// The object at this index of the list value of the previous `Prop`.
    Item(usize),
}

type Path = Vec<Step>;

/// One property that differs between worlds.
struct Change {
    owner: Option<NodeId>,
    /// The owning object's type.
    ty: String,
    base: Option<Value>,
    /// The worlds where it differs from `base` (`None`: not set there).
    worlds: BTreeMap<usize, Option<Value>>,
}

#[derive(Default)]
struct Diffs {
    changes: BTreeMap<(Path, String), Change>,
    /// Subtrees whose shape differs in a world: nothing is known there.
    unknown: Vec<(Path, usize)>,
    /// Elements whose form differs in some world.
    reshaped: BTreeSet<Option<NodeId>>,
}

impl Diffs {
    fn diff(
        &mut self,
        world: usize,
        base: &Object,
        other: &Object,
        path: &mut Path,
        owner: Option<NodeId>,
        doc: &Document,
    ) {
        let owner = marked(base).and_then(|i| node_at(doc, i)).or(owner);
        let children = |o: &Object| -> Vec<usize> {
            o.members
                .iter()
                .enumerate()
                .filter_map(|(i, m)| matches!(m, Member::Child(_)).then_some(i))
                .collect()
        };
        let (bc, oc) = (children(base), children(other));
        if base.type_name != other.type_name || base.id != other.id || bc.len() != oc.len() {
            self.unknown.push((path.clone(), world));
            self.reshaped.insert(owner);
            return;
        }
        let props = |o: &Object| -> Vec<(String, Value)> {
            o.members
                .iter()
                .filter_map(|m| match m {
                    Member::Property { name, value } => Some((name.clone(), value.clone())),
                    _ => None,
                })
                .collect()
        };
        let (bp, op) = (props(base), props(other));
        let mut names: Vec<&str> = bp.iter().map(|(n, _)| n.as_str()).collect();
        for (n, _) in &op {
            if !names.contains(&n.as_str()) {
                names.push(n);
            }
        }
        let get = |list: &[(String, Value)], name: &str| {
            list.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
        };
        for name in names {
            let (b, o) = (get(&bp, name), get(&op, name));
            if b == o {
                continue;
            }
            match (&b, &o) {
                (Some(Value::Object(b)), Some(Value::Object(o))) => {
                    path.push(Step::Prop(name.to_owned()));
                    self.diff(world, b, o, path, owner, doc);
                    path.pop();
                }
                (Some(Value::List(bl)), Some(Value::List(ol)))
                    if bl.len() == ol.len()
                        && bl.iter().chain(ol).all(|v| matches!(v, Value::Object(_))) =>
                {
                    for (k, (b, o)) in bl.iter().zip(ol).enumerate() {
                        let (Value::Object(b), Value::Object(o)) = (b, o) else {
                            unreachable!("checked above");
                        };
                        path.push(Step::Prop(name.to_owned()));
                        path.push(Step::Item(k));
                        self.diff(world, b, o, path, owner, doc);
                        path.pop();
                        path.pop();
                    }
                }
                _ => {
                    let change = self
                        .changes
                        .entry((path.clone(), name.to_owned()))
                        .or_insert_with(|| Change {
                            owner,
                            ty: base.type_name.clone(),
                            base: b.clone(),
                            worlds: BTreeMap::new(),
                        });
                    change.worlds.insert(world, o);
                }
            }
        }
        for (bi, oi) in bc.into_iter().zip(oc) {
            let (Member::Child(b), Member::Child(o)) = (&base.members[bi], &other.members[oi])
            else {
                unreachable!("children");
            };
            path.push(Step::Child(bi));
            self.diff(world, b, o, path, owner, doc);
            path.pop();
        }
    }

    fn known(&self, path: &Path, world: usize) -> bool {
        !self
            .unknown
            .iter()
            .any(|(p, w)| *w == world && path.starts_with(p))
    }
}

fn node_at(doc: &Document, index: usize) -> Option<NodeId> {
    doc.descendants(doc.root()).find(|n| n.index() == index)
}

fn object_at<'o>(root: &'o mut Object, path: &[Step]) -> Option<&'o mut Object> {
    let mut at = root;
    let mut i = 0;
    while i < path.len() {
        at = match &path[i] {
            Step::Child(m) => match at.members.get_mut(*m) {
                Some(Member::Child(c)) => c,
                _ => return None,
            },
            Step::Prop(name) => {
                let value = at.members.iter_mut().find_map(|m| match m {
                    Member::Property { name: n, value } if n == name => Some(value),
                    _ => None,
                })?;
                match (value, path.get(i + 1)) {
                    (Value::Object(o), _) => o,
                    (Value::List(items), Some(Step::Item(k))) => {
                        i += 1;
                        match items.get_mut(*k) {
                            Some(Value::Object(o)) => o,
                            _ => return None,
                        }
                    }
                    _ => return None,
                }
            }
            Step::Item(_) => return None,
        };
        i += 1;
    }
    Some(at)
}

/// Every marked object's path.
fn marker_paths(obj: &Object, path: &mut Path, out: &mut HashMap<usize, Path>) {
    if let Some(i) = marked(obj) {
        out.entry(i).or_insert_with(|| path.clone());
    }
    for (i, m) in obj.members.iter().enumerate() {
        if let Member::Child(c) = m {
            path.push(Step::Child(i));
            marker_paths(c, path, out);
            path.pop();
        }
    }
}

fn strip_markers(obj: &mut Object) {
    obj.members
        .retain(|m| !matches!(m, Member::Comment(c) if c.starts_with(MARKER)));
    for m in &mut obj.members {
        match m {
            Member::Child(c) => strip_markers(c),
            Member::Property { value, .. } => strip_value(value),
            _ => {}
        }
    }
}

fn strip_value(value: &mut Value) {
    match value {
        Value::Object(o) => strip_markers(o),
        Value::List(items) => items.iter_mut().for_each(strip_value),
        _ => {}
    }
}

struct Dynamic<'a, 'd> {
    cx: &'a Inputs<'a>,
    conds: Vec<Cond>,
    found: ConditionalStyles,
    diags: &'d mut Diagnostics,
}

/// What one emission of a world leaves.
struct Emitted {
    root: Object,
    targets: BTreeMap<NodeId, Target>,
    forms: BTreeMap<NodeId, Form>,
    imports: BTreeSet<&'static str>,
    ids: Ids,
    inline_covered: BTreeSet<NodeId>,
}

impl Dynamic<'_, '_> {
    fn run(mut self) -> Emission {
        for element in &mut self.found.elements {
            normalize(&mut element.by_mask);
        }
        let sticky: HashSet<NodeId> = self
            .found
            .elements
            .iter()
            .filter(|e| is_sticky(&e.by_mask))
            .map(|e| e.node)
            .collect();
        let strips: HashMap<NodeId, [bool; 4]> = self
            .found
            .elements
            .iter()
            .filter_map(|e| border_strips(&e.by_mask).map(|s| (e.node, s)))
            .collect();
        if self.found.elements.iter().any(|e| {
            outline_changes(&e.by_mask)
                && e.by_mask
                    .iter()
                    .any(|s| super::decoration::is_visible(&s.outline))
        }) {
            self.diags.info(
                "css",
                None,
                "an outline that changes under a class or state is not drawn".to_owned(),
            );
        }
        let plain_edges: HashSet<NodeId> = self
            .found
            .elements
            .iter()
            .filter(|e| edges_change_form(&e.by_mask))
            .map(|e| e.node)
            .collect();
        // The display an element hidden in some combinations has when shown.
        let shown: HashMap<NodeId, Display> = self
            .found
            .elements
            .iter()
            .filter_map(|e| {
                let none = e.by_mask.iter().any(|s| s.display == Display::None);
                let shown = e.by_mask.iter().find(|s| s.display != Display::None)?;
                (none && shown.display != Display::Inline).then_some((e.node, shown.display))
            })
            .collect();
        let (styles, collapsed) = self.world(&[], &shown);
        let Emitted {
            mut root,
            targets,
            forms,
            inline_covered,
            mut imports,
            mut ids,
        } = Self::emit_world(
            self.cx,
            &styles,
            &sticky,
            &strips,
            &plain_edges,
            collapsed,
            self.diags,
        );
        let mut markers = HashMap::new();
        marker_paths(&root, &mut Vec::new(), &mut markers);
        // A condition on an element without an item of its own (an inline
        // link in a paragraph) has nothing to bind; its combinations would
        // only change the form of the text around it.
        let unmarked: BTreeSet<u16> = (0..self.conds.len())
            .filter(|&c| !markers.contains_key(&node_of(&self.conds[c].cond).index()))
            .map(|c| u16::try_from(c).expect("conditions fit u16"))
            .collect();
        for &c in &unmarked {
            // The runtime styles a node's classes itself, by the page's rules.
            if let Condition::Class { node, .. } = &self.conds[usize::from(c)].cond
                && self.cx.runtime_dom
                && inline_covered.contains(node)
            {
                continue;
            }
            no_item(self.cx.doc, &self.conds[usize::from(c)].cond, self.diags);
        }
        let worlds = self.worlds(&unmarked);
        let mut trees = Vec::with_capacity(worlds.len());
        for flipped in &worlds[1..] {
            let (styles, collapsed) = self.world(flipped, &shown);
            // Only the document's world reports; the others repeat it.
            let mut scratch = Diagnostics::new(&self.diags.document);
            let emitted = Self::emit_world(
                self.cx,
                &styles,
                &sticky,
                &strips,
                &plain_edges,
                collapsed,
                &mut scratch,
            );
            imports.extend(emitted.imports);
            trees.push(emitted.root);
        }

        let mut diffs = Diffs::default();
        for (w, tree) in trees.iter().enumerate() {
            diffs.diff(w + 1, &root, tree, &mut Vec::new(), None, self.cx.doc);
        }
        for owner in diffs.reshaped.iter().filter(|_| self.cx.probe.is_none()) {
            let what = owner.map_or_else(|| "the page".to_owned(), |n| describe(self.cx.doc, n));
            self.diags.warning(
                "css",
                owner.and_then(|n| node_line(self.cx.doc, n)),
                format!(
                    "{what} changes its form under a class or state (dynamic-form); it keeps the document's"
                ),
            );
        }
        let mut sources = Sources {
            probe: self.cx.probe.is_some(),
            ..Sources::default()
        };
        let mut bindings: BTreeMap<Path, Vec<(String, String, Option<Member>)>> = BTreeMap::new();
        let gate = self.cx.scripts.is_some();
        let mut gated = false;
        let changes = std::mem::take(&mut diffs.changes);
        for ((path, prop), change) in &changes {
            let Some(order) = self.dependencies(
                change,
                path,
                &diffs,
                &worlds,
                &markers,
                &mut sources,
                &mut root,
                &mut ids,
            ) else {
                continue;
            };
            let Some(binding) = self.binding(&order, path, prop, change, &diffs, &worlds, &sources)
            else {
                continue;
            };
            let behavior = self.behavior(change, prop, gate);
            gated |= behavior.is_some();
            bindings
                .entry(path.clone())
                .or_default()
                .push((prop.clone(), binding, behavior));
        }
        for (path, props) in bindings {
            let Some(obj) = object_at(&mut root, &path) else {
                continue;
            };
            for (prop, binding, behavior) in props {
                obj.set(&prop, Value::Expr(binding));
                if let Some(b) = behavior {
                    obj.members.push(b);
                }
            }
        }
        let locals = std::mem::take(&mut sources.locals);
        sources.apply(&mut root, self.cx.doc, &mut ids, &mut imports);
        let mut probe = ProbeOut::default();
        if self.cx.probe.is_some() {
            probe = ProbeOut {
                conditions: self
                    .conds
                    .iter()
                    .map(|c| (c.cond.clone(), c.initial))
                    .collect(),
                locals,
                reshaped: diffs.reshaped.iter().flatten().copied().collect(),
            };
        } else {
            strip_markers(&mut root);
        }
        Emission {
            root,
            targets,
            forms,
            imports,
            gated: gated && gate,
            probe,
            inline_covered,
        }
    }

    /// The styles where `flipped` differ from the document, and the elements
    /// hidden there.
    fn world(
        &self,
        flipped: &[u16],
        shown: &HashMap<NodeId, Display>,
    ) -> (StyleMap, HashSet<NodeId>) {
        let mut collapsed = HashSet::new();
        let overrides: Vec<(NodeId, ComputedStyle)> = self
            .found
            .elements
            .iter()
            .map(|e| {
                let mask = e
                    .depends_on
                    .iter()
                    .enumerate()
                    .filter(|&(_, c)| self.holds(*c, flipped))
                    .fold(0usize, |m, (i, _)| m | 1 << i);
                let mut style = (*e.by_mask[mask]).clone();
                // A drawn outline is another object: one that comes and goes
                // would make the worlds differ in form, and every other state
                // binding of the page's tree would go with it.
                if outline_changes(&e.by_mask) {
                    style.outline = Default::default();
                    style.outline_offset = Default::default();
                }
                if style.display == Display::None
                    && let Some(display) = shown.get(&e.node)
                {
                    style.display = *display;
                    collapsed.insert(e.node);
                }
                (e.node, style)
            })
            .collect();
        (self.cx.styles.with_styles(overrides), collapsed)
    }

    fn holds(&self, c: u16, flipped: &[u16]) -> bool {
        self.conds[usize::from(c)].initial ^ flipped.contains(&c)
    }

    /// Every combination of flipped conditions some element depends on,
    /// the document's (none flipped) first.
    fn worlds(&mut self, excluded: &BTreeSet<u16>) -> Vec<Vec<u16>> {
        let mut set: BTreeSet<(usize, Vec<u16>)> = BTreeSet::new();
        set.insert((0, Vec::new()));
        for e in &self.found.elements {
            let deps: Vec<u16> = e
                .depends_on
                .iter()
                .copied()
                .filter(|c| !excluded.contains(c))
                .collect();
            for mask in 0..1usize << deps.len() {
                let mut w: Vec<u16> = deps
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & 1 << i != 0)
                    .map(|(_, c)| *c)
                    .collect();
                w.sort_unstable();
                set.insert((w.len(), w));
            }
        }
        if set.len() > MAX_WORLDS {
            self.diags.warning(
                "css",
                None,
                format!(
                    "classes and states combine in {} ways; only {MAX_WORLDS} are converted",
                    set.len()
                ),
            );
        }
        set.into_iter().take(MAX_WORLDS).map(|(_, w)| w).collect()
    }

    fn emit_world(
        cx: &Inputs<'_>,
        styles: &StyleMap,
        sticky: &HashSet<NodeId>,
        strips: &HashMap<NodeId, [bool; 4]>,
        plain_edges: &HashSet<NodeId>,
        collapsed: HashSet<NodeId>,
        diags: &mut Diagnostics,
    ) -> Emitted {
        let tree = boxes::build_with(
            cx.doc,
            styles,
            Some(cx.wanted),
            cx.probe.map(|p| &p.inline_items),
            diags,
        );
        let body = cx.doc.body().and_then(|b| styles.style(b));
        let mut emitter = Emitter::new(cx.doc, styles, cx.options, diags, cx.wanted);
        emitter.images = cx.images.clone();
        emitter.background_images = cx.background_images.clone();
        emitter.runtime_dom = cx.runtime_dom;
        if let Some(scripts) = cx.scripts {
            emitter.open_elements = scripts.analysis.open_elements;
            emitter.hides_items = scripts.analysis.touches_inline_style;
            emitter.gains.clone_from(&scripts.gains.nodes);
            emitter.live_tables.clone_from(&scripts.live_tables);
            emitter.gains_any_empty = scripts.gains.any_empty;
            // A row's `style` counts toward what its cells paint later.
            emitter.cascade_rules = scripts.analysis.static_tables;
        }
        emitter.by_path = cx.scripts.is_some_and(|s| s.analysis.dispatch_by_path);
        emitter.script_attrs = cx
            .scripts
            .is_some_and(|s| s.analysis.reads_attributes || s.analysis.needs_cascade());
        emitter.markers = true;
        if let Some(probe) = cx.probe {
            emitter.no_carry = true;
            emitter.inline_items = probe.inline_items.clone();
        }
        for id in cx.probe.iter().flat_map(|p| &p.reserved) {
            emitter.ids.reserve(id);
        }
        emitter.sticky = sticky.clone();
        emitter.strips = strips.clone();
        emitter.plain_edges = plain_edges.clone();
        emitter.collapsed = collapsed;
        let root = emitter.document(tree.as_ref(), body);
        let emitted = Emitted {
            root,
            targets: std::mem::take(&mut emitter.targets),
            forms: std::mem::take(&mut emitter.forms),
            imports: std::mem::take(&mut emitter.imports),
            ids: std::mem::take(&mut emitter.ids),
            inline_covered: std::mem::take(&mut emitter.inline_covered),
        };
        drop(emitter);
        drop(tree);
        emitted
    }

    /// The conditions `change` depends on, ordered for the ternary: the
    /// owner's, then its ancestors' nearest first, then the rest.
    #[allow(clippy::too_many_arguments)]
    fn dependencies(
        &mut self,
        change: &Change,
        path: &Path,
        diffs: &Diffs,
        worlds: &[Vec<u16>],
        markers: &HashMap<usize, Path>,
        sources: &mut Sources,
        root: &mut Object,
        ids: &mut Ids,
    ) -> Option<Vec<u16>> {
        // A condition matters when flipping it alone changes the value
        // somewhere; being flipped together with one that does is not
        // enough (`.a.on + .b` makes `b` depend on `a`, not on `b`'s
        // next sibling).
        let index: HashMap<&[u16], usize> = worlds
            .iter()
            .enumerate()
            .map(|(i, w)| (w.as_slice(), i))
            .collect();
        let value = |w: usize| {
            (w == 0 || diffs.known(path, w)).then(|| change.worlds.get(&w).unwrap_or(&change.base))
        };
        let mut deps = BTreeSet::new();
        for (w, set) in worlds.iter().enumerate() {
            let Some(here) = value(w) else {
                continue;
            };
            for c in 0..self.conds.len() {
                let c = u16::try_from(c).expect("conditions fit u16");
                if set.contains(&c) || deps.contains(&c) {
                    continue;
                }
                let mut with = set.clone();
                with.push(c);
                with.sort_unstable();
                if let Some(&other) = index.get(with.as_slice())
                    && value(other).is_some_and(|there| there != here)
                {
                    deps.insert(c);
                }
            }
        }
        let mut usable = Vec::new();
        for c in deps {
            if sources.source(self, c, markers, root, ids).is_some() {
                usable.push(c);
            }
        }
        if usable.is_empty() {
            return None;
        }
        let doc = self.cx.doc;
        let rank = |c: &u16| -> usize {
            let node = node_of(&self.conds[usize::from(*c)].cond);
            let Some(owner) = change.owner else {
                return usize::MAX;
            };
            let mut at = Some(owner);
            let mut depth = 0;
            while let Some(n) = at {
                if n == node {
                    return depth;
                }
                depth += 1;
                at = doc.parent(n);
            }
            usize::MAX
        };
        usable.sort_by_key(|c| (rank(c), *c));
        Some(usable)
    }

    #[allow(clippy::too_many_arguments)]
    fn binding(
        &mut self,
        order: &[u16],
        path: &Path,
        prop: &str,
        change: &Change,
        diffs: &Diffs,
        worlds: &[Vec<u16>],
        sources: &Sources,
    ) -> Option<String> {
        let line = change.owner.and_then(|n| node_line(self.cx.doc, n));
        let mut missing = false;
        let mut text = |v: Option<Value>| -> Option<String> {
            let v = match v {
                Some(v) => v,
                None => {
                    let d = default_value(&change.ty, prop);
                    if d.is_none() {
                        missing = true;
                    }
                    d?
                }
            };
            literal(&v)
        };
        // The value where exactly `flipped` (sorted) differ from the document.
        let value = |flipped: &[u16]| -> Option<Value> {
            // A world that agrees on `order`, with the fewest others flipped.
            let agreeing = worlds
                .iter()
                .enumerate()
                .filter(|(w, set)| {
                    (*w == 0 || diffs.known(path, *w))
                        && order.iter().all(|c| set.contains(c) == flipped.contains(c))
                })
                .min_by_key(|(_, set)| set.len())
                .map(|(w, _)| w);
            if let Some(w) = agreeing {
                return change.worlds.get(&w).unwrap_or(&change.base).clone();
            }
            let best = worlds
                .iter()
                .enumerate()
                .filter(|(w, set)| {
                    set.iter().all(|c| flipped.contains(c)) && (*w == 0 || diffs.known(path, *w))
                })
                .max_by_key(|(_, set)| set.len())
                .map_or(0, |(w, _)| w);
            if best == 0 {
                change.base.clone()
            } else {
                match change.worlds.get(&best) {
                    Some(v) => v.clone(),
                    None => change.base.clone(),
                }
            }
        };
        let mut leaves: HashMap<Vec<u16>, Option<String>> = HashMap::new();
        for mask in 0..1usize << order.len() {
            let mut flipped: Vec<u16> = order
                .iter()
                .enumerate()
                .filter(|(i, c)| (mask & 1 << i != 0) != self.conds[usize::from(**c)].initial)
                .map(|(_, c)| *c)
                .collect();
            flipped.sort_unstable();
            let t = text(value(&flipped));
            leaves.insert(holds_key(order, mask), t);
        }
        if missing || leaves.values().any(Option::is_none) {
            let what = change
                .owner
                .map_or_else(|| "an element".to_owned(), |n| describe(self.cx.doc, n));
            self.diags.info(
                "css",
                line,
                format!("`{prop}` of {what} changes under a class or state but has no QML form here; it keeps the document's value"),
            );
            return None;
        }
        let leaves: HashMap<Vec<u16>, String> = leaves
            .into_iter()
            .map(|(k, v)| (k, v.expect("checked")))
            .collect();
        Some(expand(order, 0, 0, &leaves, sources))
    }

    /// A `Behavior` for `prop` when its owner's style transitions it.
    fn behavior(&self, change: &Change, prop: &str, gate: bool) -> Option<Member> {
        if !self.cx.options.animations {
            return None;
        }
        let owner = change.owner?;
        let longhands = css_of(&change.ty, prop)?;
        let kind = change
            .worlds
            .values()
            .flatten()
            .chain(change.base.as_ref())
            .next()?;
        let animation = match kind {
            Value::Color(_) => "ColorAnimation",
            Value::Number(_) => "NumberAnimation",
            _ => return None,
        };
        let styles: Vec<&ComputedStyle> = match self.found.elements.iter().find(|e| e.node == owner)
        {
            Some(e) => e.by_mask.iter().map(|s| &**s).collect(),
            None => self.cx.styles.style(owner).into_iter().collect(),
        };
        let transition = styles.iter().find_map(|s| {
            s.transitions.iter().rev().find(|t| {
                t.property == "all"
                    || longhands.contains(&t.property.as_str())
                    || longhands
                        .iter()
                        .any(|l| l.starts_with(&format!("{}-", t.property)))
            })
        })?;
        if transition.duration_ms <= 0.0 {
            return None;
        }
        let duration = fmt_number(f64::from(transition.duration_ms.round()));
        let mut anim = Object::new(animation);
        anim.set(
            "duration",
            if gate {
                // The document's classes are set by the page's init, which
                // must not animate.
                Value::expr(format!("{ROOT_ID}.eldurAnimate ? {duration} : 0"))
            } else {
                Value::Number(f64::from(transition.duration_ms.round()))
            },
        );
        super::keyframes::set_easing(&mut anim, transition.easing, false);
        // A negative delay would start part way through, which QML cannot.
        if transition.delay_ms > 0.0 {
            let delay = fmt_number(f64::from(transition.delay_ms.round()));
            let mut pause = Object::new("PauseAnimation");
            pause.set(
                "duration",
                if gate {
                    Value::expr(format!("{ROOT_ID}.eldurAnimate ? {delay} : 0"))
                } else {
                    Value::Number(f64::from(transition.delay_ms.round()))
                },
            );
            let mut seq = Object::new("SequentialAnimation");
            seq.push_child(pause);
            seq.push_child(anim);
            anim = seq;
        }
        Some(Member::Behavior {
            property: prop.to_owned(),
            animation: anim,
        })
    }
}

/// The key of a leaf: whether each condition of `order` holds.
fn holds_key(order: &[u16], mask: usize) -> Vec<u16> {
    order
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & 1 << i != 0)
        .map(|(_, c)| *c)
        .collect()
}

/// Shannon expansion over `order[at..]`, `mask` holding the choices so far.
fn expand(
    order: &[u16],
    at: usize,
    mask: usize,
    leaves: &HashMap<Vec<u16>, String>,
    sources: &Sources,
) -> String {
    if at == order.len() {
        return leaves[&holds_key(order, mask)].clone();
    }
    let yes = expand(order, at + 1, mask | 1 << at, leaves, sources);
    let no = expand(order, at + 1, mask, leaves, sources);
    if yes == no {
        return yes;
    }
    let cond = sources.expr(order[at]);
    match (yes.as_str(), no.as_str()) {
        ("true", "false") => cond.to_owned(),
        ("false", "true") => negate(cond),
        _ => format!("{cond} ? {} : {}", group(&yes), group(&no)),
    }
}

fn negate(cond: &str) -> String {
    if cond.contains(' ') {
        return format!("!({cond})");
    }
    cond.strip_prefix('!')
        .map_or_else(|| format!("!{cond}"), str::to_owned)
}

fn group(e: &str) -> String {
    if e.contains(" ? ") {
        format!("({e})")
    } else {
        e.to_owned()
    }
}

/// `v` as a JavaScript expression.
fn literal(v: &Value) -> Option<String> {
    Some(match v {
        Value::Expr(e) => {
            let name = |s: &str| {
                s.chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
            };
            // A name, or a call such as `Qt.vector2d(0, 2)`, binds tighter
            // than `?:`.
            let call = e.split_once('(').is_some_and(|(f, rest)| {
                name(f)
                    && rest
                        .strip_suffix(')')
                        .is_some_and(|a| !a.contains(['(', ')']))
            });
            if name(e) || call {
                e.clone()
            } else {
                format!("({e})")
            }
        }
        Value::String(s) => eldur_qml::quote_string(s),
        Value::Number(n) => fmt_number(*n),
        Value::Bool(b) => b.to_string(),
        Value::Color(c) => eldur_qml::quote_string(&c.to_qml_string()),
        Value::List(_) | Value::Object(_) | Value::Block(_) => return None,
    })
}

/// A QML property's value when it is not set, where the emitter leaves it
/// out at its default.
fn default_value(ty: &str, prop: &str) -> Option<Value> {
    let n = |v: f64| Some(Value::Number(v));
    match prop {
        "opacity" | "xScale" | "yScale" | "scale" => n(1.0),
        "visible" | "enabled" => Some(Value::Bool(true)),
        "clip" | "font.bold" | "font.italic" | "font.underline" | "font.strikeout" => {
            Some(Value::Bool(false))
        }
        "color" if ty == "Text" => Some(Value::Color(eldur_qml::Color::BLACK)),
        "color" if ty == "Rectangle" => Some(Value::Color(eldur_qml::Color::WHITE)),
        "border.color" => Some(Value::Color(eldur_qml::Color::BLACK)),
        "border.width" | "radius" | "spread" | "x" | "y" | "z" | "angle" | "rotation"
        | "font.letterSpacing" | "font.wordSpacing" | "topLeftRadius" | "topRightRadius"
        | "bottomLeftRadius" | "bottomRightRadius" => n(0.0),
        "font.weight" => n(400.0),
        "horizontalAlignment" => Some(Value::expr("Text.AlignLeft")),
        "offset" if ty == "RectangularShadow" => Some(Value::expr("Qt.vector2d(0, 0)")),
        p if p.starts_with("anchors.") && p.ends_with("Margin") => n(0.0),
        p if p.starts_with("Layout.") && p.ends_with("Margin") => n(0.0),
        _ => None,
    }
}

/// The CSS longhands a QML property of an object of type `ty` shows, for
/// matching `transition-property`.
fn css_of(ty: &str, prop: &str) -> Option<&'static [&'static str]> {
    Some(match (ty, prop) {
        (_, "opacity") => &["opacity"],
        ("Text", "color") => &["color"],
        ("Rectangle", "color") => &["background-color"],
        ("RectangularShadow", "color" | "blur" | "spread") => &["box-shadow"],
        (_, "border.color") => &["border-top-color", "border-color"],
        (_, "border.width") => &["border-top-width", "border-width"],
        (
            _,
            "radius" | "topLeftRadius" | "topRightRadius" | "bottomLeftRadius"
            | "bottomRightRadius",
        ) => &["border-top-left-radius", "border-radius"],
        ("Translate", "x" | "y") | ("Scale", "xScale" | "yScale") | ("Rotation", "angle") => {
            &["transform"]
        }
        _ => return None,
    })
}

/// Pads `transform` and `box-shadow` lists to one shape across `styles`, so
/// each combination emits the same objects.
fn normalize(styles: &mut [Box<ComputedStyle>]) {
    if let Some(template) = styles
        .iter()
        .map(|s| s.transform.clone())
        .max_by_key(Vec::len)
        .filter(|t| !t.is_empty())
    {
        let identity: Vec<TransformFn> = template.iter().map(identity_of).collect();
        for s in styles.iter_mut() {
            if s.transform.is_empty() {
                s.transform = identity.clone();
            }
        }
    }
    let outer = |s: &ComputedStyle| s.box_shadow.iter().filter(|s| !s.inset).count();
    let most = styles.iter().map(|s| outer(s)).max().unwrap_or(0);
    for s in styles.iter_mut() {
        for _ in outer(s)..most {
            s.box_shadow.push(Shadow {
                offset_x: 0.0,
                offset_y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: Rgba::TRANSPARENT,
                inset: false,
            });
        }
    }
}

fn identity_of(f: &TransformFn) -> TransformFn {
    match f {
        TransformFn::Translate(..) | TransformFn::TranslateCalc(..) => {
            TransformFn::Translate(LengthPercentage::ZERO, LengthPercentage::ZERO)
        }
        TransformFn::Scale(..) => TransformFn::Scale(1.0, 1.0),
        TransformFn::Rotate(_) => TransformFn::Rotate(0.0),
        TransformFn::Skew(..) => TransformFn::Skew(0.0, 0.0),
        TransformFn::Matrix(_) => TransformFn::Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        TransformFn::Unsupported(t) => TransformFn::Unsupported(t.clone()),
    }
}

/// A drawn border or an outline is an object of its own, and states change the
/// values of the objects a box has in all of them. When they disagree on either,
/// the box keeps the plain border every combination can express.
fn outline_changes(styles: &[Box<ComputedStyle>]) -> bool {
    styles
        .iter()
        .any(|s| s.outline != styles[0].outline || s.outline_offset != styles[0].outline_offset)
}

fn edges_change_form(styles: &[Box<ComputedStyle>]) -> bool {
    styles
        .iter()
        .any(|s| super::decoration::needs_drawn_border(s))
        && styles
            .iter()
            .any(|s| s.border != styles[0].border || s.border_radius != styles[0].border_radius)
}

/// The sides to draw as strips in every combination, when the
/// combinations disagree on how the borders are drawn (one uniform
/// `border`, or strips on the visible sides).
fn border_strips(styles: &[Box<ComputedStyle>]) -> Option<[bool; 4]> {
    let form = |s: &ComputedStyle| {
        let b = &s.border;
        let sides = [&b.top, &b.right, &b.bottom, &b.left];
        let visible = sides.map(super::decoration::is_visible);
        let uniform = visible[0] && sides.iter().all(|side| **side == *sides[0]);
        (uniform, visible)
    };
    let first = form(&styles[0]);
    if styles.iter().all(|s| form(s) == first) {
        return None;
    }
    let mut union = [false; 4];
    for s in styles {
        for (u, v) in union.iter_mut().zip(form(s).1) {
            *u |= v;
        }
    }
    Some(union)
}

/// Whether some combination paints the box, or they differ in what makes
/// a wrapper (padding, border, shadow, radius).
fn is_sticky(styles: &[Box<ComputedStyle>]) -> bool {
    let paints = |s: &ComputedStyle| {
        !s.background_color.is_transparent()
            || [
                &s.border.top,
                &s.border.right,
                &s.border.bottom,
                &s.border.left,
            ]
            .iter()
            .any(|b| super::decoration::is_visible(b))
    };
    let first = &styles[0];
    styles.iter().any(|s| paints(s))
        && styles.iter().any(|s| {
            s.background_color != first.background_color
                || s.border != first.border
                || s.padding != first.padding
                || s.box_shadow != first.box_shadow
                || s.border_radius != first.border_radius
        })
        || styles.iter().any(|s| s.padding != first.padding)
}

/// Where each condition is read from.
#[derive(Default)]
struct Sources {
    /// `None`: the condition has no QML source here.
    exprs: BTreeMap<u16, Option<String>>,
    /// `cls_` properties to declare, by the path of the object.
    classes: BTreeMap<Path, (NodeId, BTreeSet<String>)>,
    /// Hover/press handlers to add: path, handler type, id.
    handlers: BTreeMap<Path, Vec<(&'static str, String)>>,
    /// What takes the focus by `tabindex`, with no control's policy to do it.
    focusable: BTreeSet<Path>,
    /// Some style follows the focus, so a click on nothing has to take it away.
    reads_focus: bool,
    /// Controls that must report hover.
    hover_enabled: BTreeSet<Path>,
    /// Elements that read their place: path, node, and whether the
    /// same-tag place is read too.
    positions: BTreeMap<Path, (NodeId, bool)>,
    /// A probe emission: bindings read [`token`]s, and `locals` has what
    /// each stands for inside its element's template.
    probe: bool,
    locals: BTreeMap<u16, String>,
}

impl Sources {
    fn expr(&self, c: u16) -> &str {
        self.exprs[&c].as_deref().expect("usable condition")
    }

    fn source(
        &mut self,
        dynamic: &mut Dynamic<'_, '_>,
        c: u16,
        markers: &HashMap<usize, Path>,
        root: &mut Object,
        ids: &mut Ids,
    ) -> Option<&str> {
        if !self.exprs.contains_key(&c) {
            let made = self.make(dynamic, c, markers, root, ids);
            self.exprs.insert(c, made);
        }
        self.exprs[&c].as_deref()
    }

    fn make(
        &mut self,
        dynamic: &mut Dynamic<'_, '_>,
        c: u16,
        markers: &HashMap<usize, Path>,
        root: &mut Object,
        ids: &mut Ids,
    ) -> Option<String> {
        let doc = dynamic.cx.doc;
        let cond = &dynamic.conds[usize::from(c)].cond;
        let node = node_of(cond);
        // `run` leaves out conditions without an item.
        let path = markers.get(&node.index())?;
        let obj = object_at(root, path)?;
        let id = match &obj.id {
            Some(id) => id.clone(),
            None => {
                let hint = doc
                    .element(node)
                    .and_then(|e| e.classes().next().map(str::to_owned))
                    .or_else(|| doc.element(node).map(|e| e.local_name().to_owned()))
                    .unwrap_or_else(|| "item".to_owned());
                let id = ids.claim(&hint);
                obj.id = Some(id.clone());
                id
            }
        };
        let control = classify(doc, node);
        let expr = match cond {
            Condition::Class { class, .. } => {
                self.classes
                    .entry(path.clone())
                    .or_insert_with(|| (node, BTreeSet::new()))
                    .1
                    .insert(class.clone());
                format!("{id}.{}", crate::metadata::class_prop(class))
            }
            Condition::State { state, .. } => match (state, control) {
                (PseudoState::Hover, Some(_)) => {
                    self.hover_enabled.insert(path.clone());
                    format!("{id}.hovered")
                }
                (PseudoState::Hover, None) => {
                    let handler = ids.claim(&format!("{id} hover"));
                    self.handlers
                        .entry(path.clone())
                        .or_default()
                        .push(("HoverHandler", handler.clone()));
                    format!("{handler}.hovered")
                }
                (PseudoState::Active, _) => {
                    // Passive: it does not take the press from a TapHandler
                    // or MouseArea a script listens on. It also stays when the
                    // mouse leaves still held, which a control's `pressed` does
                    // not; that says a held Space.
                    let handler = ids.claim(&format!("{id} press"));
                    self.handlers
                        .entry(path.clone())
                        .or_default()
                        .push(("PointHandler", handler.clone()));
                    if control.is_some() {
                        format!("{handler}.active || {id}.pressed")
                    } else {
                        format!("{handler}.active")
                    }
                }
                (PseudoState::FocusVisible, Some(Control::Button { .. })) => {
                    format!("{id}.visualFocus")
                }
                (PseudoState::Focus | PseudoState::FocusVisible, focusing) => {
                    self.reads_focus = true;
                    if focusing.is_none() {
                        self.focusable.insert(path.clone());
                    }
                    format!("{id}.activeFocus")
                }
                (PseudoState::Checked, _) => format!("{id}.checked"),
                (PseudoState::Disabled, _) => format!("!{id}.enabled"),
                (PseudoState::PlaceholderShown, _) => format!("{id}.text === \"\""),
                (PseudoState::Invalid, _) => broken_constraint(&id, doc.element(node)?, control)?,
                (PseudoState::Valid, _) => {
                    negate(&broken_constraint(&id, doc.element(node)?, control)?)
                }
                (PseudoState::FocusWithin, _) => {
                    self.reads_focus = true;
                    let mut holders = Vec::new();
                    for inner in focusable_within(doc, node) {
                        let Some(inner_path) = markers.get(&inner.index()) else {
                            continue;
                        };
                        let Some(held) = object_at(root, inner_path) else {
                            continue;
                        };
                        let inner_id = match &held.id {
                            Some(have) => have.clone(),
                            None => {
                                let made = ids.claim("within");
                                held.id = Some(made.clone());
                                made
                            }
                        };
                        holders.push(format!("{inner_id}.activeFocus"));
                    }
                    if holders.is_empty() {
                        return None;
                    }
                    holders.join(" || ")
                }
                (PseudoState::Visited, _) => return None,
            },
            Condition::Structural { test, .. } => {
                let of_type = &mut self
                    .positions
                    .entry(path.clone())
                    .or_insert((node, false))
                    .1;
                *of_type |= test.of_type();
                structural_expr(*test, &id)
            }
        };
        if self.probe {
            self.locals.insert(c, expr);
            return Some(token(c));
        }
        Some(expr)
    }

    /// Adds what the conditions read: `cls_` properties, handlers and
    /// `hoverEnabled`. Deepest paths first, since a declaration shifts the
    /// members of its object.
    fn apply(
        self,
        root: &mut Object,
        doc: &Document,
        _ids: &mut Ids,
        _imports: &mut BTreeSet<&'static str>,
    ) {
        let mut paths: BTreeSet<&Path> = self.classes.keys().collect();
        paths.extend(self.handlers.keys());
        paths.extend(self.hover_enabled.iter());
        paths.extend(self.positions.keys());
        paths.extend(self.focusable.iter());
        for path in paths.into_iter().rev() {
            let Some(obj) = object_at(root, path) else {
                continue;
            };
            for (ty, id) in self.handlers.get(path).into_iter().flatten() {
                obj.push_child(Object::new(*ty).with_id(id.clone()));
            }
            if self.hover_enabled.contains(path) {
                obj.set("hoverEnabled", true);
            }
            // What a `tabindex` made focusable already has its handler.
            if self.focusable.contains(path)
                && !obj
                    .members
                    .iter()
                    .any(|m| matches!(m, Member::Property { name, .. } if name == "activeFocusOnTab"))
            {
                obj.set("activeFocusOnTab", true);
                obj.push_child(takes_focus_on_tap());
            }
            if let Some((node, classes)) = self.classes.get(path) {
                crate::metadata::declare_classes(obj, doc, *node, classes);
            }
            if let Some(&(node, of_type)) = self.positions.get(path) {
                crate::metadata::declare_position(obj, doc, node, of_type);
            }
        }
        // A scroller takes the focus of a click on nothing by itself.
        if self.reads_focus && !self.probe && !has_scroll_input(root) {
            root.push_child(takes_focus_on_tap());
        }
    }
}

/// A page with no runtime has only Qt to take the focus: a stop in its chain, or
/// with a negative `tabindex` a press alone. A control has its own policy.
pub(super) fn takes_focus_by_tabindex(obj: &mut Object, doc: &Document, node: NodeId) {
    let Some(element) = doc.element(node) else {
        return;
    };
    let Some(index) = element.attr("tabindex").and_then(|t| t.trim().parse::<i32>().ok()) else {
        return;
    };
    if matches!(
        element.local_name(),
        "button" | "input" | "select" | "textarea" | "a" | "summary"
    ) {
        return;
    }
    if index >= 0 {
        obj.set("activeFocusOnTab", true);
    }
    obj.push_child(takes_focus_on_tap());
}

fn takes_focus_on_tap() -> Object {
    let mut tap = Object::new("TapHandler");
    tap.set("onTapped", Value::expr("parent.forceActiveFocus()"));
    tap
}

fn has_scroll_input(obj: &Object) -> bool {
    obj.type_name == "EldurScrollInput"
        || obj.members.iter().any(|m| match m {
            Member::Child(child) => has_scroll_input(child),
            _ => false,
        })
}

fn no_item(doc: &Document, cond: &Condition, diags: &mut Diagnostics) {
    let (node, what) = match cond {
        Condition::Class { node, class } => (*node, format!("class `{class}`")),
        Condition::State { node, state } => (*node, format!("`{}`", state_name(*state))),
        Condition::Structural { node, test } => (*node, format!("`{}`", structural_name(*test))),
    };
    diags.info(
        "css",
        node_line(doc, node),
        format!(
            "styles under {what} of {} are left out: it has no item of its own",
            describe(doc, node)
        ),
    );
}

fn state_name(state: PseudoState) -> &'static str {
    match state {
        PseudoState::Hover => ":hover",
        PseudoState::Active => ":active",
        PseudoState::Focus => ":focus",
        PseudoState::FocusVisible => ":focus-visible",
        PseudoState::FocusWithin => ":focus-within",
        PseudoState::Checked => ":checked",
        PseudoState::Disabled => ":disabled",
        PseudoState::Visited => ":visited",
        PseudoState::PlaceholderShown => ":placeholder-shown",
        PseudoState::Valid => ":valid",
        PseudoState::Invalid => ":invalid",
    }
}

/// `node` and what inside it can hold the focus: QML's `activeFocus` says so of
/// the one item only, so `:focus-within` asks each.
fn focusable_within(doc: &Document, node: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(at) = stack.pop() {
        if classify(doc, at).is_some() {
            out.push(at);
        }
        stack.extend(doc.element_children(at));
    }
    out.sort_by_key(|n| n.index());
    out
}

/// When `id`, an item for `element`, breaks a constraint of its attributes:
/// `None` if nothing there can. `minlength` is left out: it only counts once a
/// person has edited, which no property says.
fn broken_constraint(id: &str, element: &Element, control: Option<Control>) -> Option<String> {
    if element.has_attr("disabled") || element.has_attr("readonly") {
        return None;
    }
    let required = element.has_attr("required");
    match control? {
        Control::CheckBox | Control::Radio if required => Some(format!("!{id}.checked")),
        Control::Select if required => Some(format!("{id}.currentText === \"\"")),
        Control::Text | Control::TextArea => {
            let text = format!("{id}.text");
            let mut broken = Vec::new();
            if required {
                broken.push(format!("{text} === \"\""));
            }
            let test = match element.attr("type").map(str::to_ascii_lowercase).as_deref() {
                Some("email") => Some(EMAIL.to_owned()),
                Some("url") => Some(URL.to_owned()),
                _ => None,
            };
            if let Some(test) = test {
                broken.push(format!("{text} !== \"\" && !{test}.test({text})"));
            }
            if let Some(pattern) = element.attr("pattern") {
                broken.push(format!(
                    "{text} !== \"\" && !new RegExp({}).test({text})",
                    super::controls::js_string(&format!("^(?:{pattern})$"))
                ));
            }
            (!broken.is_empty()).then(|| broken.join(" || "))
        }
        _ => None,
    }
}

// WHATWG's valid e-mail address, and a scheme for a URL.
const EMAIL: &str = r"/^[a-zA-Z0-9.!#$%&'*+\/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$/";
const URL: &str = r"/^[a-zA-Z][a-zA-Z0-9+.-]*:\S*$/";
