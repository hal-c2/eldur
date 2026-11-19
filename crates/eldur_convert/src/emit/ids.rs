//! Unique QML ids.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use super::qt_members::QT_MEMBERS;
#[cfg(test)]
use super::qt_members::QT_TYPES;

#[derive(Debug, Default)]
pub(crate) struct Ids {
    used: HashSet<String>,
    /// The number after the last one `claim` gave each base: the ones below
    /// are taken for good, as ids are never given back, so a page with a
    /// thousand `p`s does not try `p2` to `p999` for the thousandth.
    next: HashMap<String, usize>,
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
        let mut n = self.next.get(&base).copied().unwrap_or(2);
        loop {
            let candidate = format!("{base}{n}");
            n += 1;
            if !is_member(&candidate) && self.used.insert(candidate.clone()) {
                self.next.insert(base, n);
                return candidate;
            }
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
        assert!(QT_TYPES.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn every_qt_type_the_emitters_write_is_in_the_member_list() {
        // A type left out of tools/qt-members/generate.py would let an id
        // take one of its property names. Every string literal in the
        // emitters that names a Qt type counts, as a type can reach
        // `Object::new` through a variable (`Object::new(kind)`).
        let generator = include_str!("../../../../tools/qt-members/generate.py");
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut dirs = vec![src];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).expect("src").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("qt_members.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("source");
                // Every run between two quotes, not only every other one: an
                // escaped quote or a char literal would put the pairs out of step.
                for literal in text.split('"') {
                    // `Page` is page.js's import alias, not the Controls type.
                    if literal != "Page"
                        && QT_TYPES.binary_search(&literal).is_ok()
                        && !generator.contains(&format!("\"{literal}\""))
                    {
                        missing.push(literal.to_owned());
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "add to generate.py's TYPES and regenerate: {missing:?}"
        );
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

    #[test]
    fn a_number_taken_another_way_is_skipped_however_the_ids_go() {
        let mut ids = Ids::default();
        ids.reserve("p3");
        assert_eq!(ids.claim("p"), "p");
        assert_eq!(ids.claim("p"), "p2");
        assert!(ids.try_exact("p5"));
        assert_eq!(ids.claim("p"), "p4");
        assert_eq!(ids.claim("p"), "p6");
        assert_eq!(ids.claim("p1"), "p1");
        assert_eq!(ids.claim("p"), "p7");
    }
}
