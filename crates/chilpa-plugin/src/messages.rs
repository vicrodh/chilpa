//! What Chilpa and a plugin say to each other.
//!
//! The v1 API messages, in JSON (ADR-007). The types themselves live in
//! `chilpa-plugin-api`, a crate of its own outside this repository, so that
//! a plugin builds against the very definition the host reads with: if the
//! API cannot carry our own features, it is no good (ADR-003). They are
//! re-exported here, and everything in Chilpa keeps reaching them through
//! this module.
//!
//! What stays on this side is what a plugin has no use for: turning bytes
//! into messages without trusting them, and wording things for the user.

pub use chilpa_plugin_api::*;
use serde::{Deserialize, Serialize};

/// How the user is told what a [`Delivery`] is when it cannot be played.
///
/// It lives here and not on the type because it goes through the catalog,
/// and a plugin does not carry Chilpa's translations.
pub fn describe_delivery(delivery: Delivery) -> String {
    match delivery {
        Delivery::Progressive => chilpa_i18n::t("a direct URL"),
        Delivery::Dash => chilpa_i18n::t("a DASH manifest, which Chilpa cannot read yet"),
        Delivery::Hls => chilpa_i18n::t("an HLS manifest, which Chilpa cannot read yet"),
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
