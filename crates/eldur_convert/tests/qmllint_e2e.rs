//! Every corpus case's QML, the support files written beside `Main.qml`
//! included, passes Qt 6's `qmllint` without a word (AGENTS.md rule 6). The
//! same check as `mise run corpus && mise run qml:check`, without loading the
//! pages, so `mise run test` catches a converter change that breaks it.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use eldur_convert::{Input, Options, Resource, Resources};

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

#[test]
fn converted_corpus_passes_qt6_qmllint() {
    let Some(qmllint) = support::qmllint() else {
        eprintln!(
            "skipped: Qt 6's qmllint not found (set QMLLINT, or install Qt 6's declarative \
             tools, which bring qmltestrunner too)"
        );
        return;
    };
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("qmllint-corpus");
    if out_root.exists() {
        fs::remove_dir_all(&out_root).expect("clear the last run's output");
    }
    let mut cases: Vec<PathBuf> = fs::read_dir(&corpus)
        .expect("tests/corpus exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("index.html").is_file())
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no corpus cases found");
    let mut qml_files = Vec::new();
    for case in &cases {
        let html = fs::read_to_string(case.join("index.html")).expect("index.html is readable");
        let output = eldur_convert::convert(
            &Input {
                html,
                url: "index.html".to_owned(),
            },
            &Options::default(),
            &mut CaseFiles { dir: case.clone() },
        );
        let dir = out_root.join(case.file_name().expect("case directory has a name"));
        for file in &output.files {
            let path = dir.join(&file.path);
            fs::create_dir_all(path.parent().expect("a file has a directory")).expect("mkdir");
            fs::write(&path, &file.contents).expect("write output");
            if path.extension().is_some_and(|e| e == "qml") {
                qml_files.push(path);
            }
        }
    }
    // One run per case keeps each file's report next to its case and the
    // command lines short.
    let mut problems = Vec::new();
    for case in &cases {
        let dir = out_root.join(case.file_name().expect("case directory has a name"));
        let files: Vec<&PathBuf> = qml_files.iter().filter(|f| f.starts_with(&dir)).collect();
        let run = Command::new(&qmllint)
            .args(&files)
            .output()
            .expect("qmllint starts");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        if !run.status.success() || !text.trim().is_empty() {
            problems.push(text);
        }
    }
    assert!(
        problems.is_empty(),
        "qmllint reported on {} of {} cases:\n{}",
        problems.len(),
        cases.len(),
        problems.join("\n")
    );
}
