//! Telling a server what plays from it and what was heard (#51), as each
//! protocol allows it (read on 2026-11-26):
//!
//! - **OpenSubsonic.** With the `playbackReport` extension, `reportPlayback`
//!   says the state and the position; `ignoreScrobble=true` keeps it from
//!   counting the play. Without it, `scrobble` with `submission=false` says
//!   what plays now and nothing else. A play is counted apart, with
//!   `scrobble` and `submission=true`, which is also what the server hands on
//!   to the services its user connected.
//! - **Jellyfin.** `Sessions/Playing`, `Sessions/Playing/Progress` and
//!   `Sessions/Playing/Stopped`. The server counts the play when it hears
//!   that it started, so a report is a count and there is no count apart.
//! - **Plex.** `/:/timeline` says the state and the position; `/:/scrobble`
//!   marks the track as played.
//!
//! A DLNA server is told nothing.

use serde_json::json;

use crate::model::{Credential, Failure, Kind, Server};
use crate::net::{Net, Request};
use crate::text::encode;
use crate::{jellyfin, plex, subsonic};

/// Where a track stands, as a report says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    /// It began: the first report of a play.
    Started,
    Playing,
    Paused,
    Stopped,
}

/// One report: the track as the server names it, where in it the listener
/// is, and how long it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report<'a> {
    pub id: &'a str,
    pub state: PlayState,
    pub position_ms: u64,
    pub duration_ms: u64,
}

/// Whether the server takes reports at all.
pub fn reports(kind: Kind) -> bool {
    kind != Kind::Dlna
}

/// Whether a play is counted on its own, apart from the reports: false where
/// the reports themselves count it, and where nothing is counted.
pub fn counts_apart(kind: Kind) -> bool {
    matches!(kind, Kind::Subsonic | Kind::Plex)
}

/// Tells the server where a track stands. A state the protocol has no use
/// for sends nothing.
pub fn report(net: &dyn Net, server: &Server, key: &Credential, report: &Report<'_>) -> Result<(), Failure> {
    match server.kind {
        Kind::Subsonic => subsonic_report(net, server, key, report),
        Kind::Jellyfin => jellyfin_report(net, server, key, report),
        Kind::Plex => plex_report(net, server, key, report),
        Kind::Dlna => Ok(()),
    }
}

/// Counts a play of the track `id` that started at `started_ms` (unix
/// milliseconds), where a play is counted apart (`counts_apart`).
pub fn count(net: &dyn Net, server: &Server, key: &Credential, id: &str, started_ms: i64) -> Result<(), Failure> {
    match server.kind {
        Kind::Subsonic => {
            let extra = format!("&id={}&submission=true&time={started_ms}", encode(id));
            subsonic::send(net, Request::get(subsonic::url(&server.address, "scrobble", Some(key), &extra), Vec::new()), subsonic::error_of).map(|_| ())
        }
        Kind::Plex => {
            let url = format!("{}/:/scrobble?key={}&identifier=com.plexapp.plugins.library", server.address, encode(id));
            plex::ask(net, Request::get(url, plex::headers(&server.client, plex::server_token(key)))).map(|_| ())
        }
        Kind::Jellyfin | Kind::Dlna => Ok(()),
    }
}

fn subsonic_report(net: &dyn Net, server: &Server, key: &Credential, report: &Report<'_>) -> Result<(), Failure> {
    let extra = if server.details.extensions.iter().any(|name| name == "playbackReport") {
        let state = match report.state {
            PlayState::Started => "starting",
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Stopped => "stopped",
        };
        // The count is sent apart, at Chilpa's own mark, or not at all.
        format!("&mediaId={}&mediaType=song&positionMs={}&state={state}&ignoreScrobble=true", encode(report.id), report.position_ms)
    } else if report.state == PlayState::Started {
        format!("&id={}&submission=false", encode(report.id))
    } else {
        return Ok(());
    };
    let endpoint = if extra.starts_with("&mediaId=") { "reportPlayback" } else { "scrobble" };
    subsonic::send(net, Request::get(subsonic::url(&server.address, endpoint, Some(key), &extra), Vec::new()), subsonic::error_of).map(|_| ())
}

/// Jellyfin counts time in ticks of 100 ns.
const TICKS_PER_MS: u64 = 10_000;

fn jellyfin_report(net: &dyn Net, server: &Server, key: &Credential, report: &Report<'_>) -> Result<(), Failure> {
    let ticks = report.position_ms * TICKS_PER_MS;
    let (path, body) = match report.state {
        PlayState::Started => ("Sessions/Playing", json!({"ItemId": report.id, "PositionTicks": ticks, "IsPaused": false, "CanSeek": true, "PlayMethod": "DirectPlay"})),
        PlayState::Playing | PlayState::Paused => {
            let paused = report.state == PlayState::Paused;
            ("Sessions/Playing/Progress", json!({
                "ItemId": report.id, "PositionTicks": ticks, "IsPaused": paused, "CanSeek": true, "PlayMethod": "DirectPlay",
                "EventName": if paused { "Pause" } else { "TimeUpdate" },
            }))
        }
        PlayState::Stopped => ("Sessions/Playing/Stopped", json!({"ItemId": report.id, "PositionTicks": ticks})),
    };
    let request = Request::post(
        format!("{}/{path}", server.address),
        jellyfin::headers(&server.client, Some(key)),
        Some(("application/json".into(), body.to_string().into_bytes())),
    );
    jellyfin::ask(net, request).map(|_| ())
}

fn plex_report(net: &dyn Net, server: &Server, key: &Credential, report: &Report<'_>) -> Result<(), Failure> {
    let state = match report.state {
        PlayState::Started | PlayState::Playing => "playing",
        PlayState::Paused => "paused",
        PlayState::Stopped => "stopped",
    };
    let id = encode(report.id);
    let url = format!(
        "{}/:/timeline?ratingKey={id}&key=%2Flibrary%2Fmetadata%2F{id}&state={state}&time={}&duration={}",
        server.address, report.position_ms, report.duration_ms
    );
    plex::ask(net, Request::get(url, plex::headers(&server.client, plex::server_token(key)))).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Client;
    use crate::net::testing::Canned;

    const OK: &[u8] = br#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;

    fn client() -> Client {
        Client { version: "0.1.4".into(), platform: "Linux".into(), device: "desk".into(), id: "4f1c9a0b2d3e4f50".into() }
    }

    fn subsonic_server(extensions: &[&str]) -> (Server, Credential) {
        (subsonic::server("http://nd.lan:4533", client(), Vec::new(), extensions.iter().map(|e| e.to_string()).collect()),
            Credential::Password { user: "lab".into(), password: "pw".into() })
    }

    fn report_of(state: PlayState) -> Report<'static> {
        Report { id: "tr 1", state, position_ms: 61_000, duration_ms: 240_000 }
    }

    #[test]
    fn opensubsonic_reports_the_state_without_counting_the_play_when_it_can() {
        let (server, key) = subsonic_server(&["playbackReport"]);
        let net = Canned::new().answer(200, OK).answer(200, OK);
        report(&net, &server, &key, &report_of(PlayState::Started)).unwrap();
        report(&net, &server, &key, &report_of(PlayState::Paused)).unwrap();
        let asked = net.asked();
        assert!(asked[0].url.contains("/rest/reportPlayback.view?"), "{}", asked[0].url);
        assert!(asked[0].url.contains("&mediaId=tr%201&mediaType=song&positionMs=61000&state=starting&ignoreScrobble=true"), "{}", asked[0].url);
        assert!(asked[1].url.contains("&state=paused&"));
    }

    #[test]
    fn a_plain_subsonic_server_hears_only_what_plays_now_and_the_count_apart() {
        let (server, key) = subsonic_server(&[]);
        let net = Canned::new().answer(200, OK).answer(200, OK);
        report(&net, &server, &key, &report_of(PlayState::Started)).unwrap();
        report(&net, &server, &key, &report_of(PlayState::Paused)).unwrap();
        report(&net, &server, &key, &report_of(PlayState::Stopped)).unwrap();
        count(&net, &server, &key, "tr 1", 1_764_000_000_000).unwrap();
        let asked = net.asked();
        assert_eq!(asked.len(), 2, "a pause and a stop have nothing to say without the extension");
        assert!(asked[0].url.contains("/rest/scrobble.view?") && asked[0].url.contains("&id=tr%201&submission=false"), "{}", asked[0].url);
        assert!(asked[1].url.contains("&id=tr%201&submission=true&time=1764000000000"), "{}", asked[1].url);
    }

    #[test]
    fn a_refusal_from_the_server_is_said_and_not_swallowed() {
        let (server, key) = subsonic_server(&[]);
        let net = Canned::new().answer(200, br#"{"subsonic-response":{"status":"failed","error":{"code":70,"message":"not found"}}}"#);
        assert!(count(&net, &server, &key, "gone", 0).is_err());
    }

    #[test]
    fn jellyfin_hears_each_state_and_counts_nothing_apart() {
        let server = jellyfin::server("http://jf.lan:8096", client(), Vec::new(), "user");
        let key = Credential::Token { token: "tok".into() };
        let net = Canned::new().answer(204, b"").answer(204, b"").answer(204, b"");
        for state in [PlayState::Started, PlayState::Paused, PlayState::Stopped] {
            report(&net, &server, &key, &report_of(state)).unwrap();
        }
        count(&net, &server, &key, "tr 1", 0).unwrap();
        let asked = net.asked();
        assert_eq!(asked.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(), [
            "http://jf.lan:8096/Sessions/Playing", "http://jf.lan:8096/Sessions/Playing/Progress", "http://jf.lan:8096/Sessions/Playing/Stopped",
        ], "and no request for the count");
        let body: serde_json::Value = serde_json::from_slice(&asked[1].body.as_ref().unwrap().1).unwrap();
        assert_eq!((body["ItemId"].as_str(), body["PositionTicks"].as_u64(), body["IsPaused"].as_bool()), (Some("tr 1"), Some(610_000_000), Some(true)));
        assert!(!counts_apart(Kind::Jellyfin) && counts_apart(Kind::Subsonic) && counts_apart(Kind::Plex));
    }

    #[test]
    fn plex_hears_the_timeline_and_the_play_is_marked_apart() {
        let server = plex::server("http://plex.lan:32400", client(), Vec::new(), Vec::new());
        let key = Credential::Plex { account: "acct".into(), server: "srv".into() };
        let net = Canned::new().answer(200, b"").answer(200, b"");
        report(&net, &server, &key, &report_of(PlayState::Started)).unwrap();
        count(&net, &server, &key, "4242", 0).unwrap();
        let asked = net.asked();
        assert_eq!(asked[0].url, "http://plex.lan:32400/:/timeline?ratingKey=tr%201&key=%2Flibrary%2Fmetadata%2Ftr%201&state=playing&time=61000&duration=240000");
        assert_eq!(asked[1].url, "http://plex.lan:32400/:/scrobble?key=4242&identifier=com.plexapp.plugins.library");
        assert!(asked[1].headers.iter().any(|(k, v)| k == "X-Plex-Token" && v == "srv"));
        assert!(!reports(Kind::Dlna));
    }
}
