//! Unique QML ids.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::qt_members::QT_MEMBERS;

#[derive(Debug, Default)]
pub(crate) struct Ids {
    used: HashSet<String>,
}

impl Ids {
    pub(crate) fn reserve(&mut self, id: &str) {
        self.used.insert(id.to_owned());
    }

    /// A unique id derived from `raw` (an HTML id, a class, a tag name):
    /// `card`, `card2`, `card3`, …. A name that is a property of a type the
    /// converter writes gets a `_`, as a reserved word does: read in that
    /// type's scope, the id would hide the property (`id: text` in a Text).
    pub(crate) fn claim(&mut self, raw: &str) -> String {
        let base = eldur_qml::sanitize_id(raw);
        // Only the bare name can shadow a property; `text2` is not one.
        let first = if is_member(&base) {
            format!("{base}_")
        } else {
            base.clone()
        };
        if self.used.insert(first.clone()) {
            return first;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}{n}");
            if !is_member(&candidate) && self.used.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Claims exactly `id` if it is free.
    pub(crate) fn try_exact(&mut self, id: &str) -> bool {
        !eldur_qml::is_reserved(id) && !is_member(id) && self.used.insert(id.to_owned())
    }
}

/// Whether `name` is a property of a Qt type or of a component the converter
/// writes.
fn is_member(name: &str) -> bool {
    static SHIPPED: OnceLock<HashSet<String>> = OnceLock::new();
    QT_MEMBERS.binary_search(&name).is_ok()
        || SHIPPED
            .get_or_init(super::components::property_names)
            .contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_members_are_sorted_for_binary_search() {
        assert!(QT_MEMBERS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn an_id_never_shadows_a_property() {
        let mut ids = Ids::default();
        assert_eq!(ids.claim("state"), "state_");
        assert_eq!(ids.claim("x-scale"), "xScale_");
        assert_eq!(ids.claim("text"), "text_");
        assert_eq!(ids.claim("text"), "text2");
        // EldurScrollInput's own property.
        assert_eq!(ids.claim("scroller"), "scroller_");
        assert_eq!(ids.claim("card"), "card");
        assert!(!ids.try_exact("width"));
    }
}
