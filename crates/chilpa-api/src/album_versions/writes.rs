//! The Library's grouping choices, kept away from the window. They live in
//! the library, which a server's sync or a scan may be writing: kept on the
//! window's thread, a switch waited out the library's 5 s busy timeout, the
//! window froze that long, and a choice that still found the library busy
//! was lost without a word. Here they go to a thread of their own, in the
//! order asked. Until one is kept the facade answers with it; once kept the
//! library is read again, and one that could not be kept is said, and the
//! switches show the library as it is.

use crate::Chilpa;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

/// A grouping choice of the Library screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Editions of a record as one card.
    Versions(bool),
    /// The same on an artist's page.
    ArtistVersions(bool),
    /// A record kept apart from the other editions of its name, or not.
    Apart(i64, bool),
}

impl Change {
    /// Writes it; true when something changed.
    fn apply(self, lib: &chilpa_library::Library) -> Result<bool, Refused> {
        let written = match self {
            Change::Versions(on) => lib.set_group_album_versions(on).map(|()| true),
            Change::ArtistVersions(on) => lib.set_group_artist_versions(on).map(|()| true),
            Change::Apart(album, apart) => lib.set_album_apart(album, apart),
        };
        written.map_err(|error| Refused { busy: chilpa_library::is_busy(&error), why: error.to_string() })
    }
}

/// Why the library did not take a change, and whether it was only busy.
#[derive(Debug)]
struct Refused {
    busy: bool,
    why: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

type Kept = (Change, Result<bool, String>);

/// The thread that keeps them, and what was asked and is not kept yet.
#[derive(Default)]
pub(crate) struct Writes {
    jobs: Option<Sender<Change>>,
    kept: Option<Receiver<Kept>>,
    /// Oldest first: what the facade answers until each is kept.
    pending: Vec<Change>,
}

impl Writes {
    fn ask(&mut self, path: &Path, change: Change) -> bool {
        if self.jobs.is_none() {
            let Some((jobs, kept)) = start(path.to_path_buf()) else { return false };
            (self.jobs, self.kept) = (Some(jobs), Some(kept));
        }
        if self.jobs.as_ref().is_some_and(|jobs| jobs.send(change).is_ok()) {
            self.pending.push(change);
            return true;
        }
        false
    }

    /// The newest choice asked and not kept yet that `pick` reads.
    pub(crate) fn latest<T>(&self, pick: impl Fn(Change) -> Option<T>) -> Option<T> {
        self.pending.iter().rev().find_map(|change| pick(*change))
    }
}

fn start(path: PathBuf) -> Option<(Sender<Change>, Receiver<Kept>)> {
    let (jobs, inbox) = channel::<Change>();
    let (outbox, kept) = channel::<Kept>();
    std::thread::Builder::new()
        .name("chilpa-grouping".into())
        .spawn(move || {
            let mut lib = None;
            for change in inbox {
                if lib.is_none() {
                    lib = chilpa_library::Library::open_existing(&path).map_err(|e| eprintln!("chilpa-library: grouping writer: {e}")).ok();
                }
                let result = match &lib {
                    Some(lib) => crate::servers::background::retried("keeping a grouping choice", &crate::servers::background::WAITS,
                        |refused: &Refused| refused.busy, || change.apply(lib)).map_err(|refused| said(&refused)),
                    None => Err(chilpa_i18n::t_args("This choice could not be kept: {}", &[&chilpa_i18n::t("the library could not be written")])),
                };
                if outbox.send((change, result)).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some((jobs, kept))
}

fn said(refused: &Refused) -> String {
    if refused.busy {
        chilpa_i18n::t("The library stayed busy with another task, so this choice was not kept. Try again in a moment.")
    } else {
        chilpa_i18n::t_args("This choice could not be kept: {}", &[&refused.why])
    }
}

impl Chilpa {
    /// Keeps a grouping choice: in place for a library without a file
    /// (tests), which has no other writer, and otherwise on the thread. True
    /// when it was asked; whether it changed anything is known when kept.
    pub(crate) fn keep_grouping(&mut self, change: Change) -> bool {
        let Some(lib) = self.library() else { return false };
        let Some(path) = lib.connection().path().filter(|path| !path.is_empty()).map(PathBuf::from) else {
            return match change.apply(lib) {
                Ok(changed) => changed,
                Err(refused) => {
                    self.now.error = said(&refused);
                    false
                }
            };
        };
        self.grouping.ask(&path, change)
    }

    /// What the thread kept since the last poll. Anything kept, or refused,
    /// has the window read the library again; a refusal is said once.
    pub(crate) fn poll_grouping(&mut self) {
        let Some(kept) = self.grouping.kept.as_ref() else { return };
        let (mut landed, mut gone) = (Vec::new(), false);
        loop {
            match kept.try_recv() {
                Ok(one) => landed.push(one),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    gone = true;
                    break;
                }
            }
        }
        let mut read_again = gone;
        for (change, result) in landed {
            if let Some(at) = self.grouping.pending.iter().position(|asked| *asked == change) {
                self.grouping.pending.remove(at);
            }
            match result {
                Ok(changed) => read_again |= changed,
                Err(why) => {
                    self.now.error = why;
                    read_again = true;
                }
            }
        }
        if gone {
            self.grouping = Writes::default();
        }
        if read_again {
            self.mark_library_changed();
        }
    }

    /// Whether every grouping choice asked was kept or refused.
    #[cfg(test)]
    pub(crate) fn grouping_settled(&self) -> bool {
        self.grouping.pending.is_empty()
    }
}

#[cfg(test)]
mod tests;
