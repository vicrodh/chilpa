//! Records on a server of mine, in the editor.
//!
//! Their overrides live in the library (`chilpa_library::store::overlay`),
//! not beside any file, so what the worker reads from disk for a folder is
//! read here from the library, on the window's thread, and what a save
//! leaves for them is written here too: a row and, at most, a cover beside
//! the library. Nothing of it goes to the server.

use super::artwork::Waiting;
use super::draft::HomeState;
use super::save::HomePlan;
use chilpa_library::{EditTrack, Library};

/// The homes on a server among a record's tracks, each with the overlay
/// in force for it.
pub(super) fn homes(lib: &Library, tracks: &[EditTrack]) -> Vec<HomeState> {
    let mut out: Vec<HomeState> = Vec::new();
    for track in tracks.iter().filter(|t| t.hosted.is_some()) {
        let home = track.home();
        if out.iter().any(|h| h.home == home) {
            continue;
        }
        let (sidecar, unreadable) = match lib.overlay(&home) {
            Ok(sidecar) => (sidecar, String::new()),
            Err(why) => (None, why),
        };
        out.push(HomeState { home, of: track.external_id.clone(), sidecar, unreadable, xml: None });
    }
    out
}

/// Writes what a save planned for the homes on a server: each overlay,
/// with the cover in wait when it takes it, laid over the library at once.
/// Returns the tracks reached, and stops at the first that fails and says
/// why; what landed before stays.
pub(super) fn land(lib: &Library, homes: &[HomePlan], cover: Option<&Waiting>, now: i64) -> (Vec<i64>, Option<String>) {
    let mut landed = Vec::new();
    for home in homes {
        let image = cover.filter(|_| home.cover).map(|w| (w.bytes.as_slice(), w.extension));
        match lib.store_overlay(&home.home, &home.sidecar, image, now) {
            Ok(_) => landed.extend(&home.tracks),
            Err(why) => return (landed, Some(chilpa_i18n::t_args("the changes could not be kept in the library: {}", &[&why]))),
        }
    }
    (landed, None)
}
