//! The QML components a page ships: those its `Main.qml`, its runtime or
//! another shipped component makes.

use std::path::PathBuf;

use crate::OutputFile;

/// A component and the files it is written as.
struct Component {
    name: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}

const COMPONENTS: &[Component] = &[
    Component {
        name: "EldurBackgroundLayer",
        files: &[(
            "EldurBackgroundLayer.qml",
            super::ELDUR_BACKGROUND_LAYER_QML.as_bytes(),
        )],
    },
    Component {
        name: "EldurBorder",
        files: &[("EldurBorder.qml", super::ELDUR_BORDER_QML.as_bytes())],
    },
    Component {
        name: "EldurDisclosure",
        files: &[(
            "EldurDisclosure.qml",
            super::ELDUR_DISCLOSURE_QML.as_bytes(),
        )],
    },
    Component {
        name: "EldurFilter",
        files: &[
            ("EldurFilter.qml", super::ELDUR_FILTER_QML.as_bytes()),
            ("eldur_filter.frag.qsb", super::ELDUR_FILTER_QSB),
        ],
    },
    Component {
        name: "EldurInlineBox",
        files: &[("EldurInlineBox.qml", super::ELDUR_INLINE_BOX_QML.as_bytes())],
    },
    Component {
        name: "EldurLines",
        files: &[("EldurLines.qml", super::ELDUR_LINES_QML.as_bytes())],
    },
    Component {
        name: "EldurRoundClip",
        files: &[("EldurRoundClip.qml", super::ELDUR_ROUND_CLIP_QML.as_bytes())],
    },
    Component {
        name: "EldurRun",
        files: &[("EldurRun.qml", super::ELDUR_RUN_QML.as_bytes())],
    },
    Component {
        name: "EldurScrollInput",
        files: &[(
            "EldurScrollInput.qml",
            super::ELDUR_SCROLL_INPUT_QML.as_bytes(),
        )],
    },
    Component {
        name: "EldurScroller",
        files: &[("EldurScroller.qml", super::ELDUR_SCROLLER_QML.as_bytes())],
    },
    Component {
        name: "EldurSticky",
        files: &[("EldurSticky.qml", super::ELDUR_STICKY_QML.as_bytes())],
    },
    Component {
        name: "EldurTextLines",
        files: &[("EldurTextLines.qml", super::ELDUR_TEXT_LINES_QML.as_bytes())],
    },
    Component {
        name: "EldurTextShadow",
        files: &[(
            "EldurTextShadow.qml",
            super::ELDUR_TEXT_SHADOW_QML.as_bytes(),
        )],
    },
];

/// Whether `text` makes `name`: as an object (`EldurLines {`) in QML or in a
/// string given to `Qt.createQmlObject`, or by file (`"EldurLines.qml"`) for
/// `Qt.createComponent`.
fn makes(text: &str, name: &str) -> bool {
    text.contains(&format!("{name} {{")) || text.contains(&format!("{name}.qml"))
}

/// The component files `main` and `runtime` (empty for a page without
/// scripts) make, with those the components themselves make, sorted by name.
pub(crate) fn components(main: &str, runtime: &str) -> Vec<OutputFile> {
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
            if makes(main, component.name) || makes(runtime, component.name) || by_shipped {
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
        let files = components("Item { EldurLines { EldurRun { } } }", "");
        assert_eq!(names(&files), ["EldurLines.qml", "EldurRun.qml"]);
    }

    #[test]
    fn a_scroller_brings_its_input() {
        let files = components("", "Qt.createComponent(\"EldurScroller.qml\")");
        assert_eq!(names(&files), ["EldurScrollInput.qml", "EldurScroller.qml"]);
    }

    #[test]
    fn a_filter_brings_its_shader() {
        let files = components(
            "",
            "Qt.createQmlObject(\"import QtQuick; EldurFilter { \" + x)",
        );
        assert_eq!(names(&files), ["EldurFilter.qml", "eldur_filter.frag.qsb"]);
    }

    #[test]
    fn nothing_made_is_nothing_shipped() {
        assert!(components("Item { Text { text: \"EldurLines\" } }", "").is_empty());
    }
}
