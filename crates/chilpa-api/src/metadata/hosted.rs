//! Records on a server of mine, in the editor.
//!
//! Their sidecar -- what the listener chose to keep on this device -- lives
//! in the library (`chilpa_library::store::overlay`), not beside any file,
//! so what the worker reads from disk for a folder is read here from the
//! library, on the window's thread, and what a save leaves in it is written
//! here too: a row and, at most, a cover beside the library. Nothing of it
//! goes to the server.

use super::artwork::Waiting;
use super::draft::{Draft, HomeState};
use super::fields::{ALBUM, TRACK};
use super::save::HomePlan;
use chilpa_library::sidecar::{Override, Sidecar};
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

/// A field's text as a document lays it: empty clears.
fn said(text: &str) -> Override<String> {
    if text.is_empty() {
        Override::Cleared
    } else {
        Override::Value(text.to_string())
    }
}

/// What the server says of the fields each home's sidecar stops laying
/// over, as the record opened reading it: laid back over the library once
/// the sidecar is written (`Library::lay_word`), so the server's word shows
/// again at once. `None` for a home nothing is taken off, or whose tracks
/// the server could not be asked about.
pub(super) fn words(draft: &Draft, homes: &[HomePlan]) -> Vec<Option<Sidecar>> {
    homes
        .iter()
        .map(|plan| {
            let at = draft.homes.iter().position(|h| h.home == plan.home)?;
            let old = draft.homes[at].sidecar.as_ref()?;
            let rows: Vec<_> = draft.rows.iter().filter(|row| row.home == at && row.known).collect();
            let first = rows.first()?;
            let mut word = Sidecar::new(0);
            for (i, field) in ALBUM.iter().enumerate() {
                if !old.album.get(field.name).is_absent() && plan.sidecar.album.get(field.name).is_absent() {
                    word.album.put(field.name, said(&first.album_file[i]));
                }
            }
            for row in rows {
                let Some(was) = old.tracks.get(&row.key) else { continue };
                let now = plan.sidecar.tracks.get(&row.key);
                for (i, field) in TRACK.iter().enumerate() {
                    if !was.get(field.name).is_absent() && now.is_none_or(|t| t.get(field.name).is_absent()) {
                        word.track_mut(&row.key).put(field.name, said(&row.file[i]));
                    }
                }
            }
            (!word.is_empty()).then_some(word)
        })
        .collect()
}

/// Writes what a save planned for the homes on a server: each overlay,
/// with the cover in wait when it takes it, laid over the library at once,
/// and the server's word for what it stopped laying (`words`, one per
/// home). Returns the tracks reached, and stops at the first that fails
/// and says why; what landed before stays.
pub(super) fn land(lib: &Library, homes: &[HomePlan], cover: Option<&Waiting>, now: i64, words: &[Option<Sidecar>]) -> (Vec<i64>, Option<String>) {
    let mut landed = Vec::new();
    for (at, home) in homes.iter().enumerate() {
        let image = cover.filter(|_| home.cover).map(|w| (w.bytes.as_slice(), w.extension));
        match lib.store_overlay(&home.home, &home.sidecar, image, now) {
            Ok(_) => landed.extend(&home.tracks),
            Err(why) => return (landed, Some(chilpa_i18n::t_args("the changes could not be kept in the library: {}", &[&why]))),
        }
        if let Some(Some(word)) = words.get(at) {
            if let Err(why) = lib.lay_word(&home.home, word) {
                return (landed, Some(chilpa_i18n::t_args("the changes could not be kept in the library: {}", &[&why.to_string()])));
            }
        }
    }
    (landed, None)
}
