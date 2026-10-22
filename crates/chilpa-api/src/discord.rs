//! Discord Rich Presence: what plays, as the listener's Discord status.
//!
//! Ported from QBZ (`qbz-integrations/src/discord.rs` and the push in
//! `qbz-qt/src/integrations_qt.rs`). It is opt-in: nothing connects until
//! the listener turns it on, and a failed connect keeps quiet for five
//! minutes. The IPC calls block, so they run on a thread of their own, which
//! lives only while the integration is on. The facade tells it what plays on
//! each change of track and of play or pause, and clears it when nothing
//! plays. A track from a source names its plugin; a local one names none.

use crate::Chilpa;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc::{channel, Receiver, Sender};
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

/// What changes the presence: another track, or play and pause. The
/// position is left out, so a track that plays is not told again.
type Edge = (String, usize, String, String, String, bool);

pub(crate) struct Discord {
    /// The preference, read once and then kept by the setting.
    enabled: Option<bool>,
    worker: Option<Sender<Message>>,
    /// What the worker was last told; `None` after it cleared the presence.
    told: Option<Edge>,
    /// What the thread runs. Tests run one that never reaches Discord.
    serve: fn(Receiver<Message>),
}

impl Default for Discord {
    fn default() -> Self {
        Self { enabled: None, worker: None, told: None, serve }
    }
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
        let edge = self.now.playing.then(|| {
            let now = &self.now;
            (now.path.clone(), now.index, now.title.clone(), now.artist.clone(), now.album.clone(), !now.paused)
        });
        if self.discord.worker.is_some() && edge == self.discord.told {
            return;
        }
        let message = if edge.is_some() { Message::Update(self.listening()) } else { Message::Clear };
        let serve = self.discord.serve;
        let worker = self.discord.worker.get_or_insert_with(|| {
            let (tell, messages) = channel();
            let _ = std::thread::Builder::new().name("chilpa-discord".into()).spawn(move || serve(messages));
            tell
        });
        let _ = worker.send(message);
        self.discord.told = edge;
    }

    /// The snapshot of what plays. Discord fetches the cover itself, so only
    /// a source's http(s) cover goes; a local cover falls back to the asset.
    fn listening(&self) -> NowListening {
        let now = &self.now;
        let plugin = crate::source_opener::of_plugin(&now.path).map(|(plugin, _)| plugin);
        let cover_url = plugin.as_ref().and_then(|_| self.catalog.track_by_path(&now.path)).map(|(track, _)| track.image)
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
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
}
