//! How the end-to-end tests find the tool a QMLTESTRUNNER or QMLLINT names:
//! the way setup does, so setup and `mise run test` agree on one tool.

mod support;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

/// A fresh directory holding an empty file named `name`, which can run.
fn dir_with(case: &str, name: &str) -> PathBuf {
    let dir = plain_dir_with(case, name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o755))
            .expect("the tool made runnable");
    }
    dir
}

/// A fresh directory holding an empty file named `name`, with no mode bits
/// added (a file that cannot run, on Unix).
fn plain_dir_with(case: &str, name: &str) -> PathBuf {
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

#[cfg(unix)]
#[test]
fn a_file_that_cannot_run_is_passed_over_for_one_further_down_path() {
    let first = plain_dir_with("cannot-run", "qmltestrunner-standin");
    let second = dir_with("runs", "qmltestrunner-standin");
    let search = std::env::join_paths([&first, &second]).expect("a PATH");
    let found = support::named(OsStr::new("qmltestrunner-standin"), Some(&search));
    assert_eq!(found, second.join("qmltestrunner-standin"));
    fs::remove_dir_all(&first).expect("the temp dir removed");
    fs::remove_dir_all(&second).expect("the temp dir removed");
}
