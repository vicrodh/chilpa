//! What Chilpa and a plugin say to each other.
//!
//! These are the v1 API messages. They travel as JSON, and both sides build
//! against this one definition: the host inside Chilpa and every plugin,
//! first party or not, speak exactly the same language.
//!
//! The crate depends on `serde` and nothing else, because it is compiled
//! into every plugin. How a message is turned into bytes, and how anything
//! here is worded for the user, belongs to whoever uses it.
//!
//! Two rules visible in the types:
//!
//! 1. **Nothing names a service.** A plugin describes what is its own with
//!    data -- external id, quality, availability -- never with a platform's
//!    concepts.
//! 2. **Everything coming in is cleaned.** A plugin can answer anything;
//!    what gets past [`Results::clean`] is already trimmed, with no empty
//!    fields and no endless lists.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The operations a plugin can export.
///
/// A plugin exports the ones it knows how to do; Chilpa only asks it for the
/// ones its manifest declared.
pub mod op {
    /// Search. [`super::Query`] in, [`super::Results`] out.
    pub const SEARCH: &str = "chilpa_search";
    /// The user's lists on that source.
    pub const PLAYLISTS: &str = "chilpa_playlists";
    /// The tracks of one of those lists.
    pub const PLAYLIST_TRACKS: &str = "chilpa_playlist_tracks";
    /// From an external id to something playable.
    pub const RESOLVER: &str = "chilpa_resolver";
    /// "Forget whatever of mine you hold." It is asked before uninstalling.
    ///
    /// It is the only operation Chilpa sends without anyone asking for it
    /// from the window, and the only one whose reply is used for nothing:
    /// what matters is having sent it. A plugin that handles its own
    /// account (`own_account`) is the one with something to delete here;
    /// for the rest it is a no-op.
    pub const REVOKE: &str = "chilpa_revoke";
    /// Walk the source's own tree. [`super::Browse`] in, [`super::Listing`]
    /// out.
    pub const BROWSE: &str = "chilpa_browse";
}

/// How much is accepted from a single reply, per kind of entity.
///
/// A plugin that answers a hundred thousand tracks is not helping: it is
/// filling Chilpa's memory. It is trimmed and things move on.
pub const MAX_PER_REPLY: usize = 500;

/// How many facts an entry carries, at most.
pub const MAX_FACTS: usize = 24;

/// What a plugin is asked when searching.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    /// How many results are wanted, at most.
    pub limit: usize,
}

impl Query {
    pub fn new(text: &str, limit: usize) -> Self {
        Self { text: text.to_string(), limit: limit.min(MAX_PER_REPLY) }
    }
}

/// The quality a source offers something at.
///
/// Everything is optional because not every source knows it, and saying zero
/// when it is not known is worse than saying nothing.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Quality {
    pub codec: Option<String>,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u16>,
}

impl Quality {
    /// How it is said, for display. Empty if nothing is known.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(c) = &self.codec {
            parts.push(c.to_ascii_uppercase());
        }
        match (self.bit_depth, self.sample_rate_hz) {
            (Some(b), Some(r)) => parts.push(format!("{b} bits a {r} Hz")),
            (None, Some(r)) => parts.push(format!("{r} Hz")),
            (Some(b), None) => parts.push(format!("{b} bits")),
            _ => {}
        }
        parts.join(" · ")
    }
}

/// A track that lives on a source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemoteTrack {
    /// The id its source gives it. It is half of its identity; the other
    /// half is the source itself, and Chilpa supplies that.
    pub external_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration_ms: u64,
    /// For matching against what is local without relying on the name.
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub musicbrainz_recording_id: Option<String>,
    #[serde(default)]
    pub quality: Quality,
    /// The cover, if the source offers it by URL.
    #[serde(default)]
    pub artwork_url: Option<String>,
}

impl RemoteTrack {
    /// True if it has the minimum to be shown and asked for again.
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.title.trim().is_empty()
    }

    /// What it can be matched against the local library by. `None` if it
    /// carries nothing solid: matching by name is as good as not matching.
    ///
    /// The MBID is preferred because it identifies the recording; an ISRC
    /// can repeat across reissues of the same master.
    pub fn fingerprint(&self) -> Option<String> {
        self.musicbrainz_recording_id
            .as_deref()
            .and_then(mbid_fingerprint)
            .or_else(|| self.isrc.as_deref().and_then(isrc_fingerprint))
    }
}

/// A MBID's fingerprint, normalized. It lives on its own because the local
/// library has to write exactly the same string for matching to mean
/// anything; two similar normalizations match nothing.
pub fn mbid_fingerprint(mbid: &str) -> Option<String> {
    let m = mbid.trim();
    (!m.is_empty()).then(|| format!("mbid:{}", m.to_ascii_lowercase()))
}

/// The same for an ISRC, which by standard is written in uppercase.
pub fn isrc_fingerprint(isrc: &str) -> Option<String> {
    let i = isrc.trim();
    (!i.is_empty()).then(|| format!("isrc:{}", i.to_ascii_uppercase()))
}

/// How what a source hands over to play arrives.
///
/// Measured against the APIs of two large services: the usual case is a
/// single already signed URL, but segmented manifests are handed over too.
/// It is declared rather than guessed from the extension, because a signed
/// URL has no extension to look at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// A single URL that is opened and read straight through. It is what
    /// Chilpa knows how to play today.
    #[default]
    Progressive,
    /// A DASH manifest: the audio comes in segments.
    Dash,
    /// An HLS manifest, for the same reasons.
    Hls,
}

impl Delivery {
    /// True if Chilpa knows how to play that today.
    pub fn playable(&self) -> bool {
        matches!(self, Delivery::Progressive)
    }
}

/// What the source is asked for in order to play something of its own.
///
/// It goes in through [`op::RESOLVER`] and a [`Stream`] comes out. It is
/// separate from search because resolving costs: many sources sign one URL
/// per call, and asking for one per result in a list would hand them work
/// and burn the account's rate limit.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Resolve {
    /// The id that same source gave when searching.
    pub external_id: String,
}

impl Resolve {
    pub fn new(external_id: &str) -> Self {
        Self { external_id: external_id.to_string() }
    }
}

/// An album that lives on a source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemoteAlbum {
    pub external_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub track_count: usize,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub musicbrainz_release_id: Option<String>,
}

impl RemoteAlbum {
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.title.trim().is_empty()
    }
}

/// A list that lives on a source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemotePlaylist {
    pub external_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub track_count: usize,
    #[serde(default)]
    pub artwork_url: Option<String>,
}

impl RemotePlaylist {
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.name.trim().is_empty()
    }
}

/// What a plugin answers to a search.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Results {
    #[serde(default)]
    pub tracks: Vec<RemoteTrack>,
    #[serde(default)]
    pub albums: Vec<RemoteAlbum>,
    #[serde(default)]
    pub playlists: Vec<RemotePlaylist>,
}

impl Results {
    /// Drops what is no good and trims what is too much.
    ///
    /// It happens on this side, always, before anything touches a view. A
    /// plugin can answer whatever it likes; what gets past here is already
    /// clean.
    pub fn clean(mut self) -> Self {
        self.tracks.retain(RemoteTrack::usable);
        self.albums.retain(RemoteAlbum::usable);
        self.playlists.retain(RemotePlaylist::usable);
        self.tracks.truncate(MAX_PER_REPLY);
        self.albums.truncate(MAX_PER_REPLY);
        self.playlists.truncate(MAX_PER_REPLY);
        self
    }

    pub fn empty(&self) -> bool {
        self.tracks.is_empty() && self.albums.is_empty() && self.playlists.is_empty()
    }

    /// How many things it carries in total.
    pub fn count(&self) -> usize {
        self.tracks.len() + self.albums.len() + self.playlists.len()
    }
}

/// What a plugin answers when it is asked to resolve a track.
///
/// Chilpa decodes and delivers; the plugin touches neither the audio nor the
/// device (ADR-003). That is why there is only where to read from here, not
/// how to sound.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Stream {
    /// Where it is read from. It has to be `http` or `https`.
    ///
    /// Measured against the APIs of two large services: the URL comes
    /// **already signed in the query**, so it carries no authentication
    /// header. What is signed is the API call that hands it over, and the
    /// plugin already does that with `http` (ADR-008).
    #[serde(default)]
    pub url: String,
    /// When a track comes split across several URLs read in order.
    ///
    /// It is not the same as a segmented manifest: these are whole files,
    /// one after another. **The plugin decodes its service's manifest and
    /// hands over the finished list**; Chilpa knows nothing about manifest
    /// formats, which is what ADR-004 says.
    #[serde(default)]
    pub urls: Vec<String>,
    /// What the source says it is, for choosing the decoder.
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub quality: Quality,
    /// How long that URL is good for, in unix seconds. Many sources hand
    /// out links that expire.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// How it arrives. By default a direct URL, which is the common case.
    #[serde(default)]
    pub delivery: Delivery,
}

impl Stream {
    /// Where to read from, in order. `url` first if present, then `urls`.
    pub fn parts(&self) -> Vec<&str> {
        std::iter::once(self.url.as_str())
            .filter(|u| !u.is_empty())
            .chain(self.urls.iter().map(String::as_str))
            .collect()
    }

    /// True if opening can be attempted: there is at least one part and all
    /// of them are http.
    pub fn usable(&self) -> bool {
        let parts = self.parts();
        !parts.is_empty()
            && parts.iter().all(|u| u.starts_with("https://") || u.starts_with("http://"))
    }

    /// True if it has already expired according to the clock it is given.
    pub fn expired(&self, now_unix: i64) -> bool {
        self.expires_at.is_some_and(|t| t <= now_unix)
    }
}

/// Where in a source's tree Chilpa is standing, and what it asks to see.
///
/// A source that is browsed is a tree only the plugin knows the shape of:
/// countries and genres for one, shows and their episodes for another.
/// Chilpa walks it without understanding it, one listing at a time.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Browse {
    /// The id of the entry being opened. Empty is the root.
    #[serde(default)]
    pub at: String,
    /// What the previous listing of this same place gave as `next`. `None`
    /// asks for the beginning.
    #[serde(default)]
    pub cursor: Option<String>,
    /// What the user typed, when `at` is an entry of kind
    /// [`EntryKind::Search`].
    #[serde(default)]
    pub text: Option<String>,
    /// The language the window speaks, as a two letter code. A plugin words
    /// its own folders ("By country") and has no catalog but its own, so it
    /// is told which language to answer in. Empty means it is not known.
    #[serde(default)]
    pub language: String,
    /// What the listener picked in the columns of the root, by column id:
    /// the id of the entry picked. Only the root is asked with picks.
    #[serde(default)]
    pub picks: BTreeMap<String, String>,
}

impl Browse {
    pub fn root() -> Self {
        Self::default()
    }

    pub fn at(id: &str) -> Self {
        Self { at: id.to_string(), ..Self::default() }
    }
}

/// What an entry of a listing is, which is what a click on it does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// It opens: browsing at its id gives another listing.
    #[default]
    Folder,
    /// It sounds: its id is resolved through [`op::RESOLVER`].
    Item,
    /// A place to type. Browsing at its id with a `text` gives the matches.
    Search,
}

/// One line of a listing.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Entry {
    /// What the plugin wants back to open or resolve this entry. Chilpa
    /// never reads into it.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub kind: EntryKind,
    /// True for something with no end and no seeking, like a station.
    #[serde(default)]
    pub live: bool,
    /// Zero when it is not known, or when it is live.
    #[serde(default)]
    pub duration_ms: u64,
    /// When it came out, in unix seconds.
    #[serde(default)]
    pub published_at: Option<i64>,
    /// False when the plugin already knows Chilpa cannot play it. It is
    /// still listed, with `why_not` saying the reason: an entry that
    /// vanishes is worse than one that explains itself.
    #[serde(default = "yes")]
    pub playable: bool,
    #[serde(default)]
    pub why_not: String,
    /// What it is about, in a paragraph: an episode's notes, a show's
    /// blurb. Plain text.
    #[serde(default)]
    pub description: String,
    /// What else is known about it, as short labelled lines for a page of
    /// its own: where a station is from, what it broadcasts in.
    #[serde(default)]
    pub facts: Vec<Fact>,
}

/// One labelled line about an entry. The plugin words the label, in the
/// language it was asked in.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Fact {
    pub label: String,
    pub value: String,
    /// Where the value leads, when it is a page that can be opened.
    #[serde(default)]
    pub url: Option<String>,
}

impl Fact {
    pub fn new(label: &str, value: &str) -> Self {
        Self { label: label.to_string(), value: value.to_string(), url: None }
    }
}

fn yes() -> bool {
    true
}

impl Entry {
    /// True if it has the minimum to be shown and asked for again.
    pub fn usable(&self) -> bool {
        !self.id.trim().is_empty() && !self.title.trim().is_empty()
    }
}

/// How many columns a root can be narrowed by.
pub const MAX_COLUMNS: usize = 4;

/// One way to narrow the root of a source: its countries, its languages.
///
/// A source too large to walk folder by folder offers a few of these with
/// its root listing. The window shows them side by side, the listener picks
/// in any of them, and the root is asked again with every pick; its entries
/// are then what matches all of them. A column does not depend on the
/// others: one the source cannot narrow by the rest is offered whole.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub title: String,
    /// What can be picked. An entry's `id` is what goes back in `picks`,
    /// and its `subtitle` says how much there is behind it.
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// What counts as picked until the listener picks something else: the
    /// root without picks was answered as if it were.
    #[serde(default)]
    pub preset: Option<String>,
}

impl Column {
    pub fn usable(&self) -> bool {
        !self.id.trim().is_empty() && !self.title.trim().is_empty() && !self.entries.is_empty()
    }
}

/// What a plugin answers when it is browsed.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Listing {
    /// What the place is called, for the header.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// What to send as `cursor` to get what follows. `None` is the end.
    #[serde(default)]
    pub next: Option<String>,
    /// The columns the root can be narrowed by. Sent with the root's first
    /// listing, the one asked with no picks and no cursor; the window keeps
    /// them while the listener picks.
    #[serde(default)]
    pub columns: Vec<Column>,
}

impl Listing {
    /// Drops what is no good and trims what is too much, like
    /// [`Results::clean`].
    ///
    /// `asked` is the cursor this listing answers. A `next` equal to it
    /// would send Chilpa round the same page forever, so it is dropped; so
    /// is an empty one, and one on a page that brought nothing.
    pub fn clean(mut self, asked: Option<&str>) -> Self {
        self.entries.retain(Entry::usable);
        self.entries.truncate(MAX_PER_REPLY);
        for entry in &mut self.entries {
            entry.facts.retain(|f| !f.label.trim().is_empty() && !f.value.trim().is_empty());
            entry.facts.truncate(MAX_FACTS);
        }
        for column in &mut self.columns {
            column.entries.retain(Entry::usable);
            column.entries.truncate(MAX_PER_REPLY);
            if column.preset.as_ref().is_some_and(|p| !column.entries.iter().any(|e| &e.id == p)) {
                column.preset = None;
            }
        }
        self.columns.retain(Column::usable);
        self.columns.truncate(MAX_COLUMNS);
        let stuck = match self.next.as_deref() {
            None => false,
            Some(n) => n.trim().is_empty() || Some(n) == asked || self.entries.is_empty(),
        };
        if stuck {
            self.next = None;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_never_asks_for_more_than_fits() {
        assert_eq!(Query::new("something", 10).limit, 10);
        assert_eq!(Query::new("something", 100_000).limit, MAX_PER_REPLY);
    }

    #[test]
    fn a_reply_with_only_the_required_fields_is_read() {
        let r: Results =
            serde_json::from_str(r#"{"tracks":[{"external_id":"a","title":"One"}]}"#).unwrap();
        assert_eq!(r.tracks.len(), 1);
        assert_eq!(r.tracks[0].artist, "", "what is missing takes its default value");
        assert_eq!(r.count(), 1);
    }

    #[test]
    fn the_delivery_travels_in_snake_case() {
        let s: Stream =
            serde_json::from_str(r#"{"url":"https://a.b/c","delivery":"hls"}"#).unwrap();
        assert_eq!(s.delivery, Delivery::Hls);
        assert!(!s.delivery.playable());
        assert_eq!(serde_json::to_string(&Delivery::Progressive).unwrap(), r#""progressive""#);
    }

    #[test]
    fn what_is_useless_is_dropped_and_what_is_extra_is_trimmed() {
        let mut r = Results::default();
        r.tracks.push(RemoteTrack { external_id: "a".into(), title: "One".into(), ..Default::default() });
        r.tracks.push(RemoteTrack { external_id: "".into(), title: "Another".into(), ..Default::default() });
        for i in 0..1000 {
            r.albums.push(RemoteAlbum {
                external_id: i.to_string(),
                title: "Album".into(),
                ..Default::default()
            });
        }
        let r = r.clean();
        assert_eq!(r.tracks.len(), 1, "only the one that works");
        assert_eq!(r.albums.len(), MAX_PER_REPLY, "the rest is trimmed");
    }

    #[test]
    fn an_entry_is_playable_unless_it_says_otherwise() {
        let l: Listing = serde_json::from_str(
            r#"{"title":"Top","entries":[
                {"id":"a","title":"One","kind":"item","live":true},
                {"id":"b","title":"Two","kind":"item","playable":false,"why_not":"HLS"},
                {"id":"c","title":"Three"}]}"#,
        )
        .unwrap();
        assert!(l.entries[0].playable && l.entries[0].live);
        assert!(!l.entries[1].playable);
        assert_eq!(l.entries[2].kind, EntryKind::Folder, "a folder unless it says otherwise");
    }

    #[test]
    fn a_listing_drops_what_cannot_be_shown_and_a_cursor_that_goes_nowhere() {
        let entry = |id: &str, title: &str| Entry { id: id.into(), title: title.into(), ..Default::default() };
        let l = Listing {
            title: "Top".into(),
            entries: vec![entry("a", "One"), entry("", "No id"), entry("b", " ")],
            next: Some("2".into()),
            ..Default::default()
        };
        let cleaned = l.clone().clean(Some("1"));
        assert_eq!(cleaned.entries.len(), 1);
        assert_eq!(cleaned.next.as_deref(), Some("2"));

        assert_eq!(l.clone().clean(Some("2")).next, None, "the same cursor again is a loop");
        let empty = Listing { next: Some("2".into()), ..Default::default() };
        assert_eq!(empty.clean(None).next, None, "a page with nothing has no next");
        let blank = Listing { next: Some(" ".into()), ..l };
        assert_eq!(blank.clean(None).next, None);
    }

    #[test]
    fn a_listing_is_trimmed_to_what_fits() {
        let entries = (0..MAX_PER_REPLY + 50)
            .map(|i| Entry { id: i.to_string(), title: "x".into(), ..Default::default() })
            .collect();
        let l = Listing { entries, ..Default::default() }.clean(None);
        assert_eq!(l.entries.len(), MAX_PER_REPLY);
    }

    #[test]
    fn an_entry_keeps_its_facts_that_say_something() {
        let entry = Entry {
            id: "a".into(),
            title: "One".into(),
            facts: vec![Fact::new("Country", "Mexico"), Fact::new("", "x"), Fact::new("Votes", " ")],
            ..Default::default()
        };
        let l = Listing { entries: vec![entry], ..Default::default() }.clean(None);
        assert_eq!(l.entries[0].facts, [Fact::new("Country", "Mexico")]);
        let read: Entry = serde_json::from_str(r#"{"id":"a","title":"One"}"#).unwrap();
        assert!(read.facts.is_empty() && read.description.is_empty(), "both are optional");
    }

    #[test]
    fn a_root_keeps_the_columns_that_can_be_picked_from() {
        let entry = |id: &str| Entry { id: id.into(), title: id.to_uppercase(), ..Default::default() };
        let column = |id: &str, entries: Vec<Entry>, preset: Option<&str>| Column {
            id: id.into(),
            title: id.into(),
            entries,
            preset: preset.map(str::to_string),
        };
        let l = Listing {
            columns: vec![
                column("country", vec![entry("mx"), entry("")], Some("mx")),
                column("empty", vec![], None),
                column("language", vec![entry("es")], Some("nowhere")),
                column("a", vec![entry("1")], None),
                column("b", vec![entry("1")], None),
                column("c", vec![entry("1")], None),
            ],
            ..Default::default()
        }
        .clean(None);
        assert_eq!(l.columns.len(), MAX_COLUMNS);
        assert_eq!(l.columns[0].entries.len(), 1, "an entry with no id is not one to pick");
        assert_eq!(l.columns[0].preset.as_deref(), Some("mx"));
        assert_eq!(l.columns[1].id, "language", "a column with nothing to pick is dropped");
        assert_eq!(l.columns[1].preset, None, "a preset that is not one of its entries is dropped");

        let asked: Browse = serde_json::from_str(r#"{"at":"","picks":{"country":"mx"}}"#).unwrap();
        assert_eq!(asked.picks.get("country").map(String::as_str), Some("mx"));
        assert!(serde_json::from_str::<Browse>(r#"{"at":""}"#).unwrap().picks.is_empty());
    }
}
