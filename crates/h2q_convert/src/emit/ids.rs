//! Unique QML ids.

use std::collections::HashSet;

#[derive(Debug, Default)]
pub(crate) struct Ids {
    used: HashSet<String>,
}

impl Ids {
    pub(crate) fn reserve(&mut self, id: &str) {
        self.used.insert(id.to_owned());
    }

    /// A unique id derived from `raw` (an HTML id, a class, a tag name):
    /// `card`, `card2`, `card3`, ….
    pub(crate) fn claim(&mut self, raw: &str) -> String {
        let base = h2q_qml::sanitize_id(raw);
        if self.used.insert(base.clone()) {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}{n}");
            if self.used.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Claims exactly `id` if it is free.
    pub(crate) fn try_exact(&mut self, id: &str) -> bool {
        !h2q_qml::is_reserved(id) && self.used.insert(id.to_owned())
    }
}
