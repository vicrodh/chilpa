//! Unit-test constructors require an explicitly owned, disposable profile.
use std::path::{Path, PathBuf};

const ROOT: &str = "CHILPA_TEST_PROFILE_ROOT";
const MARKER: &str = ".chilpa-api-test-profile";
const CONTENT: &[u8] = b"chilpa-api-unit-tests-v1\n";
const REJECTED: &str = "API unit tests require an isolated profile; use scripts/run-api-tests-isolated.py";

/// I describe a child profile without changing the process-wide environment.
pub(crate) fn profile_environment(root: &Path) -> [(&'static str, PathBuf); 4] {
    let root = root.canonicalize().expect("test profile must already exist");
    std::fs::write(root.join(MARKER), CONTENT).expect("test profile marker must be writable");
    let [(config, config_path), (data, data_path), (cache, cache_path)] = chilpa_dirs::profile_environment(&root);
    [(config, config_path), (data, data_path), (cache, cache_path), (ROOT, root)]
}

pub(crate) fn assert_isolated_profile() {
    let root = std::env::var_os(ROOT).map(PathBuf::from).expect(REJECTED);
    assert!(root.is_absolute() && root.parent().is_some(), "{REJECTED}");
    assert!(root.canonicalize().is_ok_and(|path| path == root), "{REJECTED}");
    let marker = root.join(MARKER);
    assert!(std::fs::symlink_metadata(&marker).is_ok_and(|m| m.file_type().is_file()), "{REJECTED}");
    assert!(std::fs::read(&marker).is_ok_and(|bytes| bytes == CONTENT), "{REJECTED}");
    for (key, expected) in chilpa_dirs::profile_environment(&root) {
        assert!(std::env::var_os(key).is_some_and(|held| PathBuf::from(held) == expected), "{REJECTED}");
        // Existing profile directories must not redirect writes elsewhere.
        for path in [&expected, &expected.join("chilpa")] {
            match std::fs::symlink_metadata(path) {
                Ok(metadata) => assert!(metadata.is_dir() && !metadata.file_type().is_symlink()
                    && path.canonicalize().is_ok_and(|actual| actual == *path), "{REJECTED}"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(_) => panic!("{REJECTED}"),
            }
        }
    }
}

#[test]
fn untrusted_profiles_are_rejected_before_profile_io() {
    const CHILD: &str = "CHILPA_TEST_REJECT_PROFILE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let _api = crate::Chilpa::with_listed(Vec::new(), Vec::new());
        panic!("untrusted profile reached the constructor");
    }
    let faults = ["missing descriptor", "missing marker", "wrong marker", "mismatched directory",
        #[cfg(unix)] "redirected directory"];
    for fault in faults {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let environment = profile_environment(&root);
        let files = ["config/chilpa/settings.conf", "data/chilpa/listenlist.json", "data/chilpa/library.db", "cache/chilpa/source-catalog.json"];
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"profile sentinel").unwrap();
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "test_support::untrusted_profiles_are_rejected_before_profile_io", "--nocapture"])
            .envs(environment).env(CHILD, "1");
        match fault {
            "missing descriptor" => { child.env_remove(ROOT); },
            "missing marker" => std::fs::remove_file(root.join(MARKER)).unwrap(),
            "wrong marker" => std::fs::write(root.join(MARKER), b"not a test profile").unwrap(),
            #[cfg(unix)]
            "redirected directory" => {
                std::fs::rename(root.join("data"), root.join("held-data")).unwrap();
                std::os::unix::fs::symlink(root.join("held-data"), root.join("data")).unwrap();
            },
            _ => { child.env(chilpa_dirs::profile_environment(&root)[1].0, root.join("other-data")); },
        }
        let before = inventory(&root);
        let result = child.output().unwrap();
        assert!(!result.status.success(), "{fault} was accepted");
        assert!(String::from_utf8_lossy(&result.stdout).contains(REJECTED)
            || String::from_utf8_lossy(&result.stderr).contains(REJECTED), "profile guard did not reject {fault}");
        assert_eq!(inventory(&root), before, "rejected profile was changed: {fault}");
    }
}

type Inventory = std::collections::BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)>;
fn inventory(root: &Path) -> Inventory {
    fn visit(root: &Path, path: &Path, files: &mut Inventory) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { visit(root, &path, files); }
            else { files.insert(path.strip_prefix(root).unwrap().into(),
                (std::fs::read(&path).unwrap(), path.metadata().unwrap().modified().unwrap())); }
        }
    }
    let mut files = Default::default();
    visit(root, root, &mut files);
    files
}
