//! Which commit the About window's Build row names, decided apart from the
//! build script so the library's tests reach it. Standard library only:
//! the build script includes this file as it is.
//!
//! Git is asked first. A build without git, or from a tree that is not its
//! own work tree (a source archive unpacked inside some other repository,
//! or a sandbox that copied the sources without `.git`), takes the commit
//! from `CHILPA_BUILD_COMMIT` instead, and with neither the row names none.

use std::path::Path;

/// The variable a packager sets when the tree carries no git metadata.
pub const COMMIT_VARIABLE: &str = "CHILPA_BUILD_COMMIT";

/// How many hex digits of a commit the row shows, as `git rev-parse
/// --short=9` gives them.
pub const SHORT: usize = 9;

/// The longest value taken from the variable.
const LONGEST: usize = 64;

/// The commit the row names.
///
/// `workspace` is the root of the workspace being built; `work_tree` is
/// where git says the work tree starts, and `head` the short commit git
/// gave, each `None` when git did not answer. Git's commit counts only when
/// its work tree is this workspace: anything else is another repository's.
pub fn commit(workspace: &Path, work_tree: Option<&Path>, head: Option<&str>, variable: Option<&str>) -> String {
    let own_tree = work_tree.is_some_and(|work_tree| same_directory(work_tree, workspace));
    if let (true, Some(head)) = (own_tree, head.map(str::trim).filter(|head| !head.is_empty())) {
        return head.to_string();
    }
    variable.map(from_variable).unwrap_or_default()
}

/// A value set by hand, kept only when it reads as a commit or a version
/// tag: letters, digits and `.-_+`, on one line. A full hex commit is cut to
/// the length git's path gives, so both name a build the same way.
fn from_variable(value: &str) -> String {
    let value = value.trim();
    let fits = !value.is_empty()
        && value.len() <= LONGEST
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b));
    if !fits {
        return String::new();
    }
    if value.len() > SHORT && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return value[..SHORT].to_ascii_lowercase();
    }
    value.to_string()
}

fn same_directory(one: &Path, other: &Path) -> bool {
    match (one.canonicalize(), other.canonicalize()) {
        (Ok(one), Ok(other)) => one == other,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that exists, and one above it: this crate and its parent.
    fn here() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn git_names_the_commit_when_the_tree_is_its_own() {
        let named = commit(here(), Some(here()), Some("abcdef123\n"), Some("0123456789abcdef"));
        assert_eq!(named, "abcdef123", "git is the default, and the variable does not override it");
    }

    #[test]
    fn without_git_the_variable_names_the_commit() {
        assert_eq!(commit(here(), None, None, Some("abcdef123")), "abcdef123");
        let full = "0123456789ABCDEF0123456789abcdef01234567";
        assert_eq!(commit(here(), None, None, Some(full)), "012345678", "a full commit is cut as git cuts it");
        assert_eq!(commit(here(), None, None, Some("v0.1.3")), "v0.1.3", "a tag is kept as written");
    }

    #[test]
    fn a_commit_of_another_repository_is_not_this_one() {
        // A source archive unpacked inside some other git checkout: git
        // answers, about that other checkout.
        let outer = here().parent().unwrap();
        assert_eq!(commit(here(), Some(outer), Some("fedcba987"), Some("abcdef123")), "abcdef123");
        assert_eq!(commit(here(), Some(outer), Some("fedcba987"), None), "", "with nothing else, no commit");
        let missing = here().join("no-such-directory");
        assert_eq!(commit(&missing, Some(&missing), Some("fedcba987"), None), "", "a path that does not exist matches nothing");
    }

    #[test]
    fn a_value_that_is_not_a_commit_is_left_out() {
        let long = "a".repeat(LONGEST + 1);
        for value in ["", "   ", "abc def", "abc\ndef", "a\"b", long.as_str()] {
            assert_eq!(commit(here(), None, None, Some(value)), "", "{value:?}");
        }
        assert_eq!(commit(here(), None, None, None), "");
    }

    #[test]
    fn a_git_that_answers_nothing_falls_to_the_variable() {
        assert_eq!(commit(here(), Some(here()), Some("  "), Some("abcdef123")), "abcdef123");
        assert_eq!(commit(here(), Some(here()), None, Some("abcdef123")), "abcdef123");
    }

    #[test]
    fn the_variable_keeps_the_name_packagers_set() {
        // The Flatpak build notes and any package recipe name it; renaming it
        // silently empties their Build row.
        assert_eq!(COMMIT_VARIABLE, "CHILPA_BUILD_COMMIT");
    }
}
