//! "Remember position in playlists and albums": the last track that began
//! of each album and playlist, so that Play there again offers to go on from
//! it instead of starting over. Off unless turned on.
//!
//! Play does not start then: it leaves an offer, which the window asks
//! about, and the answer plays. A place at the first or the last track is
//! not offered: the first is a start over, and the last an album heard.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::queue_origin::QueueOrigin;
use crate::Chilpa;

/// The toggle under Playback.
pub const REMEMBER: &str = "playback.rememberPosition";
/// Places kept; past it the oldest goes.
const KEPT_AT_MOST: usize = 500;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Place {
    path: String,
    /// When it began, in unix seconds: the oldest goes first.
    at: i64,
}

/// Every album's and playlist's place, by `key_of`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Places {
    by: BTreeMap<String, Place>,
}

/// Play waiting for its answer: the list, where it would go on, and where
/// it was asked from.
#[derive(Clone, Debug)]
pub(crate) struct Offer {
    paths: Vec<String>,
    at: usize,
    title: String,
    origin: Option<QueueOrigin>,
}

fn file() -> PathBuf {
    chilpa_dirs::data().join("positions.json")
}

/// Replaces the file whole. Not synced: a place lost to a power cut is a
/// track to pick again, not worth a wait on every track.
fn keep(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// The collection a queue entry came from, when it is one with a place.
fn key_of(origin: &QueueOrigin) -> Option<String> {
    match origin.screen.as_str() {
        "album" | "playlist" if origin.id != 0 => Some(format!("{}/{}", origin.screen, origin.id)),
        "source" if !origin.key.is_empty() => Some(format!("source/{}", origin.key)),
        _ => None,
    }
}

impl Places {
    /// The kept places; a file past a megabyte is not one this wrote.
    fn read(path: &Path) -> Self {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        let read = std::fs::File::open(path).and_then(|file| file.take(1 << 20).read_to_end(&mut bytes));
        read.ok().and_then(|_| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    fn note(&mut self, key: String, path: &str, at: i64) {
        self.by.insert(key, Place { path: path.to_string(), at });
        while self.by.len() > KEPT_AT_MOST {
            let Some(oldest) = self.by.iter().min_by_key(|(_, place)| place.at).map(|(key, _)| key.clone()) else { break };
            self.by.remove(&oldest);
        }
    }

    /// Where `paths` would go on from: past the first track and before
    /// the last.
    fn point(&self, key: &str, paths: &[String]) -> Option<usize> {
        let place = self.by.get(key)?;
        let at = paths.iter().position(|path| *path == place.path)?;
        (at > 0 && at + 1 < paths.len()).then_some(at)
    }
}

impl Chilpa {
    pub fn remember_position(&self) -> bool {
        self.preference(REMEMBER).is_some_and(|v| v == "true")
    }

    fn places(&mut self) -> &mut Places {
        self.resume_places.get_or_insert_with(|| Places::read(&file()))
    }

    /// A track began in front of the window: it is its collection's place.
    pub(crate) fn note_place(&mut self, index: usize, path: &Path) {
        if !self.remember_position() {
            return;
        }
        let Some(key) = self.queue_origins.get(index).cloned().flatten().as_ref().and_then(key_of) else { return };
        let path = path.to_string_lossy().into_owned();
        let now = crate::now_unix();
        let places = self.places();
        if places.by.get(&key).is_some_and(|place| place.path == path) {
            return;
        }
        places.note(key, &path, now);
        let written = serde_json::to_vec(places).ok();
        if let Some(bytes) = written {
            if let Err(error) = keep(&file(), &bytes) {
                eprintln!("positions: not kept: {error}");
            }
        }
    }

    /// Play on a list, in its order: an offer when the list has a place to
    /// go on from, and the list from its start otherwise.
    pub fn play_list_or_offer(&mut self, paths: &[String]) {
        self.resume_offer = None;
        let key = self.browse_origin.as_ref().and_then(key_of);
        if let (true, Some(key)) = (self.remember_position(), key) {
            let point = self.places().point(&key, paths);
            if let Some(at) = point {
                let title = self.library().and_then(|lib| lib.tracks_by_paths(&paths[at..=at]).ok())
                    .and_then(|found| found.into_iter().next().flatten()).map(|track| track.title)
                    .unwrap_or_else(|| Path::new(&paths[at]).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                self.resume_offer = Some(Offer { paths: paths.to_vec(), at, title, origin: self.browse_origin.clone() });
                return;
            }
        }
        self.play_list(paths, false);
    }

    /// The offer waiting for an answer, as JSON for the window: the track
    /// it would go on from and its place in the list. Empty when none.
    pub fn resume_offer_json(&self) -> String {
        self.resume_offer.as_ref().map_or_else(String::new, |offer| {
            serde_json::json!({ "title": offer.title, "number": offer.at + 1, "total": offer.paths.len() }).to_string()
        })
    }

    /// The answer: go on from the place, or start over. No answer, the
    /// offer goes and nothing plays.
    pub fn answer_resume(&mut self, answer: &str) {
        let Some(offer) = self.resume_offer.take() else { return };
        match answer {
            "continue" => self.with_queue_origin(offer.origin, |api| {
                api.shuffle = false;
                api.play_from(&offer.paths, offer.at);
            }),
            "restart" => self.with_queue_origin(offer.origin, |api| api.play_list(&offer.paths, false)),
            _ => {}
        }
    }

    pub(crate) fn resume_setting(&mut self, key: &str, value: &str) -> bool {
        if key != REMEMBER {
            return false;
        }
        if matches!(value, "true" | "false") {
            self.remember_preference(key, value);
        }
        true
    }

    pub(crate) fn resume_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != REMEMBER {
            return false;
        }
        row.value = self.remember_position().to_string();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("/music/{i}.flac")).collect()
    }

    #[test]
    fn a_place_is_offered_between_the_first_and_the_last_track() {
        let mut places = Places::default();
        let paths = list(6);
        places.note("album/7".into(), &paths[3], 10);
        assert_eq!(places.point("album/7", &paths), Some(3));
        assert_eq!(places.point("album/8", &paths), None, "another album");
        places.note("album/7".into(), &paths[0], 11);
        assert_eq!(places.point("album/7", &paths), None, "at the first track it is a start over");
        places.note("album/7".into(), &paths[5], 12);
        assert_eq!(places.point("album/7", &paths), None, "at the last the album was heard");
        places.note("album/7".into(), "/elsewhere.flac", 13);
        assert_eq!(places.point("album/7", &paths), None, "a track no longer in the list");
    }

    #[test]
    fn the_oldest_place_goes_first_past_the_limit() {
        let mut places = Places::default();
        for i in 0..=KEPT_AT_MOST as i64 {
            places.note(format!("playlist/{i}"), "/a.flac", 1000 - i);
        }
        assert_eq!(places.by.len(), KEPT_AT_MOST);
        assert!(!places.by.contains_key(&format!("playlist/{KEPT_AT_MOST}")), "the one that began longest ago");
        let saved = serde_json::to_vec(&places).unwrap();
        assert_eq!(serde_json::from_slice::<Places>(&saved).unwrap(), places);
    }

    /// Runs `name` again in a child with a profile of its own: it writes
    /// settings and the places file.
    fn own_profile(name: &str) -> bool {
        const CHILD: &str = "CHILPA_RESUME_TEST";
        if std::env::var(CHILD).as_deref() == Ok(name) {
            return true;
        }
        let profile = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"]).env(CHILD, name)
            .envs(crate::test_support::profile_environment(profile.path())).output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        false
    }

    #[test]
    fn play_on_a_list_with_a_place_waits_for_the_answer() {
        if !own_profile("queue::resume::tests::play_on_a_list_with_a_place_waits_for_the_answer") { return; }
        let mut api = crate::Chilpa::new();
        let paths = list(6);
        let album = QueueOrigin { screen: "album".into(), id: 7, ..Default::default() };
        api.queue = paths.clone();
        api.queue_origins = vec![Some(album.clone()); paths.len()];
        api.note_place(3, Path::new(&paths[3]));
        assert!(api.resume_places.as_ref().is_none_or(|places| places.by.is_empty()), "off, nothing is kept");

        assert!(api.adjust(REMEMBER, "true"));
        api.note_place(3, Path::new(&paths[3]));
        assert_eq!(Places::read(&file()).point("album/7", &paths), Some(3), "kept on disk");
        api.browse_origin = Some(album);
        api.play_list_or_offer(&paths);
        let offer: serde_json::Value = serde_json::from_str(&api.resume_offer_json()).unwrap();
        assert_eq!((offer["number"].as_u64(), offer["total"].as_u64(), offer["title"].as_str()), (Some(4), Some(6), Some("3")));
        api.answer_resume("");
        assert_eq!(api.resume_offer_json(), "", "a way out plays nothing and the offer goes");

        api.browse_origin = Some(QueueOrigin { screen: "album".into(), id: 8, ..Default::default() });
        api.play_list_or_offer(&paths);
        assert_eq!(api.resume_offer_json(), "", "another album has no place");
    }

    #[test]
    fn only_albums_playlists_and_source_records_have_a_place() {
        let origin = |screen: &str, id: i64, key: &str| QueueOrigin { screen: screen.into(), id, key: key.into(), ..Default::default() };
        assert_eq!(key_of(&origin("album", 7, "")), Some("album/7".into()));
        assert_eq!(key_of(&origin("playlist", 3, "")), Some("playlist/3".into()));
        assert_eq!(key_of(&origin("source", 0, "radio native:x")), Some("source/radio native:x".into()));
        assert_eq!(key_of(&origin("artist", 9, "")), None);
        assert_eq!(key_of(&origin("search", 0, "")), None);
    }
}
