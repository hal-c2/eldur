//! The QML components a page ships: those its `Main.qml`, its runtime or
//! another shipped component makes.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::OutputFile;

/// A component and the files it is written as.
struct Component {
    name: &'static str,
    files: &'static [(&'static str, &'static [u8])],
    /// The element the runtime makes this component for, when it makes it
    /// for one only: the runtime's text holds every component it can make, so
    /// a page that can never have that element is spared the file.
    only_for: Option<&'static str>,
}

const COMPONENTS: &[Component] = &[
    Component {
        name: "EldurBackgroundLayer",
        files: &[(
            "EldurBackgroundLayer.qml",
            super::ELDUR_BACKGROUND_LAYER_QML.as_bytes(),
        )],
        only_for: None,
    },
    Component {
        name: "EldurBorder",
        files: &[("EldurBorder.qml", super::ELDUR_BORDER_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurDisclosure",
        files: &[(
            "EldurDisclosure.qml",
            super::ELDUR_DISCLOSURE_QML.as_bytes(),
        )],
        only_for: Some("summary"),
    },
    Component {
        name: "EldurFilter",
        files: &[
            ("EldurFilter.qml", super::ELDUR_FILTER_QML.as_bytes()),
            ("eldur_filter.frag.qsb", super::ELDUR_FILTER_QSB),
        ],
        only_for: None,
    },
    Component {
        name: "EldurFlow",
        files: &[("EldurFlow.qml", super::flex::ELDUR_FLOW_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurInlineBox",
        files: &[("EldurInlineBox.qml", super::ELDUR_INLINE_BOX_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurLines",
        files: &[("EldurLines.qml", super::ELDUR_LINES_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurRoundClip",
        files: &[("EldurRoundClip.qml", super::ELDUR_ROUND_CLIP_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurRun",
        files: &[("EldurRun.qml", super::ELDUR_RUN_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurScrollInput",
        files: &[(
            "EldurScrollInput.qml",
            super::ELDUR_SCROLL_INPUT_QML.as_bytes(),
        )],
        only_for: None,
    },
    Component {
        name: "EldurScroller",
        files: &[("EldurScroller.qml", super::ELDUR_SCROLLER_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurSticky",
        files: &[("EldurSticky.qml", super::ELDUR_STICKY_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurTextLines",
        files: &[("EldurTextLines.qml", super::ELDUR_TEXT_LINES_QML.as_bytes())],
        only_for: None,
    },
    Component {
        name: "EldurTextShadow",
        files: &[(
            "EldurTextShadow.qml",
            super::ELDUR_TEXT_SHADOW_QML.as_bytes(),
        )],
        only_for: None,
    },
];

/// The names of the properties the shipped components declare, which an id
/// read inside one of them would hide.
pub(crate) fn property_names() -> HashSet<String> {
    let mut names = HashSet::new();
    for (path, bytes) in COMPONENTS.iter().flat_map(|c| c.files) {
        if !path.ends_with(".qml") {
            continue;
        }
        for line in String::from_utf8_lossy(bytes).lines() {
            let mut words = line
                .split_whitespace()
                .skip_while(|w| matches!(*w, "readonly" | "required" | "default" | "final"));
            if words.next() == Some("property") {
                // `property <type> <name>`, maybe followed by `:`.
                if let Some(name) = words.nth(1) {
                    names.insert(name.trim_end_matches(':').to_owned());
                }
            }
        }
    }
    names
}

/// Whether `text` makes `name`: as an object (`EldurLines {`) in QML or in a
/// string given to `Qt.createQmlObject`, or by file (`"EldurLines.qml"`) for
/// `Qt.createComponent`.
fn makes(text: &str, name: &str) -> bool {
    text.contains(&format!("{name} {{")) || text.contains(&format!("{name}.qml"))
}

/// The component files `main` and `runtime` (empty for a page without
/// scripts) make, with those the components themselves make, sorted by name.
/// `can_make` says whether the page can come to hold an element of a tag,
/// which decides the components the runtime makes only for that element.
pub(crate) fn components(
    main: &str,
    runtime: &str,
    can_make: impl Fn(&str) -> bool,
) -> Vec<OutputFile> {
    let mut chosen = vec![false; COMPONENTS.len()];
    let mut grew = true;
    while grew {
        grew = false;
        for (at, component) in COMPONENTS.iter().enumerate() {
            if chosen[at] {
                continue;
            }
            let by_shipped = COMPONENTS.iter().zip(&chosen).any(|(c, &on)| {
                on && c.files.iter().any(|(path, bytes)| {
                    path.ends_with(".qml") && makes(&String::from_utf8_lossy(bytes), component.name)
                })
            });
            let by_runtime =
                makes(runtime, component.name) && component.only_for.is_none_or(&can_make);
            if makes(main, component.name) || by_runtime || by_shipped {
                chosen[at] = true;
                grew = true;
            }
        }
    }
    COMPONENTS
        .iter()
        .zip(chosen)
        .filter(|(_, on)| *on)
        .flat_map(|(c, _)| c.files)
        .map(|(path, bytes)| OutputFile {
            path: PathBuf::from(path),
            contents: bytes.to_vec(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(files: &[OutputFile]) -> Vec<String> {
        files.iter().map(|f| f.path.display().to_string()).collect()
    }

    #[test]
    fn a_page_gets_the_components_it_makes() {
        let files = components("Item { EldurLines { EldurRun { } } }", "", |_| true);
        assert_eq!(names(&files), ["EldurLines.qml", "EldurRun.qml"]);
    }

    #[test]
    fn a_scroller_brings_its_input() {
        let files = components("", "Qt.createComponent(\"EldurScroller.qml\")", |_| true);
        assert_eq!(names(&files), ["EldurScrollInput.qml", "EldurScroller.qml"]);
    }

    #[test]
    fn a_component_made_for_one_element_waits_for_a_page_that_can_have_it() {
        let runtime = "Qt.createQmlObject(\"import QtQuick; EldurDisclosure { }\")";
        assert!(components("", runtime, |_| false).is_empty());
        let files = components("", runtime, |tag| tag == "summary");
        assert_eq!(names(&files), ["EldurDisclosure.qml"]);
        let files = components("Item { EldurDisclosure { } }", runtime, |_| false);
        assert_eq!(names(&files), ["EldurDisclosure.qml"]);
    }

    #[test]
    fn a_filter_brings_its_shader() {
        let files = components(
            "",
            "Qt.createQmlObject(\"import QtQuick; EldurFilter { \" + x)",
            |_| true,
        );
        assert_eq!(names(&files), ["EldurFilter.qml", "eldur_filter.frag.qsb"]);
    }

    #[test]
    fn nothing_made_is_nothing_shipped() {
        assert!(components("Item { Text { text: \"EldurLines\" } }", "", |_| true).is_empty());
    }
}
