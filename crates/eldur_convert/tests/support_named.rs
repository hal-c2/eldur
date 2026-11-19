//! How the end-to-end tests find the tool a QMLTESTRUNNER or QMLLINT names:
//! the way setup does, so setup and `mise run test` agree on one tool.

mod support;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

/// A fresh directory holding an empty file named `name`.
fn dir_with(case: &str, name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("eldur-support-named-{case}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh temp dir");
    fs::write(dir.join(name), "").expect("the tool file");
    dir
}

#[test]
fn a_bare_name_is_found_through_path() {
    let dir = dir_with("bare", "qmltestrunner-standin");
    let found = support::named(OsStr::new("qmltestrunner-standin"), Some(dir.as_os_str()));
    assert_eq!(found, dir.join("qmltestrunner-standin"));
    assert!(found.is_file());
    fs::remove_dir_all(&dir).expect("the temp dir removed");
}

#[test]
fn a_bare_name_path_lacks_stays_as_written() {
    let dir = dir_with("lacks", "other");
    let found = support::named(OsStr::new("qmltestrunner-typo"), Some(dir.as_os_str()));
    assert_eq!(
        found,
        std::path::absolute("qmltestrunner-typo").expect("the working directory")
    );
    assert!(!found.is_file());
    fs::remove_dir_all(&dir).expect("the temp dir removed");
}

#[test]
fn a_path_is_taken_as_written() {
    let dir = dir_with("path", "qmltestrunner");
    let written = dir.join("qmltestrunner");
    let found = support::named(written.as_os_str(), Some(OsStr::new("/nonexistent")));
    assert_eq!(found, written);
    fs::remove_dir_all(&dir).expect("the temp dir removed");
}
