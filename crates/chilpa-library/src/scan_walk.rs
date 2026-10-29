//! Listing the audio files under a root, once.
//!
//! A scan used to walk its root twice: once to count, so the bar has a
//! total before it moves, and once more to read. Each walk asked the file
//! system about every entry (`is_dir` is a `stat`), so a tree of 30,000
//! files paid about 100,000 metadata calls before the first track was
//! written. The listing is now taken once and handed to the reader, and a
//! regular file or folder is told apart by what the directory entry already
//! says; only a link is looked through, as before.

use crate::scan::{Containers, ScanControl};
use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "aac", "mp3", "ogg", "oga", "dsf", "dff"];

/// Every audio file under a root in name order, and the subfolders that
/// could not be opened, links that did not resolve among them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub files: Vec<PathBuf>,
    pub blocked: Vec<PathBuf>,
    /// The root itself listed nothing at all, as an empty mount point does.
    pub root_empty: bool,
}

/// How often the walk says how many files it has found so far.
const FOUND_EVERY: usize = 500;

pub(crate) fn walk(root: &Path, containers: &dyn Containers) -> Listing {
    walk_controlled(root, containers, &ScanControl::default(), &|_| {})
}

/// The listing, reporting every `FOUND_EVERY` files through `on_found`. It
/// stops where `control` says, leaving the listing unsorted and partial.
pub(crate) fn walk_controlled(root: &Path, containers: &dyn Containers, control: &ScanControl, on_found: &dyn Fn(usize)) -> Listing {
    walk_with(root, containers, control, on_found, |dir| std::fs::read_dir(dir))
}

/// The walk, with each folder listed by `read`.
fn walk_with<I>(root: &Path, containers: &dyn Containers, control: &ScanControl, on_found: &dyn Fn(usize), read: impl Fn(&Path) -> std::io::Result<I>) -> Listing
where
    I: Iterator<Item = std::io::Result<std::fs::DirEntry>>,
{
    let mut listing = Listing::default();
    let mut pending = vec![root.to_path_buf()];
    let mut first = true;
    while let Some(dir) = pending.pop() {
        if control.stopped() { break; }
        let at_root = std::mem::replace(&mut first, false);
        let Ok(entries) = read(&dir) else {
            // An unreadable subfolder neither cancels the scan nor disables the
            // prune for the rest: in QBZ a lost+found disabled it forever. What
            // was known under it is left as it was, not taken for gone.
            listing.blocked.push(dir);
            continue;
        };
        let mut entries = entries.peekable();
        listing.root_empty |= at_root && entries.peek().is_none();
        for entry in entries {
            if control.stopped() { break; }
            // A listing that breaks off, as a share's does when it drops
            // mid-walk, is not the whole folder: it is reported like one that
            // could not be opened, so what did not turn up under it is not
            // taken for deleted.
            let Ok(entry) = entry else {
                listing.blocked.push(dir);
                break;
            };
            let path = entry.path();
            // The entry's own kind costs nothing: the listing carries it. A
            // link is followed, so a linked folder is walked and a link to a
            // file is the file, which is what `is_dir` did for every entry.
            let is_dir = match entry.file_type() {
                Ok(kind) if !kind.is_symlink() => kind.is_dir(),
                _ => match std::fs::metadata(&path) {
                    Ok(meta) => meta.is_dir(),
                    // A link that does not resolve may be a folder whose place
                    // stopped answering, whatever its name says, so what was
                    // known under it is kept. One named like a file of ours is
                    // listed too, and the reader says what became of it.
                    Err(_) => {
                        listing.blocked.push(path.clone());
                        if !(is_audio(&path) || containers.may_hold(&path)) { continue; }
                        false
                    }
                },
            };
            if is_dir {
                pending.push(path);
            } else if is_audio(&path) || containers.may_hold(&path) {
                listing.files.push(path);
                if listing.files.len() % FOUND_EVERY == 0 { on_found(listing.files.len()); }
            }
        }
    }
    if !control.stopped() { listing.files.sort(); }
    listing
}

pub(crate) fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::NoContainers;
    use std::cell::Cell;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let album = dir.path().join("artist").join("album");
        std::fs::create_dir_all(album.join("disc 2")).unwrap();
        std::fs::write(album.join("02.flac"), b"x").unwrap();
        std::fs::write(album.join("01.FLAC"), b"x").unwrap();
        std::fs::write(album.join("cover.jpg"), b"x").unwrap();
        std::fs::write(album.join("disc 2").join("01.mp3"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        dir
    }

    #[test]
    fn the_listing_holds_the_audio_files_in_name_order_and_nothing_else() {
        let dir = tree();
        let listing = walk(dir.path(), &NoContainers);
        // Compared by components, not as text: the separator is the platform's.
        let names: Vec<PathBuf> = listing.files.iter().map(|p| p.strip_prefix(dir.path()).unwrap().to_path_buf()).collect();
        let album = Path::new("artist").join("album");
        assert_eq!(names, [album.join("01.FLAC"), album.join("02.flac"), album.join("disc 2").join("01.mp3")]);
        assert!(listing.blocked.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_is_walked_and_a_link_to_a_file_is_listed_as_it_was() {
        let dir = tree();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("far.flac"), b"x").unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("linked")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path().join("far.flac"), dir.path().join("link.flac")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere.flac"), dir.path().join("broken.flac")).unwrap();
        let listing = walk(dir.path(), &NoContainers);
        let names: Vec<String> = listing.files.iter().map(|p| p.strip_prefix(dir.path()).unwrap().display().to_string()).collect();
        // The broken link is listed by its name: the reader is who reports
        // it unreadable, and a link that breaks after a scan must be noticed.
        assert_eq!(names, ["artist/album/01.FLAC", "artist/album/02.flac", "artist/album/disc 2/01.mp3", "broken.flac", "link.flac", "linked/far.flac"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_opened_is_reported_and_the_rest_is_listed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tree();
        let closed = dir.path().join("closed");
        std::fs::create_dir(&closed).unwrap();
        std::fs::write(closed.join("hidden.flac"), b"x").unwrap();
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000)).unwrap();
        let listing = walk(dir.path(), &NoContainers);
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o755)).unwrap();
        if listing.blocked.is_empty() {
            eprintln!("running as root: nothing is closed; skipped");
            return;
        }
        assert_eq!(listing.blocked, [closed]);
        assert_eq!(listing.files.len(), 3);
    }

    #[test]
    fn the_listing_says_when_the_root_itself_holds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(walk(dir.path(), &NoContainers).root_empty);
        std::fs::create_dir(dir.path().join("empty")).unwrap();
        assert!(!walk(dir.path(), &NoContainers).root_empty, "an empty subfolder is something");
    }

    #[test]
    fn a_folder_whose_listing_breaks_off_is_reported_like_one_that_cannot_be_opened() {
        let dir = tree();
        let album = dir.path().join("artist").join("album");
        std::fs::create_dir(dir.path().join("artist").join("other")).unwrap();
        std::fs::write(dir.path().join("artist").join("other").join("03.flac"), b"x").unwrap();
        // The album's listing gives one entry and then fails.
        let read = |path: &Path| -> std::io::Result<Box<dyn Iterator<Item = std::io::Result<std::fs::DirEntry>>>> {
            let entries = std::fs::read_dir(path)?;
            if path == album {
                return Ok(Box::new(entries.take(1).chain(std::iter::once(Err(std::io::Error::other("injected"))))));
            }
            Ok(Box::new(entries))
        };
        let listing = walk_with(dir.path(), &NoContainers, &ScanControl::default(), &|_| {}, read);
        assert_eq!(listing.blocked, [album], "the album is not known whole");
        assert!(listing.files.contains(&dir.path().join("artist").join("other").join("03.flac")), "the rest is listed: {listing:?}");
    }

    #[test]
    fn the_walk_reports_how_many_it_has_found_and_stops_where_it_is_told() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..(FOUND_EVERY * 2 + 10) {
            std::fs::write(dir.path().join(format!("{n:05}.flac")), b"x").unwrap();
        }
        let reported = Cell::new(Vec::new());
        let listing = walk_controlled(dir.path(), &NoContainers, &ScanControl::default(), &|found| {
            let mut all = reported.take(); all.push(found); reported.set(all);
        });
        assert_eq!(listing.files.len(), FOUND_EVERY * 2 + 10);
        assert_eq!(reported.take(), [FOUND_EVERY, FOUND_EVERY * 2]);

        let control = ScanControl::default();
        let stopped = walk_controlled(dir.path(), &NoContainers, &control, &|found| { if found == FOUND_EVERY { control.stop(); } });
        assert!(stopped.files.len() < FOUND_EVERY * 2 + 10, "the walk stopped early");
    }
}
