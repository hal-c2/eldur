//! Templates for the elements scripts create (`docs/design/dynamic-styles.md`
//! §6): each predicted shape is appended to a copy of the page where the
//! script inserts it (a *probe*), the copy is cascaded and emitted with the
//! conditional bindings of [`super::dynamic`], and each probe's object
//! becomes a `Component` the runtime instantiates when the script inserts
//! such an element (`root.eldurTemplates`).
//!
//! A template is instantiated where the page is not: a binding on a
//! condition of another element reads it through `eldurUp<N>` (a created
//! ancestor, which exports its states as `eldur<State>`) or through the id of
//! a static ancestor. The runtime first instantiates every template with no
//! parent and no `eldurUp<N>`, so each such read has a default.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use eldur_css::{Condition, Position, PseudoState, Structural};
use eldur_dom::{Document, LocalName, NodeId, QualName, ns};
use eldur_js::{CreatedElement, DomQuery, ElementRef};
use eldur_qml::{Member, Object, Value};

use super::dynamic::{self, Inputs, Probe};
use super::{Target, lists};
use crate::boxes::{describe, is_containing_block, is_fixed_containing_block, is_out_of_flow};
use crate::diag::Diagnostics;
use crate::metadata::class_prop;
use crate::scripts::{Scripts, find_mut};

/// The text of a probe that gets no created children: an element with no
/// content can lose its item, while the script fills it in.
const PLACEHOLDER: &str = "eldurprobe";
/// Created elements inside created elements, at most this deep.
const MAX_DEPTH: usize = 4;
const MAX_PROBES: usize = 256;
/// Registry keys per created element, besides the `@*` ones (§6.5).
const MAX_KEYS: usize = 16;

/// What the templates add to the page besides its tree.
pub(crate) struct Added {
    /// Some template's `Behavior` waits for `root.eldurAnimate`.
    pub gated: bool,
    pub imports: BTreeSet<&'static str>,
}

/// Where a probe is.
#[derive(Debug, Clone)]
enum Context {
    /// Under the static element with this QML id.
    Static(String),
    /// Under `<body>`: the `@*` entries.
    Any,
    /// Under another probe.
    Probe(usize),
}

struct Inst {
    node: NodeId,
    /// Index into the analysis' `created`.
    created: usize,
    context: Context,
}

struct Builder<'c> {
    doc: Document,
    created: &'c [CreatedElement],
    /// The created elements inserted into each created element.
    children: Vec<Vec<usize>>,
    insts: Vec<Inst>,
    truncated: bool,
}

fn attr(local: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(local))
}

/// Elements whose text is not content.
fn holds_no_text(tag: &str) -> bool {
    matches!(
        tag,
        "input"
            | "textarea"
            | "select"
            | "img"
            | "br"
            | "hr"
            | "wbr"
            | "area"
            | "col"
            | "embed"
            | "source"
            | "track"
            | "video"
            | "audio"
            | "canvas"
            | "iframe"
    )
}

/// The classes every class set of `e` has.
fn common_classes(e: &CreatedElement) -> BTreeSet<String> {
    let mut sets = e.class_sets.iter();
    let Some(first) = sets.next() else {
        return BTreeSet::new();
    };
    let mut common: BTreeSet<String> = first.iter().cloned().collect();
    for set in sets {
        common.retain(|c| set.contains(c));
    }
    common
}

impl Builder<'_> {
    fn place(&mut self, e: usize, parent: NodeId, context: Context, depth: usize) {
        if depth > MAX_DEPTH || self.insts.len() >= MAX_PROBES {
            self.truncated = true;
            return;
        }
        let created = &self.created[e];
        let node = self.doc.create_html_element(&created.tag);
        if let Some(ty) = &created.input_type {
            self.doc
                .set_attribute(node, attr("type"), ty.clone())
                .expect("an element");
        }
        if created.tag == "a" {
            // Analysis does not record attributes besides `type` and
            // `class`; a link a script makes has an `href` (§6.8).
            self.doc
                .set_attribute(node, attr("href"), String::new())
                .expect("an element");
        }
        let common = common_classes(created);
        if !common.is_empty() {
            let value: Vec<&str> = common.iter().map(String::as_str).collect();
            self.doc
                .set_attribute(node, attr("class"), value.join(" "))
                .expect("an element");
        }
        if self.doc.append_child(parent, node).is_err() {
            return;
        }
        let me = self.insts.len();
        self.insts.push(Inst {
            node,
            created: e,
            context,
        });
        let kids = self.children[e].clone();
        for &k in &kids {
            self.place(k, node, Context::Probe(me), depth + 1);
        }
        if kids.is_empty() && !holds_no_text(&created.tag) {
            let text = self.doc.create_text(PLACEHOLDER);
            self.doc
                .append_child(node, text)
                .expect("text in an element");
        }
    }
}

/// A created cell joins a row, not the document's one grid, whose slots and
/// column widths its probe was given: it takes the size its own style says.
fn leave_table_grid(obj: &mut Object, style: Option<&eldur_css::ComputedStyle>) {
    let is_cell = obj
        .members
        .iter()
        .any(|m| matches!(m, Member::PropertyDecl { name, .. } if name == "tableSlot"));
    if !is_cell {
        return;
    }
    obj.members.retain(|m| match m {
        Member::PropertyDecl { name, .. } => name != "tableSlot" && name != "minContentWidth",
        Member::Property { name, .. } => !matches!(
            name.as_str(),
            "Layout.row" | "Layout.column" | "Layout.preferredWidth" | "Layout.preferredHeight"
        ),
        _ => true,
    });
    if let Some(eldur_css::Size::Length(width)) = style.map(|s| &s.width) {
        obj.set("Layout.preferredWidth", super::flow::size_value(width, 0.0));
    }
}

/// A template has no slot to read a static position or visibility from, and
/// says where the runtime is to put it instead of its DOM parent.
fn leave_flow(
    obj: &mut Object,
    doc: &Document,
    styles: &eldur_css::StyleMap,
    inst: &Inst,
    targets: &BTreeMap<NodeId, Target>,
    line: Option<u32>,
    diags: &mut Diagnostics,
) {
    let mut lost_static = false;
    let slot = obj.members.iter().find_map(|m| match m {
        Member::PropertyDecl {
            name,
            value: Some(Value::Expr(e)),
            ..
        } if name == lists::SLOT_PROPERTY => Some(e.clone()),
        _ => None,
    });
    let own_visible = obj.get("visible").and_then(|v| match v {
        Value::Expr(e) => e
            .split_once(") && (")
            .and_then(|(_, own)| own.strip_suffix(')'))
            .map(str::to_owned),
        _ => None,
    });
    obj.members.retain(|m| match m {
        Member::PropertyDecl { name, .. } if name == lists::SLOT_PROPERTY => false,
        Member::Property {
            name,
            value: Value::Expr(e),
        } if name == "x" || name == "y" => {
            let slot = e.contains(".staticX(") || e.contains(".staticY(");
            lost_static |= slot;
            !slot
        }
        Member::Property {
            name,
            value: Value::Expr(e),
        } if name == "visible" => !slot.as_ref().is_some_and(|s| e.contains(&format!("{s}."))),
        _ => true,
    });
    if let Some(own) = own_visible {
        obj.set("visible", Value::expr(own));
    }
    if lost_static {
        diags.warning(
            "css",
            line,
            format!(
                "a script's {} has no offsets on an axis: it sits at its containing block's start, not where it would have been in flow",
                describe(doc, inst.node)
            ),
        );
    }
    let fixed = styles
        .computed(inst.node)
        .is_some_and(|s| s.style.position == Position::Fixed);
    let block = std::iter::successors(doc.parent(inst.node), |&n| doc.parent(n)).find(|&n| {
        styles.computed(n).is_some_and(|s| {
            if fixed {
                is_fixed_containing_block(&s.style)
            } else {
                is_containing_block(&s.style)
            }
        })
    });
    let value = match block {
        None if fixed => Value::string("window"),
        None => Value::string("initial"),
        Some(b) if doc.parent(inst.node) == Some(b) => Value::string("parent"),
        Some(b) => match targets.get(&b) {
            Some(t) => Value::expr(t.qml_id.clone()),
            None => {
                diags.warning(
                    "css",
                    line,
                    format!(
                        "a script's {} is placed against the page: nothing identifies its containing block, {}, at run time",
                        describe(doc, inst.node),
                        describe(doc, b)
                    ),
                );
                Value::string("initial")
            }
        },
    };
    obj.members
        .insert(0, decl("var", "eldurBlock", value, true));
    // The runtime makes the slot when the DOM first takes the box.
    obj.members.insert(
        0,
        Member::PropertyDecl {
            ty: "Item".to_owned(),
            name: lists::SLOT_PROPERTY.to_owned(),
            value: None,
            readonly: false,
            required: false,
        },
    );
}

/// Every id in `obj`.
fn collect_ids(obj: &Object, out: &mut BTreeSet<String>) {
    if let Some(id) = &obj.id {
        out.insert(id.clone());
    }
    for m in &obj.members {
        match m {
            Member::Child(c) => collect_ids(c, out),
            Member::Property { value, .. } => collect_value_ids(value, out),
            Member::Behavior { animation, .. } => collect_ids(animation, out),
            _ => {}
        }
    }
}

fn collect_value_ids(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(o) => collect_ids(o, out),
        Value::List(items) => items.iter().for_each(|v| collect_value_ids(v, out)),
        _ => {}
    }
}

/// Applies `f` to every expression in `obj`.
fn each_expr(obj: &mut Object, f: &mut dyn FnMut(&mut String)) {
    for m in &mut obj.members {
        match m {
            Member::Child(c) => each_expr(c, f),
            Member::Property { value, .. }
            | Member::PropertyDecl {
                value: Some(value), ..
            } => each_value_expr(value, f),
            Member::Behavior { animation, .. } => each_expr(animation, f),
            _ => {}
        }
    }
}

fn each_value_expr(value: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match value {
        Value::Expr(e) => f(e),
        Value::Object(o) => each_expr(o, f),
        Value::List(items) => items.iter_mut().for_each(|v| each_value_expr(v, f)),
        _ => {}
    }
}

/// Removes the objects of nested probes and the source comment before each.
fn prune(obj: &mut Object, probes: &HashSet<usize>) {
    let members = std::mem::take(&mut obj.members);
    for m in members {
        if let Member::Child(c) = &m
            && dynamic::marked_node(c).is_some_and(|n| probes.contains(&n))
        {
            if matches!(obj.members.last(), Some(Member::Comment(c)) if !c.starts_with('\u{1}')) {
                obj.members.pop();
            }
            continue;
        }
        obj.members.push(m);
    }
    for m in &mut obj.members {
        if let Member::Child(c) = m {
            prune(c, probes);
        }
    }
}

/// Empties the probe's placeholder text.
fn blank(obj: &mut Object) {
    for m in &mut obj.members {
        match m {
            Member::Property {
                name,
                value: Value::String(s),
            } if name == "text" && s.eq_ignore_ascii_case(PLACEHOLDER) => s.clear(),
            Member::Child(c) => blank(c),
            Member::Property {
                value: Value::Object(o),
                ..
            } => blank(o),
            _ => {}
        }
    }
}

fn child_at<'o>(root: &'o Object, path: &[usize]) -> Option<&'o Object> {
    let mut at = root;
    for &i in path {
        at = match at.members.get(i) {
            Some(Member::Child(c)) => c,
            _ => return None,
        };
    }
    Some(at)
}

fn decl(ty: &str, name: &str, value: Value, readonly: bool) -> Member {
    Member::PropertyDecl {
        ty: ty.to_owned(),
        name: name.to_owned(),
        value: Some(value),
        readonly,
        required: false,
    }
}

/// The property a created ancestor exports a structural test as (§8.1):
/// `eldurIsFirstChild`, `eldurIsNthChild2n1`, `eldurIsNthLastOfTypem1n3`.
fn structural_export(test: Structural) -> String {
    let num = |v: i32| {
        if v < 0 {
            format!("m{}", -i64::from(v))
        } else {
            v.to_string()
        }
    };
    let nth = |kind: &str, a: i32, b: i32| format!("eldurIs{kind}{}n{}", num(a), num(b));
    match test {
        Structural::FirstChild => "eldurIsFirstChild".to_owned(),
        Structural::LastChild => "eldurIsLastChild".to_owned(),
        Structural::OnlyChild => "eldurIsOnlyChild".to_owned(),
        Structural::FirstOfType => "eldurIsFirstOfType".to_owned(),
        Structural::LastOfType => "eldurIsLastOfType".to_owned(),
        Structural::OnlyOfType => "eldurIsOnlyOfType".to_owned(),
        Structural::NthChild { a, b } => nth("NthChild", a, b),
        Structural::NthLastChild { a, b } => nth("NthLastChild", a, b),
        Structural::NthOfType { a, b } => nth("NthOfType", a, b),
        Structural::NthLastOfType { a, b } => nth("NthLastOfType", a, b),
    }
}

/// The property a created ancestor exports `state` as (§8.1).
fn export_name(state: PseudoState) -> Option<&'static str> {
    Some(match state {
        PseudoState::Hover => "eldurHovered",
        PseudoState::Active => "eldurPressed",
        PseudoState::Focus => "eldurFocused",
        PseudoState::FocusVisible => "eldurFocusVisible",
        PseudoState::FocusWithin => "eldurFocusWithin",
        PseudoState::Checked => "eldurChecked",
        PseudoState::Disabled => "eldurDisabled",
        PseudoState::Visited => return None,
    })
}

/// `obj` with ids renamed in order of appearance and its template name
/// left out, for finding templates that are the same.
fn canonical(obj: &Object) -> Object {
    let mut obj = obj.clone();
    obj.members
        .retain(|m| !matches!(m, Member::PropertyDecl { name, .. } if name == "eldurTemplate"));
    let mut order = Vec::new();
    ids_in_order(&obj, &mut order);
    let names: HashMap<String, String> = order
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, format!("eldurId{i}")))
        .collect();
    rename_ids(&mut obj, &names);
    each_expr(&mut obj, &mut |e| *e = rename_in_expr(e, &names));
    obj
}

fn ids_in_order(obj: &Object, out: &mut Vec<String>) {
    if let Some(id) = &obj.id
        && !out.contains(id)
    {
        out.push(id.clone());
    }
    for m in &obj.members {
        match m {
            Member::Child(c) => ids_in_order(c, out),
            Member::Property {
                value: Value::Object(o),
                ..
            } => ids_in_order(o, out),
            Member::Behavior { animation, .. } => ids_in_order(animation, out),
            _ => {}
        }
    }
}

fn rename_ids(obj: &mut Object, names: &HashMap<String, String>) {
    if let Some(id) = &obj.id
        && let Some(to) = names.get(id)
    {
        obj.id = Some(to.clone());
    }
    for m in &mut obj.members {
        match m {
            Member::Child(c) => rename_ids(c, names),
            Member::Property {
                value: Value::Object(o),
                ..
            } => rename_ids(o, names),
            Member::Behavior { animation, .. } => rename_ids(animation, names),
            _ => {}
        }
    }
}

/// `expr` with each identifier in `names` renamed; not after a `.`, where
/// it is a member.
fn rename_in_expr(expr: &str, names: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut word = String::new();
    let mut prev = '\0';
    let flush = |word: &mut String, out: &mut String, prev: char| {
        if word.is_empty() {
            return;
        }
        match names.get(word.as_str()).filter(|_| prev != '.') {
            Some(to) => out.push_str(to),
            None => out.push_str(word),
        }
        word.clear();
    };
    let mut before = '\0';
    for c in expr.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            if word.is_empty() {
                before = prev;
            }
            word.push(c);
        } else {
            flush(&mut word, &mut out, before);
            out.push(c);
        }
        prev = c;
    }
    flush(&mut word, &mut out, before);
    out
}

/// A `Component` id fragment for `tag`.
fn id_part(tag: &str) -> String {
    tag.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// `tag[type=…]`, as the runtime's keys start.
fn tag_part(e: &CreatedElement) -> String {
    match (&*e.tag, &e.input_type) {
        ("input", Some(ty)) => format!("input[type={}]", ty.to_ascii_lowercase()),
        _ => e.tag.clone(),
    }
}

/// Adds the templates of the elements `scripts` create to `root`: their
/// `Component`s, `root.eldurTemplates`, and `eldurKey` and `cls_` properties on
/// the static elements they read. `None` when there are none.
/// The tags the runtime keeps as nodes in a line (`_vStaysInline`).
pub(crate) fn stays_inline(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "ins"
            | "kbd"
            | "label"
            | "mark"
            | "q"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strike"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "wbr"
    )
}

pub(crate) fn add(
    cx: &Inputs<'_>,
    scripts: &Scripts,
    root: &mut Object,
    targets: &BTreeMap<NodeId, Target>,
    inline_rules: bool,
    diags: &mut Diagnostics,
) -> Option<Added> {
    // Such an element becomes a node of the line, styled by the page's rules.
    let styled_as_node = |tag: &str| inline_rules && stays_inline(tag);
    let created = &scripts.analysis.created;
    if created.is_empty() || !cx.options.state_variants {
        return None;
    }
    let styled = eldur_css::styled_classes(cx.sheets);
    let position: HashMap<_, usize> = created.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let mut children = vec![Vec::new(); created.len()];
    for (i, e) in created.iter().enumerate() {
        for parent in &e.parents {
            if let ElementRef::Created(p) = parent
                && let Some(&p) = position.get(p)
                && !children[p].contains(&i)
            {
                children[p].push(i);
            }
        }
    }
    let mut builder = Builder {
        doc: cx.doc.clone(),
        created,
        children,
        insts: Vec::new(),
        truncated: false,
    };
    let body = cx.doc.body()?;
    for (i, e) in created.iter().enumerate() {
        for parent in &e.parents {
            match parent {
                ElementRef::Query(query) => {
                    for node in scripts.matches(query) {
                        match targets.get(node) {
                            Some(t) => {
                                builder.place(i, *node, Context::Static(t.qml_id.clone()), 0);
                            }
                            None if styled_as_node(&e.tag) => {}
                            None => diags.info(
                                "css",
                                e.line,
                                format!(
                                    "a created <{}> goes into {}, which has no item of its own; it is styled as if in <body> (dynamic-context)",
                                    e.tag,
                                    describe(cx.doc, *node)
                                ),
                            ),
                        }
                    }
                }
                ElementRef::Unknown => diags.info(
                    "css",
                    e.line,
                    format!(
                        "a created <{}> goes where the script decides at run time; it is styled as if in <body> (dynamic-context)",
                        e.tag
                    ),
                ),
                ElementRef::Created(_) => {}
            }
        }
        // Made in a helper and handed to an insert analysis cannot pair it
        // with: it may go in any box a script gives such a node.
        if e.parents
            .iter()
            .all(|parent| matches!(parent, ElementRef::Unknown))
        {
            for node in scripts.gains.nodes.iter().copied().collect::<BTreeSet<_>>() {
                if let Some(t) = targets.get(&node) {
                    builder.place(i, node, Context::Static(t.qml_id.clone()), 0);
                }
            }
        }
    }
    for i in 0..created.len() {
        builder.place(i, body, Context::Any, 0);
    }
    if builder.truncated {
        diags.warning(
            "css",
            None,
            format!(
                "created elements nest deeper than {MAX_DEPTH} or make more than {MAX_PROBES} templates; the rest use generic items (dynamic-templates)"
            ),
        );
    }
    let Builder { doc, insts, .. } = builder;

    // The probes' class conditions: the classes some but not all of its
    // sets have, and those scripts toggle on it.
    let mut classes: BTreeMap<NodeId, BTreeMap<String, bool>> = BTreeMap::new();
    let mut candidates: HashMap<String, Vec<NodeId>> = HashMap::new();
    for inst in &insts {
        let e = &created[inst.created];
        let common = common_classes(e);
        let mut names: BTreeSet<String> = e
            .class_sets
            .iter()
            .flatten()
            .filter(|c| !common.contains(*c))
            .cloned()
            .collect();
        for write in &scripts.analysis.class_writes {
            // A query or an unattributed target can reach a created element
            // once it is in the page (`list.querySelector('.label')
            // .classList.toggle(…)`); for the latter, the style sheet says
            // which elements the class could style (§2.2).
            let found = match &write.target {
                ElementRef::Query(DomQuery::Selector(sel) | DomQuery::SelectorAll(sel)) => {
                    eldur_css::element_matches(&doc, inst.node, sel) == Some(true)
                }
                ElementRef::Unknown => write.names.iter().any(|c| {
                    candidates
                        .entry(c.clone())
                        .or_insert_with(|| eldur_css::class_candidates(&doc, cx.sheets, c))
                        .contains(&inst.node)
                }),
                _ => false,
            };
            if found {
                names.extend(write.names.iter().cloned());
            } else if write.target == ElementRef::Created(e.id) {
                // An assignment is where the class sets come from; the
                // classes all of them have stay.
                names.extend(
                    write
                        .names
                        .iter()
                        .filter(|c| !write.assigns || !common.contains(*c))
                        .cloned(),
                );
            }
        }
        let conds: BTreeMap<String, bool> = names
            .into_iter()
            .filter(|c| styled.contains(c))
            .map(|c| {
                let initial = common.contains(&c);
                (c, initial)
            })
            .collect();
        if !conds.is_empty() {
            classes.insert(inst.node, conds);
        }
    }
    let probe_nodes: HashSet<NodeId> = insts.iter().map(|i| i.node).collect();
    let mut relevant = probe_nodes.clone();
    for inst in &insts {
        relevant.extend(doc.ancestors(inst.node));
    }
    let mut reserved = BTreeSet::new();
    collect_ids(root, &mut reserved);
    // Under a static element the page's own object is what the script
    // appends to, and a static line has no place for an item (§6.8).
    let inline_items = insts
        .iter()
        .filter(|inst| !matches!(inst.context, Context::Static(_)))
        .map(|inst| inst.node)
        .collect();
    let probe = Probe {
        classes,
        reserved,
        relevant,
        nodes: probe_nodes.clone(),
        inline_items,
    };
    let styles = eldur_css::compute_styles(&doc, cx.sheets, &cx.options.viewport);
    let mut wanted = cx.wanted.clone();
    wanted.extend(probe_nodes.iter().copied());
    // The probe document repeats the page's diagnostics.
    let mut scratch = Diagnostics::new(&diags.document);
    let emission = dynamic::emit(
        &Inputs {
            doc: &doc,
            styles: &styles,
            sheets: cx.sheets,
            options: cx.options,
            wanted: &wanted,
            images: cx.images,
            runtime_dom: cx.runtime_dom,
            scripts: cx.scripts,
            probe: Some(&probe),
        },
        &mut scratch,
    );
    let out = &emission.probe;
    let paths = dynamic::element_paths(&emission.root);
    let probe_indices: HashSet<usize> = probe_nodes.iter().map(|n| n.index()).collect();
    let inst_of: HashMap<NodeId, usize> = insts
        .iter()
        .enumerate()
        .map(|(i, inst)| (inst.node, i))
        .collect();

    // Each probe's template, before names.
    let mut objects: Vec<Option<Object>> = Vec::with_capacity(insts.len());
    let mut ups: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); insts.len()];
    let mut exports: Vec<BTreeMap<String, String>> = vec![BTreeMap::new(); insts.len()];
    let mut static_classes: BTreeMap<String, (NodeId, BTreeSet<String>)> = BTreeMap::new();
    let mut static_positions: BTreeMap<String, (NodeId, bool)> = BTreeMap::new();
    let mut unreachable: BTreeSet<(usize, u16)> = BTreeSet::new();
    let mut reported: HashSet<usize> = HashSet::new();
    let mut has_out_of_flow = false;
    for (i, inst) in insts.iter().enumerate() {
        let e = &created[inst.created];
        let Some(obj) = paths
            .get(&inst.node.index())
            .and_then(|p| child_at(&emission.root, p))
        else {
            let place = match &inst.context {
                Context::Any => "in <body>",
                Context::Static(_) | Context::Probe(_) => "where the script puts it",
            };
            if !styled_as_node(&e.tag) {
                diags.info(
                "css",
                e.line,
                format!(
                    "a created <{}> has no item of its own {place}, so it has no template there (dynamic-templates)",
                    e.tag
                ),
            );
            }
            objects.push(None);
            continue;
        };
        if out.reshaped.contains(&inst.node) && reported.insert(inst.created) {
            diags.warning(
                "css",
                e.line,
                format!(
                    "a created <{}> changes its form under a class or state (dynamic-form); its template keeps one form",
                    e.tag
                ),
            );
        }
        let mut obj = obj.clone();
        let nested: HashSet<usize> = probe_indices
            .iter()
            .copied()
            .filter(|&n| n != inst.node.index())
            .collect();
        prune(&mut obj, &nested);
        blank(&mut obj);
        let mut expr_of = |c: u16| -> String {
            let (cond, initial) = &out.conditions[usize::from(c)];
            let local = out.locals.get(&c).cloned().unwrap_or_default();
            let node = match cond {
                Condition::Class { node, .. }
                | Condition::State { node, .. }
                | Condition::Structural { node, .. } => *node,
            };
            let distance = std::iter::successors(Some(inst.node), |&n| doc.parent_element(n))
                .position(|n| n == node);
            match (distance, cond) {
                (Some(0), _) => return local,
                (Some(d), _) if inst_of.contains_key(&node) => {
                    let name = match cond {
                        Condition::Class { class, .. } => Some(class_prop(class)),
                        Condition::State { state, .. } => export_name(*state).map(|name| {
                            exports[inst_of[&node]].insert(name.to_owned(), local.clone());
                            name.to_owned()
                        }),
                        Condition::Structural { test, .. } => {
                            let name = structural_export(*test);
                            exports[inst_of[&node]].insert(name.clone(), local.clone());
                            Some(name)
                        }
                    };
                    if let Some(name) = name {
                        ups[i].insert(d);
                        return format!("(eldurUp{d} ? eldurUp{d}.{name} : false)");
                    }
                }
                (Some(_), Condition::Class { class, .. }) => {
                    if let Some(t) = targets.get(&node) {
                        static_classes
                            .entry(t.qml_id.clone())
                            .or_insert_with(|| (node, BTreeSet::new()))
                            .1
                            .insert(class.clone());
                        return format!("{}.{}", t.qml_id, class_prop(class));
                    }
                }
                (Some(_), Condition::Structural { test, .. }) => {
                    if let Some(t) = targets.get(&node) {
                        let of_type = &mut static_positions
                            .entry(t.qml_id.clone())
                            .or_insert((node, false))
                            .1;
                        *of_type |= test.of_type();
                        return format!("({})", dynamic::structural_expr(*test, &t.qml_id));
                    }
                }
                _ => {}
            }
            unreachable.insert((inst.created, c));
            initial.to_string()
        };
        each_expr(&mut obj, &mut |e| {
            *e = dynamic::replace_tokens(e, &mut expr_of);
        });
        dynamic::remove_markers(&mut obj);
        leave_table_grid(&mut obj, styles.computed(inst.node).map(|s| &s.style));
        if styles
            .computed(inst.node)
            .is_some_and(|s| is_out_of_flow(&s.style))
        {
            leave_flow(&mut obj, &doc, &styles, inst, targets, e.line, diags);
            has_out_of_flow = true;
        }
        objects.push(Some(obj));
    }
    for (e, c) in unreachable {
        let (cond, initial) = &out.conditions[usize::from(c)];
        let what = match cond {
            Condition::Class { node, class } => {
                format!("class `{class}` of {}", describe(&doc, *node))
            }
            Condition::State { node, .. } => format!("a state of {}", describe(&doc, *node)),
            Condition::Structural { node, test } => format!(
                "`{}` of {}",
                dynamic::structural_name(*test),
                describe(&doc, *node)
            ),
        };
        diags.info(
            "css",
            created[e].line,
            format!(
                "styles of a created <{}> under {what} are left out: a template cannot read it; it is taken as {}",
                created[e].tag,
                if *initial { "set" } else { "unset" }
            ),
        );
    }

    // Metadata, then one Component for each distinct template.
    for (i, obj) in objects.iter_mut().enumerate() {
        let Some(obj) = obj else {
            continue;
        };
        let mut decls = vec![decl("string", "eldurTemplate", Value::string(""), false)];
        for d in &ups[i] {
            decls.push(decl(
                "Item",
                &format!("eldurUp{d}"),
                Value::expr("null"),
                false,
            ));
        }
        let classes_at = obj
            .members
            .iter()
            .position(|m| matches!(m, Member::PropertyDecl { name, .. } if name == "eldurClasses"));
        let at = match classes_at {
            Some(at) => {
                obj.members[at] = decl("string", "eldurClasses", Value::string(""), false);
                at + 1
            }
            None => {
                let tag = obj
                    .members
                    .iter()
                    .position(
                        |m| matches!(m, Member::PropertyDecl { name, .. } if name == "eldurTag"),
                    )
                    .map_or(0, |t| t + 1);
                decls.insert(0, decl("string", "eldurClasses", Value::string(""), false));
                tag
            }
        };
        obj.members.splice(at..at, decls);
        let end = obj
            .members
            .iter()
            .take_while(|m| matches!(m, Member::PropertyDecl { .. }))
            .count();
        let exported: Vec<Member> = exports[i]
            .iter()
            .map(|(name, expr)| decl("bool", name, Value::expr(expr.clone()), true))
            .collect();
        obj.members.splice(end..end, exported);
    }
    let mut distinct: Vec<(Object, usize)> = Vec::new();
    let mut template_of: Vec<Option<usize>> = Vec::with_capacity(insts.len());
    for obj in &objects {
        let Some(obj) = obj else {
            template_of.push(None);
            continue;
        };
        let canon = canonical(obj);
        let k = match distinct.iter().position(|(c, _)| *c == canon) {
            Some(k) => k,
            None => {
                distinct.push((canon, template_of.len()));
                distinct.len() - 1
            }
        };
        template_of.push(Some(k));
    }
    let names: Vec<String> = distinct
        .iter()
        .enumerate()
        .map(|(k, (_, i))| format!("eldurT_{}_{k}", id_part(&created[insts[*i].created].tag)))
        .collect();

    // The registry, exact contexts first since the first entry of a key wins.
    let mut keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<usize, usize> = HashMap::new();
    let mut capped: BTreeSet<usize> = BTreeSet::new();
    let mut shadows: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    let mut keyed: Vec<usize> = (0..insts.len()).collect();
    keyed.sort_by_key(|&i| matches!(insts[i].context, Context::Any));
    for i in keyed {
        let Some(k) = template_of[i] else {
            continue;
        };
        let inst = &insts[i];
        let e = &created[inst.created];
        let contexts: Vec<String> = match &inst.context {
            Context::Static(id) => vec![id.clone()],
            Context::Any => vec!["*".to_owned()],
            Context::Probe(p) => match template_of[*p] {
                // The runtime finds the parent under the template that won
                // its key, which may be another probe's.
                Some(pk) => std::iter::once(pk)
                    .chain(shadows.get(&pk).into_iter().flatten().copied())
                    .map(|t| names[t].clone())
                    .collect(),
                None => continue,
            },
        };
        let mut sets: Vec<Vec<String>> = if e.class_sets.is_empty() {
            vec![Vec::new()]
        } else {
            e.class_sets.clone()
        };
        // The template follows the classes scripts toggle on it, so the
        // sets a toggle makes are keys of the same template (§6.4 step 3).
        if let Some(conds) = probe.classes.get(&inst.node) {
            let toggled: Vec<&String> = conds.keys().collect();
            let bases = sets.clone();
            for mask in 1..(1usize << toggled.len().min(4)) {
                for base in &bases {
                    let mut set = base.clone();
                    for (b, c) in toggled.iter().enumerate() {
                        if mask & (1 << b) != 0 && !set.contains(c) {
                            set.push((*c).clone());
                        }
                    }
                    if !sets.contains(&set) {
                        sets.push(set);
                    }
                }
            }
        }
        for (context, set) in contexts
            .iter()
            .flat_map(|c| sets.iter().map(move |s| (c, s)))
        {
            let mut set: Vec<&str> = set
                .iter()
                .map(String::as_str)
                .filter(|c| styled.contains(*c))
                .collect();
            set.sort_unstable();
            set.dedup();
            let mut key = tag_part(e);
            for c in set {
                key.push('.');
                key.push_str(c);
            }
            key.push('@');
            key.push_str(context);
            if let Some(&won) = keys.get(&key) {
                if won != k {
                    shadows.entry(k).or_default().insert(won);
                }
                continue;
            }
            if context != "*" {
                let count = counts.entry(inst.created).or_default();
                if *count >= MAX_KEYS {
                    capped.insert(inst.created);
                    continue;
                }
                *count += 1;
            }
            keys.insert(key.clone(), k);
            order.push(key);
        }
    }
    for e in capped {
        diags.warning(
            "css",
            created[e].line,
            format!(
                "a created <{}> has more than {MAX_KEYS} class sets and places; the rest use the `@*` templates (dynamic-templates)",
                created[e].tag
            ),
        );
    }
    if keys.is_empty() {
        return None;
    }

    for (qml_id, (node, classes)) in &static_classes {
        if let Some(obj) = find_mut(root, qml_id) {
            crate::metadata::declare_classes(obj, cx.doc, *node, classes);
        }
    }
    for (qml_id, &(node, of_type)) in &static_positions {
        if let Some(obj) = find_mut(root, qml_id) {
            crate::metadata::declare_position(obj, cx.doc, node, of_type);
        }
    }
    let contexts: BTreeSet<&str> = insts
        .iter()
        .filter_map(|inst| match &inst.context {
            Context::Static(id) => Some(id.as_str()),
            _ => None,
        })
        .collect();
    for id in contexts {
        if let Some(obj) = find_mut(root, id) {
            let at = obj
                .members
                .iter()
                .take_while(
                    |m| matches!(m, Member::PropertyDecl { name, .. } if name.starts_with("eldur")),
                )
                .count();
            obj.members
                .insert(at, decl("string", "eldurKey", Value::string(id), false));
        }
    }
    let entries: Vec<String> = order
        .iter()
        .map(|key| format!("{}: {}", eldur_qml::quote_string(key), names[keys[key]]))
        .collect();
    if has_out_of_flow {
        super::declare_initial_block(root);
    }
    root.members.insert(
        0,
        decl(
            "var",
            "eldurTemplates",
            Value::expr(format!("({{ {} }})", entries.join(", "))),
            true,
        ),
    );
    for (k, (_, i)) in distinct.iter().enumerate() {
        let inst = &insts[*i];
        let mut obj = objects[*i]
            .clone()
            .expect("a distinct template has an object");
        for m in &mut obj.members {
            if let Member::PropertyDecl { name, value, .. } = m
                && name == "eldurTemplate"
            {
                *value = Some(Value::string(names[k].clone()));
            }
        }
        if cx.options.source_comments {
            let place = match &inst.context {
                Context::Static(id) => format!(", in {id}"),
                Context::Any => String::new(),
                Context::Probe(p) => template_of[*p]
                    .map(|pk| format!(", in {}", names[pk]))
                    .unwrap_or_default(),
            };
            root.members.push(Member::Comment(format!(
                "{} created by a script{place}",
                describe(&doc, inst.node)
            )));
        }
        root.members.push(Member::Child(
            Object::new("Component")
                .with_id(names[k].clone())
                .child(obj),
        ));
    }
    Some(Added {
        gated: emission.gated,
        imports: emission.imports.clone(),
    })
}
