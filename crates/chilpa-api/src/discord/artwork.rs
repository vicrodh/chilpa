//! The cover a status shows, and what the file says about its album.
//!
//! Fooyin finds it by the file's MusicBrainz IDs alone
//! (`discordcoverartresolver.cpp`): the release, then the release group,
//! each at 1200 then 500 pixels, on the Cover Art Archive. The address it
//! finds is the image Discord fetches. The same addresses come out here,
//! in the same order. Each entity is asked once, for its index: the listing
//! says whether there is a front cover and which sizes it comes in, which
//! is what Fooyin's `HEAD` requests learn one size at a time.
//!
//! A file without the IDs is looked up by its album's name on MusicBrainz,
//! as Chilpa did before: Fooyin shows no cover for it. Everything goes to
//! MusicBrainz and the archive directly, never through Chilpa's proxy.

use chilpa_meta::text::is_uuid;
use chilpa_meta::{cover_art, musicbrainz, Http, Net};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long one request may take (Fooyin's `RequestTimeoutMs`).
const WAIT: Duration = Duration::from_secs(10);
/// An index lists every image of a release: a few kilobytes.
const INDEX_LARGEST: usize = 256 * 1024;
/// The sizes Discord is given, best first (Fooyin's candidates).
const SIZES: [u32; 2] = [1200, 500];
/// How long a found cover and a missing one are believed (Fooyin's
/// `SuccessCacheSecs` and `NegativeCacheSecs`).
const FOUND_FOR: Duration = Duration::from_secs(24 * 60 * 60);
const MISSING_FOR: Duration = Duration::from_secs(60 * 60);
/// How many albums are remembered at once.
const KEPT: usize = 256;
/// What MusicBrainz has to score a release found by name to be the album
/// that plays, and how many such releases are tried before their group.
const SURE: u8 = 90;
const RELEASES_TRIED: usize = 3;

/// What the file says about its album, for the formats and the cover.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Tags {
    pub album_artist: String,
    pub date: String,
    pub genres: Vec<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub release: String,
    pub group: String,
}

/// The tags of a file on this machine, with what its sidecar lays over the
/// album. Nothing for a source's track or a server's: neither is a file
/// here.
pub(crate) fn read_tags(path: &str) -> Tags {
    let mut tags = Tags::default();
    if path.is_empty() || chilpa_models::RemoteEntry::parse(path).is_some() {
        return tags;
    }
    let container = chilpa_core::sacd_tracks::parse(std::path::Path::new(path)).map(|(file, _)| file.display().to_string());
    if container.is_none() {
        if let Ok(read) = chilpa_tags::edit::snapshot(std::path::Path::new(path)) {
            tags = Tags {
                album_artist: read.album_artist.unwrap_or_default(),
                date: read.date.unwrap_or_default(),
                genres: read.genres,
                track: read.track_number,
                disc: read.disc_number,
                release: read.musicbrainz_release_id.unwrap_or_default(),
                group: read.musicbrainz_release_group_id.unwrap_or_default(),
            };
        }
    }
    let home = chilpa_library::sidecar::Home::of(path, container.as_deref());
    if let Some(laid) = chilpa_library::sidecar::load(&home, path).ok().flatten() {
        use chilpa_library::sidecar::Override;
        let album = laid.album;
        for (laid, kept) in [(album.album_artist, &mut tags.album_artist), (album.date, &mut tags.date),
            (album.musicbrainz_release_id, &mut tags.release), (album.musicbrainz_release_group_id, &mut tags.group)] {
            match laid {
                Override::Absent => {}
                Override::Cleared => kept.clear(),
                Override::Value(value) => *kept = value,
            }
        }
    }
    tags
}

/// How an album's cover is looked for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Query {
    /// By the file's MusicBrainz IDs, as Fooyin does.
    Ids { release: String, group: String },
    /// By the album's name and its artist on MusicBrainz.
    Name { album: String, artist: String },
}

impl Query {
    /// The query for an album, with the key it is remembered by. `None`
    /// when there is nothing to look for.
    pub(crate) fn of(tags: &Tags, album: &str, artist: &str) -> Option<(String, Query)> {
        let release = Some(tags.release.trim().to_lowercase()).filter(|id| is_uuid(id)).unwrap_or_default();
        let group = Some(tags.group.trim().to_lowercase()).filter(|id| is_uuid(id)).unwrap_or_default();
        if !release.is_empty() || !group.is_empty() {
            return Some((format!("mbid:{release}:{group}"), Query::Ids { release, group }));
        }
        let artist = if tags.album_artist.trim().is_empty() { artist.trim() } else { tags.album_artist.trim() };
        let album = album.trim();
        if album.is_empty() {
            return None;
        }
        let key = format!("name:{}\u{1f}{}", artist.to_lowercase(), album.to_lowercase());
        Some((key, Query::Name { album: album.to_string(), artist: artist.to_string() }))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    Found(String),
    /// A cover uploaded to a host that keeps it for so long.
    Uploaded(String, Duration),
    Missing,
    /// The album stopped playing before the search ended: nothing is learnt.
    Abandoned,
}

/// The largest size a Cover Art Archive index has a front cover in, of the
/// ones Discord is given.
pub(crate) fn front_size(index: &[u8]) -> Option<u32> {
    let index: Value = serde_json::from_slice(index).ok()?;
    let front = index["images"].as_array()?.iter().find(|image| image["front"] == true)?;
    SIZES.into_iter().find(|size| front["thumbnails"][size.to_string()].is_string())
}

/// The front cover of a release, or of a release group, at its best size.
fn front_of(http: &dyn Http, group: bool, id: &str) -> Option<String> {
    let kind = if group { "release-group" } else { "release" };
    let reply = http.get(&format!("{}/{kind}/{id}", cover_art::HOST), "application/json", WAIT, INDEX_LARGEST).ok()?;
    let size = (reply.status == 200).then(|| front_size(&reply.body)).flatten()?;
    if group { cover_art::group_front(id, size).ok() } else { cover_art::release_front(id, size).ok() }
}

/// Looks for the cover, asking `wanted` before each request whether the
/// album still plays. A request that fails counts as no cover, as in
/// Fooyin.
pub(crate) fn find(query: &Query, archive: &dyn Http, musicbrainz: &Net, wanted: &dyn Fn() -> bool) -> Outcome {
    let try_front = |group: bool, id: &str| -> Option<Outcome> {
        if !wanted() {
            return Some(Outcome::Abandoned);
        }
        front_of(archive, group, id).map(Outcome::Found)
    };
    match query {
        Query::Ids { release, group } => {
            let tried = [(false, release), (true, group)].into_iter().filter(|(_, id)| !id.is_empty());
            tried.into_iter().find_map(|(group, id)| try_front(group, id.as_str())).unwrap_or(Outcome::Missing)
        }
        Query::Name { album, artist } => {
            if !wanted() {
                return Outcome::Abandoned;
            }
            let hits: Vec<_> = musicbrainz::search(musicbrainz, album, artist, "").unwrap_or_default().into_iter().filter(|hit| hit.score >= SURE).collect();
            let releases = hits.iter().take(RELEASES_TRIED).find_map(|hit| try_front(false, &hit.id));
            let group = || hits.first().filter(|hit| is_uuid(&hit.release_group_id)).and_then(|hit| try_front(true, &hit.release_group_id));
            releases.or_else(group).unwrap_or(Outcome::Missing)
        }
    }
}

/// [`find`] on the network.
pub(crate) fn find_online(query: &Query, wanted: &dyn Fn() -> bool) -> Outcome {
    let http = crate::metadata::worker::Network::new();
    find(query, &http, &Net::musicbrainz(&http), wanted)
}

/// One way to a cover.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    MusicBrainz(Query),
    Upload(super::upload::Job),
}

/// Each way in its turn until one has a cover (Fooyin's
/// `tryArtworkSource`): a way that finds none hands over to the next.
pub(crate) fn find_first(steps: &[Step], wanted: &dyn Fn() -> bool, find: &dyn Fn(&Step, &dyn Fn() -> bool) -> Outcome) -> Outcome {
    for step in steps {
        match find(step, wanted) {
            Outcome::Missing => {}
            done => return done,
        }
    }
    Outcome::Missing
}

/// A way to a cover, on the network.
pub(crate) fn find_step_online(step: &Step, wanted: &dyn Fn() -> bool) -> Outcome {
    match step {
        Step::MusicBrainz(query) => find_online(query, wanted),
        Step::Upload(job) => super::upload::send_online(job, wanted),
    }
}

/// The covers found and not found, for as long as each is believed.
#[derive(Default)]
pub(crate) struct Covers {
    known: HashMap<String, (Option<String>, Instant)>,
}

impl Covers {
    /// What is known of an album while it is still believed: `Some(None)`
    /// is an album known to have no cover.
    pub(crate) fn get(&self, key: &str, now: Instant) -> Option<Option<&str>> {
        self.known.get(key).filter(|(_, until)| now < *until).map(|(url, _)| url.as_deref())
    }

    /// When a cover that was found stops being believed.
    pub(crate) fn until(&self, key: &str) -> Option<Instant> {
        self.known.get(key).filter(|(url, _)| url.is_some()).map(|(_, until)| *until)
    }

    /// Keeps what a search learnt: a found cover for a day, an uploaded one
    /// for as long as its host keeps it, a missing one for an hour. An
    /// abandoned search learnt nothing.
    pub(crate) fn learn(&mut self, key: String, outcome: &Outcome, now: Instant) {
        let (url, kept) = match outcome {
            Outcome::Found(url) => (Some(url.clone()), FOUND_FOR),
            Outcome::Uploaded(url, kept) => (Some(url.clone()), *kept),
            Outcome::Missing => (None, MISSING_FOR),
            Outcome::Abandoned => return,
        };
        if self.known.len() >= KEPT && !self.known.contains_key(&key) {
            self.known.retain(|_, (_, until)| now < *until);
            if self.known.len() >= KEPT {
                self.known.clear();
            }
        }
        self.known.insert(key, (url, now + kept));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// What the Cover Art Archive answered on 2026-11-11 for the group of
    /// "The Dark Side of the Moon", cut down to its first two images.
    const INDEX: &str = include_str!("fixtures/caa-release-group.json");
    /// What MusicBrainz answered on 2026-11-11 to a search for that album
    /// by "Pink Floyd", cut down to three rows and the keys read here.
    const SEARCH: &str = include_str!("fixtures/musicbrainz-search.json");
    const RELEASE: &str = "956fbc58-362d-43b8-b880-3779e0508559";
    const GROUP: &str = "f5093c06-23e3-404f-aeaa-40f72885ee3a";
    const FIRST: &str = "8d17f5db-b9d0-4e35-9a06-9e1bffebc5be";
    const SECOND: &str = "12fb801c-5903-4a99-a10c-7af599fe3986";
    const THIRD: &str = "e92282b9-8963-4362-87b3-bbc990d40360";

    /// Answers each address it knows, and writes down what it was asked.
    struct Archive {
        known: Vec<(String, u16, String)>,
        asked: Mutex<Vec<String>>,
    }

    impl Archive {
        fn new(known: &[(&str, u16, &str)]) -> Self {
            Self { known: known.iter().map(|(url, status, body)| (url.to_string(), *status, body.to_string())).collect(), asked: Mutex::default() }
        }
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl Http for Archive {
        fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<chilpa_meta::Reply, String> {
            self.asked.lock().unwrap().push(url.to_string());
            let (_, status, body) = self.known.iter().find(|(known, ..)| url == known || url.starts_with(&format!("{known}?"))).ok_or("nothing here")?;
            Ok(chilpa_meta::Reply { status: *status, headers: Default::default(), body: body.clone().into_bytes() })
        }
    }

    fn index(kind: &str, id: &str) -> String {
        format!("https://coverartarchive.org/{kind}/{id}")
    }

    fn found(query: &Query, archive: &Archive) -> Outcome {
        let pace = chilpa_meta::Pace::new(Duration::ZERO);
        find(query, archive, &Net { http: archive, pace: &pace, nap: |_| {} }, &|| true)
    }

    #[test]
    fn an_index_says_the_largest_front_discord_is_given() {
        assert_eq!(front_size(INDEX.as_bytes()), Some(1200));
        let mut index: Value = serde_json::from_str(INDEX).unwrap();
        index["images"][0]["thumbnails"].as_object_mut().unwrap().remove("1200");
        assert_eq!(front_size(index.to_string().as_bytes()), Some(500), "an older upload has no 1200 thumbnail");
        index["images"][0]["front"] = false.into();
        assert_eq!(front_size(index.to_string().as_bytes()), None, "a back cover is no front");
        assert_eq!(front_size(b"<!doctype html>"), None);
    }

    #[test]
    fn the_file_ids_find_the_release_then_the_group_at_the_same_addresses() {
        let ids = Query::Ids { release: RELEASE.into(), group: GROUP.into() };
        let archive = Archive::new(&[(index("release", RELEASE).as_str(), 200, INDEX), (index("release-group", GROUP).as_str(), 200, INDEX)]);
        assert_eq!(found(&ids, &archive), Outcome::Found(format!("https://coverartarchive.org/release/{RELEASE}/front-1200")));
        assert_eq!(archive.asked(), [index("release", RELEASE)], "the group only when the release has none");
        let archive = Archive::new(&[(index("release", RELEASE).as_str(), 404, "No cover art found"), (index("release-group", GROUP).as_str(), 200, INDEX)]);
        assert_eq!(found(&ids, &archive), Outcome::Found(format!("https://coverartarchive.org/release-group/{GROUP}/front-1200")));
        let archive = Archive::new(&[]);
        assert_eq!(found(&ids, &archive), Outcome::Missing, "no answer is no cover");
        assert_eq!(archive.asked().len(), 2);
    }

    #[test]
    fn a_file_without_ids_is_looked_up_by_its_albums_name() {
        let name = Query::Name { album: "The Dark Side of the Moon".into(), artist: "Pink Floyd".into() };
        let search = musicbrainz::search_url("The Dark Side of the Moon", "Pink Floyd", "", 12).unwrap();
        let archive = Archive::new(&[(search.as_str(), 200, SEARCH), (index("release", FIRST).as_str(), 404, ""), (index("release", SECOND).as_str(), 200, INDEX)]);
        assert_eq!(found(&name, &archive), Outcome::Found(format!("https://coverartarchive.org/release/{SECOND}/front-1200")));
        assert_eq!(archive.asked(), [search.clone(), index("release", FIRST), index("release", SECOND)]);
        let archive = Archive::new(&[(search.as_str(), 200, SEARCH), (index("release-group", GROUP).as_str(), 200, INDEX)]);
        assert_eq!(found(&name, &archive), Outcome::Found(format!("https://coverartarchive.org/release-group/{GROUP}/front-1200")));
        assert_eq!(archive.asked(), [search, index("release", FIRST), index("release", SECOND), index("release", THIRD), index("release-group", GROUP)]);
    }

    #[test]
    fn a_name_search_waits_for_the_shared_pace() {
        let name = Query::Name { album: "The Dark Side of the Moon".into(), artist: "Pink Floyd".into() };
        let archive = Archive::new(&[]);
        let pace = chilpa_meta::Pace::new(Duration::from_millis(150));
        let start = Instant::now();
        for _ in 0..2 {
            find(&name, &archive, &Net { http: &archive, pace: &pace, nap: |_| {} }, &|| true);
        }
        assert!(start.elapsed() >= Duration::from_millis(150), "two searches are a pace apart");
    }

    #[test]
    fn a_search_stops_when_the_album_no_longer_plays() {
        let ids = Query::Ids { release: RELEASE.into(), group: GROUP.into() };
        let archive = Archive::new(&[(index("release", RELEASE).as_str(), 404, "")]);
        let asked = Mutex::new(0);
        let wanted = || {
            let mut asked = asked.lock().unwrap();
            *asked += 1;
            *asked == 1
        };
        let pace = chilpa_meta::Pace::new(Duration::ZERO);
        assert_eq!(find(&ids, &archive, &Net { http: &archive, pace: &pace, nap: |_| {} }, &wanted), Outcome::Abandoned);
        assert_eq!(archive.asked(), [index("release", RELEASE)], "the group is not asked for");
    }

    #[test]
    fn each_way_is_tried_in_turn_until_one_has_a_cover() {
        let ids = Step::MusicBrainz(Query::Ids { release: RELEASE.into(), group: String::new() });
        let upload = Step::Upload(super::super::upload::Job { cover: "/covers/moon.jpg".into(), hours: 24 });
        let tried = Mutex::new(Vec::new());
        let record = |step: &Step, _: &dyn Fn() -> bool| {
            let uploading = matches!(step, Step::Upload(_));
            tried.lock().unwrap().push(uploading);
            if uploading { Outcome::Uploaded("https://litter.catbox.moe/x.jpg".into(), Duration::from_secs(60)) } else { Outcome::Missing }
        };
        assert!(matches!(find_first(&[ids.clone(), upload], &|| true, &record), Outcome::Uploaded(..)));
        assert_eq!(*tried.lock().unwrap(), [false, true], "the search first, then the upload");
        assert_eq!(find_first(&[ids], &|| true, &|_, _| Outcome::Abandoned), Outcome::Abandoned, "an abandoned way stops the rest");
        assert_eq!(find_first(&[], &|| true, &record), Outcome::Missing);
    }

    #[test]
    fn an_album_is_known_by_its_ids_or_else_by_its_name() {
        let tags = Tags { release: RELEASE.to_uppercase(), ..Tags::default() };
        assert_eq!(Query::of(&tags, "x", "y").unwrap().0, format!("mbid:{RELEASE}:"));
        let tags = Tags { release: "not an id".into(), album_artist: "Pink Floyd".into(), ..Tags::default() };
        let (key, query) = Query::of(&tags, "The Dark Side of the Moon", "David Gilmour").unwrap();
        assert_eq!(key, "name:pink floyd\u{1f}the dark side of the moon");
        assert_eq!(query, Query::Name { album: "The Dark Side of the Moon".into(), artist: "Pink Floyd".into() }, "the album's artist first");
        assert_eq!(Query::of(&Tags::default(), " ", "Someone"), None);
    }

    #[test]
    fn a_cover_is_believed_a_day_and_a_missing_one_an_hour() {
        let mut covers = Covers::default();
        let now = Instant::now();
        covers.learn("found".into(), &Outcome::Found("https://coverartarchive.org/release/x/front-1200".into()), now);
        covers.learn("missing".into(), &Outcome::Missing, now);
        covers.learn("uploaded".into(), &Outcome::Uploaded("https://litter.catbox.moe/x.jpg".into(), Duration::from_secs(55 * 60)), now);
        covers.learn("abandoned".into(), &Outcome::Abandoned, now);
        assert_eq!(covers.get("abandoned", now), None, "an abandoned search learnt nothing");
        assert_eq!(covers.until("uploaded"), Some(now + Duration::from_secs(55 * 60)), "an upload is kept as long as its host keeps it");
        assert_eq!(covers.until("missing"), None);
        assert_eq!(covers.get("found", now + Duration::from_secs(23 * 3600)), Some(Some("https://coverartarchive.org/release/x/front-1200")));
        assert_eq!(covers.get("missing", now + Duration::from_secs(59 * 60)), Some(None));
        assert_eq!(covers.get("missing", now + MISSING_FOR), None, "asked again after an hour");
        assert_eq!(covers.get("found", now + FOUND_FOR), None, "and the found one after a day");
        assert_eq!(covers.get("never", now), None);
    }

    #[test]
    fn the_covers_kept_are_bounded() {
        let mut covers = Covers::default();
        let now = Instant::now();
        for n in 0..KEPT {
            covers.learn(n.to_string(), &Outcome::Missing, now);
        }
        covers.learn("one more".into(), &Outcome::Missing, now);
        assert!(covers.known.len() <= KEPT);
        assert_eq!(covers.get("one more", now), Some(None));
    }
}
