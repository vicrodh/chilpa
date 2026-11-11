//! Discord Rich Presence: what plays, as the listener's Discord status,
//! the way Fooyin's Discord plugin shows it (`src/plugins/discord`).
//!
//! It is opt-in, and off while Chilpa is offline. The window's thread only
//! notices what changed -- another track, play and pause, a setting, a
//! seek -- and hands a snapshot to a thread of its own, which lives only
//! while the presence is on. That thread reads the file's tags, fills in
//! the three track formats, looks for the cover and talks to Discord
//! (`presence`). The settings are this feature's, here; the window's
//! bridge learns none of their keys (frontend ADR-0015).
//!
//! Chilpa keeps its own Discord application, its icon, its "cover" asset
//! and its button; the behaviour is Fooyin's.

mod artwork;
mod format;
mod ipc;
mod presence;
mod upload;

use crate::Chilpa;
use presence::{Message, Options, Playing, Source};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const ENABLED: &str = "integrations.discord.enabled";
const SHOW_STATE: &str = "integrations.discord.showState";
const CLEAR_ON_PAUSE: &str = "integrations.discord.clearOnPause";
const TITLE_FORMAT: &str = "integrations.discord.titleFormat";
const ARTIST_FORMAT: &str = "integrations.discord.artistFormat";
const ALBUM_FORMAT: &str = "integrations.discord.albumFormat";
const ARTWORK: &str = "integrations.discord.artwork";
const UPLOAD: &str = "integrations.discord.upload";
const SOURCE: &str = "integrations.discord.artworkSource";
const RETENTION: &str = "integrations.discord.artworkRetention";

/// The switches and their defaults: Fooyin's, but the artwork, which
/// Chilpa showed before Fooyin's behaviour came. Uploading the cover to a
/// public host is the listener's choice, and off until it is made.
const SWITCHES: [(&str, bool); 4] = [(SHOW_STATE, true), (CLEAR_ON_PAUSE, false), (ARTWORK, true), (UPLOAD, false)];
/// Which way to a cover goes first once uploading is allowed, as Fooyin
/// lists them; the first is the default.
const SOURCES: [(&str, Source, &str); 3] = [
    ("search-then-upload", Source::SearchThenUpload, chilpa_i18n::mark("Search MusicBrainz, or upload if not found")),
    ("upload", Source::UploadOnly, chilpa_i18n::mark("Upload only")),
    ("upload-then-search", Source::UploadThenSearch, chilpa_i18n::mark("Upload, or search MusicBrainz if it fails")),
];
/// How long the host keeps an upload, Fooyin's four; 24 hours by default.
const RETENTIONS: [(&str, &str); 4] = [
    ("1", chilpa_i18n::mark("1 hour")),
    ("12", chilpa_i18n::mark("12 hours")),
    ("24", chilpa_i18n::mark("24 hours")),
    ("72", chilpa_i18n::mark("72 hours")),
];
const RETENTION_DEFAULT: &str = "24";
/// The formats and their defaults, Fooyin's.
const FORMATS: [(&str, &str); 3] = [(TITLE_FORMAT, "%title%"), (ARTIST_FORMAT, "%artist%"), (ALBUM_FORMAT, "%album%")];
/// What a format may be, in characters.
const FORMAT_LONGEST: usize = 512;
/// A playing track whose start moved further than this was sought: its
/// timestamps are given again (Fooyin's `positionMoved`).
const DRIFT: i64 = 2;

/// The thread, by the sender that feeds it. Dropping it tells the thread
/// to clear the status and end.
struct Worker(Sender<Message>);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.send(Message::Stop);
    }
}

/// What the thread was last handed for a track that plays.
struct Told {
    path: String,
    index: usize,
    title: String,
    artist: String,
    album: String,
    paused: bool,
    started_at: i64,
    artwork_path: String,
    options: Arc<Options>,
}

pub(crate) struct Discord {
    /// The preference, read once and then kept by the setting.
    enabled: Option<bool>,
    /// The rest of the settings, read once, and again after one changes.
    options: Option<Arc<Options>>,
    worker: Option<Worker>,
    /// What the thread was last told; `None` after it was told to clear.
    told: Option<Told>,
    /// What the thread runs. Tests run one that never reaches Discord.
    serve: fn(Receiver<Message>, Sender<Message>),
    /// The time, in Unix seconds. Tests set it.
    clock: fn() -> f64,
}

fn unix_seconds() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_secs_f64()).unwrap_or_default()
}

impl Default for Discord {
    fn default() -> Self {
        Self { enabled: None, options: None, worker: None, told: None, serve: presence::serve, clock: unix_seconds }
    }
}

impl Chilpa {
    fn discord_switch(&self, key: &str) -> bool {
        let default = SWITCHES.iter().find(|(known, _)| *known == key).is_some_and(|(_, on)| *on);
        self.preference(key).map_or(default, |value| value == "true")
    }

    /// A format as set, or its default when it is not set or is empty.
    fn discord_format(&self, key: &str) -> String {
        let default = FORMATS.iter().find(|(known, _)| *known == key).map_or("", |(_, format)| format);
        self.preference(key).filter(|format| !format.trim().is_empty()).unwrap_or_else(|| default.to_string())
    }

    /// The way to a cover that goes first, as set, or the default.
    fn discord_source(&self) -> &'static str {
        let chosen = self.preference(SOURCE);
        SOURCES.iter().map(|(value, ..)| *value).find(|value| chosen.as_deref() == Some(*value)).unwrap_or(SOURCES[0].0)
    }

    /// The hours an upload is kept, as set, or the default.
    fn discord_retention(&self) -> &'static str {
        let chosen = self.preference(RETENTION);
        RETENTIONS.iter().map(|(value, _)| *value).find(|value| chosen.as_deref() == Some(*value)).unwrap_or(RETENTION_DEFAULT)
    }

    fn discord_enabled(&self) -> bool {
        self.discord.enabled.unwrap_or_else(|| self.preference(ENABLED).as_deref() == Some("true"))
    }

    fn discord_options(&mut self) -> Arc<Options> {
        if let Some(options) = &self.discord.options {
            return options.clone();
        }
        let options = Arc::new(Options {
            show_state: self.discord_switch(SHOW_STATE),
            clear_on_pause: self.discord_switch(CLEAR_ON_PAUSE),
            title: self.discord_format(TITLE_FORMAT),
            artist: self.discord_format(ARTIST_FORMAT),
            album: self.discord_format(ALBUM_FORMAT),
            artwork: self.discord_switch(ARTWORK),
            upload: self.discord_switch(UPLOAD),
            source: SOURCES.iter().find(|(value, ..)| *value == self.discord_source()).map_or(Source::SearchThenUpload, |(_, source, _)| *source),
            hours: self.discord_retention().parse().unwrap_or(24),
        });
        self.discord.options = Some(options.clone());
        options
    }

    /// Keeps a setting of the presence. Turned off, the thread ends and the
    /// status goes; any other change is told to Discord at once, as Fooyin
    /// does on each setting.
    pub(crate) fn discord_setting(&mut self, key: &str, value: &str) -> bool {
        if key == ENABLED {
            let on = value == "true";
            self.remember_preference(key, &on.to_string());
            self.discord.enabled = Some(on);
        } else if SWITCHES.iter().any(|(known, _)| *known == key) {
            self.remember_preference(key, &(value == "true").to_string());
        } else if FORMATS.iter().any(|(known, _)| *known == key) {
            let format: String = value.chars().take(FORMAT_LONGEST).collect();
            self.remember_preference(key, &format);
        } else if key == SOURCE && SOURCES.iter().any(|(known, ..)| *known == value)
            || key == RETENTION && RETENTIONS.iter().any(|(known, _)| *known == value) {
            self.remember_preference(key, value);
        } else if key == SOURCE || key == RETENTION {
            return true;
        } else {
            return false;
        }
        self.discord.options = None;
        true
    }

    /// The rows of the presence: everything under the switch goes while it
    /// is off, and while Chilpa is offline; the upload goes with the
    /// artwork, and its way and time with the upload.
    pub(crate) fn discord_row(&mut self, row: &mut crate::SettingsRow) -> bool {
        let key = row.key.as_str();
        if key == ENABLED {
            row.value = self.discord_enabled().to_string();
            return true;
        }
        let switch = SWITCHES.iter().any(|(known, _)| *known == key);
        let choice = key == SOURCE || key == RETENTION;
        if !switch && !choice && !FORMATS.iter().any(|(known, _)| *known == key) {
            return false;
        }
        let shown = self.discord_enabled() && !self.offline()
            && (!(choice || key == UPLOAD) || self.discord_switch(ARTWORK))
            && (!choice || self.discord_switch(UPLOAD));
        if !shown {
            row.control.clear();
        } else if choice {
            let (chosen, labels): (&str, Vec<(&str, &str)>) = if key == SOURCE {
                (self.discord_source(), SOURCES.iter().map(|(value, _, label)| (*value, *label)).collect())
            } else {
                (self.discord_retention(), RETENTIONS.to_vec())
            };
            for (value, label) in labels {
                row.values.push(value.to_string());
                row.options.push(chilpa_i18n::t(label));
            }
            row.value = chosen.to_string();
        } else if switch {
            row.value = self.discord_switch(key).to_string();
        } else {
            row.value = self.discord_format(key);
            if let Err(unreadable) = format::Format::parse(&row.value) {
                let from = (unreadable.at + 1).to_string();
                row.description = chilpa_i18n::t_args("This format cannot be read from character {}, so Discord gets an empty line. Close every %, [, ( and quote, and use only the functions listed under Title.", &[&from]);
            }
        }
        true
    }

    /// Tells the presence's thread what plays when it changed. Off or
    /// offline, nothing runs: Discord would carry the status over the
    /// network.
    pub(crate) fn advance_discord(&mut self) {
        let enabled = match self.discord.enabled {
            Some(enabled) => enabled,
            None => *self.discord.enabled.insert(self.preference(ENABLED).as_deref() == Some("true")),
        };
        if !enabled || self.offline() {
            self.discord.worker = None;
            self.discord.told = None;
            return;
        }
        let options = self.discord_options();
        let started_at = ((self.discord.clock)() - self.now.seconds).round() as i64;
        let now = &self.now;
        let same = match &self.discord.told {
            None => !now.playing && self.discord.worker.is_some(),
            Some(told) => {
                now.playing && told.path == now.path && told.index == now.index && told.title == now.title && told.artist == now.artist
                    && told.album == now.album && told.paused == now.paused && told.artwork_path == now.artwork_path
                    && Arc::ptr_eq(&told.options, &options)
                    && (now.paused || (told.started_at - started_at).abs() <= DRIFT)
            }
        };
        if same {
            return;
        }
        let message = if now.playing { Message::Show(Box::new(self.discord_playing(started_at)), options.clone()) } else { Message::Clear };
        let told = now.playing.then(|| Told {
            path: now.path.clone(),
            index: now.index,
            title: now.title.clone(),
            artist: now.artist.clone(),
            album: now.album.clone(),
            paused: now.paused,
            started_at,
            artwork_path: now.artwork_path.clone(),
            options,
        });
        let serve = self.discord.serve;
        let worker = self.discord.worker.get_or_insert_with(|| {
            let (tell, messages) = channel();
            let back = tell.clone();
            let _ = std::thread::Builder::new().name("chilpa-discord".into()).spawn(move || serve(messages, back));
            Worker(tell)
        });
        let _ = worker.0.send(message);
        self.discord.told = told;
    }

    /// What plays, for the presence. A source's track brings its cover's
    /// address and its numbers from the catalog; a local one is looked up.
    fn discord_playing(&self, started_at: i64) -> Playing {
        let now = &self.now;
        let plugin = crate::source_opener::of_plugin(&now.path).map(|(plugin, _)| plugin);
        let listed = plugin.as_ref().and_then(|_| self.catalog.track_by_path(&now.path)).map(|(track, _)| track);
        Playing {
            path: now.path.clone(),
            title: now.title.clone(),
            artist: now.artist.clone(),
            album: now.album.clone(),
            source: plugin.as_ref().map(|plugin| self.natives.display_name(plugin)).unwrap_or_default(),
            codec: now.quality.format.clone(),
            rate_hz: now.quality.rate_hz,
            bits: now.quality.bits,
            duration: now.total_seconds,
            paused: now.paused,
            started_at,
            cover_url: listed.as_ref().map(|track| track.image.clone()).filter(|url| url.starts_with("https://") || url.starts_with("http://")),
            track: listed.as_ref().map(|track| u32::from(track.number)).filter(|n| *n > 0),
            disc: listed.as_ref().map(|track| u32::from(track.disc)).filter(|n| *n > 0),
            look_up: plugin.is_none() && self.musicbrainz_enabled(),
            artwork_path: now.artwork_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static NOW: Cell<f64> = const { Cell::new(1_800_000_000.0) };
    }

    fn clock() -> f64 {
        NOW.with(Cell::get)
    }

    fn later(seconds: f64) {
        NOW.with(|now| now.set(now.get() + seconds));
    }

    fn quiet(messages: Receiver<Message>, _: Sender<Message>) {
        for message in messages {
            if matches!(message, Message::Stop) {
                break;
            }
        }
    }

    fn started(api: &Chilpa) -> Option<i64> {
        api.discord.told.as_ref().map(|told| told.started_at)
    }

    #[test]
    fn it_runs_only_while_on_and_tells_only_what_changed() {
        let mut api = Chilpa::new();
        api.discord.serve = quiet;
        api.discord.clock = clock;
        api.discord.enabled = Some(false);
        api.advance_discord();
        assert!(api.discord.worker.is_none(), "off, no thread");
        api.discord.enabled = Some(true);
        api.advance_discord();
        assert!(api.discord.worker.is_some(), "on, it says at once that nothing plays");
        assert!(api.discord.told.is_none());
        api.now.playing = true;
        api.now.paused = false;
        api.now.path = "/music/a.flac".into();
        api.now.seconds = 10.0;
        api.advance_discord();
        let first = started(&api);
        assert!(api.discord.told.as_ref().is_some_and(|told| !told.paused), "a track that plays is told");
        later(30.0);
        api.now.seconds = 40.0;
        api.advance_discord();
        assert_eq!(started(&api), first, "a position that follows the clock is not a change");
        api.now.seconds = 200.0;
        api.advance_discord();
        assert_eq!(started(&api), first.map(|start| start - 160), "a seek is: its timestamps move");
        api.now.paused = true;
        api.advance_discord();
        assert!(api.discord.told.as_ref().is_some_and(|told| told.paused), "a pause is");
        later(60.0);
        api.advance_discord();
        assert_eq!(started(&api), first.map(|start| start - 160), "a paused track has no clock to follow");
        api.now.playing = false;
        api.advance_discord();
        assert!(api.discord.told.is_none(), "stopped, the status is cleared");
        api.discord_setting(ENABLED, "false");
        api.advance_discord();
        assert!(api.discord.worker.is_none() && api.discord.told.is_none(), "off again, the thread ends");
    }

    fn row(api: &mut Chilpa, key: &str, control: &str) -> crate::SettingsRow {
        let mut row = crate::SettingsRow { key: key.into(), control: control.into(), ..Default::default() };
        assert!(api.discord_row(&mut row));
        row
    }

    #[test]
    fn the_upload_is_off_until_chosen_and_its_rows_follow_it() {
        assert_eq!(RETENTIONS.map(|(hours, _)| hours.parse::<u32>().unwrap()), upload::HOURS, "the host's times");
        let mut api = Chilpa::new();
        if api.offline() {
            return;
        }
        api.discord.enabled = Some(true);
        if api.preference(UPLOAD).is_none() {
            let options = api.discord_options();
            assert!(!options.upload, "off until the listener chooses it");
            assert_eq!((options.source, options.hours), (Source::SearchThenUpload, 24));
        }
        api.discord_setting(ARTWORK, "true");
        api.discord_setting(UPLOAD, "false");
        assert!(row(&mut api, SOURCE, "select").control.is_empty(), "no way to choose while nothing is uploaded");
        api.discord_setting(UPLOAD, "true");
        let source = row(&mut api, SOURCE, "select");
        assert_eq!(source.values, ["search-then-upload", "upload", "upload-then-search"]);
        assert_eq!(source.options.len(), 3);
        api.discord_setting(RETENTION, "72");
        api.discord_setting(RETENTION, "48");
        assert_eq!(row(&mut api, RETENTION, "select").value, "72", "a time the host does not offer is not kept");
        assert_eq!(api.discord_options().hours, 72);
        api.discord_setting(ARTWORK, "false");
        assert!(row(&mut api, UPLOAD, "toggle").control.is_empty(), "the upload goes with the artwork");
        api.discord_setting(ARTWORK, "true");
        api.discord_setting(UPLOAD, "false");
        api.discord_setting(RETENTION, RETENTION_DEFAULT);
    }

    #[test]
    fn a_setting_is_told_at_once_and_its_rows_go_with_the_switch() {
        let mut api = Chilpa::new();
        api.discord.serve = quiet;
        api.discord.clock = clock;
        api.discord.enabled = Some(true);
        api.now.playing = true;
        api.now.path = "/music/a.flac".into();
        api.advance_discord();
        let before = api.discord.told.as_ref().map(|told| told.options.clone()).unwrap();
        assert_eq!(before.title, api.discord_format(TITLE_FORMAT));
        assert!(api.discord_setting(CLEAR_ON_PAUSE, api.discord_switch(CLEAR_ON_PAUSE).to_string().as_str()));
        api.advance_discord();
        let after = api.discord.told.as_ref().map(|told| told.options.clone()).unwrap();
        assert!(!Arc::ptr_eq(&before, &after), "the options are read again and the status told again");
        assert!(!api.discord_setting("integrations.discord.nothing", "x"));
        let mut row = crate::SettingsRow { key: TITLE_FORMAT.into(), control: "text".into(), ..Default::default() };
        assert!(api.discord_row(&mut row));
        assert_eq!(row.control, if api.offline() { "" } else { "text" });
        api.discord.enabled = Some(false);
        let mut row = crate::SettingsRow { key: ARTWORK.into(), control: "toggle".into(), ..Default::default() };
        assert!(api.discord_row(&mut row));
        assert!(row.control.is_empty(), "under a switch that is off, the row goes");
    }

    #[test]
    fn a_format_is_bounded_and_an_empty_one_is_the_default() {
        let mut api = Chilpa::new();
        let previous = api.preference(TITLE_FORMAT);
        api.discord_setting(TITLE_FORMAT, &"x".repeat(FORMAT_LONGEST + 10));
        assert_eq!(api.discord_format(TITLE_FORMAT).chars().count(), FORMAT_LONGEST);
        api.discord_setting(TITLE_FORMAT, "  ");
        assert_eq!(api.discord_format(TITLE_FORMAT), "%title%");
        api.discord_setting(TITLE_FORMAT, "ab%title");
        api.discord.enabled = Some(true);
        let mut row = crate::SettingsRow { key: TITLE_FORMAT.into(), control: "text".into(), ..Default::default() };
        api.discord_row(&mut row);
        assert!(api.offline() || row.description.contains("character 3"), "the row says where it breaks: {}", row.description);
        api.discord_setting(TITLE_FORMAT, previous.as_deref().unwrap_or(""));
    }
}
