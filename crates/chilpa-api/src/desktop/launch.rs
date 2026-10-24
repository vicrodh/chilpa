//! What a launch is asked to open: the arguments a file manager passes.
//!
//! "Open with Chilpa" hands over `%U`: paths or `file://` addresses, one per
//! item selected. Anything that is not an existing file or folder is
//! dropped, so a stray flag or a file that was moved never reaches the
//! window. Paths are made absolute here, because a second launch may run
//! from another directory than the Chilpa it hands them to.

use std::ffi::OsString;
use std::path::Path;

/// The existing paths among `args`, absolute, in the order given.
pub fn paths_from(args: impl IntoIterator<Item = OsString>) -> Vec<String> {
    let here = std::env::current_dir().unwrap_or_default();
    args.into_iter()
        .filter_map(|arg| {
            let text = arg.to_string_lossy().into_owned();
            if text.starts_with('-') {
                return None;
            }
            let path = crate::paths::path_of(&text);
            let path = here.join(Path::new(&path));
            path.exists().then(|| path.to_string_lossy().into_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::paths_from;
    use std::ffi::OsString;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn addresses_and_paths_become_the_same_existing_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Track [24-96].flac");
        std::fs::write(&file, b"x").unwrap();
        let address = format!("file://{}/Track%20%5B24-96%5D.flac", dir.path().display());
        let found = paths_from(args(&[&address, file.to_str().unwrap()]));
        assert_eq!(found, [file.to_string_lossy(), file.to_string_lossy()]);
    }

    #[test]
    fn flags_and_paths_that_do_not_exist_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.flac");
        let found = paths_from(args(&["--hidden", missing.to_str().unwrap(), dir.path().to_str().unwrap()]));
        assert_eq!(found, [dir.path().to_string_lossy()]);
    }

    #[test]
    fn a_relative_path_is_taken_from_where_the_launch_runs() {
        let here = std::env::current_dir().unwrap();
        let found = paths_from(args(&["Cargo.toml"]));
        assert_eq!(found, [here.join("Cargo.toml").to_string_lossy()]);
    }
}
