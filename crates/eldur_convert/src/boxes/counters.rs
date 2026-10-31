//! CSS counters (CSS Lists 3 §4), numbered in document order as the box
//! tree is built: the page is static, so every value is known.

use eldur_css::{ComputedStyle, CounterChange, Display, ListStyleType};

/// The counters in scope, one frame per open element: a counter lives in
/// the frame of the element that created it, so that it reaches that
/// element's following siblings and their descendants.
#[derive(Clone)]
pub(super) struct Counters {
    frames: Vec<Vec<(String, i64)>>,
}

impl Counters {
    pub(super) fn new() -> Self {
        Self {
            frames: vec![Vec::new()],
        }
    }

    /// Entering an element's children.
    pub(super) fn push(&mut self) {
        self.frames.push(Vec::new());
    }

    pub(super) fn pop(&mut self) {
        self.frames.pop();
    }

    /// The element's `counter-reset`, then `counter-increment`, then
    /// `counter-set`: the order Chromium applies them in.
    pub(super) fn apply(&mut self, style: &ComputedStyle) {
        for CounterChange { name, value } in &style.counter_reset {
            let frame = self.frames.last_mut().expect("a frame");
            match frame.iter_mut().find(|(n, _)| n == name) {
                Some(c) => c.1 = i64::from(*value),
                None => frame.push((name.clone(), i64::from(*value))),
            }
        }
        let increments_list_item = style
            .counter_increment
            .iter()
            .any(|c| c.name == "list-item");
        if style.display == Display::ListItem && !increments_list_item {
            // UA: a list item increments `list-item`.
            *self.innermost("list-item") += 1;
        }
        for CounterChange { name, value } in &style.counter_increment {
            *self.innermost(name) += i64::from(*value);
        }
        for CounterChange { name, value } in &style.counter_set {
            *self.innermost(name) = i64::from(*value);
        }
    }

    /// `counter(name)`: 0 when no counter of the name is in scope.
    pub(super) fn value(&self, name: &str) -> i64 {
        self.all(name).last().copied().unwrap_or(0)
    }

    /// `counters(name)`: outermost first.
    pub(super) fn all(&self, name: &str) -> Vec<i64> {
        self.frames
            .iter()
            .flat_map(|f| f.iter().filter(|(n, _)| n == name).map(|(_, v)| *v))
            .collect()
    }

    /// The innermost counter of `name`, created at 0 in the current frame
    /// when none is in scope.
    fn innermost(&mut self, name: &str) -> &mut i64 {
        let at = self
            .frames
            .iter()
            .rposition(|f| f.iter().any(|(n, _)| n == name));
        let frame = match at {
            Some(i) => &mut self.frames[i],
            None => {
                let frame = self.frames.last_mut().expect("a frame");
                frame.push((name.to_owned(), 0));
                frame
            }
        };
        &mut frame
            .iter_mut()
            .find(|(n, _)| n == name)
            .expect("the counter")
            .1
    }
}

/// A counter value in a list style; the bullets are their symbols.
pub(super) fn format(n: i64, style: ListStyleType) -> String {
    match style {
        ListStyleType::None => String::new(),
        ListStyleType::Disc => "\u{2022}".to_owned(),
        ListStyleType::Circle => "\u{25e6}".to_owned(),
        ListStyleType::Square => "\u{25aa}".to_owned(),
        ListStyleType::Decimal => n.to_string(),
        ListStyleType::DecimalLeadingZero => {
            let sign = if n < 0 { "-" } else { "" };
            format!("{sign}{:02}", n.abs())
        }
        ListStyleType::LowerAlpha => super::alpha(n).to_lowercase(),
        ListStyleType::UpperAlpha => super::alpha(n),
        ListStyleType::LowerRoman => super::roman(n).to_lowercase(),
        ListStyleType::UpperRoman => super::roman(n),
    }
}
