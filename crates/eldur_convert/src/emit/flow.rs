//! Normal-flow block layout → `ColumnLayout` (layout.md §1).
//!
//! Vertical margins are collapsed here, at conversion time (CSS 2.2
//! §8.3.1): the collapsed gap goes on the following item's
//! `Layout.topMargin`, margins that escape through a parent's top or
//! bottom edge are handed up to the parent's own margins, and empty boxes
//! that collapse through are dropped.

use eldur_css::{
    BoxSizing, Clear, ComputedStyle, Display, Float, Length, LengthAuto, Size, TextAlign,
    TextDecorationLine, Visibility,
};
use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{fmt_number, length_expr, number, px_part, round};
use crate::boxes::{LayoutBox, describe, is_out_of_flow, node_line};

/// A set of adjoining margins: the largest positive and the most negative
/// one; the collapsed margin is their sum.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MarginSet {
    pos: f32,
    neg: f32,
}

/// Boxes in a row in a column that a script can show or hide one at a time:
/// hidden ones it can show, and shown ones it hides by their display.
#[derive(Default)]
struct HiddenRun {
    before: MarginSet,
    /// What `before` is in each case, when it depends on which of the last
    /// blocks before the run show (`Emitted::bottom_cases`).
    before_cases: Option<Escape>,
    /// What the layout gives anyway: an item holding only hidden boxes stays
    /// an item, keeping its top margin.
    given: f32,
    boxes: Vec<(String, MarginSet)>,
    /// What a box's bottom margin is in each case, for boxes whose last
    /// blocks may hide too (an open `<details>` folded in another).
    box_cases: Vec<(String, Escape)>,
    /// The run starts the column and the margins at its top escape through
    /// the parent's: while every box before an item is hidden, the item's
    /// own margin escapes too and the item has none.
    escaping: bool,
    /// What escapes while each box of an escaping run is the first one
    /// shown, and, for the case that holds as loaded, what escapes then.
    escapes: Vec<(String, MarginSet, Option<MarginSet>)>,
    /// What follows the run as the page loads, when a box in it is shown:
    /// the bottom margin of the last one that is.
    shown_bottom: Option<MarginSet>,
    /// The item holding only hidden boxes the run started at, and when its
    /// content shows: it keeps `given` as its own top margin.
    holder: Option<(usize, String)>,
}

/// An escaping run that ends with the column, or at a float, with all of it
/// hidden lets through only what came before it.
fn settled(run: &mut HiddenRun) -> (Option<MarginSet>, Option<Escape>) {
    let before = run.before;
    let (loaded, cases) = run.settle(None, before, before);
    (Some(loaded), cases)
}

/// The cases of what escapes through a block, met by the margins `before` it,
/// each also on the condition `shown` the block is; and what escapes when
/// none holds. Only the first `room` cases are kept, so that nesting does not
/// multiply them past `HiddenRun::NEAREST`: past those, what escapes is as if
/// all were hidden.
fn nested(
    shown: Option<&str>,
    cases: &Escape,
    before: MarginSet,
    room: usize,
) -> (Vec<(String, MarginSet)>, MarginSet) {
    let cases = cases.clone().joined(before);
    let each = cases
        .cases
        .into_iter()
        .take(room)
        .map(|(c, m)| match shown {
            Some(shown) => (format!("{shown} && ({c})"), m),
            None => (c, m),
        })
        .collect();
    (each, cases.otherwise)
}

impl HiddenRun {
    /// A row of hundreds of hidden panels must not write hundreds of
    /// conditions on each of them: the nearest few are what a page shows.
    const NEAREST: usize = 8;

    fn push(&mut self, id: String, bottom: MarginSet) {
        if self.boxes.len() == Self::NEAREST {
            let (gone, _) = self.boxes.remove(0);
            self.box_cases.retain(|(b, _)| *b != gone);
        }
        self.boxes.push((id, bottom));
    }

    /// Margins that come after every box of the run so far.
    fn join_after(&mut self, m: MarginSet) {
        self.before.join(m);
        self.before_cases = self.before_cases.take().map(|c| c.joined(m));
        for (_, bottom) in &mut self.boxes {
            bottom.join(m);
        }
        for (_, cases) in &mut self.box_cases {
            *cases = cases.clone().joined(m);
        }
        if let Some(bottom) = &mut self.shown_bottom {
            bottom.join(m);
        }
    }

    /// A box of an escaping run, `shown` when `id` holds: `at` escapes
    /// while it is the first shown, or its own cases, when the margins of
    /// its first blocks depend on a script too.
    fn escape(
        &mut self,
        id: &str,
        cases: Option<&Escape>,
        before: MarginSet,
        at: MarginSet,
        loaded: bool,
    ) {
        let otherwise = match cases {
            Some(cases) => {
                // The box's own case takes one place too.
                let room = Self::NEAREST.saturating_sub(self.escapes.len() + 1);
                let (each, otherwise) = nested(Some(id), cases, before, room);
                self.escapes
                    .extend(each.into_iter().map(|(c, m)| (c, m, None)));
                otherwise
            }
            None => at,
        };
        self.escapes
            .push((id.to_owned(), otherwise, loaded.then_some(at)));
    }

    /// The escaping run is over at a block that stays: what escapes is the
    /// first shown box's top margin, or with all of them hidden that block's,
    /// `at` as loaded and `cases` when its own first blocks may hide.
    /// Returns what escapes as loaded and, when it depends on them, what it
    /// is in each case.
    fn settle(
        &mut self,
        cases: Option<&Escape>,
        before: MarginSet,
        at: MarginSet,
    ) -> (MarginSet, Option<Escape>) {
        self.escaping = false;
        let mut escapes: Vec<_> = std::mem::take(&mut self.escapes);
        let loaded = escapes
            .iter()
            .find_map(|(_, _, loaded)| *loaded)
            .unwrap_or(at);
        let otherwise = match cases {
            Some(cases) => {
                let room = Self::NEAREST.saturating_sub(escapes.len());
                let (each, otherwise) = nested(None, cases, before, room);
                escapes.extend(each.into_iter().map(|(c, m)| (c, m, None)));
                otherwise
            }
            None => at,
        };
        let escape = Escape {
            cases: escapes.into_iter().map(|(id, m, _)| (id, m)).collect(),
            otherwise,
        };
        (loaded, escape.varies().then_some(escape))
    }
}

/// Margins escaping through a parent's top edge that depend on which of its
/// first blocks a script shows: those of the first case whose condition
/// holds, else `otherwise`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Escape {
    cases: Vec<(String, MarginSet)>,
    otherwise: MarginSet,
}

impl Escape {
    /// Collapsed with the margins `with` they meet on their way up.
    pub(crate) fn joined(mut self, with: MarginSet) -> Self {
        for (_, m) in &mut self.cases {
            m.join(with);
        }
        self.otherwise.join(with);
        self
    }

    /// Margins that adjoin these: in each case of both, the two sets
    /// collapse together. The pairs come first, so that a case of one
    /// alone holds only while none of the other's does.
    pub(crate) fn meet(&self, other: &Escape) -> Escape {
        let both = |a: MarginSet, b: MarginSet| {
            let mut m = a;
            m.join(b);
            m
        };
        // As `nested` does: past the nearest few boxes, as if hidden. The
        // farthest box goes from whichever side keeps more, never a case
        // from the middle of the list, and a box goes with the cases of its
        // own last blocks, so every kept box keeps all of its cases and its
        // own.
        let (cuts_a, cuts_b) = (self.whole_cuts(), other.whole_cuts());
        let (mut ia, mut ib) = (cuts_a.len() - 1, cuts_b.len() - 1);
        while cuts_a[ia] * (cuts_b[ib] + 1) + cuts_b[ib] > HiddenRun::NEAREST {
            if ia >= ib && ia > 0 {
                ia -= 1;
            } else {
                ib -= 1;
            }
        }
        let (mine, theirs) = (&self.cases[..cuts_a[ia]], &other.cases[..cuts_b[ib]]);
        let mut cases = Vec::new();
        for (a, ma) in mine {
            for (b, mb) in theirs {
                cases.push((format!("({a}) && ({b})"), both(*ma, *mb)));
            }
            cases.push((a.clone(), both(*ma, other.otherwise)));
        }
        for (b, mb) in theirs {
            cases.push((b.clone(), both(self.otherwise, *mb)));
        }
        Escape {
            cases,
            otherwise: both(self.otherwise, other.otherwise),
        }
    }

    /// Where the cases can be cut, from none to all: never between a box's
    /// own case and those `after_run` lists before it for its last blocks
    /// (`nested`'s `{box} && (…)`).
    fn whole_cuts(&self) -> Vec<usize> {
        (0..=self.cases.len())
            .filter(|&k| {
                let (kept, gone) = self.cases.split_at(k);
                !kept.iter().any(|(c, _)| {
                    gone.iter()
                        .any(|(d, _)| c.starts_with(&format!("{d} && (")))
                })
            })
            .collect()
    }

    /// The margins escaping through a column's bottom edge while its last
    /// boxes are hidden ones a script can show: the bottom margin of the
    /// last one shown, or with all of them hidden what came before them.
    fn after_run(run: &HiddenRun) -> Option<Escape> {
        let mut cases = Vec::new();
        for (id, bottom) in run.boxes.iter().rev() {
            match run.box_cases.iter().find(|(b, _)| b == id) {
                // Shown, it passes on what its own last blocks do.
                Some((_, inner)) => {
                    let room = HiddenRun::NEAREST.saturating_sub(cases.len() + 1);
                    let (each, otherwise) = nested(Some(id), inner, MarginSet::default(), room);
                    cases.extend(each);
                    cases.push((id.clone(), otherwise));
                }
                None => cases.push((id.clone(), *bottom)),
            }
        }
        let mut escape = Escape {
            cases,
            otherwise: run.before,
        };
        if let Some(before) = &run.before_cases {
            escape.cases.extend(before.cases.iter().cloned());
            escape.otherwise = before.otherwise;
        }
        escape.varies().then_some(escape)
    }

    /// Compared unrounded and with negative margins kept: cases that look
    /// the same here can still differ once the parent's margins join them.
    fn varies(&self) -> bool {
        self.cases.iter().any(|(_, m)| *m != self.otherwise)
    }

    /// The item's `Layout.topMargin`, where the margins come to rest.
    fn set_on(&self, obj: &mut Object) {
        set_top(obj, self.gap(0.0));
    }

    /// The gap the margins leave, `less` what is already given above it: a
    /// number, or a condition on the cases when they leave different ones.
    fn gap(&self, less: f32) -> Gap {
        self.each(|m| round((m.value() - less).max(0.0)))
    }

    /// `value` of the margins in each case: a number when they all give one.
    fn each(&self, value: impl Fn(MarginSet) -> f64) -> Gap {
        let otherwise = value(self.otherwise);
        if self.cases.iter().all(|(_, m)| value(*m) == otherwise) {
            return Gap::Number(otherwise);
        }
        let mut expr = fmt_number(otherwise);
        for (id, m) in self.cases.iter().rev() {
            expr = format!("{id} ? {} : {expr}", fmt_number(value(*m)));
        }
        Gap::Expr(expr)
    }
}

/// A `Layout.topMargin` as written.
#[derive(PartialEq)]
enum Gap {
    Number(f64),
    Expr(String),
}

impl Gap {
    /// As a branch of a condition.
    fn branch(&self) -> String {
        match self {
            Gap::Number(n) => fmt_number(*n),
            Gap::Expr(e) => format!("({e})"),
        }
    }
}

fn set_top(obj: &mut Object, gap: Gap) {
    set_gap(obj, "Layout.topMargin", gap);
}

fn set_gap(obj: &mut Object, prop: &str, gap: Gap) {
    match gap {
        Gap::Number(n) if n > 0.0 => {
            obj.set(prop, Value::Number(n));
        }
        Gap::Number(_) => {}
        Gap::Expr(e) => {
            obj.set(prop, Value::expr(e));
        }
    }
}

/// Floats beside the blocks being emitted (`Emitter::float_group`).
pub(crate) struct Wrap {
    row: String,
    left: bool,
}

/// The condition on which an item that only holds hidden boxes has content.
fn shown_when(obj: &Object, b: &LayoutBox<'_>, cx: &mut Emitter<'_, '_>) -> Option<String> {
    let line = b.node.and_then(|n| node_line(cx.doc, n));
    let insets = cx.insets(&b.style, line);
    if insets.vertical() != 0.0 || !matches!(b.style.height, eldur_css::Size::Auto) {
        return None;
    }
    if let Some(shown) = waits_for_text(obj) {
        return Some(shown);
    }
    let mut ids = Vec::new();
    let mut kids: Vec<&Object> = vec![obj];
    while let Some(holder) = kids.pop() {
        for member in &holder.members {
            if let Member::Child(kid) = member {
                match (&kid.id, kid.get("visible")) {
                    (Some(id), Some(Value::Bool(false))) => ids.push(format!("{id}.visible")),
                    _ if let Some(shown) = waits_for_text(kid) => ids.push(shown),
                    // An item's line of text is its only child.
                    _ if kid.type_name == "EldurLines" => kids.push(kid),
                    _ => return None,
                }
            }
        }
    }
    (!ids.is_empty() && ids.len() <= HiddenRun::NEAREST).then(|| ids.join(" || "))
}

/// An empty block a script may fill is kept as a `Text` with no line box; until
/// it has text or an element, margins collapse through it as through any empty
/// block.
fn waits_for_text(obj: &Object) -> Option<String> {
    let id = obj.id.as_ref()?;
    (obj.type_name == "Text"
        && matches!(obj.get("text"), Some(Value::String(t)) if t.is_empty())
        && matches!(obj.get("Layout.preferredHeight"), Some(Value::Expr(_))))
    .then(|| format!("({id}.text !== \"\" || {id}.children.length > 0)"))
}

/// The top margin of the item after a hidden run: collapsed with the bottom
/// margin of the nearest box shown, or with what came before them all. Its
/// own top margin is `top`, or `cases` when its first blocks may hide too.
fn set_hidden_margin(obj: &mut Object, run: &HiddenRun, top: MarginSet, cases: Option<&Escape>) {
    let gap = |base: MarginSet, less: f32| match cases {
        Some(cases) => cases.clone().joined(base).gap(less),
        None => {
            let mut margin = base;
            margin.join(top);
            Gap::Number(round((margin.value() - less).max(0.0)))
        }
    };
    // While all of an escaping run is hidden, the margins are the parent's.
    let otherwise = if run.escaping {
        Gap::Number(0.0)
    } else if let Some(before) = &run.before_cases {
        match cases {
            Some(cases) => before.meet(cases),
            None => before.clone().joined(top),
        }
        .gap(run.given)
    } else {
        gap(run.before, run.given)
    };
    let shown: Vec<_> = run
        .boxes
        .iter()
        .map(|(id, b)| {
            let g = match run.box_cases.iter().find(|(c, _)| c == id) {
                // Shown, it passes on what its own last blocks do.
                Some((_, inner)) => match cases {
                    Some(cases) => inner.meet(cases),
                    None => inner.clone().joined(top),
                }
                .gap(0.0),
                None => gap(*b, 0.0),
            };
            (id, g)
        })
        .collect();
    if shown.iter().all(|(_, g)| *g == otherwise) {
        set_top(obj, otherwise);
        return;
    }
    let mut expr = otherwise.branch();
    for (id, g) in shown {
        expr = format!("{id} ? {} : {expr}", g.branch());
    }
    obj.set("Layout.topMargin", Value::expr(expr));
}

impl MarginSet {
    pub(crate) fn of(px: f32) -> Self {
        Self {
            pos: px.max(0.0),
            neg: px.min(0.0),
        }
    }

    pub(crate) fn join(&mut self, other: Self) {
        self.pos = self.pos.max(other.pos);
        self.neg = self.neg.min(other.neg);
    }

    pub(crate) fn value(self) -> f32 {
        self.pos + self.neg
    }
}

/// A converted block-level box, as its parent's flow sees it.
pub(crate) struct Emitted {
    pub obj: Object,
    /// Collapsible margin at the top edge (own top margin, joined with
    /// what escaped from the first child).
    pub top: MarginSet,
    pub bottom: MarginSet,
    /// Empty box whose top and bottom margins collapse together; it is
    /// not emitted.
    pub collapse_through: bool,
    /// What `top` is while a script shows or hides the first blocks whose
    /// margins escape through this box's top edge, when that changes it.
    pub top_cases: Option<Escape>,
    /// What `bottom` is while a script, a rule or a closed `<details>`
    /// shows or hides the last blocks whose margins escape through this
    /// box's bottom edge, when that changes it (#70).
    pub bottom_cases: Option<Escape>,
}

/// The children of a block container as a `ColumnLayout`.
pub(crate) struct Column {
    /// `None` when every child collapsed through.
    pub obj: Option<Object>,
    /// Margins that escaped through the top edge (everything, when there
    /// is no item).
    pub escaped_top: MarginSet,
    /// What escapes through the top as a script hides or shows the first
    /// blocks, when that changes it; `escaped_top` is what it is as loaded.
    pub escaped_cases: Option<Escape>,
    pub escaped_bottom: MarginSet,
    /// What escapes through the bottom as the last blocks show or hide,
    /// when that changes it; `escaped_bottom` is what it is as loaded.
    pub escaped_bottom_cases: Option<Escape>,
    /// Only out-of-flow boxes' slots, which take no room: the box's own
    /// margins still collapse through it.
    pub only_slots: bool,
}

impl Emitter<'_, '_> {
    /// `top_open`/`bottom_open`: whether the parent's own margins collapse
    /// with its first/last child's.
    /// The overlays the block just placed sent to this column's stop, after it.
    fn place_hoisted(&mut self, stop: Option<usize>, layout: &mut Object) {
        let Some(stop) = stop else {
            return;
        };
        let (here, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.hoisted)
            .into_iter()
            .partition(|(s, _)| *s == stop);
        self.hoisted = rest;
        for (_, host) in here {
            layout.push_child(host);
        }
    }

    pub(super) fn column(
        &mut self,
        children: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
        top_open: bool,
        bottom_open: bool,
    ) -> Column {
        let mut layout = Object::new("ColumnLayout");
        layout.set("spacing", Value::Number(0.0));
        let mut pending = MarginSet::default();
        // What `pending` is in each case, when the item before it ends with
        // blocks that may hide.
        let mut pending_cases: Option<Escape> = None;
        let mut escaped_top = None;
        let mut escaped_cases = None;
        let mut last: Option<usize> = None;
        // The last items were hidden boxes a script can show (`display:
        // none`, kept invisible): the margins before them, which reach past
        // them while they are hidden, and each one's id and bottom margin.
        let mut hidden = HiddenRun::default();
        // The item before the hidden one, which keeps its bottom margin
        // while the hidden one ends the column.
        let mut before_hidden: Option<usize> = None;
        // A stop's blocks send their overlays here when a box that follows
        // them paints in the positioned layer itself: else their blocks'
        // lifts already order them. A block lifted only for what it holds
        // counts for nothing, as its own flow would then paint over them,
        // which in CSS paints under every overlay.
        let stop = self.column_stop.take();
        let own_hoist = self.hoist_into;
        let own_icb_hoist = self.icb_hoist_into;
        let mut later_rises = vec![self.writes_z_index; children.len()];
        // The initial block's overlays paint over the whole page, so they
        // go too when a box before them rises: else the stop's column
        // and the initial block would split what CSS orders as one layer.
        let mut others_rise = later_rises.clone();
        // And a block after one that sent what it holds there: what rises
        // in it paints over that, as it follows it in the positioned layer.
        let mut after_sent = later_rises.clone();
        if stop.is_some() && !self.writes_z_index {
            // A box a class or state restacks may paint later in one
            // combination only, which would give the blocks before it one
            // shape in some and another in the rest: it counts in all of
            // them, and sending an overlay to the stop orders it right
            // whatever follows.
            let later: Vec<bool> = children
                .iter()
                .map(|c| super::lists::paints_later(c) || self.restacked(c))
                .collect();
            let mut any = false;
            for (i, &l) in later.iter().enumerate().rev() {
                later_rises[i] = any;
                any = any || l;
            }
            let mut any = false;
            let mut sent = false;
            for (i, &l) in later.iter().enumerate() {
                others_rise[i] = any || later_rises[i];
                after_sent[i] = sent;
                any = any || l;
                sent = sent || (later_rises[i] && l);
            }
        }
        let mut rest = children;
        while let Some(child) = rest.first() {
            if let Some(stop) = stop {
                let i = children.len() - rest.len();
                self.hoist_into = (later_rises[i] || after_sent[i]).then_some(stop);
                self.icb_hoist_into = others_rise[i].then_some(stop);
            }
            if is_out_of_flow(&child.style) {
                rest = &rest[1..];
                // Its static position is past the margins so far, unless
                // they escape through the parent's top edge, which moves
                // the slot with them.
                let margin = if last.is_none() && top_open {
                    0.0
                } else {
                    pending.value()
                };
                if let Some(slot) = self.out_of_flow_box(child, margin) {
                    layout.push_child(slot);
                }
                self.place_hoisted(stop, &mut layout);
                continue;
            }
            let run = float_run(rest);
            let wrapped = rest.get(run..).map_or(0, |after| {
                let side = child.style.float;
                after
                    .iter()
                    .take_while(|b| float_run(std::slice::from_ref(b)) == 0 && !clears(b, side))
                    .count()
            });
            if run > 0 && wrapped > 0 {
                let floats = &rest[..run];
                let around = &rest[run..run + wrapped];
                rest = &rest[run + wrapped..];
                let escapes = last.is_none() && top_open;
                let clearing = rest
                    .first()
                    .filter(|b| float_run(std::slice::from_ref(b)) == 0);
                let clear_top = clearing.map(|b| {
                    let line = b.node.and_then(|n| node_line(self.doc, n));
                    MarginSet::of(self.vertical_margin(&b.style.margin.top, line))
                });
                let above = pending_cases.as_ref().filter(|_| !escapes);
                let (mut obj, top, top_cases, bottom, bottom_cases) = self.float_group(
                    floats,
                    around,
                    decoration,
                    (pending, above),
                    escapes,
                    clear_top,
                );
                if escapes {
                    escaped_top = Some(top);
                } else if let Some(cases) = top_cases {
                    cases.set_on(&mut obj);
                } else {
                    self.set_margin(&mut obj, "Layout.topMargin", top.value());
                }
                layout.push_child(obj);
                last = Some(layout.members.len() - 1);
                self.place_hoisted(stop, &mut layout);
                pending = bottom;
                pending_cases = bottom_cases;
                if hidden.escaping {
                    (escaped_top, escaped_cases) = settled(&mut hidden);
                }
                hidden = HiddenRun::default();
                continue;
            }
            if run > 1 {
                let floats = &rest[..run];
                rest = &rest[run..];
                let mut obj = self.float_row(floats, decoration);
                if let Some(next) = rest
                    .first()
                    .filter(|b| float_run(std::slice::from_ref(b)) > 0)
                {
                    let line = next.node.and_then(|n| crate::boxes::node_line(self.doc, n));
                    self.diags.warning(
                        "css",
                        line,
                        format!(
                            "{} is placed below the floats before it, not beside them",
                            next.node
                                .map_or_else(|| "a box".to_owned(), |n| describe(self.doc, n))
                        ),
                    );
                }
                // Margins do not collapse with a float's.
                if last.is_none() && top_open && pending == MarginSet::default() {
                    escaped_top = Some(pending);
                } else if let Some(cases) = &pending_cases {
                    cases.set_on(&mut obj);
                } else {
                    self.set_margin(&mut obj, "Layout.topMargin", pending.value());
                }
                layout.push_child(obj);
                last = Some(layout.members.len() - 1);
                self.place_hoisted(stop, &mut layout);
                pending = MarginSet::default();
                pending_cases = None;
                if hidden.escaping {
                    (escaped_top, escaped_cases) = settled(&mut hidden);
                }
                hidden = HiddenRun::default();
                continue;
            }
            rest = &rest[1..];
            self.column_child = true;
            // What a `<details>` folds the runtime hides while it is closed
            // and shows when it opens: a closed one's as a block hidden until
            // a script shows it, an open one's as one a script can hide (#70).
            // A template's probe can't read its siblings' ids; the runtime
            // works out a made one's margins as it opens and closes.
            let folds = child
                .node
                .filter(|_| self.runtime_dom && !self.no_carry)
                .and_then(|n| crate::scripts::details_fold(self.doc, n));
            let collapsed = child.style.visibility == Visibility::Collapse
                || child.node.is_some_and(|n| self.collapsed.contains(&n))
                || folds == Some(false);
            // A block raised over what follows its own block, in a column
            // that makes no stacking context, goes to its stop's: in CSS
            // only it paints over those boxes, and the rest of its block
            // under them. A script that walks the DOM reads it where its
            // slot is.
            let rise = stop.is_none()
                && self.hoist_into.is_some()
                && !self.no_carry
                && !collapsed
                && !child.node.is_some_and(|n| self.script_hides.contains(&n))
                // A block a `<details>` folds hides with it, as one a script
                // hides does, so it stays in its block.
                && folds.is_none()
                && !child.node.is_some_and(|n| self.restacks.contains(&n))
                && super::lists::rises_in_flow(&child.style)
                // A box at `auto` is no stacking context: a level it holds,
                // or one a class or state may give it or what it holds, or
                // one a script may give what it holds, lifts only that box,
                // so it stays in its block and the block lifts. A level a
                // script gives the box itself stacks the risen box alone,
                // where Chromium puts it among its block's later siblings.
                && (child.style.z_index.is_some()
                    || !((self.writes_z_index && super::lists::holds_element(child))
                        || self.restacked(child)
                        || super::lists::holds_level(child)));
            self.rise_out = rise;
            let emitted = self.block(child, decoration);
            if emitted.collapse_through {
                self.place_hoisted(stop, &mut layout);
                pending.join(emitted.top);
                pending.join(emitted.bottom);
                pending_cases = pending_cases.map(|c| c.joined(emitted.top).joined(emitted.bottom));
                // An empty block's margins collapse with whichever box before
                // it shows, or with what came before them all.
                if !hidden.boxes.is_empty() {
                    for m in [emitted.top, emitted.bottom] {
                        hidden.join_after(m);
                    }
                }
                continue;
            }
            let mut emitted = emitted;
            let mut obj = std::mem::replace(&mut emitted.obj, Object::new("Item"));
            self.flow_item(&mut obj, child);
            // Its line box separates its margins from its neighbours'.
            let strut = if hidden.boxes.is_empty() {
                line_strut(
                    self.doc,
                    child,
                    &obj,
                    emitted.top.value(),
                    emitted.bottom.value(),
                )
            } else {
                None
            };
            if strut.is_some() {
                emitted.top = MarginSet::default();
                emitted.bottom = MarginSet::default();
                emitted.bottom_cases = None;
            }
            // A block a script hides by its display (#51): the margins
            // either side of it then collapse together, as they do around one
            // that is hidden until a script shows it.
            let reached = !collapsed
                && (folds.is_some() || child.node.is_some_and(|n| self.script_hides.contains(&n)))
                && crate::boxes::is_block_level(child.style.display);
            let hides = (collapsed || reached) && obj.id.is_some();
            let shown_when_hides = obj.id.as_ref().map(|id| format!("{id}.visible"));
            // An item holding only hidden boxes: its margins collapse through
            // it until something in it shows.
            let holds = if hides {
                None
            } else {
                shown_when(&obj, child, self)
            };
            if hidden.boxes.is_empty() {
                let before = pending;
                if hides {
                    hidden.before = pending;
                    hidden.before_cases = pending_cases.clone();
                    hidden.given = 0.0;
                    hidden.shown_bottom = None;
                    before_hidden = last;
                }
                pending.join(emitted.top);
                if last.is_none() && top_open {
                    if hides && let Some(id) = shown_when_hides.clone() {
                        hidden.escaping = true;
                        let cases = emitted.top_cases.as_ref();
                        hidden.escape(&id, cases, before, pending, !collapsed);
                    } else if let Some(shown) = &holds {
                        // Empty at the column's top, it lets the margins
                        // after it escape too (#51).
                        hidden.escaping = true;
                        let cases = emitted.top_cases.as_ref();
                        hidden.escape(&format!("({shown})"), cases, before, pending, false);
                    } else {
                        escaped_top = Some(pending);
                        escaped_cases = emitted.top_cases.clone().map(|e| e.joined(before));
                    }
                } else if let Some(cases) = emitted.top_cases.clone() {
                    match &pending_cases {
                        Some(above) => above.meet(&cases).set_on(&mut obj),
                        None => cases.joined(before).set_on(&mut obj),
                    }
                } else if strut.is_none()
                    && let Some(above) = &pending_cases
                {
                    // The item before passes on what its last shown block's
                    // bottom margin is; a line box between keeps it a number.
                    above.clone().joined(emitted.top).set_on(&mut obj);
                } else {
                    self.set_margin(&mut obj, "Layout.topMargin", pending.value());
                }
            } else {
                // Shown, a box before this one separates the margins before
                // it from this one's; hidden, they collapse with this one's.
                set_hidden_margin(&mut obj, &hidden, emitted.top, emitted.top_cases.as_ref());
                if hidden.escaping {
                    let mut at = hidden.before;
                    at.join(emitted.top);
                    let cases = emitted.top_cases.as_ref();
                    let before = hidden.before;
                    match shown_when_hides.clone() {
                        Some(id) if hides && hidden.escapes.len() < HiddenRun::NEAREST => {
                            hidden.escape(&id, cases, before, at, !collapsed);
                        }
                        // The first block that stays, or as far as conditions
                        // are written: what escapes is known.
                        _ => {
                            let (loaded, cases) = hidden.settle(cases, before, at);
                            escaped_top = Some(loaded);
                            escaped_cases = cases;
                        }
                    }
                }
                if !hides {
                    before_hidden = None;
                }
            }
            if let Some((above, below)) = strut {
                let set = match obj.get("Layout.topMargin") {
                    Some(Value::Number(n)) => *n as f32,
                    _ => 0.0,
                };
                self.set_margin(&mut obj, "Layout.topMargin", set + above);
                self.set_margin(&mut obj, "Layout.bottomMargin", below);
            }
            if hides && let Some(id) = &obj.id {
                let shown = format!("{id}.visible");
                if let Some(cases) = &emitted.bottom_cases {
                    hidden.box_cases.push((shown.clone(), cases.clone()));
                }
                hidden.push(shown, emitted.bottom);
                if !collapsed {
                    hidden.shown_bottom = Some(emitted.bottom);
                }
            } else {
                hidden.boxes.clear();
                hidden.box_cases.clear();
                hidden.shown_bottom = None;
                hidden.holder = None;
                if let Some(shown) = holds {
                    hidden.holder = Some((usize::MAX, shown.clone()));
                    let mut given = pending;
                    given.join(emitted.top);
                    hidden.before = given;
                    hidden.before.join(emitted.bottom);
                    hidden.before_cases = pending_cases
                        .clone()
                        .map(|c| c.joined(emitted.top).joined(emitted.bottom));
                    // Escaping, its top margin is the parent's.
                    hidden.given = if hidden.escaping {
                        0.0
                    } else {
                        given.value().max(0.0)
                    };
                    hidden.push(shown, emitted.bottom);
                    before_hidden = None;
                }
            }
            if self.options.source_comments
                && let Some(node) = child.node
            {
                layout
                    .members
                    .push(Member::Comment(describe(self.doc, node)));
            }
            match self.hoist_into.filter(|_| rise) {
                Some(to) => {
                    let (slot, host) = self.risen_host(child, obj);
                    layout.push_child(slot);
                    self.hoisted.push((to, host));
                }
                None => {
                    layout.push_child(obj);
                }
            }
            last = Some(layout.members.len() - 1);
            pending = emitted.bottom;
            pending_cases = emitted.bottom_cases.clone();
            if let Some((at, _)) = &mut hidden.holder
                && *at == usize::MAX
            {
                *at = layout.members.len() - 1;
            }
            self.place_hoisted(stop, &mut layout);
        }
        self.hoist_into = own_hoist;
        self.icb_hoist_into = own_icb_hoist;
        // Its margins escape through the top too while all of it is hidden.
        let escapes_through = hidden.escaping;
        // Every block of the column is one a script can hide.
        if hidden.escaping {
            (escaped_top, escaped_cases) = settled(&mut hidden);
        }
        let only_slots =
            last.is_none() && layout.members.iter().any(|m| matches!(m, Member::Child(_)));
        let Some(mut last) = last else {
            return Column {
                obj: only_slots.then_some(layout),
                escaped_top: pending,
                escaped_cases: None,
                escaped_bottom: MarginSet::default(),
                escaped_bottom_cases: None,
                only_slots,
            };
        };
        // A slot after the last item counts its static position from past
        // the item's margins, so those go below the slot. A hidden slot isn't
        // laid out: margins there would be lost.
        if let Some(tail) = layout.members.iter().rposition(|m| {
            matches!(m, Member::Child(o) if !matches!(o.get("visible"), Some(Value::Bool(false)))
                && !super::lists::is_host(o))
        }) {
            last = tail;
        }
        let mut escaped_bottom_cases = None;
        // What passes on as the last blocks show or hide: the bottom margin
        // of the last one shown, or what came before them all. A column all
        // of whose blocks can hide lets its margins escape through the top.
        let escaped_bottom = if bottom_open {
            escaped_bottom_cases = if escapes_through {
                None
            } else if hidden.boxes.is_empty() {
                pending_cases.clone()
            } else {
                Escape::after_run(&hidden)
            };
            // The page as loaded: a hidden last box passes on the margins
            // before it, or the bottom one of the last box shown.
            if hidden.boxes.is_empty() {
                pending
            } else if let Some(bottom) = hidden.shown_bottom {
                bottom
            } else {
                // A holder of hidden boxes at the end keeps the margins
                // before it as its top one only while they show; hidden,
                // they pass on with the column's, not on top of them.
                if hidden.given > 0.0
                    && let Some((i, shown)) = &hidden.holder
                    && let Some(Member::Child(obj)) = layout.members.get_mut(*i)
                {
                    let kept = match obj.get("Layout.topMargin") {
                        Some(Value::Number(n)) => Some(fmt_number(*n)),
                        Some(Value::Expr(e)) => Some(format!("({e})")),
                        _ => None,
                    };
                    if let Some(kept) = kept {
                        obj.set(
                            "Layout.topMargin",
                            Value::expr(format!("{shown} ? {kept} : 0")),
                        );
                    }
                }
                hidden.before
            }
        } else {
            if let Some(Member::Child(obj)) = layout.members.get_mut(last) {
                match pending_cases.as_ref().filter(|_| hidden.boxes.is_empty()) {
                    Some(cases) => set_gap(obj, "Layout.bottomMargin", cases.gap(0.0)),
                    None => self.set_margin(obj, "Layout.bottomMargin", pending.value()),
                }
            }
            // With all of the run hidden, the margins before it: in each case,
            // when the boxes before it end with blocks that may hide too.
            let all_hidden = match &hidden.before_cases {
                Some(cases) => cases.gap(0.0),
                None => Gap::Number(round(hidden.before.value().max(0.0))),
            };
            if !hidden.boxes.is_empty()
                && let Some(i) = before_hidden
                && let Some(Member::Child(obj)) = layout.members.get_mut(i)
                && !matches!(all_hidden, Gap::Number(n) if n <= 0.0)
            {
                let shown = hidden
                    .boxes
                    .iter()
                    .map(|(shown, _)| shown.clone())
                    .collect::<Vec<_>>()
                    .join(" || ");
                obj.set(
                    "Layout.bottomMargin",
                    Value::expr(format!("{shown} ? 0 : {}", all_hidden.branch())),
                );
            }
            MarginSet::default()
        };
        self.carry(&mut layout);
        Column {
            obj: Some(layout),
            escaped_top: escaped_top.unwrap_or_default(),
            escaped_cases,
            escaped_bottom,
            escaped_bottom_cases,
            only_slots: false,
        }
    }

    /// Floats and the blocks after them that wrap around them: the floats'
    /// row over a column of the blocks, where each line beside the floats'
    /// margin box is shortened by it, in nested blocks too (CSS 2.2 §9.5).
    /// Blocks themselves are not narrowed: only their lines move.
    ///
    /// A float is placed past the margins `pending` so far, which still
    /// collapse with the first block's beside it; when they escape through
    /// the parent (`escapes`) the floats wait for them at its top. A block
    /// that clears them after (`clear_top`, its top margin) has its border
    /// edge at their bottom unless the margins above it take it further,
    /// which absorbs them (§9.5.2). Returns the group, the margins above
    /// it, and those escaping through its bottom.
    fn float_group(
        &mut self,
        floats: &[LayoutBox<'_>],
        around: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
        // The margins before the group, and what they are in each case when
        // the blocks before it may hide.
        (pending, pending_cases): (MarginSet, Option<&Escape>),
        escapes: bool,
        clear_top: Option<MarginSet>,
    ) -> (Object, MarginSet, Option<Escape>, MarginSet, Option<Escape>) {
        let left = floats[0].style.float == Float::Left;
        let mut row = self.float_row(floats, decoration);
        let row_id = row.id.clone().expect("a float row has an id");
        row.members
            .retain(|m| !matches!(m, Member::Property { name, .. } if name == "Layout.fillWidth"));
        for edge in ["left", "right", "top"] {
            row.set(
                &format!("anchors.{edge}"),
                Value::expr(format!("parent.{edge}")),
            );
        }
        let outer = self.wrap.replace(Wrap {
            row: row_id.clone(),
            left,
        });
        let column = self.column(around, decoration, true, true);
        self.wrap = outer;
        let mut group = Object::new("Item");
        group.set("Layout.fillWidth", true);
        let Some(mut col) = column.obj else {
            group.set(
                "implicitHeight",
                Value::expr(format!("{row_id}.implicitHeight")),
            );
            group.push_child(row);
            // Every block beside the floats collapsed through: their margins
            // go on past the floats.
            return (
                group,
                pending,
                pending_cases.cloned(),
                column.escaped_top,
                None,
            );
        };
        let mut top = pending;
        top.join(column.escaped_top);
        // The floats stay where the margins before them end; the wrapped
        // column's own top margin may take the group further. When the
        // margins before depend on which blocks show, so do both.
        let escaped = column.escaped_top;
        let top_cases = pending_cases.map(|c| c.clone().joined(escaped));
        let shift = match pending_cases {
            Some(cases) => cases.each(|m| {
                let mut with = m;
                with.join(escaped);
                round(m.value() - with.value())
            }),
            None if escapes => Gap::Number(0.0),
            None => Gap::Number(round(pending.value() - top.value())),
        };
        match shift {
            Gap::Number(0.0) => {}
            Gap::Number(n) => {
                row.set("anchors.topMargin", Value::Number(n));
            }
            Gap::Expr(e) => {
                row.set("anchors.topMargin", Value::expr(e));
            }
        }
        let col_id = self.ids.claim("wrapped");
        col.id = Some(col_id.clone());
        for edge in ["left", "right", "top"] {
            col.set(
                &format!("anchors.{edge}"),
                Value::expr(format!("parent.{edge}")),
            );
        }
        let mut texts = Vec::new();
        self.cut_texts(&mut col, &row_id, &mut texts);
        if !texts.is_empty() {
            row.members.push(Member::Handler {
                name: "onChildrenRectChanged".to_owned(),
                params: Vec::new(),
                body: texts
                    .iter()
                    .map(|t| format!("Qt.callLater({t}.forceLayout);"))
                    .collect(),
            });
        }
        // Not the wrapped text's: its unwrapped width only settles after the
        // first polish, and an ancestor its layout has not sized yet takes
        // it as its width for a frame, where an SVG rendered at that width
        // exceeds Qt's image allocation limit. A shrink-to-fit container
        // therefore sizes this group to its floats.
        group.set(
            "implicitWidth",
            Value::expr(format!("{row_id}.implicitWidth")),
        );
        let (height, bottom, bottom_cases) = match clear_top {
            // The clearing block keeps its own top margin, which the group
            // leaves room for: the column's bottom margins past it, in each
            // case when they depend on which blocks show.
            Some(clear) => {
                let past = |m: MarginSet| {
                    let mut merged = m;
                    merged.join(clear);
                    round(merged.value() - clear.value())
                };
                let more = match &column.escaped_bottom_cases {
                    Some(cases) => cases.each(past),
                    None => Gap::Number(past(column.escaped_bottom)),
                };
                let more = match more {
                    Gap::Number(0.0) => String::new(),
                    Gap::Number(n) if n < 0.0 => format!(" - {}", fmt_number(-n)),
                    Gap::Number(n) => format!(" + {}", fmt_number(n)),
                    Gap::Expr(e) => format!(" + ({e})"),
                };
                let below = match round(clear.value()) {
                    0.0 => String::new(),
                    px if px < 0.0 => format!(" + {}", fmt_number(-px)),
                    px => format!(" - {}", fmt_number(px)),
                };
                (
                    format!(
                        "Math.max({col_id}.implicitHeight{more}, {row_id}.y + {row_id}.implicitHeight{below})"
                    ),
                    MarginSet::default(),
                    None,
                )
            }
            None => (
                format!("Math.max({col_id}.implicitHeight, {row_id}.y + {row_id}.implicitHeight)"),
                column.escaped_bottom,
                column.escaped_bottom_cases,
            ),
        };
        group.set("implicitHeight", Value::expr(height));
        // Floats are painted over the blocks' backgrounds (CSS 2.2
        // Appendix E); the lines beside them do not reach them.
        group.push_child(col);
        group.push_child(row);
        (group, top, top_cases, bottom, bottom_cases)
    }

    /// Gives line boxes the floats around them as their `exclusion`, in
    /// their own terms.
    pub(super) fn wrap_flow(&mut self, flow: &mut Object) {
        let Some(wrap) = &self.wrap else {
            return;
        };
        let (row_id, left) = (&wrap.row, wrap.left);
        // The arguments are what the place depends on, which a binding on
        // `mapToItem` alone would not follow.
        flow.set(
            "exclusion",
            Value::expr(format!("eldurExclusion({row_id}.childrenRect, y)")),
        );
        flow.members.push(Member::Function {
            name: "eldurExclusion".to_owned(),
            params: vec!["rect".to_owned(), "top".to_owned()],
            body: vec![
                format!("const at = mapToItem({row_id}, 0, 0);"),
                format!(
                    "return {{ left: {left}, from: rect.x - at.x, to: rect.x + rect.width - at.x, \
                     top: rect.y - at.y, bottom: rect.y + rect.height - at.y }};"
                ),
            ],
        });
    }

    /// Shortens each line of `text` that is beside the floats around it.
    pub(super) fn wrap_lines(&mut self, text: &mut Object) {
        let Some(wrap) = &self.wrap else {
            return;
        };
        let (row_id, left) = (wrap.row.clone(), wrap.left);
        let rect = format!("{row_id}.childrenRect");
        let cut = if left {
            format!("{rect}.x + {rect}.width - at.x")
        } else {
            format!("at.x + width - {rect}.x")
        };
        // A fixed line height spaces lines evenly from the top. With this
        // handler Qt puts a fixed line's leading above its glyphs (`line.y`
        // is offset within the line), where without it the leading is
        // below, which the `Text`'s half-leading shift assumes: the line
        // goes back to the top.
        let mut body = vec![
            "const fixed = lineHeightMode === Text.FixedHeight;".to_owned(),
            "if (fixed) {".to_owned(),
            "    line.y = line.number * lineHeight;".to_owned(),
            "}".to_owned(),
            format!("const at = mapToItem({row_id}, 0, 0);"),
            "const top = at.y + (fixed ? line.number * lineHeight : line.y);".to_owned(),
            "const bottom = top + (fixed ? lineHeight : line.height);".to_owned(),
            format!("if (top < {rect}.y + {rect}.height && bottom > {rect}.y) {{"),
            format!("    const cut = {cut};"),
            "    if (cut > 0) {".to_owned(),
            "        line.width -= cut;".to_owned(),
        ];
        if left {
            body.push("        line.x += cut;".to_owned());
        }
        body.extend(["    }".to_owned(), "}".to_owned()]);
        // A function, so the runtime's measuring twin cuts its lines the same.
        text.members.push(Member::Function {
            name: "eldurCut".to_owned(),
            params: vec!["line".to_owned()],
            body,
        });
        // A summary's marker moves its first line in by its own handler:
        // one handler does both, the marker's first, as the line then starts
        // past the floats and the marker before its text.
        let mut lines = Vec::new();
        text.members.retain(|m| match m {
            Member::Property {
                name,
                value: Value::Expr(e),
            } if name == "onLineLaidOut" => {
                lines.push(format!("({e})(line);"));
                false
            }
            _ => true,
        });
        lines.push("eldurCut(line)".to_owned());
        text.members.push(Member::Handler {
            name: "onLineLaidOut".to_owned(),
            params: vec!["line".to_owned()],
            body: lines,
        });
        // Lines are laid out for the place the text had then. Later: the
        // handler above reads geometry whose change would relayout the text
        // under it.
        text.members.push(Member::Handler {
            name: "onYChanged".to_owned(),
            params: Vec::new(),
            body: vec!["Qt.callLater(forceLayout)".to_owned()],
        });
    }

    /// The ids of the `Text`s in `obj` whose lines `row_id`'s floats cut,
    /// given one where the block had none: the floats tell them to relayout.
    fn cut_texts(&mut self, obj: &mut Object, row_id: &str, out: &mut Vec<String>) {
        let mark = format!("const at = mapToItem({row_id}, 0, 0);");
        let cut = obj.members.iter().any(|m| {
            matches!(m, Member::Function { name, body, .. } if name == "eldurCut" && body.contains(&mark))
        });
        if cut {
            let id = match &obj.id {
                Some(id) => id.clone(),
                None => {
                    let id = self.ids.claim("wrapped text");
                    obj.id = Some(id.clone());
                    id
                }
            };
            out.push(id);
        }
        for child in obj.children_mut() {
            self.cut_texts(child, row_id, out);
        }
    }

    /// A run of floats to one side as a `Flow`: each takes its margin box
    /// along the line, and one that does not fit starts a new line below
    /// (which is where CSS puts it when the floats are as tall).
    fn float_row(&mut self, floats: &[LayoutBox<'_>], decoration: TextDecorationLine) -> Object {
        let id = self.ids.claim("floats");
        let mut row = Object::new("Flow").with_id(id.clone());
        row.set("Layout.fillWidth", true);
        if floats[0].style.float == Float::Right {
            row.set("layoutDirection", Value::expr("Qt.RightToLeft"));
        }
        let base = format!("{id}.width");
        for b in floats {
            self.in_float_run = true;
            let mut obj = self.block(b, decoration).obj;
            self.in_float_run = false;
            let style = &*b.style;
            let line = b.node.and_then(|n| crate::boxes::node_line(self.doc, n));
            let insets = self.insets(style, line);
            let (extra_w, extra_h) = match style.box_sizing {
                BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
                BoxSizing::BorderBox => (0.0, 0.0),
            };
            // `auto` margins of a float are zero; percentages are of the
            // containing block's width.
            let margin = |m: &LengthAuto| match m {
                LengthAuto::Length(l) => match l.as_px() {
                    Some(px) => fmt_number(round(px)),
                    None => format!("({})", length_expr(l, &base)),
                },
                LengthAuto::Auto => "0".to_owned(),
            };
            let m = &style.margin;
            let (mt, mr, mb, ml) = (
                margin(&m.top),
                margin(&m.right),
                margin(&m.bottom),
                margin(&m.left),
            );
            let child_id = match &obj.id {
                Some(child_id) => child_id.clone(),
                None => {
                    let child_id = self.ids.claim(&format!("{} box", super::hint(self.doc, b)));
                    obj.id = Some(child_id.clone());
                    child_id
                }
            };
            match &style.width {
                Size::Length(w) => {
                    let w = length_expr(w, &base);
                    obj.set(
                        "width",
                        Value::expr(if extra_w == 0.0 {
                            w
                        } else {
                            format!("{w} + {}", fmt_number(round(extra_w)))
                        }),
                    );
                }
                // Shrink-to-fit, within the line.
                _ => {
                    obj.set(
                        "width",
                        Value::expr(format!("Math.min(implicitWidth, {base} - {ml} - {mr})")),
                    );
                }
            }
            if let Size::Length(h) = &style.height
                && let Some(px) = h.as_px()
            {
                obj.set("height", number(px + extra_h));
            }
            if ml != "0" {
                obj.set("x", Value::expr(ml.clone()));
            }
            if mt != "0" {
                obj.set("y", Value::expr(mt.clone()));
            }
            let mut cell = Object::new("Item");
            cell.set(
                "width",
                Value::expr(format!("{child_id}.width + {ml} + {mr}").replace(" + 0", "")),
            );
            cell.set(
                "height",
                Value::expr(format!("{child_id}.height + {mt} + {mb}").replace(" + 0", "")),
            );
            if self.options.source_comments
                && let Some(node) = b.node
            {
                row.members.push(Member::Comment(describe(self.doc, node)));
            }
            cell.push_child(obj);
            row.push_child(cell);
        }
        row
    }

    /// `Layout.*` sizing of a block-level box in a `ColumnLayout`.
    fn flow_item(&mut self, obj: &mut Object, b: &LayoutBox<'_>) {
        let style = &*b.style;
        let line = b.node.and_then(|n| crate::boxes::node_line(self.doc, n));
        let insets = self.insets(style, line);
        // Sizes are of the content box unless `border-box`; QML sizes are
        // of the whole item.
        let extra_w = match style.box_sizing {
            BoxSizing::ContentBox => insets.horizontal(),
            BoxSizing::BorderBox => 0.0,
        };
        let extra_h = match style.box_sizing {
            BoxSizing::ContentBox => insets.vertical(),
            BoxSizing::BorderBox => 0.0,
        };
        let auto_left = style.margin.left == LengthAuto::Auto;
        let auto_right = style.margin.right == LengthAuto::Auto;
        // Floats stack as blocks here, so one that does not share its line
        // is placed right; `auto` margins of a float are zero.
        let alignment = match (style.float, auto_left, auto_right) {
            (Float::Right, ..) => Some("Qt.AlignRight"),
            (Float::Left, ..) => None,
            (_, true, true) => Some("Qt.AlignHCenter"),
            (_, true, false) => Some("Qt.AlignRight"),
            _ => None,
        };
        // An `inline-flex`/`inline-grid` laid out as a block keeps its
        // shrink-to-fit width.
        let shrink = b.kind == crate::boxes::BoxKind::Replaced
            || matches!(
                style.display,
                Display::InlineFlex
                    | Display::InlineGrid
                    | Display::InlineTable
                    | Display::InlineBlock
            );
        // An inline-level box's place in its line comes from the `text-align`
        // of the block it is in; its own (a button's is `center`) is for its
        // content.
        let alignment = alignment.or(match (style.display, self.line_align) {
            (Display::InlineBlock | Display::InlineTable, TextAlign::Center) => {
                Some("Qt.AlignHCenter")
            }
            (Display::InlineBlock | Display::InlineTable, TextAlign::Right | TextAlign::End) => {
                Some("Qt.AlignRight")
            }
            _ => None,
        });
        // A Qt Layout fills its cell by default, which would override a
        // width the box has of its own.
        if is_layout(&obj.type_name) && matches!(style.width, Size::Length(_)) {
            obj.set("Layout.fillWidth", false);
        }
        // A preferred width bound to the column's width feeds the column's
        // implicit width back into itself, which can widen the whole page,
        // and a control that does not fill cannot be narrower than its
        // implicit width: an item fills up to its percentage instead. A
        // Layout's own width does not depend on its column's.
        let fill_percent = super::image::is_percent_width(style) && !is_layout(&obj.type_name);
        let mut overflow_column = false;
        match &style.width {
            Size::Length(w) if fill_percent => {
                obj.set("Layout.fillWidth", true);
                if *w != Length::Linear(eldur_css::LengthPercentage::percent(100.0)) {
                    obj.set("Layout.maximumWidth", size_value(w, extra_w));
                }
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
            Size::Length(w) => {
                obj.set("Layout.preferredWidth", size_value(w, extra_w));
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
                overflow_column = self.column_fills && alignment.is_none() && w.as_px().is_some();
            }
            _ if shrink => {
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
            _ => {
                obj.set("Layout.fillWidth", true);
                // A fill item with a maximum width is centred by the
                // alignment (measured), as auto margins do in CSS.
                if (matches!(style.max_width, Size::Length(_)) || style.display == Display::Table)
                    && let Some(a) = alignment
                {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
        }
        if let Size::Length(max) = &style.max_width {
            obj.set("Layout.maximumWidth", size_value(max, extra_w));
        } else if is_layout(&obj.type_name) && obj.get("Layout.maximumWidth").is_none() {
            // A layout's maximum width defaults to its children's largest;
            // a block without `max-width` still spans its container.
            obj.set(
                "Layout.maximumWidth",
                Value::expr("Number.POSITIVE_INFINITY"),
            );
        }
        if let Size::Length(min) = &style.min_width
            && min.as_px() != Some(0.0)
        {
            obj.set("Layout.minimumWidth", size_value(min, extra_w));
        }
        for (name, margin) in [
            ("Layout.leftMargin", &style.margin.left),
            ("Layout.rightMargin", &style.margin.right),
        ] {
            if let LengthAuto::Length(l) = margin
                && l.as_px() != Some(0.0)
            {
                obj.set(name, length_value_nonneg(l));
            }
        }
        // A Layout's column is at least as wide as an item that does not fill,
        // so an item wider than the column would widen its siblings. In CSS it
        // overflows them. The margin asks the column for nothing, and the item
        // stays at its preferred width in whatever cell it gets. It cannot follow
        // the column's width: a column once widened would stay as wide.
        if overflow_column
            && obj.get("Layout.leftMargin").is_none()
            && obj.get("Layout.rightMargin").is_none()
        {
            // A script can move it to a row or a grid, where the cell is its own.
            obj.set(
                "Layout.rightMargin",
                Value::expr("parent instanceof ColumnLayout ? -Layout.preferredWidth : 0"),
            );
        }
        // A filling item is no wider than its cell, but a percentage of the
        // content box overflows by the padding and border, as in CSS; it
        // overflows on the right, so an item placed right keeps its edge.
        if fill_percent
            && extra_w > 0.0
            && alignment.is_none()
            && let LengthAuto::Length(l) = &style.margin.right
            && let Some(right) = l.as_px()
        {
            obj.set("Layout.rightMargin", number(right.max(0.0) - extra_w));
        }
        match &style.height {
            Size::Length(h) if h.as_px().is_some() || is_viewport_relative(h) => {
                obj.set("Layout.preferredHeight", size_value(h, extra_h));
            }
            Size::Length(Length::Linear(l))
                if let Some(base) = self.percent_height_base
                    && l.vw == 0.0
                    && l.vh == 0.0 =>
            {
                obj.set(
                    "Layout.preferredHeight",
                    number(l.px + l.percent / 100.0 * base + extra_h),
                );
            }
            Size::Length(Length::Linear(l))
                if self.percent_of_parent && l.px == 0.0 && l.vw == 0.0 && l.vh == 0.0 =>
            {
                // A full-height item with a sibling overflows its column, which Qt's
                // layouts answer with a recursive rearrange: it takes what the sibling
                // leaves.
                if self.percent_fills {
                    obj.set("Layout.preferredHeight", 0.0);
                    obj.set("Layout.fillHeight", true);
                } else {
                    let h = format!(
                        "parent.height * {}",
                        fmt_number(f64::from(l.percent) / 100.0)
                    );
                    obj.set("Layout.preferredHeight", Value::expr(h.clone()));
                    obj.set("Layout.minimumHeight", Value::expr(h));
                }
            }
            Size::Length(Length::Linear(l))
                if let Some(vh) = self.percent_of_vh
                    && l.vw == 0.0
                    && l.vh == 0.0 =>
            {
                let of_viewport = eldur_css::LengthPercentage {
                    vh: vh * l.percent / 100.0,
                    percent: 0.0,
                    ..*l
                };
                obj.set(
                    "Layout.preferredHeight",
                    size_value(&Length::Linear(of_viewport), extra_h),
                );
            }
            Size::Length(_) => {
                self.diags.info(
                    "css",
                    line,
                    format!(
                        "percentage height on {} is treated as `auto`",
                        b.node
                            .map_or("an anonymous box".to_owned(), |n| describe(self.doc, n))
                    ),
                );
            }
            _ => {}
        }
        if let Size::Length(min) = &style.min_height
            && let Some(px) = min.as_px()
            && px != 0.0
        {
            obj.set("Layout.minimumHeight", number(px + extra_h));
        }
        if let Size::Length(max) = &style.max_height
            && let Some(px) = max.as_px()
        {
            obj.set("Layout.maximumHeight", number(px + extra_h));
        }
    }

    /// `Layout.topMargin` of the item after a hidden box `id`: `shown`
    /// while that is visible, else `hidden`.
    fn set_margin(&mut self, obj: &mut Object, name: &str, px: f32) {
        if px > 0.0 {
            obj.set(name, number(px));
        } else if px < 0.0 {
            self.diags.info(
                "qml",
                None,
                "negative vertical margins are approximated as 0".to_owned(),
            );
        }
    }

    /// A box's own vertical margin in px (CSS: `auto` is 0 in block flow;
    /// percentages are of the containing block's width, which is not
    /// known at conversion time).
    pub(super) fn vertical_margin(&mut self, margin: &LengthAuto, line: Option<u32>) -> f32 {
        match margin {
            LengthAuto::Auto => 0.0,
            LengthAuto::Length(l) => {
                let (px, dropped) = px_part(l);
                if dropped {
                    self.diags.info(
                        "css",
                        line,
                        "percentage vertical margin uses its px part only".to_owned(),
                    );
                }
                px
            }
        }
    }
}

/// Whether a box's `height` (or `min-height`) keeps its margins apart.
/// Whether `b` is placed below floats to `side` rather than beside them.
fn clears(b: &LayoutBox<'_>, side: Float) -> bool {
    match b.style.clear {
        Clear::None => false,
        Clear::Both => true,
        Clear::Left => side == Float::Left,
        Clear::Right => side == Float::Right,
    }
}

/// How many boxes from the first float to the same side in a row; 0 when
/// the first is not a float.
fn float_run(boxes: &[LayoutBox<'_>]) -> usize {
    let side = |b: &LayoutBox<'_>| match b.style.float {
        Float::None => None,
        _ if b.style.display == Display::None => None,
        side => Some(side),
    };
    let Some(first) = boxes.first().and_then(side) else {
        return 0;
    };
    boxes.iter().take_while(|b| side(b) == Some(first)).count()
}

impl Emitter<'_, '_> {
    /// Whether a class or state restacks the box or one it holds, short of
    /// a stacking context: CSS keeps a level inside the context that holds
    /// it, as `lists::holds_level` does. A class that makes or unmakes the
    /// context restacks the context itself, so that one still counts.
    fn restacked(&self, b: &LayoutBox<'_>) -> bool {
        b.node.is_some_and(|n| self.restacks.contains(&n))
            || matches!(&b.content, crate::boxes::Content::Blocks(kids) if kids.iter().any(|k| {
                if is_context(&k.style) {
                    k.node.is_some_and(|n| self.restacks.contains(&n))
                } else {
                    self.restacked(k)
                }
            }))
    }

    /// Whether a percentage height on a child of the current block means
    /// anything.
    pub(super) fn percent_resolves(&self) -> bool {
        self.percent_height_base.is_some() || self.percent_of_vh.is_some()
    }
}

/// Whether a box makes a stacking context: one at `auto` that still
/// makes one, or a positioned box with a level.
fn is_context(style: &ComputedStyle) -> bool {
    crate::boxes::is_context_at_auto(style)
        || (crate::boxes::is_containing_block(style) && style.z_index.is_some())
}

/// A stretched block fills after all if a child's percentage height needs that.
pub(crate) fn holds_percent_height(b: &LayoutBox<'_>) -> bool {
    matches!(&b.content, crate::boxes::Content::Blocks(kids) if kids.iter().any(|k| {
        matches!(&k.style.height, Size::Length(Length::Linear(l)) if l.percent != 0.0)
    }))
}

pub(crate) fn has_height(style: &ComputedStyle) -> bool {
    let definite = |s: &Size| matches!(s, Size::Length(l) if l.as_px() != Some(0.0));
    definite(&style.height) || definite(&style.min_height)
}

/// QML Layout types: their size limits default to their children's, not
/// to infinity as for other items.
pub(crate) fn is_layout(type_name: &str) -> bool {
    matches!(type_name, "ColumnLayout" | "RowLayout" | "GridLayout")
}

/// A length of px and viewport units only: definite at any depth (unlike a
/// percentage height, which needs a definite containing block).
pub(crate) fn is_viewport_relative(length: &Length) -> bool {
    matches!(length, Length::Linear(l) if l.percent == 0.0)
}

pub(crate) fn size_value(length: &Length, extra: f32) -> Value {
    match length.as_px() {
        Some(px) => number(px + extra),
        None if extra == 0.0 => Value::Expr(length_expr(length, "parent.width")),
        None => Value::Expr(format!(
            "{} + {}",
            length_expr(length, "parent.width"),
            super::units::fmt_number(super::units::round(extra))
        )),
    }
}

/// Layout margins are not negative in Qt; negative CSS margins are
/// approximated as 0.
pub(crate) fn length_value_nonneg(l: &Length) -> Value {
    match l.as_px() {
        Some(px) => number(px.max(0.0)),
        None => Value::Expr(format!("Math.max(0, {})", length_expr(l, "parent.width"))),
    }
}

/// The space a block-laid-out inline replaced element `b` (border box
/// `obj`) keeps above and below it in its line box, margins included: its
/// bottom margin edge sits on the baseline, raised by `vertical-align`,
/// and the line is at least `line-height` tall (CSS 2.2 §10.8). `None`
/// for a block-level box, one whose baseline is its text's, or one whose
/// height or alignment is not known here.
fn line_strut(
    doc: &eldur_dom::Document,
    b: &LayoutBox<'_>,
    obj: &Object,
    mt: f32,
    mb: f32,
) -> Option<(f32, f32)> {
    use eldur_css::VerticalAlign;
    let style = &*b.style;
    if b.kind != crate::boxes::BoxKind::Replaced
        || crate::boxes::is_block_level(style.display)
        || style.float != Float::None
    {
        return None;
    }
    // A form control that shows text has that text's baseline.
    let element = doc.element(b.node?)?;
    let bottom_baseline = element.is_svg()
        || (element.is_html()
            && matches!(
                element.local_name(),
                "img" | "video" | "canvas" | "iframe" | "embed" | "object" | "progress" | "meter"
            ));
    if !bottom_baseline {
        return None;
    }
    let raise = match style.vertical_align {
        VerticalAlign::Baseline => 0.0,
        VerticalAlign::Length(v) => v,
        _ => return None,
    };
    let Some(Value::Number(h)) = obj
        .get("Layout.preferredHeight")
        .or_else(|| obj.get("implicitHeight"))
    else {
        return None;
    };
    let line = super::inline::line_height(style);
    let bottom_edge = super::inline::text_baseline(style) - raise;
    let top_edge = bottom_edge - (mt + *h as f32 + mb);
    let line_top = top_edge.min(0.0);
    let line_bottom = bottom_edge.max(line);
    Some((top_edge - line_top + mt, line_bottom - bottom_edge + mb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(prefix: &str, margins: &[f32], otherwise: f32) -> Escape {
        Escape {
            cases: margins
                .iter()
                .enumerate()
                .map(|(i, m)| (format!("{prefix}{i}"), MarginSet::of(*m)))
                .collect(),
            otherwise: MarginSet::of(otherwise),
        }
    }

    /// The margin the cases give while the boxes in `shown` are shown.
    fn under(escape: &Escape, shown: &[String]) -> f32 {
        let holds = |cond: &str| {
            cond.split(" && ").all(|c| {
                shown
                    .iter()
                    .any(|s| *s == c.trim_matches(|ch| ch == '(' || ch == ')'))
            })
        };
        escape
            .cases
            .iter()
            .find(|(c, _)| holds(c))
            .map_or(escape.otherwise, |(_, m)| *m)
            .value()
    }

    #[test]
    fn meeting_cases_past_the_nearest_keeps_each_kept_box_alone() {
        // Nearest first on both sides, as `after_run` and `nested` list them.
        let above = side("a", &[10.0, 20.0, 30.0], 4.0);
        let below = side("b", &[12.0, 24.0, 36.0], 2.0);
        let met = above.meet(&below);
        assert!(met.cases.len() <= HiddenRun::NEAREST);
        let names = |p: &'static str| (0..3).map(move |i| format!("{p}{i}"));
        for a in (0..=3).map(|n| names("a").take(n).collect::<Vec<_>>()) {
            for b in (0..=3).map(|n| names("b").take(n).collect::<Vec<_>>()) {
                // Each side shows its nearest boxes: the dropped ones are
                // the farthest, so these margins are exact.
                let first = |s: &[String], e: &Escape| {
                    s.first().map_or(e.otherwise, |id| {
                        e.cases
                            .iter()
                            .find(|(c, _)| c == id)
                            .expect("a case for each kept id")
                            .1
                    })
                };
                let mut want = first(&a, &above);
                want.join(first(&b, &below));
                let shown: Vec<String> = a.iter().chain(&b).cloned().collect();
                assert_eq!(under(&met, &shown), want.value(), "shown {shown:?}");
            }
        }
        // The nearest box alone on the lower side still holds when the upper
        // side is all hidden: the case the old cut dropped.
        assert_eq!(under(&met, &["b0".to_owned()]), 12.0);
        assert_eq!(under(&met, &["a0".to_owned(), "b0".to_owned()]), 12.0);
    }

    #[test]
    fn meeting_cases_keeps_a_box_with_the_cases_of_its_last_blocks() {
        // As `after_run` lists a box `a` whose last blocks `x`, `y` may show.
        let above = Escape {
            cases: vec![
                ("a && (x)".to_owned(), MarginSet::of(16.0)),
                ("a && (y)".to_owned(), MarginSet::of(24.0)),
                ("a".to_owned(), MarginSet::of(0.0)),
            ],
            otherwise: MarginSet::of(40.0),
        };
        let below = side("b", &[12.0, 24.0, 36.0], 2.0);
        let met = above.meet(&below);
        assert!(met.cases.len() <= HiddenRun::NEAREST);
        let shown = |s: &[&str]| s.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(under(&met, &shown(&["a"])), 2.0);
        assert_eq!(under(&met, &shown(&["a", "x"])), 16.0);
        assert_eq!(under(&met, &shown(&["a", "y"])), 24.0);
        assert_eq!(under(&met, &shown(&[])), 40.0);
    }

    #[test]
    fn a_box_past_the_nearest_takes_its_cases_with_it() {
        let mut run = HiddenRun::default();
        for i in 0..=HiddenRun::NEAREST {
            let id = format!("h{i}");
            run.box_cases.push((id.clone(), side("c", &[1.0], 0.0)));
            run.push(id, MarginSet::of(0.0));
        }
        assert_eq!(run.boxes.len(), HiddenRun::NEAREST);
        assert_eq!(run.box_cases.len(), HiddenRun::NEAREST);
        assert!(run.box_cases.iter().all(|(b, _)| b != "h0"));
    }
}
