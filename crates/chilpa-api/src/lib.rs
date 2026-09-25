//! Chilpa's facade: the only thing a frontend may use (ADR-001).
//!
//! A shell sends [`Command`]s and receives [`Event`]s. It sees neither the
//! core, nor the library, nor the player. If a view needs something that is
//! not here, this API is widened first.

pub use chilpa_core::queue_playback::Repeat;
pub use chilpa_library::{AlbumSort, Stats};
pub use plugins::PluginInfo;
pub use search::{Hit, KEYSTROKE_DEBOUNCE, SOURCE_COOLDOWN};

mod library;
mod queue;
mod playlists;
mod accounts;
mod approvals;
mod plugins;
mod search;

use chilpa_models::{AudioQuality, SourceId, TrackRef};
use std::path::PathBuf;

/// What a frontend asks Chilpa for.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// Play a track from any source.
    Play(TrackRef),
    Pause,
    Resume,
    /// List the registered sources.
    ListSources,
    /// Play a local file on a device, with the given output policy. It is
    /// the minimal path while the library does not exist.
    PlayFile { path: PathBuf, device: String, bit_perfect: bool },
}

/// What Chilpa tells the frontend.
#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Sources(Vec<SourceId>),
    /// Something started playing, with the output's real quality.
    Playing { track: TrackRef, quality: AudioQuality },
    Paused,
    /// A file finished playing: how many frames went out, what format the
    /// device was opened with and whether it was bit-perfect.
    Played { frames: u64, quality: AudioQuality, bit_perfect: bool },
    /// Something went wrong, in text the UI can show.
    Failed { message: String },
}

/// An audio output the user can choose.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Output {
    pub id: String,
    /// What a person recognises: "Fosi Audio ZH3".
    pub label: String,
    /// The second line: how it is attached and what it says it can do.
    pub detail: String,
    /// Whether samples reach it with nothing in between.
    pub bit_perfect: bool,
}

/// One audio stack, as the window's first picker needs it.
///
/// It comes before the outputs because it decides them: which exist, what
/// they are called, and whether bit-perfect is on the table at all.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OutputStack {
    /// What it is called on disk. Not translated, does not change.
    pub key: String,
    /// What it is called on screen, with who is answering for it when that
    /// is not itself: "JACK (through PipeWire)".
    pub label: String,
    /// Whether it can be used on this machine.
    pub present: bool,
    /// When it cannot: what is missing, in words somebody can act on.
    pub missing: String,
    /// "exact", "depends" or "never". A word and not a flag, because the
    /// middle one is the true answer for a sound server and rounding it
    /// either way is a lie.
    pub fidelity: String,
    /// When the fidelity is not exact: why, and what to use instead.
    pub caveat: String,
}

/// What is playing, as a view needs it.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct NowPlaying {
    pub path: String,
    /// The title the file states; if it states none, its name.
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The cover already on disk, ready to show. Empty if there is none.
    pub artwork_path: String,
    pub playing: bool,
    pub paused: bool,
    /// Frames delivered and their equivalent in seconds.
    pub frames: u64,
    pub seconds: f64,
    /// Total duration, when it is known.
    pub total_seconds: f64,
    /// What the device was opened with, ready to show.
    pub format: String,
    pub bit_perfect: bool,
    /// Its place in the queue and how many tracks it has.
    pub index: usize,
    pub queue_len: usize,
    /// What the analysis says about what is playing: "128 BPM - 8B". Empty
    /// if it has not been analysed yet.
    pub analysis: String,
    /// The lyrics of what is playing, one line per entry. Empty if none.
    pub lyrics: Vec<String>,
    /// Which line is current, or -1 when the lyrics carry no timings or
    /// none has come in yet.
    pub lyrics_line: i32,
    /// True if the lyrics carry timings and can be followed.
    pub lyrics_synced: bool,
    /// Where it came from: "file", "tags" or empty.
    pub lyrics_source: String,
    /// The last problem, empty if there is none.
    pub error: String,
}

/// The extensions Chilpa knows how to open today.
const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// A folder's audio files, in alphabetical order.
///
/// It does not descend into subfolders: that is the library's job, not a view's.
pub fn audio_files_in(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

/// The walk that keeps the tempo most even, starting at `from`.
///
/// It is a nearest neighbour, on purpose: a mix does not need the optimal
/// order, it needs not to jump forty BPM between two tracks. Whatever
/// carries no tempo goes to the end, in its own order.
fn order_by_tempo(bpms: &[Option<f32>], from: usize) -> Vec<usize> {
    let mut missing: Vec<usize> = (0..bpms.len()).filter(|i| *i != from).collect();
    let mut order = vec![from];
    let mut previous = bpms.get(from).copied().flatten();

    while !missing.is_empty() {
        // Of those with a tempo, the closest to the previous one.
        let chosen = match previous {
            Some(t) => missing
                .iter()
                .enumerate()
                .filter(|(_, i)| bpms[**i].is_some())
                .min_by(|a, b| {
                    let da = (bpms[*a.1].unwrap() - t).abs();
                    let db = (bpms[*b.1].unwrap() - t).abs();
                    da.total_cmp(&db)
                })
                .map(|(pos, _)| pos),
            None => None,
        };
        // If none with a tempo are left, the rest is emptied out as is.
        let pos = chosen.unwrap_or(0);
        let i = missing.remove(pos);
        previous = bpms[i].or(previous);
        order.push(i);
    }
    order
}

/// What the analysis found, in one line.
///
/// Loudness is stated, not applied: correcting it would change the samples,
/// and there goes bit-perfect.
pub(crate) fn describe_analysis(
    bpm: Option<f32>,
    tone: Option<&str>,
    lufs: Option<f32>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(b) = bpm {
        parts.push(format!("{b:.0} BPM"));
    }
    if let Some(t) = tone {
        parts.push(t.to_string());
    }
    if let Some(l) = lufs {
        parts.push(format!("{l:.1} LUFS"));
    }
    parts.join(" · ")
}

/// Translates a library track into what a view needs.
fn track_of(t: chilpa_library::StoredTrack) -> LibraryTrack {
    LibraryTrack {
        id: t.id,
        path: t.external_id,
        title: t.title,
        artist: t.artist,
        album: t.album,
        track_number: t.track_number,
        duration_ms: t.duration_ms,
        present: t.present,
        analysis: String::new(),
    }
}

/// The title a queue shows: the tag's, or the file's name.
fn title_of(path: &std::path::Path) -> String {
    chilpa_tags::read(path)
        .ok()
        .and_then(|info| info.tags.title)
        .unwrap_or_else(|| {
            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        })
}

/// Now, in seconds since the epoch.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// How a channel count is named, already plural where it should be.
fn channels(n: u16) -> String {
    match n {
        1 => chilpa_i18n::t("mono"),
        2 => chilpa_i18n::t("stereo"),
        others => chilpa_i18n::tf(
            "{} channel",
            "{} channels",
            i64::from(others),
            &[&others.to_string()],
        ),
    }
}

/// What is known about the playing track, to log it when it ends.
#[derive(Clone, Debug)]
struct InProgress {
    path: String,
    started_at: i64,
    sample_rate_hz: u32,
    bit_depth: Option<u8>,
    bit_perfect: bool,
    duration_ms: u64,
}

/// A smart list, with whatever it yields today.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SmartListInfo {
    pub name: String,
    pub track_count: usize,
}

/// A playlist, ready to show.
///
/// `provider` says where it came from. The view can filter on it: that is
/// what lets a plugin bring its own lists without them being confused with
/// the ones from here.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Playlist {
    /// The library's row id. Zero on a source's list, which has no row.
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub track_count: usize,
    /// The plugin it came from, empty when it is one of ours.
    pub source: String,
    /// The id that source gives it, empty when it is one of ours.
    pub external_id: String,
}

/// What the wizard found about an output.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct WizardReport {
    pub device: String,
    /// True if it was measured by opening the device.
    pub probed: bool,
    pub summary: String,
    pub rates: Vec<u32>,
    pub formats: Vec<String>,
    pub rows: Vec<WizardRow>,
}

/// A format and what happens to it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WizardRow {
    pub name: String,
    pub format: String,
    /// "bit-perfect", "converted to ..." or "cannot be done".
    pub verdict: String,
}

/// An artist from the library, ready to show.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Artist {
    pub name: String,
    pub album_count: usize,
    pub track_count: usize,
    pub artwork_path: String,
}

/// An album as a view needs it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub track_count: usize,
    /// The cover on disk, empty if there is none.
    pub artwork_path: String,
    /// How many plays it has, counted from the log.
    pub play_count: usize,
}

/// A track from the library.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LibraryTrack {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub present: bool,
    /// What the analysis found, empty if it has not been analysed yet.
    pub analysis: String,
}

/// How the background analysis is going.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AnalysisStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    /// The last track looked at, so progress is visible.
    pub last: String,
    /// How many ended up analysed, without a pulse, and unreadable.
    pub analyzed: usize,
    pub without_pulse: usize,
    pub failed: usize,
}

/// How the background scan is going.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    /// What the last finished scan left behind.
    pub summary: ScanSummary,
}

/// What happened in a scan, in one line for the view.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanSummary {
    pub seen: usize,
    pub read: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub unreachable: bool,
}

/// The Chilpa instance a frontend talks to.
pub struct Chilpa {
    core: chilpa_core::Core,
    library: Option<chilpa_library::Library>,
    /// What is needed to log the play when it ends.
    in_progress: Option<InProgress>,
    /// The current queue's paths, whole and in the order they were queued.
    /// Going back does not trim it.
    queue: Vec<String>,
    /// Which position of `queue` was the first one handed to the player.
    /// Going back restarts from there, and the position the view sees is
    /// counted over the whole queue.
    base: usize,
    shuffle: bool,
    repeat: Repeat,
    album_sort: AlbumSort,
    /// The artist the grid is filtered to, empty if there is no filter.
    artist: String,
    /// Which positions of `queue` were removed, so the view greys them out.
    removed: Vec<bool>,
    /// The lyrics of what is playing, so they are not reread on every tick.
    lyrics: Option<chilpa_lyrics::Lyrics>,
    /// The plugin sandbox, built the first time it is asked for. It is
    /// shared because search queries it from its own thread: a slow source
    /// must not leave the window waiting.
    plugins: Option<std::sync::Arc<std::sync::Mutex<plugins::Plugins>>>,
    /// What actually goes out the device, for the visualizers.
    tap: chilpa_core::tap::Tap,
    /// The background scan, while it runs.
    scan: Option<chilpa_library::ScanJob>,
    scan_status: ScanStatus,
    analysis: Option<chilpa_core::analysis_job::AnalysisJob>,
    analysis_status: AnalysisStatus,
    player: Option<chilpa_core::player::Player>,
    /// The search in progress: what is local, what the sources contributed
    /// and what the view draws.
    search: search::SearchState,
    /// The account being connected, between the browser opening and the
    /// user coming back.
    connecting: Option<plugins::Connecting>,
    now: NowPlaying,
    sample_rate: u32,
    /// Set while a relaunch we asked for is unwinding the old playback.
    ///
    /// Stopping in order to start again is not the listener stopping, and
    /// without this the `Stopped` that comes back would be filed as a skip
    /// and the window would blink to "not playing" on its way to the new
    /// device. What it holds is what to do with the interrupted listen.
    relaunching: Option<Relaunch>,
    /// The last answer from the catalogue about which outputs exist. Kept so
    /// the window can be told when it changed without asking the stacks for
    /// their whole list on a timer.
    outputs_generation: String,
}

/// Why the player was told to stop, when it was us who told it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Relaunch {
    /// Another track of the same queue: going back is the case. The one that
    /// was interrupted is a skip, because somebody chose to leave it.
    Track,
    /// The same track, on another output. Nobody skipped anything: the music
    /// is where it was and it is coming out somewhere else, so the
    /// interrupted stretch is dropped rather than logged as its own listen.
    Output,
}

impl Default for Chilpa {
    fn default() -> Self {
        Self::new()
    }
}

impl Chilpa {
    pub fn new() -> Self {
        // The language is set before anything can speak: a message built
        // during startup has to come out in the same language as the rest.
        let saved = chilpa_core::settings::load();
        chilpa_i18n::set_language(if saved.language.is_empty() {
            chilpa_i18n::from_environment()
        } else {
            &saved.language
        });
        Self {
            core: chilpa_core::Core::new(),
            library: None,
            in_progress: None,
            queue: Vec::new(),
            base: 0,
            shuffle: false,
            repeat: Repeat::Off,
            album_sort: AlbumSort::from_name(&saved.album_sort),
            artist: String::new(),
            removed: Vec::new(),
            lyrics: None,
            plugins: None,
            tap: chilpa_core::tap::Tap::new(),
            scan: None,
            scan_status: ScanStatus::default(),
            analysis: None,
            analysis_status: AnalysisStatus::default(),
            player: None,
            search: search::SearchState::default(),
            connecting: None,
            now: NowPlaying::default(),
            sample_rate: 0,
            relaunching: None,
            outputs_generation: String::new(),
        }
    }

    /// Whether the machine's outputs changed since this was last asked.
    ///
    /// The window calls it on its timer so that plugging a DAC in makes it
    /// appear without anybody pressing anything. It is cheap on purpose: the
    /// stacks that can answer without opening or asking over a socket hand
    /// back something short, and only when that moves does the window go and
    /// rebuild the list.
    ///
    /// The first call is always false. There is nothing to have changed from
    /// yet, and answering true would make the window rebuild the list one
    /// tick after it had just built it.
    pub fn outputs_changed(&mut self) -> bool {
        let now = chilpa_core::backends::generation();
        if self.outputs_generation.is_empty() {
            self.outputs_generation = now;
            return false;
        }
        if now == self.outputs_generation {
            return false;
        }
        self.outputs_generation = now;
        true
    }

    /// The library, opened the first time it is used.
    pub(crate) fn library(&mut self) -> Option<&chilpa_library::Library> {
        if self.library.is_none() {
            match chilpa_core::open_library() {
                Ok(lib) => self.library = Some(lib),
                Err(e) => {
                    self.now.error = chilpa_i18n::t_args("the library could not be opened: {}", &[&e.to_string()]);
                    return None;
                }
            }
        }
        self.library.as_ref()
    }










    /// What the analysis already knew about the track that is starting.
    fn load_analysis(&mut self, path: &std::path::Path) {
        self.now.analysis = String::new();
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let Some(id) = lib.track_by_path(source, path).ok().flatten() else { return };
        let Some((bpm, tone, loudness)) =
            lib.track_features(id, chilpa_core::analysis_job::ANALYZER).ok().flatten()
        else {
            return;
        };
        self.now.analysis = describe_analysis(bpm, tone.as_deref(), loudness);
    }

    /// Looks up the lyrics of what is starting to play.
    ///
    /// It is read once per track, not on every tick: it is a few kilobytes, but
    /// opening a file sixty times a minute makes no sense at all.
    fn load_lyrics(&mut self, path: &std::path::Path) {
        let found = chilpa_lyrics::find(path);
        self.now.lyrics = found
            .as_ref()
            .map(|h| h.lyrics.lines.iter().map(|l| l.text.clone()).collect())
            .unwrap_or_default();
        self.now.lyrics_synced = found.as_ref().is_some_and(|h| h.lyrics.synced);
        self.now.lyrics_source = match found.as_ref().map(|h| &h.source) {
            Some(chilpa_lyrics::Source::Sidecar(_)) => chilpa_i18n::t("file"),
            Some(chilpa_lyrics::Source::Embedded) => chilpa_i18n::t("tags"),
            None => String::new(),
        };
        self.now.lyrics_line = -1;
        self.lyrics = found.map(|h| h.lyrics);
    }

    /// Which line of the lyrics belongs to that second.
    fn line_at(&self, seconds: f64) -> i32 {
        let Some(lyrics) = &self.lyrics else { return -1 };
        let ms = (seconds.max(0.0) * 1000.0) as u64;
        lyrics.line_at(ms).map(|i| i as i32).unwrap_or(-1)
    }



































    /// The CD drives on the machine.
    ///
    /// The usual nodes are checked; a drive that is not there is not an
    /// error, it simply does not show up on the list.
    pub fn cd_drives(&self) -> Vec<String> {
        let mut units = Vec::new();
        for name in ["/dev/sr0", "/dev/sr1", "/dev/cdrom", "/dev/dvd"] {
            let p = std::path::Path::new(name);
            if p.exists() && !units.iter().any(|u: &String| u == name) {
                units.push(name.to_string());
            }
        }
        units
    }

    /// Starts playing whatever CD is in that drive, from start to finish.
    pub fn play_cd(&mut self, drive: &str, device: &str, bit_perfect: bool) -> bool {
        let unit = if drive.is_empty() {
            match self.cd_drives().first() {
                Some(u) => u.clone(),
                None => {
                    self.now.error = chilpa_i18n::t("I did not find a CD drive");
                    return false;
                }
            }
        } else {
            drive.to_string()
        };
        self.queue = vec![format!("cdda:{unit}")];
        self.removed = vec![false];
        self.spawn(0, device, bit_perfect);
        true
    }

    /// Starts playing a radio, or any stream over HTTP.
    pub fn play_url(&mut self, url: &str, device: &str, bit_perfect: bool) {
        let url = url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.now.error = chilpa_i18n::t("that does not look like a stream address");
            return;
        }
        self.queue = vec![url.clone()];
        self.spawn(0, device, bit_perfect);
    }




    /// What Chilpa remembers: the last output, whether bit-perfect is asked
    /// for, and the last folder opened.
    pub fn settings(&self) -> chilpa_core::settings::Settings {
        chilpa_core::settings::load()
    }

    /// Remembers the chosen output and policy.
    pub fn remember(&mut self, output: &str, bit_perfect: bool) {
        let mut s = chilpa_core::settings::load();
        s.output = output.to_string();
        s.bit_perfect = bit_perfect;
        // If it cannot be saved, the session carries on: it is a
        // preference, not a part of the player.
        let _ = chilpa_core::settings::save(&s);
    }

    /// Remembers which visualizer is being drawn.
    pub fn remember_visualizer(&mut self, visualizer: &str) {
        let mut s = chilpa_core::settings::load();
        s.visualizer = visualizer.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Sets the language and remembers it.
    ///
    /// An empty code means whatever the environment asks for, which is what
    /// a fresh install has and what someone gets back by clearing the choice.
    pub fn choose_language(&mut self, language: &str) {
        let code =
            if language.is_empty() { chilpa_i18n::from_environment() } else { language };
        chilpa_i18n::set_language(code);
        let mut s = chilpa_core::settings::load();
        s.language = language.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// The language the window is speaking now.
    pub fn language(&self) -> &'static str {
        chilpa_i18n::current_language()
    }

    /// The languages that can be chosen.
    pub fn languages(&self) -> &'static [&'static str] {
        chilpa_i18n::languages()
    }

    /// Marks a literal that is defined here and shown elsewhere, so the
    /// extractor finds it. It translates nothing.
    pub const fn mark(msgid: &'static str) -> &'static str {
        chilpa_i18n::mark(msgid)
    }

    /// Translates a msgid, which is the English string itself.
    ///
    /// The shell only ever talks to the facade (ADR-001), so the catalog is
    /// reached through here and not by depending on `chilpa-i18n`.
    pub fn t(&self, msgid: &str) -> String {
        chilpa_i18n::t(msgid)
    }

    /// The same for a count, choosing singular or plural.
    pub fn tn(&self, singular: &str, plural: &str, n: i64) -> String {
        chilpa_i18n::tn(singular, plural, n)
    }

    /// The same, filling each `{}` with the next argument.
    pub fn t_args(&self, msgid: &str, args: &[&str]) -> String {
        chilpa_i18n::t_args(msgid, args)
    }

    /// Both at once: a count and its holes.
    pub fn tf(&self, singular: &str, plural: &str, n: i64, args: &[&str]) -> String {
        chilpa_i18n::tf(singular, plural, n, args)
    }

    /// Remembers the chosen window layout.
    pub fn remember_layout(&mut self, layout: &str) {
        let mut s = chilpa_core::settings::load();
        s.layout = layout.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Remembers the last folder opened.
    pub fn remember_folder(&mut self, folder: &str) {
        let mut s = chilpa_core::settings::load();
        s.last_folder = folder.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// An output's diagnosis: which formats come out untouched.
    ///
    /// It measures the device for real. It configures nothing: QBZ's wizard
    /// wrote settings blind, and that is worse than not knowing.
    pub fn wizard(&mut self, device: &str) -> WizardReport {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        let caps = match chilpa_core::probe_device(&id) {
            Ok(c) => c,
            Err(e) => {
                return WizardReport {
                    device: id,
                    probed: false,
                    summary: chilpa_i18n::t_args("the device could not be opened: {}", &[&format!("{e:?}")]),
                    rates: Vec::new(),
                    formats: Vec::new(),
                    rows: Vec::new(),
                }
            }
        };
        let d = chilpa_core::wizard::diagnose(&id, &caps);
        WizardReport {
            device: d.device,
            probed: d.probed,
            summary: d.summary,
            rates: d.sample_rates,
            formats: d.formats,
            rows: d
                .rows
                .into_iter()
                .map(|f| WizardRow {
                    // The name and the spelled out format are marked where
                    // they are built, in the wizard; translating them here is
                    // what makes the panel follow the chosen language.
                    name: chilpa_i18n::t(&f.name),
                    format: f.format,
                    verdict: match f.verdict {
                        chilpa_core::wizard::Verdict::BitPerfect => "bit-perfect".into(),
                        chilpa_core::wizard::Verdict::Converted { a } => {
                            chilpa_i18n::t_args("converted to {}", &[&a])
                        }
                        chilpa_core::wizard::Verdict::Unsupported => chilpa_i18n::t("cannot be done"),
                    },
                })
                .collect(),
        }
    }

    /// The chosen output, or the first one there is.
    fn device_or_first(&self) -> String {
        self.outputs().first().map(|o| o.id.clone()).unwrap_or_default()
    }

    /// What the chosen output's own volume control is doing.
    ///
    /// Chilpa never scales samples, so the only volume there is is the
    /// device's. An output without one says so rather than growing a slider
    /// that would silently stop the audio being bit-perfect.
    pub fn volume(&mut self, device: &str) -> chilpa_core::VolumeState {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        chilpa_core::volume_of(&id)
    }

    /// Moves the output's own volume control. Says why if it cannot.
    pub fn set_volume(&mut self, device: &str, percent: u8) -> bool {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        match chilpa_core::set_volume(&id, percent) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = e;
                false
            }
        }
    }

    /// Mutes or unmutes the output. Says why if it cannot.
    pub fn set_muted(&mut self, device: &str, muted: bool) -> bool {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        match chilpa_core::set_muted(&id, muted) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = e;
                false
            }
        }
    }

    /// The audio stacks, present or not.
    ///
    /// The ones that are not here come with their reason rather than being
    /// dropped: a picker that silently leaves out what it could not find
    /// teaches nobody anything, and the reason is usually one package away.
    pub fn stacks(&self) -> Vec<OutputStack> {
        chilpa_core::backends::described()
            .into_iter()
            .map(|s| OutputStack {
                key: s.kind.key().to_string(),
                label: s.label(),
                present: s.present,
                missing: s.missing,
                fidelity: match s.fidelity {
                    chilpa_core::backends::Fidelity::Exact => "exact",
                    chilpa_core::backends::Fidelity::Depends => "depends",
                    chilpa_core::backends::Fidelity::Never => "never",
                }
                .to_string(),
                caveat: s.caveat,
            })
            .collect()
    }

    /// What one stack offers. Empty for one that is not here, and for one
    /// that is here with nothing plugged into it -- which are different
    /// things, and `stacks` is where they are told apart.
    pub fn stack_outputs(&self, key: &str) -> Vec<Output> {
        let Some(kind) = chilpa_core::backends::StackKind::from_key(key) else {
            return Vec::new();
        };
        chilpa_core::backends::outputs_of(kind)
            .into_iter()
            .map(|o| Output {
                id: o.id,
                label: o.label,
                detail: o.detail,
                bit_perfect: o.bit_perfect,
            })
            .collect()
    }

    /// The stack an output belongs to, out of its identifier.
    pub fn stack_of(&self, output_id: &str) -> String {
        output_id.split_once(':').map(|(key, _)| key.to_string()).unwrap_or_default()
    }

    /// The outputs the user can choose from.
    pub fn outputs(&self) -> Vec<Output> {
        chilpa_core::outputs()
            .into_iter()
            .map(|o| Output {
                id: o.id,
                label: o.label,
                detail: o.detail,
                bit_perfect: o.bit_perfect,
            })
            .collect()
    }

    /// Starts a file playing. It returns immediately: whatever happens next
    /// arrives through [`Chilpa::poll`].
    pub fn play_file(&mut self, path: &std::path::Path, device: &str, bit_perfect: bool) {
        // A lone file is a one track queue too: that way it goes down the
        // same path and inherits the tap, the repeat and the order.
        self.queue = vec![path.display().to_string()];
        self.removed = vec![false];
        self.spawn(0, device, bit_perfect);
        self.now.path = path.display().to_string();
    }

    /// The envelope of what just went out, as `points` values from 0 to 1.
    ///
    /// It is each slice's peak, not its average: a wave drawn from averages
    /// looks flat and does not resemble what is heard.
    pub fn scope_envelope(&self, points: usize) -> Vec<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        let frames = snapshot.samples.len() / channels;
        if frames == 0 || points == 0 {
            return Vec::new();
        }
        let per_point = (frames / points).max(1);
        let mut output = Vec::with_capacity(points);
        for i in 0..points {
            let from = i * per_point;
            if from >= frames {
                break;
            }
            let to = (from + per_point).min(frames);
            let mut peak = 0.0f32;
            for f in from..to {
                for c in 0..channels {
                    peak = peak.max(snapshot.samples[f * channels + c].abs());
                }
            }
            output.push(peak);
        }
        output
    }

    /// The spectrum of what is going out, as `bands` values from 0 to 1.
    ///
    /// The channels are mixed to mono: a spectrum per channel looks the same
    /// on almost all music and takes twice the screen.
    pub fn scope_spectrum(&self, bands: usize) -> Vec<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        if snapshot.samples.is_empty() {
            return Vec::new();
        }
        let mono: Vec<f32> = snapshot
            .samples
            .chunks_exact(channels)
            .map(|f| f.iter().sum::<f32>() / channels as f32)
            .collect();
        chilpa_core::spectrum::bands(&mono, snapshot.sample_rate_hz, bands)
    }

    /// (left, right) pairs of what just went out, for the goniometer. On
    /// mono the same value is returned twice, which draws a line at 45
    /// degrees, as it should.
    pub fn scope_pairs(&self, points: usize) -> Vec<(f32, f32)> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        let frames = snapshot.samples.len() / channels;
        if frames == 0 || points == 0 {
            return Vec::new();
        }
        // One in every so many is taken: drawing eight thousand points per
        // frame does not look different but is certainly felt.
        let skip = (frames / points).max(1);
        (0..frames)
            .step_by(skip)
            .take(points)
            .map(|f| {
                let i = snapshot.samples[f * channels];
                let d = if channels > 1 { snapshot.samples[f * channels + 1] } else { i };
                (i, d)
            })
            .collect()
    }

    /// The correlation between the two channels, from -1 to 1.
    ///
    /// One is mono, zero is wide stereo and below zero there is something
    /// out of phase, which cancels in mono. It is the number a phase meter
    /// shows, and the one that says whether a mix will survive a single
    /// speaker.
    pub fn scope_correlation(&self) -> Option<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        if channels < 2 || snapshot.samples.is_empty() {
            return None;
        }
        let (mut left_sum, mut right_sum, mut cross_sum) = (0.0f64, 0.0f64, 0.0f64);
        for f in snapshot.samples.chunks_exact(channels) {
            let (i, d) = (f64::from(f[0]), f64::from(f[1]));
            left_sum += i * i;
            right_sum += d * d;
            cross_sum += i * d;
        }
        let den = (left_sum * right_sum).sqrt();
        (den > 0.0).then(|| (cross_sum / den) as f32)
    }

    /// The level per channel, from 0 to 1: peak and rms.
    pub fn scope_levels(&self) -> Vec<(f32, f32)> {
        let snapshot = self.tap.snapshot();
        snapshot.peaks.iter().copied().zip(snapshot.rms.iter().copied()).collect()
    }

    /// What is actually going out the device, for drawing.
    ///
    /// These are the delivered samples, copied after packing: what is seen
    /// is what played, not what was decoded.
    pub fn scope(&self) -> chilpa_core::tap::Snapshot {
        self.tap.snapshot()
    }

    /// Starts a whole folder playing, in alphabetical order.
    ///
    /// It returns the queued titles: the tag's when the file carries one,
    /// and the file's name otherwise.
    pub fn play_folder(
        &mut self,
        folder: &std::path::Path,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let tracks = audio_files_in(folder);
        if tracks.is_empty() {
            self.now.error = chilpa_i18n::t_args(
                "there is nothing Chilpa can play in {}",
                &[&folder.display().to_string()],
            );
            return Vec::new();
        }
        // The folder goes into the queue too: otherwise saving it or going
        // back would have nothing to read from.
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    /// Pauses or resumes what is playing.
    pub fn set_paused(&mut self, paused: bool) {
        if let Some(p) = &self.player {
            p.set_paused(paused);
            self.now.paused = paused;
        }
    }

    /// Seeks to that second of the playing track.
    pub fn seek(&mut self, seconds: f64) {
        if let Some(p) = &self.player {
            let safe = seconds.max(0.0);
            p.seek(std::time::Duration::from_secs_f64(safe));
        }
    }

    /// Skips to the next track in the queue.
    pub fn next_track(&mut self) {
        if let Some(p) = &self.player {
            p.next();
        }
    }

    /// Goes back to the previous track.
    ///
    /// There is no signal for this: the queue is relaunched from the previous
    /// track, which is what the player knows how to do. The queue is not
    /// trimmed, only where it starts changes.
    ///
    /// Relaunching stops what is playing first, and it has to: the player
    /// thread serves one order at a time and a play order blocks in it until
    /// the queue is done, so without the stop this did nothing at all until
    /// the queue ran out -- and with repeat on, nothing ever.
    ///
    /// With shuffle on there is no meaningful "previous one": what happens
    /// is that the playing one restarts.
    pub fn previous_track(&mut self, device: &str, bit_perfect: bool) {
        if self.queue.is_empty() || self.now.index == 0 || self.shuffle {
            // On the first track, "previous" means starting it over.
            self.seek(0.0);
            return;
        }
        self.spawn(self.now.index - 1, device, bit_perfect);
    }

    /// Stops whatever is playing.
    pub fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
    }

    /// Collects whatever has happened and updates the state the view sees.
    pub fn poll(&mut self) -> &NowPlaying {
        use chilpa_core::player::PlayerEvent;
        self.advance_search();
        self.advance_connect();
        self.now.paused = self.player.as_ref().is_some_and(|p| p.is_paused());
        let events = self.player.as_ref().map(|p| p.poll()).unwrap_or_default();
        for event in events {
            match event {
                PlayerEvent::Started {
                    path,
                    opened,
                    bits,
                    bit_perfect,
                    total_frames,
                    tags,
                    artwork,
                    index,
                    queue_len,
                    dsd_multiple,
                } => {
                    self.sample_rate = opened.sample_rate_hz;
                    self.now.path = path.display().to_string();
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.now.artwork_path = artwork
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    match &tags {
                        Some(t) => {
                            self.now.title = t.title.clone().unwrap_or(name);
                            self.now.artist = t.artist_line();
                            self.now.album = t.album.clone().unwrap_or_default();
                        }
                        None => {
                            self.now.title = name;
                            self.now.artist = String::new();
                            self.now.album = String::new();
                        }
                    }
                    // What came from a source already carries its card from
                    // the search. Without this the window would show the raw
                    // URL, which is all the decoder knows about it.
                    self.name_from_source(&path);
                    self.now.playing = true;
                    self.now.bit_perfect = bit_perfect;
                    self.load_lyrics(&path);
                    self.load_analysis(&path);
                    // The player counts over what it was given; the view
                    // counts over the whole queue.
                    self.now.index = self.base + index;
                    self.now.queue_len = if self.queue.is_empty() {
                        queue_len
                    } else {
                        self.queue.len()
                    };
                    // Every start closes the previous one: in a queue, the
                    // track that starts is the proof the one before it
                    // finished.
                    self.log_play(false);
                    self.in_progress = Some(InProgress {
                        path: path.display().to_string(),
                        started_at: now_unix(),
                        sample_rate_hz: opened.sample_rate_hz,
                        // The bits that carry the music, not the word they
                        // travel in: a CD rip widened for a DAC that only
                        // takes 32 bit words was logged as 24.
                        bit_depth: bits,
                        bit_perfect,
                        duration_ms: match (total_frames, opened.sample_rate_hz) {
                            (Some(f), rate) if rate > 0 => f * 1000 / u64::from(rate),
                            _ => 0,
                        },
                    });
                    self.now.total_seconds = match (total_frames, opened.sample_rate_hz) {
                        (Some(f), rate) if rate > 0 => f as f64 / f64::from(rate),
                        _ => 0.0,
                    };
                    self.now.format = match dsd_multiple {
                        // On DSD what matters is the DSD rate; the DoP
                        // carrier is stated separately, because it is the
                        // format the device was opened with.
                        Some(multiple) => chilpa_i18n::t_args(
                            "DSD{} · DoP at {} Hz · {}",
                            &[
                                &multiple.to_string(),
                                &opened.sample_rate_hz.to_string(),
                                &channels(opened.channels),
                            ],
                        ),
                        None => match bits {
                            Some(bits) => format!(
                                "{} Hz · {} bits · {}",
                                opened.sample_rate_hz,
                                bits,
                                channels(opened.channels)
                            ),
                            // A lossy track has no bit depth, and the word it
                            // was rounded into is not one: "32 bits" on an
                            // MP3 reads as more than it is.
                            None => format!("{} Hz · {}", opened.sample_rate_hz, channels(opened.channels)),
                        },
                    };
                }
                PlayerEvent::StreamTitle { title } => {
                    // On a radio the title is sent by the station and
                    // changes with every song.
                    self.now.title = title;
                }
                PlayerEvent::Progress { frames } => {
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.now.lyrics_line = self.line_at(self.now.seconds);
                }
                PlayerEvent::Finished { frames } => {
                    self.now.frames = frames;
                    self.now.playing = false;
                    self.log_play(false);
                }
                PlayerEvent::Stopped { frames } => {
                    self.now.frames = frames;
                    // A stop we asked for in order to start again is not the
                    // listener stopping. Saying `playing = false` here would
                    // blink the window to stopped on its way to the new
                    // device, and the play that is already queued behind this
                    // event is what decides what happens next.
                    match self.relaunching.take() {
                        // Going back: the track that was left is a skip,
                        // because somebody chose to leave it.
                        Some(Relaunch::Track) => self.log_play(true),
                        // Moving output: `in_progress` was already dropped,
                        // so there is nothing to write down.
                        Some(Relaunch::Output) => {}
                        None => {
                            self.now.playing = false;
                            // Stopping mid track counts as a skip.
                            self.log_play(true);
                        }
                    }
                }
                PlayerEvent::Failed { message } => {
                    self.now.playing = false;
                    self.now.error = message;
                    // A device failure is not a listen, but whatever did
                    // play is logged, marked as interrupted.
                    self.log_play(true);
                }
            }
        }
        &self.now
    }

    /// Records in the log how much of the current track had played.
    ///
    /// The log is append-only: the stats, the smart lists and the
    /// play count sync with a DAP all come out of here.
    fn log_play(&mut self, skipped: bool) {
        let Some(current) = self.in_progress.take() else { return };
        let frames = self.now.frames;
        if current.sample_rate_hz == 0 || frames == 0 {
            return;
        }
        let played_ms = frames * 1000 / u64::from(current.sample_rate_hz);
        let path = std::path::PathBuf::from(&current.path);
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let track = lib.track_by_path(source, &path).ok().flatten();
        let _ = lib.log_play(
            track,
            Some(source),
            current.started_at,
            played_ms,
            current.duration_ms,
            skipped,
            Some(current.sample_rate_hz),
            current.bit_depth,
            current.bit_perfect,
        );
    }

    /// The current state, without collecting events.
    pub fn now_playing(&self) -> &NowPlaying {
        &self.now
    }

    /// Runs a command and returns what the frontend should know.
    pub fn dispatch(&mut self, command: Command) -> Event {
        match command {
            Command::ListSources => Event::Sources(self.core.sources().to_vec()),
            Command::PlayFile { path, device, bit_perfect } => {
                match self.core.play_local(&path, &device, bit_perfect) {
                    Ok(played) => Event::Played {
                        frames: played.frames,
                        quality: played.quality,
                        bit_perfect: played.bit_perfect,
                    },
                    Err(message) => Event::Failed { message },
                }
            }
            Command::Play(_) | Command::Pause | Command::Resume => {
                Event::Failed { message: "the player is not behaving yet".into() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_sources_returns_the_local_library() {
        let mut chilpa = Chilpa::new();
        assert_eq!(chilpa.dispatch(Command::ListSources), Event::Sources(vec![SourceId::local()]));
    }

    #[test]
    fn the_mix_walks_tempo_to_tempo_without_jumps() {
        // It starts at the 120 one and should go on 118, 125, 90.
        let bpms = vec![Some(90.0), Some(120.0), Some(125.0), Some(118.0)];
        assert_eq!(order_by_tempo(&bpms, 1), vec![1, 3, 2, 0]);
    }

    #[test]
    fn what_has_no_tempo_goes_last() {
        let bpms = vec![Some(120.0), None, Some(122.0), None, Some(160.0)];
        let order = order_by_tempo(&bpms, 0);
        assert_eq!(&order[..3], &[0, 2, 4], "the ones that do come first, nearest to nearest");
        let queue: Vec<usize> = order[3..].to_vec();
        assert_eq!(queue, vec![1, 3], "and the blind ones at the end, in their own order");
    }

    #[test]
    fn a_mix_loses_no_track_and_repeats_none() {
        let bpms = vec![Some(100.0), Some(140.0), None, Some(101.0), Some(139.0), None];
        let order = order_by_tempo(&bpms, 3);
        let mut seen = order.clone();
        seen.sort_unstable();
        assert_eq!(seen, (0..6).collect::<Vec<_>>(), "it is a permutation: {order:?}");
        assert_eq!(order[0], 3, "it starts where it was told to");
    }

    #[test]
    fn with_no_tempo_at_all_the_mix_leaves_the_order_alone() {
        let bpms = vec![None, None, None];
        assert_eq!(order_by_tempo(&bpms, 0), vec![0, 1, 2]);
    }

    #[test]
    fn channels_are_named_the_way_they_are_said() {
        assert_eq!(channels(1), "mono");
        assert_eq!(channels(2), "stereo");
        assert_eq!(channels(6), "6 channels");
    }

    #[test]
    fn a_folder_yields_audio_files_only_and_in_order() {
        let mut dir = std::env::temp_dir();
        dir.push("chilpa-api-folder");
        let _ = std::fs::create_dir_all(&dir);
        for name in ["b.flac", "a.wav", "notes.txt", "c.MP3"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let found: Vec<String> = audio_files_in(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(found, vec!["a.wav", "b.flac", "c.MP3"], "without the .txt and in order");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_state_starts_empty_and_without_error() {
        let chilpa = Chilpa::new();
        assert!(!chilpa.now_playing().playing);
        assert!(chilpa.now_playing().error.is_empty());
        assert_eq!(chilpa.now_playing().frames, 0);
    }

    #[test]
    fn every_listed_output_names_the_stack_it_belongs_to() {
        // A bare "hw:1,0" does not say which stack it came from, and that is
        // exactly what somebody has to be told when a remembered output is
        // no longer there.
        for output in Chilpa::new().outputs() {
            let (stack, bare) = output
                .id
                .split_once(':')
                .unwrap_or_else(|| panic!("{} names no stack", output.id));
            // Asked of the core and not of the audio crate. The facade is
            // not allowed to reach into the engine (ADR-001), and the answer
            // to "which stacks are there" belongs to whoever assembles them.
            // Writing the list out here instead would be the same leak with
            // the wires hidden: a copy that goes stale the day one is added.
            assert!(
                chilpa_core::backends::StackKind::from_key(stack).is_some(),
                "{} names no stack this build has",
                output.id
            );
            assert!(!bare.is_empty(), "{} names no output", output.id);
            assert!(!output.label.is_empty(), "{} has no name to show", output.id);
        }
    }

    #[test]
    fn what_does_not_exist_fails_with_a_message_for_the_ui() {
        let mut chilpa = Chilpa::new();
        let track = TrackRef { source: SourceId::local(), external_id: "1".into() };
        assert!(matches!(chilpa.dispatch(Command::Play(track)), Event::Failed { .. }));
    }
}
