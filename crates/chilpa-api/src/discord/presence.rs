//! The thread that keeps Discord told: what the status says, and when.
//!
//! It does what Fooyin's plugin object does (`discordplugin.cpp`): changes
//! that come within 50 ms go as one, the three lines come out of the track
//! formats, the timestamps only while it plays, and the cover is looked for
//! one second after the track changes, once, with the answer kept for a
//! day (or an hour when there is none). Stopped, or paused with "Clear on
//! pause", the status is cleared and the connection stays.

use super::artwork::{self, Covers, Outcome, Query, Tags};
use super::format::Format;
use super::ipc::{self, Connection, Failure};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Chilpa's public Discord application ID, not a secret.
const APPLICATION_ID: &str = "1562851627461910589";
const HOMEPAGE: &str = "https://chilpa.app";
/// The art assets of Chilpa's Discord app: "icon", the small badge, and
/// "cover", the large image while there is no artwork.
const BADGE: &str = "icon";
const COVER_ASSET: &str = "cover";
/// Where Fooyin shows its "play" and "pause" badges. Chilpa's app has only
/// its icon so far, so both are the icon and the text tells them apart.
const PLAYING_BADGE: &str = BADGE;
const PAUSED_BADGE: &str = BADGE;
/// Changes this close together go as one (Fooyin's `SignalThrottler`).
const SETTLE: Duration = Duration::from_millis(50);
/// How long a track has to stay before its cover is looked for (Fooyin's
/// `ArtworkLoadDelayMs`), so skipping through tracks asks for none.
const COVER_DELAY: Duration = Duration::from_secs(1);
/// Discord refuses a line longer than this.
const LONGEST: usize = 128;

/// What the settings ask of the status.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Options {
    pub show_state: bool,
    pub clear_on_pause: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: bool,
}

/// What plays, as the window knows it when it changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Playing {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The plugin it comes from, by its name; empty for a local track.
    pub source: String,
    pub codec: String,
    pub rate_hz: Option<u32>,
    pub bits: Option<u8>,
    /// Seconds; 0 when unknown, and then the status has no end.
    pub duration: f64,
    pub paused: bool,
    /// When it would have started, in Unix seconds, had it played straight
    /// through to where it is.
    pub started_at: i64,
    /// A source's cover on the web, which Discord fetches as it is.
    pub cover_url: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    /// Whether its cover may be looked for on MusicBrainz and the archive.
    pub look_up: bool,
}

pub(crate) enum Message {
    Show(Box<Playing>, Arc<Options>),
    Clear,
    Cover(String, Outcome),
    Stop,
}

/// The status, as Discord is given it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Presence {
    pub details: String,
    pub state: String,
    pub large_text: String,
    pub large_image: String,
    pub small: Option<(&'static str, &'static str)>,
    pub start: Option<i64>,
    pub end: Option<i64>,
}

/// The year a date starts with, as Fooyin's `%year%`.
fn year(date: &str) -> String {
    let year: String = date.trim().chars().take(4).collect();
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) { year } else { String::new() }
}

/// `mm:ss`, or `hh:mm:ss`, as Fooyin writes a duration.
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 { format!("{hours:02}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

/// A field of the formats, by its upper-case name, as Fooyin names it.
fn field(playing: &Playing, tags: &Tags, name: &str) -> String {
    let number = |n: Option<u32>| n.filter(|n| *n > 0).map(|n| n.to_string()).unwrap_or_default();
    match name {
        "TITLE" => playing.title.clone(),
        "ARTIST" => playing.artist.clone(),
        "ALBUM" => playing.album.clone(),
        // Fooyin's effective album artist: the track's artist when the file
        // names none.
        "ALBUMARTIST" => if tags.album_artist.is_empty() { playing.artist.clone() } else { tags.album_artist.clone() },
        "DATE" => tags.date.clone(),
        "YEAR" => year(&tags.date),
        "GENRE" => tags.genres.join(", "),
        "TRACK" | "TRACKNUMBER" => number(tags.track.or(playing.track)),
        "DISC" | "DISCNUMBER" => number(tags.disc.or(playing.disc)),
        "CODEC" => playing.codec.clone(),
        "SAMPLERATE" => playing.rate_hz.map(|rate| rate.to_string()).unwrap_or_default(),
        "BITDEPTH" => playing.bits.map(|bits| bits.to_string()).unwrap_or_default(),
        "DURATION" => if playing.duration > 0.0 { clock(playing.duration) } else { String::new() },
        "SOURCE" => playing.source.clone(),
        _ => String::new(),
    }
}

/// A line as Discord takes it: cut at 128 characters, and two spaces after
/// one shorter than two, as Fooyin pads it.
fn line(text: String) -> String {
    let mut text = if text.chars().count() > LONGEST { text.chars().take(LONGEST - 1).chain(['…']).collect() } else { text };
    if text.chars().count() < 2 {
        text.push_str("  ");
    }
    text
}

/// The formats of a set of options, read once. One that cannot be read
/// says nothing.
pub(crate) struct Formats {
    options: Arc<Options>,
    title: Format,
    artist: Format,
    album: Format,
}

impl Formats {
    pub(crate) fn of(options: &Arc<Options>) -> Formats {
        let read = |text: &str| Format::parse(text).unwrap_or_default();
        Formats { options: options.clone(), title: read(&options.title), artist: read(&options.artist), album: read(&options.album) }
    }
}

/// The status for what plays, or `None` when it is cleared.
pub(crate) fn presence(playing: &Playing, tags: &Tags, formats: &Formats, artwork: Option<&str>) -> Option<Presence> {
    let options = &formats.options;
    if playing.paused && options.clear_on_pause {
        return None;
    }
    let fields = |name: &str| field(playing, tags, name);
    let small = options.show_state.then_some(match (playing.paused, artwork.is_some()) {
        (true, _) => (PAUSED_BADGE, "Paused"),
        (false, true) => (BADGE, "Playing"),
        (false, false) => (PLAYING_BADGE, "Playing"),
    });
    let (start, end) = if playing.paused {
        (None, None)
    } else {
        (Some(playing.started_at), (playing.duration > 0.0).then(|| playing.started_at + playing.duration.round() as i64))
    };
    Some(Presence {
        details: line(formats.title.render(&fields)),
        state: line(formats.artist.render(&fields)),
        large_text: line(formats.album.render(&fields)),
        large_image: artwork.unwrap_or(COVER_ASSET).to_string(),
        small,
        start,
        end,
    })
}

/// The activity of `SET_ACTIVITY`: Fooyin's (`buildActivityPayload`), with
/// Chilpa's button.
pub(crate) fn activity(presence: &Presence) -> Value {
    let mut timestamps = Map::new();
    if let Some(start) = presence.start {
        timestamps.insert("start".into(), start.into());
    }
    if let Some(end) = presence.end {
        timestamps.insert("end".into(), end.into());
    }
    let mut assets = Map::new();
    assets.insert("large_image".into(), presence.large_image.clone().into());
    assets.insert("large_text".into(), presence.large_text.clone().into());
    if let Some((image, text)) = presence.small {
        assets.insert("small_image".into(), image.into());
        assets.insert("small_text".into(), text.into());
    }
    json!({
        "type": 2,
        "instance": false,
        "state": presence.state,
        "details": presence.details,
        "timestamps": timestamps,
        "assets": assets,
        "buttons": [{"label": "Get Chilpa", "url": HOMEPAGE}],
    })
}

/// Whoever carries the status to Discord.
pub(crate) trait Rpc {
    /// `false` when it did not reach Discord.
    fn show(&mut self, activity: Value) -> bool;
    fn clear(&mut self);
    fn close(&mut self);
}

/// Discord itself. Without a connection a status connects first, every
/// time, as Fooyin's `updateActivity` does; a clear does not.
pub(crate) struct Discord {
    pipes: fn() -> Vec<PathBuf>,
    connection: Option<Connection>,
    /// Said once until Discord answers again.
    unreachable: bool,
}

impl Default for Discord {
    fn default() -> Self {
        Self { pipes: ipc::pipes, connection: None, unreachable: false }
    }
}

impl Discord {
    fn command(&mut self, activity: Value) -> bool {
        let Some(connection) = self.connection.as_mut() else { return false };
        match connection.set_activity(activity) {
            Ok(()) => true,
            Err(Failure::Refused(why)) => {
                eprintln!("chilpa-discord: Discord refused the status: {why}");
                true
            }
            Err(Failure::Broken(why)) => {
                eprintln!("chilpa-discord: the connection to Discord broke: {why}");
                self.connection = None;
                false
            }
        }
    }
}

impl Rpc for Discord {
    fn show(&mut self, activity: Value) -> bool {
        if self.connection.is_none() {
            match Connection::open(&(self.pipes)(), APPLICATION_ID) {
                Ok(connection) => {
                    self.connection = Some(connection);
                    self.unreachable = false;
                }
                Err(_) => {
                    if !std::mem::replace(&mut self.unreachable, true) {
                        eprintln!("chilpa-discord: Discord is not reachable (not running, or its socket is not visible); the next change tries again");
                    }
                    return false;
                }
            }
        }
        self.command(activity)
    }

    fn clear(&mut self) {
        self.command(Value::Null);
    }

    fn close(&mut self) {
        if let Some(connection) = self.connection.take() {
            connection.close();
        }
    }
}

/// Where the cover of what plays comes from.
#[derive(Clone, Debug, PartialEq)]
enum Plan {
    None,
    Url(String),
    Lookup(String, Query),
}

impl Plan {
    fn of(playing: &Playing, tags: &Tags, options: &Options) -> Plan {
        if !options.artwork {
            return Plan::None;
        }
        if let Some(url) = &playing.cover_url {
            return Plan::Url(url.clone());
        }
        if !playing.look_up {
            return Plan::None;
        }
        Query::of(tags, &playing.album, &playing.artist).map_or(Plan::None, |(key, query)| Plan::Lookup(key, query))
    }

    fn key(&self) -> Option<&str> {
        match self {
            Plan::Lookup(key, _) => Some(key),
            _ => None,
        }
    }
}

/// Starts looking for a cover; the outcome comes back as a
/// [`Message::Cover`].
type LookUp = Box<dyn FnMut(String, Query) + Send>;

pub(crate) struct Presenter<R: Rpc> {
    rpc: R,
    read_tags: fn(&str) -> Tags,
    look_up: LookUp,
    /// The album whose cover is wanted now, which a search reads to know
    /// whether to go on.
    wanted: Arc<Mutex<Option<String>>>,
    shown: Option<(Box<Playing>, Arc<Options>)>,
    tags: Tags,
    tags_of: Option<String>,
    formats: Option<Formats>,
    plan: Plan,
    covers: Covers,
    looking: Option<String>,
    cover_due: Option<Instant>,
    pending_since: Option<Instant>,
    /// What Discord was last given; `Some(None)` is a cleared status.
    told: Option<Option<Presence>>,
}

impl<R: Rpc> Presenter<R> {
    fn new(rpc: R, read_tags: fn(&str) -> Tags, look_up: LookUp, wanted: Arc<Mutex<Option<String>>>) -> Self {
        Presenter {
            rpc, read_tags, look_up, wanted, shown: None, tags: Tags::default(), tags_of: None, formats: None, plan: Plan::None,
            covers: Covers::default(), looking: None, cover_due: None, pending_since: None, told: None,
        }
    }

    fn want(&mut self, plan: Plan, now: Instant) {
        if plan.key() != self.plan.key() {
            *self.wanted.lock().unwrap_or_else(|e| e.into_inner()) = plan.key().map(str::to_string);
            self.cover_due = plan.key().filter(|key| self.covers.get(key, now).is_none()).map(|_| now + COVER_DELAY);
        }
        self.plan = plan;
    }

    /// Takes in a message; `false` once told to stop.
    fn receive(&mut self, message: Message, now: Instant) -> bool {
        match message {
            Message::Show(playing, options) => {
                if self.tags_of.as_deref() != Some(playing.path.as_str()) {
                    self.tags = (self.read_tags)(&playing.path);
                    self.tags_of = Some(playing.path.clone());
                }
                if !self.formats.as_ref().is_some_and(|formats| Arc::ptr_eq(&formats.options, &options)) {
                    self.formats = Some(Formats::of(&options));
                }
                self.want(Plan::of(&playing, &self.tags, &options), now);
                self.shown = Some((playing, options));
            }
            Message::Clear => {
                self.shown = None;
                self.want(Plan::None, now);
            }
            Message::Cover(key, outcome) => {
                if self.looking.as_deref() == Some(key.as_str()) {
                    self.looking = None;
                }
                match outcome {
                    Outcome::Found(url) => self.covers.put(key.clone(), Some(url), now),
                    Outcome::Missing => self.covers.put(key.clone(), None, now),
                    Outcome::Abandoned => {}
                }
                if self.plan.key() != Some(key.as_str()) {
                    return true;
                }
            }
            Message::Stop => return false,
        }
        self.pending_since.get_or_insert(now);
        true
    }

    /// When something is due, if anything is.
    fn next_wake(&self) -> Option<Instant> {
        let settle = self.pending_since.map(|since| since + SETTLE);
        let cover = self.cover_due.filter(|_| self.looking.is_none());
        settle.into_iter().chain(cover).min()
    }

    fn tick(&mut self, now: Instant) {
        if let (Some(due), Plan::Lookup(key, query)) = (self.cover_due, &self.plan) {
            if due <= now && self.looking.is_none() {
                self.cover_due = None;
                if self.covers.get(key, now).is_none() {
                    self.looking = Some(key.clone());
                    (self.look_up)(key.clone(), query.clone());
                }
            }
        }
        if self.pending_since.is_some_and(|since| now >= since + SETTLE) {
            self.pending_since = None;
            self.flush(now);
        }
    }

    fn flush(&mut self, now: Instant) {
        let artwork = match &self.plan {
            Plan::None => None,
            Plan::Url(url) => Some(url.as_str()),
            Plan::Lookup(key, _) => self.covers.get(key, now).flatten(),
        };
        let wanted = match (&self.shown, &self.formats) {
            (Some((playing, _)), Some(formats)) => presence(playing, &self.tags, formats, artwork),
            _ => None,
        };
        if self.told.as_ref() == Some(&wanted) {
            return;
        }
        let reached = match &wanted {
            Some(presence) => self.rpc.show(activity(presence)),
            None => {
                self.rpc.clear();
                true
            }
        };
        self.told = reached.then_some(wanted);
    }
}

/// The thread's whole life: it ends, clearing the status, when told to
/// stop.
pub(crate) fn serve(messages: Receiver<Message>, tell: Sender<Message>) {
    let wanted = Arc::new(Mutex::new(None));
    let still = wanted.clone();
    let look_up: LookUp = Box::new(move |key, query| {
        let (wanted, tell, back) = (still.clone(), tell.clone(), tell.clone());
        let named = key.clone();
        let started = std::thread::Builder::new().name("chilpa-discord-cover".into()).spawn(move || {
            let current = || wanted.lock().unwrap_or_else(|e| e.into_inner()).as_deref() == Some(key.as_str());
            let outcome = artwork::find_online(&query, &current);
            let _ = tell.send(Message::Cover(key, outcome));
        });
        if started.is_err() {
            let _ = back.send(Message::Cover(named, Outcome::Missing));
        }
    });
    let mut presenter = Presenter::new(Discord::default(), artwork::read_tags, look_up, wanted);
    loop {
        let message = match presenter.next_wake() {
            Some(at) => messages.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(message) => {
                if !presenter.receive(message, Instant::now()) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        presenter.tick(Instant::now());
    }
    presenter.rpc.clear();
    presenter.rpc.close();
}

#[cfg(test)]
#[path = "presence_tests.rs"]
mod tests;
