//! Converts every `tests/corpus/*/index.html` and snapshots its `Main.qml`,
//! its diagnostics and the list of files it writes. `mise run corpus` writes the same output to
//! `target/corpus/` for `mise run qml:check target/corpus`.

use std::fs;
use std::path::{Path, PathBuf};

use eldur_convert::{Input, Options, Resource, Resources};

/// Files next to the page, named relative to the case directory so the
/// snapshots do not depend on where the checkout is.
struct CaseFiles {
    dir: PathBuf,
}

impl Resources for CaseFiles {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        if url.contains("://") || url.starts_with("data:") {
            return None;
        }
        let base_dir = Path::new(base).parent().unwrap_or(Path::new(""));
        let relative = base_dir.join(url);
        let bytes = fs::read(self.dir.join(&relative)).ok()?;
        Some(Resource {
            resolved: relative.to_string_lossy().into_owned(),
            bytes,
        })
    }
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

#[test]
fn corpus_main_qml() {
    let mut cases: Vec<PathBuf> = fs::read_dir(corpus_dir())
        .expect("tests/corpus exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("index.html").is_file())
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no corpus cases found");
    for case in cases {
        let name = case
            .file_name()
            .expect("case directory has a name")
            .to_string_lossy()
            .into_owned();
        let html = fs::read_to_string(case.join("index.html")).expect("index.html is readable");
        let output = eldur_convert::convert(
            &Input {
                html,
                url: "index.html".to_owned(),
            },
            &Options::default(),
            &mut CaseFiles { dir: case.clone() },
        );
        let qml = output.main_qml().expect("Main.qml is always produced");
        insta::assert_snapshot!(format!("{name}-Main.qml"), qml);
        // A page gets the files it uses and no others.
        insta::assert_snapshot!(format!("{name}-files"), file_list(&output));
        let diagnostics: Vec<String> = output.diagnostics.iter().map(ToString::to_string).collect();
        insta::assert_snapshot!(format!("{name}-diagnostics"), diagnostics.join("\n"));
    }
}

fn file_list(output: &eldur_convert::Output) -> String {
    let mut paths: Vec<String> = output
        .files
        .iter()
        .map(|f| f.path.display().to_string())
        .collect();
    paths.sort();
    paths.join("\n")
}

/// The minimal output, by hand: a page gets its QML, the font module and the
/// components its QML makes; a script adds itself, the runtime and the
/// components the runtime makes for what the page can do.
#[test]
fn the_files_a_page_needs() {
    for (case, expected) in [
        (
            "74-tiny",
            &["EldurScrollInput.qml", "Main.qml", "eldur_fonts.js"][..],
        ),
        (
            "19-article",
            &[
                "EldurInlineBox.qml",
                "EldurLines.qml",
                "EldurRun.qml",
                "EldurScrollInput.qml",
                "Main.qml",
                "eldur_fonts.js",
            ][..],
        ),
        (
            // A script can restyle any element, so the runtime's layout reaches
            // every component it makes; only what nothing makes stays out.
            "38-app-todo",
            &[
                "EldurBackgroundLayer.qml",
                "EldurBorder.qml",
                "EldurFilter.qml",
                "EldurInlineBox.qml",
                "EldurLines.qml",
                "EldurRoundClip.qml",
                "EldurRun.qml",
                "EldurScrollInput.qml",
                "EldurScroller.qml",
                "EldurSticky.qml",
                "EldurTextLines.qml",
                "Main.qml",
                "eldur_filter.frag.qsb",
                "eldur_fonts.js",
                "eldur_runtime.js",
                "page.js",
            ][..],
        ),
    ] {
        let dir = corpus_dir().join(case);
        let html = fs::read_to_string(dir.join("index.html")).expect("index.html is readable");
        let output = eldur_convert::convert(
            &Input {
                html,
                url: "index.html".to_owned(),
            },
            &Options::default(),
            &mut CaseFiles { dir },
        );
        assert_eq!(file_list(&output), expected.join("\n"), "{case}");
    }
}

/// The runtime marks a summary a script makes with `EldurDisclosure`, so the
/// file ships with a page whose scripts can make one, however they spell the
/// tag, and not with one that only names an id `summary`.
#[test]
fn a_page_that_can_make_a_summary_ships_its_disclosure() {
    let label = "<p id=\"box\"></p><script>const label = 'Made';";
    for (case, script, ships) in [
        (
            "created",
            "document.getElementById('box').append(document.createElement('SUMMARY'));",
            true,
        ),
        (
            "opaque",
            "document.getElementById('box').innerHTML = ['<details><summary>', label, '</summary></details>'].join('');",
            true,
        ),
        (
            "opaque upper",
            "document.getElementById('box').innerHTML = ['<DETAILS><SUMMARY>', label, '</SUMMARY></DETAILS>'].join('');",
            true,
        ),
        (
            "an id",
            "document.getElementById('box').innerHTML = label; document.getElementById('summary');",
            false,
        ),
    ] {
        let html = format!("{label}{script}</script><p id=\"summary\">-</p>");
        let output = eldur_convert::convert_str(&html, &Options::default());
        let files = file_list(&output);
        assert!(
            files.contains("eldur_runtime.js"),
            "{case}: no runtime in\n{files}"
        );
        assert_eq!(
            files.contains("EldurDisclosure.qml"),
            ships,
            "{case}:\n{files}"
        );
    }
}

#[test]
fn page_without_body_or_scripts_converts() {
    for html in [
        "",
        "<p>hi",
        "<html style=\"display:none\">",
        "<body><script>var x = 1;</script>",
    ] {
        let output = eldur_convert::convert_str(html, &Options::default());
        let qml = output.main_qml().expect("Main.qml is always produced");
        assert!(qml.contains("ApplicationWindow"), "{html:?}:\n{qml}");
    }
}
