//! The server a track plays from, told about it (#51): what plays and
//! where, and the play counted once enough of it was heard, each turned off
//! on its own in the server's playback settings, both on until then.
//!
//! What can be told and what counts is each protocol's
//! (`chilpa_servers::report`): an OpenSubsonic server or Plex hears the
//! state apart from the count, and the count goes at the server's own mark;
//! Jellyfin counts what it hears started, so it has no count apart.
//!
//! Nothing here waits on a server. A thread of its own sends, and when it
//! falls behind what was not sent yet is let go: a report is worth nothing
//! late, and playback never hears about any of it.

use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chilpa_servers::report::{counts_apart, reports, PlayState, Report};
use chilpa_servers::Kind;
use serde_json::json;

use super::keys::Keys;

/// A server's settings, by its source id: `servers.<id>.reportPlayback`.
pub(crate) const REPORT: &str = "reportPlayback";
pub(crate) const COUNT: &str = "countPlays";
pub(crate) const MARK: &str = "playMark";
/// Where a play counts when nobody moved it, and the range it moves in.
const MARK_DEFAULT: u8 = 50;
const MARK_RANGE: std::ops::RangeInclusive<u8> = 10..=100;
/// How often what plays is told again while nothing changes.
const EVERY: Duration = Duration::from_secs(30);
/// Reports waiting to be sent. Past it the newest is let go.
const WAITING: usize = 32;

fn key(source: i64, name: &str) -> String {
    format!("servers.{source}.{name}")
}

/// What a server is told, as its settings say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Telling {
    pub report: bool,
    pub count: bool,
    pub mark: u8,
}

impl Telling {
    fn read(settings: &std::collections::BTreeMap<String, String>, source: i64) -> Self {
        let on = |name: &str| settings.get(&key(source, name)).is_none_or(|value| value != "false");
        let mark = settings.get(&key(source, MARK)).and_then(|value| value.parse::<u8>().ok())
            .filter(|mark| MARK_RANGE.contains(mark)).unwrap_or(MARK_DEFAULT);
        Self { report: on(REPORT), count: on(COUNT), mark }
    }
}

enum Order {
    Report { entry: String, state: PlayState, position_ms: u64, duration_ms: u64 },
    Count { entry: String, started_ms: i64 },
}

/// The sending thread, from the facade's side.
#[derive(Clone)]
pub(crate) struct Reporter(SyncSender<Order>);

impl Reporter {
    pub(crate) fn start(keys: Arc<Keys>) -> Self {
        let (orders, waiting) = sync_channel(WAITING);
        let _ = std::thread::Builder::new().name("chilpa-server-report".into()).spawn(move || deliver(keys, waiting));
        Self(orders)
    }

    /// Never waits: a full line lets the report go.
    fn send(&self, order: Order) {
        let _ = self.0.try_send(order);
    }
}

fn deliver(keys: Arc<Keys>, waiting: Receiver<Order>) {
    let mut library: Option<chilpa_library::Library> = None;
    while let Ok(order) = waiting.recv() {
        if chilpa_core::network::current().offline {
            continue;
        }
        if library.is_none() {
            library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
        }
        let Some(lib) = library.as_ref() else { continue };
        let entry = match &order {
            Order::Report { entry, .. } | Order::Count { entry, .. } => entry,
        };
        let Some(remote) = chilpa_models::RemoteEntry::parse(entry) else { continue };
        let Some(stored) = lib.find_source(&remote.source.provider, &remote.source.instance).ok().flatten()
            .and_then(|source| lib.server(source).ok().flatten()).filter(|stored| stored.enabled) else { continue };
        let (Some(server), Some(credential)) = (super::net::server(&stored), super::net::key(&keys, &stored)) else { continue };
        let net = super::net::Network;
        // A server that does not answer or refuses is not playback's
        // business: it is said in the trace and let go.
        let sent = match order {
            Order::Report { state, position_ms, duration_ms, .. } => chilpa_servers::report::report(&net, &server, &credential,
                &Report { id: &remote.external_id, state, position_ms, duration_ms }),
            Order::Count { started_ms, .. } => chilpa_servers::report::count(&net, &server, &credential, &remote.external_id, started_ms),
        };
        if let Err(error) = sent {
            chilpa_core::trace_playback("servers", "report_failed", format_args!("server={} error={error:?}", stored.source_id));
        }
    }
}

/// The server track in front of the window, as it was last told.
pub(crate) struct Shown {
    entry: String,
    duration_ms: u64,
    /// Where it was at the last tick: a stop is told where the track was
    /// left, after the window moved on to the next one.
    position_ms: u64,
    paused: bool,
    told: Instant,
}

/// Counts a listen on its server once enough of it was heard, once.
pub(crate) fn if_heard(reporter: Option<&Reporter>, listen: &mut crate::InProgress) {
    let Some(mark) = listen.server_mark else { return };
    if listen.server_counted || listen.duration_ms == 0 || listen.sample_rate_hz == 0 {
        return;
    }
    if listen.heard_ms() * 100 >= listen.duration_ms * u64::from(mark) {
        listen.server_counted = true;
        if let Some(reporter) = reporter {
            reporter.send(Order::Count { entry: listen.path.to_string_lossy().into_owned(), started_ms: listen.started_at * 1000 });
        }
    }
}

impl crate::Chilpa {
    /// The server a path's track is on, its kind and what it is told; none
    /// for a track that is not on a server, or on one turned off.
    fn telling_of(&mut self, path: &Path) -> Option<(i64, Kind, Telling)> {
        let remote = chilpa_models::RemoteEntry::parse(path.to_str()?)?;
        let lib = self.library()?;
        let source = lib.find_source(&remote.source.provider, &remote.source.instance).ok()??;
        let stored = lib.server(source).ok()??;
        let kind = Kind::from_provider(&stored.provider).filter(|kind| reports(*kind) && stored.enabled)?;
        Some((source, kind, Telling::read(&chilpa_core::settings::load().window, source)))
    }

    /// A track started: its server hears it, when it is the one in front of
    /// the window and told so. Returns the mark at which the play is counted
    /// there, when it is.
    pub(crate) fn server_listen_started(&mut self, path: &Path, duration_ms: u64, shown: bool) -> Option<u8> {
        let (_, kind, telling) = self.telling_of(path)?;
        if shown {
            self.server_stop_shown();
            if telling.report {
                let entry = path.to_string_lossy().into_owned();
                if let Some(reporter) = &self.servers.reporter {
                    reporter.send(Order::Report { entry: entry.clone(), state: PlayState::Started, position_ms: 0, duration_ms });
                }
                self.servers.shown = Some(Shown { entry, duration_ms, position_ms: 0, paused: false, told: Instant::now() });
            }
        }
        (telling.count && counts_apart(kind)).then_some(telling.mark)
    }

    /// The window's tick: a pause or a resume is told at once, and while it
    /// plays, where it is every so often. A play that ended is told so.
    pub(crate) fn server_tick(&mut self) {
        if self.servers.shown.is_none() {
            return;
        }
        if !self.now.playing {
            self.server_stop_shown();
            return;
        }
        let (paused, position_ms) = (self.now.paused, (self.now.seconds.max(0.0) * 1000.0) as u64);
        let Some(shown) = self.servers.shown.as_mut() else { return };
        shown.position_ms = position_ms;
        if paused == shown.paused && shown.told.elapsed() < EVERY {
            return;
        }
        shown.paused = paused;
        shown.told = Instant::now();
        let state = if paused { PlayState::Paused } else { PlayState::Playing };
        if let Some(reporter) = &self.servers.reporter {
            reporter.send(Order::Report { entry: shown.entry.clone(), state, position_ms, duration_ms: shown.duration_ms });
        }
    }

    fn server_stop_shown(&mut self) {
        let Some(shown) = self.servers.shown.take() else { return };
        if let Some(reporter) = &self.servers.reporter {
            reporter.send(Order::Report { entry: shown.entry, state: PlayState::Stopped, position_ms: shown.position_ms, duration_ms: shown.duration_ms });
        }
    }

    /// Settings: the server's menu opens its playback settings, and its
    /// line says when it is told nothing.
    pub(crate) fn server_playback_row(&mut self, source: i64, description: &mut String, values: &mut Vec<String>, options: &mut Vec<String>) {
        let Some(stored) = self.library().and_then(|lib| lib.server(source).ok().flatten()) else { return };
        if !Kind::from_provider(&stored.provider).is_some_and(reports) {
            return;
        }
        values.push("playback".into());
        options.push(chilpa_i18n::t("Playback settings…"));
        if !Telling::read(&chilpa_core::settings::load().window, source).report {
            description.push(' ');
            description.push_str(&chilpa_i18n::t("What you play is not reported to it."));
        }
    }

    /// A server's playback settings, for its dialog: what it is, what it
    /// can be told, and what it is told.
    pub fn server_playback_json(&mut self, source: i64) -> String {
        let Some(stored) = self.library().and_then(|lib| lib.server(source).ok().flatten()) else { return "{}".into() };
        let Some(kind) = Kind::from_provider(&stored.provider) else { return "{}".into() };
        let telling = Telling::read(&chilpa_core::settings::load().window, source);
        json!({
            "id": source, "name": stored.label, "kind": stored.provider,
            "reports": reports(kind), "counts_apart": counts_apart(kind),
            "report": telling.report, "count": telling.count, "mark": telling.mark,
        })
        .to_string()
    }

    /// Changes one of a server's playback settings: `reportPlayback` or
    /// `countPlays` to "true" or "false", `playMark` to a percent from 10 to
    /// 100. Anything else is refused.
    pub fn set_server_playback(&mut self, source: i64, name: &str, value: &str) -> bool {
        let valid = match name {
            REPORT | COUNT => matches!(value, "true" | "false"),
            MARK => value.parse::<u8>().is_ok_and(|mark| MARK_RANGE.contains(&mark)),
            _ => false,
        };
        if !valid || self.library().and_then(|lib| lib.server(source).ok().flatten()).is_none() {
            return false;
        }
        self.remember_preference(&key(source, name), value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn a_server_is_told_and_counted_until_its_settings_say_otherwise() {
        assert_eq!(Telling::read(&settings(&[]), 7), Telling { report: true, count: true, mark: 50 });
        let off = settings(&[("servers.7.reportPlayback", "false"), ("servers.7.countPlays", "false"), ("servers.7.playMark", "80")]);
        assert_eq!(Telling::read(&off, 7), Telling { report: false, count: false, mark: 80 });
        assert_eq!(Telling::read(&off, 8), Telling { report: true, count: true, mark: 50 }, "each server its own");
        let odd = settings(&[("servers.7.playMark", "5"), ("servers.7.countPlays", "maybe")]);
        assert_eq!(Telling::read(&odd, 7), Telling { report: true, count: true, mark: 50 }, "what cannot be read is the default");
    }

    fn listening(mark: Option<u8>, heard_ms: u64) -> crate::InProgress {
        crate::InProgress {
            context: Default::default(), path: "x".into(), carried_ms: 0, started_at: 1_764_000_000,
            sample_rate_hz: 1000, bit_depth: None, bit_perfect: false, duration_ms: 200_000,
            played_frames: heard_ms, listen: None, scrobbled: false, server_mark: mark, server_counted: false,
        }
    }

    #[test]
    fn a_listen_is_counted_on_its_server_at_its_mark_and_once() {
        let (orders, sent) = sync_channel(8);
        let reporter = Reporter(orders);
        let mut early = listening(Some(50), 99_000);
        if_heard(Some(&reporter), &mut early);
        assert!(!early.server_counted && sent.try_recv().is_err(), "not yet half");
        let mut half = listening(Some(50), 100_000);
        if_heard(Some(&reporter), &mut half);
        if_heard(Some(&reporter), &mut half);
        assert!(half.server_counted);
        assert!(matches!(sent.try_recv(), Ok(Order::Count { started_ms: 1_764_000_000_000, .. })));
        assert!(sent.try_recv().is_err(), "counted once");
        let mut uncounted = listening(None, 200_000);
        if_heard(Some(&reporter), &mut uncounted);
        assert!(sent.try_recv().is_err(), "a server told not to count hears nothing");
    }

    /// Runs `name` again in a child with a profile of its own: it writes a
    /// library and settings.
    fn own_profile(name: &str) -> bool {
        const CHILD: &str = "CHILPA_SERVER_REPORT_TEST";
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
    fn a_track_on_a_server_is_told_as_its_settings_say() {
        if !own_profile("servers::report::tests::a_track_on_a_server_is_told_as_its_settings_say") { return; }
        let keys = Keys::derived(chilpa_dirs::data().join("servers"));
        let ready = super::super::setup::Ready { kind: Kind::Subsonic, instance: super::super::keys::mint(), label: "Lab".into(), offline_folder: String::new(),
            address: "http://fixture.invalid".into(), account: "lab".into(), identity: "lab-server".into(), product: "Fixture".into(), libraries: vec![],
            details: Default::default(), key: chilpa_servers::Credential::Password { user: "lab".into(), password: "pw".into() } };
        let path = chilpa_library::Library::default_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _library = chilpa_library::Library::open(&path).unwrap();
        let source = super::super::write_server(&path, &keys, None, &ready).unwrap();
        let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("subsonic", ready.instance.clone()), "tr-1").to_entry();
        let mut api = crate::Chilpa::new();
        let (orders, sent) = sync_channel(8);
        api.servers.reporter = Some(Reporter(orders));

        assert_eq!(api.server_listen_started(Path::new(&entry), 200_000, true), Some(50));
        assert!(matches!(sent.try_recv(), Ok(Order::Report { state: PlayState::Started, position_ms: 0, duration_ms: 200_000, .. })));
        (api.now.playing, api.now.paused, api.now.seconds) = (true, true, 12.0);
        api.server_tick();
        assert!(matches!(sent.try_recv(), Ok(Order::Report { state: PlayState::Paused, position_ms: 12_000, .. })), "a pause is told at once");
        api.now.seconds = 12.5;
        api.server_tick();
        assert!(sent.try_recv().is_err(), "nothing changed");
        api.now.playing = false;
        api.server_tick();
        assert!(matches!(sent.try_recv(), Ok(Order::Report { state: PlayState::Stopped, position_ms: 12_500, .. })), "where it was left");
        api.server_tick();
        assert!(sent.try_recv().is_err(), "the stop is told once");

        assert!(api.set_server_playback(source, REPORT, "false"));
        assert!(!api.set_server_playback(source, MARK, "5"), "below the range");
        assert!(!api.set_server_playback(source, "other", "true"));
        assert!(!api.set_server_playback(source + 1, COUNT, "false"), "no such server");
        assert!(api.set_server_playback(source, MARK, "70"));
        assert_eq!(api.server_listen_started(Path::new(&entry), 200_000, true), Some(70), "the count is its own switch");
        assert!(sent.try_recv().is_err(), "a server told nothing hears nothing");
        assert!(api.set_server_playback(source, COUNT, "false"));
        assert_eq!(api.server_listen_started(Path::new(&entry), 200_000, true), None);
        let shown: serde_json::Value = serde_json::from_str(&api.server_playback_json(source)).unwrap();
        assert_eq!((shown["report"].as_bool(), shown["count"].as_bool(), shown["counts_apart"].as_bool(), shown["mark"].as_u64()),
            (Some(false), Some(false), Some(true), Some(70)));
        assert_eq!(api.server_listen_started(Path::new("/music/a.flac"), 1, true), None, "a file is on no server");
    }
}
