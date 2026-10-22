//! Discord Rich Presence: what plays, as the listener's Discord status.
//!
//! Ported from QBZ (`qbz-integrations/src/discord.rs` and the push in
//! `qbz-qt/src/integrations_qt.rs`). It is opt-in: nothing connects until
//! the listener turns it on, and a failed connect keeps quiet for five
//! minutes. The IPC calls block, so they run on a thread of their own, which
//! lives only while the integration is on. The facade tells it what plays on
//! each change of track and of play or pause, and clears it when nothing
//! plays. A track from a source names its plugin; a local one names none.
//!
//! Discord fetches the large image itself, from an address on the web: a
//! cover on this machine never reaches it. A source's cover has one; a
//! local record's is looked up by name on MusicBrainz, when it is allowed,
//! and the Cover Art Archive's front cover goes in its place.

use crate::Chilpa;
use chilpa_meta::{cover_art, musicbrainz, Http, Net};
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const ENABLED: &str = "integrations.discord.enabled";
/// Chilpa's public Discord application ID, not a secret. Its art assets
/// are "icon", the small badge, and "cover", the large image of a track
/// whose cover has no web address.
const APPLICATION_ID: &str = "1562851627461910589";
const HOMEPAGE: &str = "https://chilpa.app";
/// How long a failed IPC connect keeps the integration quiet. A change
/// inside the window pushes nothing; the first one after it retries.
const CONNECT_RETRY_AFTER: Duration = Duration::from_secs(300);
/// How many local records' covers are kept before they are all forgotten.
const COVERS_KEPT: usize = 256;
/// The archive's releases tried for a record, best match first, before
/// the cover of their group.
const RELEASES_TRIED: usize = 3;
/// What MusicBrainz has to score a release to be the record that plays, as
/// in the About tab.
const SURE: u8 = 90;
/// What a cover check may take and weigh: the 500 px front is ~100 KB.
const COVER_TIMEOUT: Duration = Duration::from_secs(10);
const COVER_LARGEST: usize = 2 * 1024 * 1024;

/// The "now listening" snapshot pushed to Discord.
#[derive(Debug, Clone, Default, PartialEq)]
struct NowListening {
    title: String,
    artist: String,
    album: String,
    /// The plugin the track comes from; empty for a local track.
    source: String,
    is_playing: bool,
    /// Current playback position, seconds.
    current_time: f64,
    /// Track duration, seconds (0 = unknown; no end timestamp then).
    duration: f64,
    /// An http(s) cover. `None` falls back to the "cover" asset key.
    cover_url: Option<String>,
}

enum Message {
    Update(NowListening),
    Clear,
}

/// What changes the presence: another track, play and pause, and a local
/// record's cover once it is found. The position is left out, so a track
/// that plays is not told again.
type Edge = (String, usize, String, String, String, bool, Option<String>);

/// A local record's front cover on the web, as far as it is known.
#[derive(Clone, Debug, PartialEq)]
enum Cover {
    Looking,
    Found(String),
    Missing,
}

pub(crate) struct Discord {
    /// The preference, read once and then kept by the setting.
    enabled: Option<bool>,
    worker: Option<Sender<Message>>,
    /// What the worker was last told; `None` after it cleared the presence.
    told: Option<Edge>,
    /// What the thread runs. Tests run one that never reaches Discord.
    serve: fn(Receiver<Message>),
    /// Local records' covers by album id, for the run.
    covers: Arc<Mutex<HashMap<i64, Cover>>>,
    /// How a record's cover is looked up, by title and artist. Tests look
    /// up nothing on the network.
    lookup: fn(&str, &str) -> Option<String>,
}

impl Default for Discord {
    fn default() -> Self {
        Self { enabled: None, worker: None, told: None, serve, covers: Arc::default(), lookup: cover_online }
    }
}

/// A release's or a group's 500 px front cover, when the archive has it.
fn front_at(http: &dyn Http, url: String) -> Option<String> {
    let reply = http.get(&url, "image/*", COVER_TIMEOUT, COVER_LARGEST).ok()?;
    (reply.status == 200 && !reply.body.is_empty()).then_some(url)
}

/// The front cover of the record MusicBrainz finds by name: of its best
/// releases, then of their group. `None` when no release is sure enough or
/// the archive has no cover of it.
fn front_cover(musicbrainz: &Net, http: &dyn Http, title: &str, artist: &str) -> Option<String> {
    let hits: Vec<_> = musicbrainz::search(musicbrainz, title, artist, "").ok()?.into_iter().filter(|hit| hit.score >= SURE).collect();
    hits.iter().take(RELEASES_TRIED)
        .find_map(|hit| front_at(http, cover_art::release_front(&hit.id, cover_art::PREVIEW).ok()?))
        .or_else(|| front_at(http, cover_art::group_front(&hits.first()?.release_group_id, cover_art::PREVIEW).ok()?))
}

fn cover_online(title: &str, artist: &str) -> Option<String> {
    let http = crate::metadata::worker::Network::new();
    front_cover(&Net::musicbrainz(&http), &http, title, artist)
}

/// Pure backoff rule: attempt when no deadline is set or it has passed.
fn connect_allowed(now: Instant, retry_after: Option<Instant>) -> bool {
    retry_after.is_none_or(|deadline| now >= deadline)
}

/// The small icon's tooltip: playing or paused, and where the track comes from.
fn small_text(meta: &NowListening) -> String {
    let state = if meta.is_playing {
        "Playing".to_string()
    } else {
        let mins = (meta.current_time / 60.0).floor() as u32;
        let secs = (meta.current_time % 60.0).floor() as u32;
        format!("Paused at {:02}:{:02}", mins, secs)
    };
    if meta.source.is_empty() { state } else { format!("{state} on {}", meta.source) }
}

/// Owns the IPC connection. Ends, clearing the presence, when the facade
/// drops its sender.
fn serve(messages: Receiver<Message>) {
    let mut client: Option<DiscordIpcClient> = None;
    let mut retry_after: Option<Instant> = None;
    for message in messages {
        match message {
            Message::Update(meta) => update(&mut client, &mut retry_after, &meta),
            Message::Clear => clear(&mut client),
        }
    }
    clear(&mut client);
}

/// Clear the activity and drop the IPC connection (no-op when not connected).
fn clear(client: &mut Option<DiscordIpcClient>) {
    if let Some(connected) = client.as_mut() {
        let _ = connected.clear_activity();
        let _ = connected.close();
    }
    *client = None;
}

/// Push the snapshot, connecting first when there is no connection. Silent
/// on every IPC failure (Discord closed, its socket not visible): the
/// connection is dropped so the next change retries cleanly.
fn update(client: &mut Option<DiscordIpcClient>, retry_after: &mut Option<Instant>, meta: &NowListening) {
    if client.is_none() {
        let now = Instant::now();
        if !connect_allowed(now, *retry_after) {
            return;
        }
        let mut connecting = DiscordIpcClient::new(APPLICATION_ID);
        if connecting.connect().is_ok() {
            *client = Some(connecting);
            *retry_after = None;
        } else {
            *retry_after = Some(now + CONNECT_RETRY_AFTER);
            eprintln!("chilpa-discord: Discord is not reachable (not running, or its socket is not visible); next attempt in {} s", CONNECT_RETRY_AFTER.as_secs());
        }
    }
    let Some(connected) = client.as_mut() else { return };
    let state = format!("by {}", meta.artist);
    let small_text = small_text(meta);
    let large_image = meta.cover_url.clone().unwrap_or_else(|| "cover".to_string());
    let mut act = activity::Activity::new()
        .details(&meta.title)
        .state(&state)
        .activity_type(activity::ActivityType::Listening)
        .assets(
            activity::Assets::new()
                .large_image(&large_image)
                .small_image("icon")
                .large_text(&meta.album)
                .small_text(&small_text),
        )
        .buttons(vec![activity::Button::new("Get Chilpa", HOMEPAGE)]);
    if meta.is_playing {
        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
            let start_time = now.as_secs() as i64 - meta.current_time as i64;
            let mut timestamps = activity::Timestamps::new().start(start_time);
            if meta.duration > 0.0 {
                timestamps = timestamps.end(start_time + meta.duration as i64);
            }
            act = act.timestamps(timestamps);
        }
    }
    if connected.set_activity(act).is_err() {
        *client = None;
    }
}

impl Chilpa {
    /// Turns the presence on or off. Off, the thread ends and the presence
    /// disappears; on again, it connects at once.
    pub(crate) fn discord_setting(&mut self, key: &str, value: &str) -> bool {
        if key != ENABLED {
            return false;
        }
        let on = value == "true";
        self.remember_preference(key, &on.to_string());
        self.discord.enabled = Some(on);
        true
    }

    /// Tells Discord what plays when it changed. Off or offline, nothing
    /// runs: Discord would carry the status over the network.
    pub(crate) fn advance_discord(&mut self) {
        let enabled = match self.discord.enabled {
            Some(enabled) => enabled,
            None => *self.discord.enabled.insert(self.preference(ENABLED).as_deref() == Some("true")),
        };
        if !enabled || self.offline() {
            // Dropping the sender ends the thread, which clears the presence.
            self.discord.worker = None;
            self.discord.told = None;
            return;
        }
        let cover = if self.now.playing { self.local_cover() } else { None };
        let edge = self.now.playing.then(|| {
            let now = &self.now;
            (now.path.clone(), now.index, now.title.clone(), now.artist.clone(), now.album.clone(), !now.paused, cover.clone())
        });
        if self.discord.worker.is_some() && edge == self.discord.told {
            return;
        }
        let message = if edge.is_some() { Message::Update(self.listening(cover)) } else { Message::Clear };
        let serve = self.discord.serve;
        let worker = self.discord.worker.get_or_insert_with(|| {
            let (tell, messages) = channel();
            let _ = std::thread::Builder::new().name("chilpa-discord".into()).spawn(move || serve(messages));
            tell
        });
        let _ = worker.send(message);
        self.discord.told = edge;
    }

    /// The front cover the Cover Art Archive has of the local record that
    /// plays, when MusicBrainz is allowed. Each record is looked up once, one
    /// at a time and off this thread; `None` until it lands, and for a
    /// record the archive has no cover of.
    fn local_cover(&mut self) -> Option<String> {
        let id = self.now.album_id;
        if id <= 0 || !self.musicbrainz_enabled() || crate::source_opener::of_plugin(&self.now.path).is_some() {
            return None;
        }
        let covers = self.discord.covers.clone();
        {
            let mut known = covers.lock().unwrap_or_else(|e| e.into_inner());
            match known.get(&id) {
                Some(Cover::Found(url)) => return Some(url.clone()),
                Some(_) => return None,
                // Skipping through records asks for the one that stays.
                None if known.values().any(|cover| *cover == Cover::Looking) => return None,
                None => {}
            }
            if known.len() >= COVERS_KEPT { known.clear(); }
            known.insert(id, Cover::Looking);
        }
        let found = self.album(id).map(|album| (album.title, album.artist)).filter(|(title, _)| !title.trim().is_empty());
        let lookup = self.discord.lookup;
        let landing = covers.clone();
        let started = found.and_then(|(title, artist)| std::thread::Builder::new().name("chilpa-discord-cover".into()).spawn(move || {
            let cover = lookup(&title, &artist).map_or(Cover::Missing, Cover::Found);
            landing.lock().unwrap_or_else(|e| e.into_inner()).insert(id, cover);
        }).ok());
        if started.is_none() {
            covers.lock().unwrap_or_else(|e| e.into_inner()).insert(id, Cover::Missing);
        }
        None
    }

    /// The snapshot of what plays. Discord fetches the cover itself, so a
    /// source's http(s) cover goes, or the one found for a local record;
    /// otherwise the asset.
    fn listening(&self, local_cover: Option<String>) -> NowListening {
        let now = &self.now;
        let plugin = crate::source_opener::of_plugin(&now.path).map(|(plugin, _)| plugin);
        let cover_url = plugin.as_ref().and_then(|_| self.catalog.track_by_path(&now.path)).map(|(track, _)| track.image)
            .filter(|url| url.starts_with("http://") || url.starts_with("https://")).or(local_cover);
        NowListening {
            title: now.title.clone(),
            artist: now.artist.clone(),
            album: now.album.clone(),
            source: plugin.map(|plugin| self.natives.display_name(&plugin)).unwrap_or_default(),
            is_playing: !now.paused,
            current_time: now.seconds,
            duration: now.total_seconds,
            cover_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_backoff_waits_for_the_deadline_then_retries() {
        let now = Instant::now();
        assert!(connect_allowed(now, None));
        let deadline = now + CONNECT_RETRY_AFTER;
        assert!(!connect_allowed(now, Some(deadline)));
        assert!(!connect_allowed(now + CONNECT_RETRY_AFTER / 2, Some(deadline)));
        assert!(connect_allowed(deadline, Some(deadline)));
        assert!(connect_allowed(deadline + Duration::from_secs(1), Some(deadline)));
    }

    #[test]
    fn a_source_is_named_and_a_local_track_names_none() {
        let mut meta = NowListening { is_playing: true, ..Default::default() };
        assert_eq!(small_text(&meta), "Playing");
        meta.source = "Radio".into();
        assert_eq!(small_text(&meta), "Playing on Radio");
        meta.is_playing = false;
        meta.current_time = 83.6;
        assert_eq!(small_text(&meta), "Paused at 01:23 on Radio");
    }

    #[test]
    fn it_runs_only_while_on_and_tells_only_what_changed() {
        let mut api = Chilpa::new();
        api.discord.serve = |messages| for _ in messages {};
        api.discord.enabled = Some(false);
        api.advance_discord();
        assert!(api.discord.worker.is_none(), "off, no thread");
        api.discord.enabled = Some(true);
        api.advance_discord();
        assert!(api.discord.worker.is_some(), "on, it says at once that nothing plays");
        assert_eq!(api.discord.told, None);
        api.now.playing = true;
        api.now.paused = false;
        api.now.path = "/music/a.flac".into();
        api.advance_discord();
        let told = api.discord.told.clone();
        assert!(told.as_ref().is_some_and(|edge| edge.5), "a track that plays is told");
        api.now.seconds = 30.0;
        api.advance_discord();
        assert_eq!(api.discord.told, told, "its position is not a change");
        api.now.paused = true;
        api.advance_discord();
        assert!(api.discord.told.as_ref().is_some_and(|edge| !edge.5), "a pause is");
        api.discord_setting(ENABLED, "false");
        api.advance_discord();
        assert!(api.discord.worker.is_none() && api.discord.told.is_none(), "off again, the thread ends");
    }

    /// Answers each address it knows, and writes down what it was asked.
    struct Fixture {
        known: Vec<(String, u16, String)>,
        asked: Mutex<Vec<String>>,
    }

    impl Http for Fixture {
        fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<chilpa_meta::Reply, String> {
            self.asked.lock().unwrap().push(url.to_string());
            let (_, status, body) = self.known.iter().find(|(known, ..)| url.starts_with(known.as_str())).ok_or("nothing here")?;
            Ok(chilpa_meta::Reply { status: *status, headers: Default::default(), body: body.clone().into_bytes() })
        }
    }

    const BEST: &str = "11111111-1111-1111-1111-111111111111";
    const NEXT: &str = "22222222-2222-2222-2222-222222222222";
    const POOR: &str = "33333333-3333-3333-3333-333333333333";
    const GROUP: &str = "99999999-9999-9999-9999-999999999999";

    fn found(fronts: &[(&str, u16)]) -> (Option<String>, Vec<String>) {
        let search = serde_json::json!({"releases": [
            {"id": BEST, "score": 100, "title": "Walküre Attack!", "release-group": {"id": GROUP}},
            {"id": NEXT, "score": 95, "title": "Walküre Attack!", "release-group": {"id": GROUP}},
            {"id": POOR, "score": 40, "title": "Walküre Trap!", "release-group": {"id": GROUP}}]});
        let mut known = vec![("https://musicbrainz.org/".to_string(), 200, search.to_string())];
        known.extend(fronts.iter().map(|(url, status)| (url.to_string(), *status, "jpeg".to_string())));
        let http = Fixture { known, asked: Mutex::default() };
        let pace = chilpa_meta::Pace::new(Duration::ZERO);
        let found = front_cover(&Net { http: &http, pace: &pace, nap: |_| {} }, &http, "Walküre Attack!", "Walküre");
        (found, http.asked.into_inner().unwrap())
    }

    #[test]
    fn a_local_record_s_cover_is_its_best_release_s_then_its_group_s() {
        let release = |id| cover_art::release_front(id, cover_art::PREVIEW).unwrap();
        let group = cover_art::group_front(GROUP, cover_art::PREVIEW).unwrap();
        let (cover, asked) = found(&[(&release(BEST), 404), (&release(NEXT), 200), (&group, 200)]);
        assert_eq!(cover, Some(release(NEXT)), "the best release the archive has a cover of");
        assert!(!asked.contains(&group), "the group only when no release has one");
        let (cover, asked) = found(&[(&release(BEST), 404), (&release(NEXT), 404), (&group, 200)]);
        assert_eq!(cover.as_deref(), Some(group.as_str()));
        assert!(!asked.contains(&release(POOR)), "a release MusicBrainz is not sure of is not this record");
        let (cover, _) = found(&[(&release(BEST), 404), (&release(NEXT), 404), (&group, 404)]);
        assert_eq!(cover, None, "no cover, and the asset stays");
    }

    #[test]
    fn a_local_record_is_told_again_once_its_cover_is_found() {
        let mut api = Chilpa::new();
        api.discord.serve = |messages| for _ in messages {};
        api.discord.lookup = |_, _| panic!("a record already being looked up is not asked again");
        api.discord.enabled = Some(true);
        api.now.playing = true;
        api.now.path = "/music/a.flac".into();
        api.now.album_id = 7;
        api.discord.covers.lock().unwrap().insert(7, Cover::Looking);
        api.advance_discord();
        assert_eq!(api.discord.told.as_ref().map(|edge| edge.6.clone()), Some(None), "the asset while it is looked up");
        let url = cover_art::release_front(BEST, cover_art::PREVIEW).unwrap();
        api.discord.covers.lock().unwrap().insert(7, Cover::Found(url.clone()));
        api.advance_discord();
        assert_eq!(api.discord.told.as_ref().map(|edge| edge.6.clone()), Some(Some(url)), "the cover, once it lands");
    }
}
