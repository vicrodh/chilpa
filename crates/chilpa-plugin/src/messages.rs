//! What Chilpa and a plugin say to each other.
//!
//! These are the v1 API messages, in JSON (ADR-007). They live here and not
//! in the facade so that a first party plugin and a third party one speak
//! exactly the same language: if the API cannot carry our own features, it
//! is no good (ADR-003).
//!
//! Two rules visible in the types:
//!
//! 1. **Nothing names a service** (ADR-004). A plugin describes what is its
//!    own with data -- external id, quality, availability -- never with a
//!    platform's concepts.
//! 2. **Everything coming in is cleaned.** A plugin can answer anything;
//!    what gets past here is already trimmed, with no empty fields and no
//!    endless lists.

use serde::{Deserialize, Serialize};

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
}

/// How much is accepted from a single reply, per kind of entity.
///
/// A plugin that answers a hundred thousand tracks is not helping: it is
/// filling Chilpa's memory. It is trimmed and things move on.
pub const MAX_PER_REPLY: usize = 500;

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

    /// How the user is told when it cannot be done.
    pub fn describe(&self) -> String {
        match self {
            Delivery::Progressive => chilpa_i18n::t("a direct URL"),
            Delivery::Dash => {
                chilpa_i18n::t("a DASH manifest, which Chilpa cannot read yet")
            }
            Delivery::Hls => chilpa_i18n::t("an HLS manifest, which Chilpa cannot read yet"),
        }
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

/// Reads what a plugin answered, trusting nothing.
pub fn read<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| chilpa_i18n::t_args("it answered something that is not understood: {}", &[&e.to_string()]))
}

/// Builds what is going to be sent to it.
pub fn write<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec())
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
    fn what_is_useless_is_dropped_and_what_is_extra_is_trimmed() {
        let mut r = Results::default();
        r.tracks.push(RemoteTrack { external_id: "a".into(), title: "One".into(), ..Default::default() });
        // With no id it cannot be asked for again; with no title it cannot be shown.
        r.tracks.push(RemoteTrack { external_id: "".into(), title: "Another".into(), ..Default::default() });
        r.tracks.push(RemoteTrack { external_id: "b".into(), title: "   ".into(), ..Default::default() });
        for i in 0..1000 {
            r.albums.push(RemoteAlbum {
                external_id: i.to_string(),
                title: "Disco".into(),
                ..Default::default()
            });
        }
        let r = r.clean();
        assert_eq!(r.tracks.len(), 1, "only the one that works");
        assert_eq!(r.albums.len(), MAX_PER_REPLY, "the rest is trimmed");
        assert_eq!(r.count(), 1 + MAX_PER_REPLY);
    }

    #[test]
    fn the_fingerprint_prefers_the_mbid_and_normalizes() {
        let t = RemoteTrack {
            isrc: Some(" usrc17607839 ".into()),
            musicbrainz_recording_id: Some(" ABC-123 ".into()),
            ..Default::default()
        };
        assert_eq!(t.fingerprint().as_deref(), Some("mbid:abc-123"));

        let isrc_only = RemoteTrack { isrc: Some("usrc17607839".into()), ..Default::default() };
        assert_eq!(isrc_only.fingerprint().as_deref(), Some("isrc:USRC17607839"));

        // With nothing solid there is no fingerprint: matching by name is not matching.
        assert_eq!(RemoteTrack::default().fingerprint(), None);
        let blank = RemoteTrack {
            isrc: Some("  ".into()),
            musicbrainz_recording_id: Some("".into()),
            ..Default::default()
        };
        assert_eq!(blank.fingerprint(), None);
    }

    #[test]
    fn a_half_known_quality_is_described_as_half_known() {
        assert_eq!(Quality::default().describe(), "");
        let c = Quality { codec: Some("flac".into()), bit_depth: Some(24), sample_rate_hz: Some(96_000), channels: Some(2) };
        assert_eq!(c.describe(), "FLAC · 24 bits a 96000 Hz");
        let rate_only = Quality { sample_rate_hz: Some(44_100), ..Default::default() };
        assert_eq!(rate_only.describe(), "44100 Hz");
    }

    #[test]
    fn a_track_can_come_in_several_files_read_in_order() {
        // This is the normal case in hi-res: the service's manifest carries
        // several whole URLs. The plugin decodes it and hands over the list;
        // Chilpa knows nothing about manifests.
        let s: Stream = read(
            br#"{"urls":["https://a.b/1.flac","https://a.b/2.flac"],"codec":"flac"}"#,
        )
        .unwrap();
        assert_eq!(s.parts(), ["https://a.b/1.flac", "https://a.b/2.flac"]);
        assert!(s.usable());

        // A single one is still written the way it always was.
        let one: Stream = read(br#"{"url":"https://a.b/x.flac"}"#).unwrap();
        assert_eq!(one.parts(), ["https://a.b/x.flac"]);
    }

    #[test]
    fn one_bad_part_spoils_the_whole_stream() {
        // One part that is not http is enough not to try any: reading half a
        // track and failing is worse than not starting.
        let s: Stream =
            read(br#"{"urls":["https://a.b/1.flac","file:///etc/passwd"]}"#).unwrap();
        assert!(!s.usable());
        assert!(read::<Stream>(br#"{"codec":"flac"}"#).unwrap().parts().is_empty());
    }

    #[test]
    fn a_stream_that_is_not_http_is_not_attempted() {
        let bad = Stream { url: "file:///etc/passwd".into(), ..Default::default() };
        assert!(!bad.usable());
        let good = Stream { url: "https://a.b/c".into(), ..Default::default() };
        assert!(good.usable());
    }

    #[test]
    fn a_link_with_a_date_expires() {
        let s = Stream {
            url: "https://a.b/c".into(),
            expires_at: Some(1000),
            ..Default::default()
        };
        assert!(!s.expired(999));
        assert!(s.expired(1000), "right at the edge it no longer works");
        assert!(s.expired(1001));
        // With no date it does not expire.
        let undated = Stream { expires_at: None, ..s };
        assert!(!undated.expired(i64::MAX));
    }

    #[test]
    fn what_a_plugin_answers_is_read_or_it_says_why_not() {
        let r: Results = read(br#"{"tracks":[{"external_id":"a","title":"One"}]}"#).unwrap();
        assert_eq!(r.tracks.len(), 1);
        assert_eq!(r.tracks[0].artist, "", "what is missing takes its default value");

        let e = read::<Results>(b"this is not json").expect_err("not understood");
        assert!(e.contains("not understood"), "{e}");
    }

    #[test]
    fn one_extra_field_does_not_break_the_read() {
        // A plugin from a newer version can send fields this Chilpa does not
        // know; they are ignored instead of rejecting everything.
        let r: Results =
            read(br#"{"tracks":[{"external_id":"a","title":"One","something_new":42}]}"#).unwrap();
        assert_eq!(r.tracks.len(), 1);
    }
}
