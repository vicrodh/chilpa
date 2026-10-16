//! Chilpa's facade: the only thing a frontend may use (ADR-001).
//!
//! A shell sends [`Command`]s and receives [`Event`]s. It sees neither the
//! core, nor the library, nor the player. If a view needs something that is
//! not here, this API is widened first.
pub use chilpa_core::queue_playback::Repeat;
pub use chilpa_library::{AlbumSort, Stats};
pub use plugins::{NativeCaller, PluginInfo};
pub use search::{Hit, KEYSTROKE_DEBOUNCE, SOURCE_COOLDOWN};

mod ab_loop;
pub mod about;
mod accounts;
mod casting;
mod dsp;
pub use chilpa_cast::{Device as CastDevice, Kind as CastKind};
mod approvals;
mod browse;
mod credits;
mod desktop;
pub use desktop::WindowRequest;
mod tracks_view;
mod albums_view;
pub use albums_view::{artist_cards, grid_rows, mixes_sources, AlbumsFound, GridRow};
pub use tracks_view::{group_of, FoundTracks, TracksQuery};
mod queries;
pub use queries::QueryWorker;
mod chosen_artwork;
mod covers;
mod folders;
mod hotkeys;
mod commands;
mod vi_keys;
mod home;
pub use home::HomeRows;
pub use hotkeys::{Pressed, Scope};
pub mod format;
pub mod paths;
mod library;
mod listening_export;
mod listening_period;
mod listening_stats;
pub mod listening;
mod metadata;
pub mod navigation;
pub mod queue_origin;
mod queue_activity;
pub use queue_activity::{QueueWork, AlbumQueueWork};
mod outputs;
mod playlists;
pub use playlists::PlaylistTarget;
mod preferences;
mod privacy;
pub use privacy::network_allowed;
mod play_indicator;
mod playlist_sources;
mod plugin_broker;
mod playlist_import;
mod pictures;
mod storage_cache;
mod plugins;
mod plugin_startup;
mod worker_services;
pub use worker_services::WorkerContext;
mod worker_install;
mod plugin_repositories;
mod appimage_updates;
pub mod appimage_desktop;
pub use plugin_repositories::{PluginRepositories, PluginRepository};
mod plugin_package;
pub use plugin_package::StagedPackage;
mod plugin_inspection;
pub use plugin_inspection::PluginInspection;
mod plugin_installation;
mod plugin_icons;
mod plugin_sidebar;
mod plugin_access;
mod svg_icon;
mod plugin_library;
mod plugin_center;
mod source_integration;
mod plugin_evidence;
mod plugin_domains;
mod natives;
mod lyrics_client;
mod lyrics_service;
mod lyrics_view;
pub use lyrics_view::lyric_dimming;
mod native_reporting;
mod native_install;
pub mod stall;
pub use plugin_installation::PreparedPlugin;
pub use chilpa_plugin::bridge::WorkerLauncher;
mod quality;
mod marks;
pub use marks::Marks;
mod ratings;
pub use ratings::{RatingTarget, Ratings};
mod queue;
mod listen;
mod mood;
pub mod discs;
pub mod tray;
pub mod ambient;
pub mod immersive;
pub mod mini;
pub mod seek_waveform;
pub mod visualizer;
pub mod wallpaper;
pub mod open_media;
mod open_link;
pub mod window_place;
pub mod player_layout;
mod found;
mod found_selection;
mod source_search;
mod source_layout;
mod source_catalog;
pub use found::{found_menu, found_rows, FoundRow, Leads};
pub use found_selection::{FoundSelection, SelectionState};
pub use source_catalog::SelectionHold;
pub use source_catalog::view::SourceShelfSnapshot;
mod track_menu;
pub use track_menu::{track_can_go, track_menu, TrackMenuContext};
mod scrobble;
mod search;
mod source_opener;
mod source_favorites;
mod sources;
mod servers;
pub mod settings_rows;
pub mod graphics_boot;
pub mod startup_chain;
pub mod settings_schema;
mod artist_page;
mod artist_library;
mod artist_scene;
mod scene_catalog;
mod scene_results;
pub use servers::{on_a_server, ServerRemoval, ServerRow, ServerSetup};
pub use source_opener::on_a_source;
pub use settings_rows::{RowNotes, SettingsRow};
pub mod sidebar;
pub mod theme;
pub mod window;
mod volume;
pub use volume::VolumeView;
pub use metadata::view::MetadataView;

pub use browse::{filter_facets, select_facets, select_tracks, AlbumDetail, FacetEntry, Facets, Filter, FolderItem, FolderView, Found, GroupHeader, Reader, Snapshot, Totals, TrackEntry, TrackOrder, TrackRow};
pub use browse::PageCache;
mod album_versions;
mod album_page;
mod album_shelves;
pub use album_page::AlbumPage;
mod album_information;
pub use album_versions::{AlbumSource, AlbumVersion};
pub use covers::{contained_cover, cover_derivative, header_atmosphere};
pub use folders::audio_files_in;
pub use quality::{Quality, Tier};
pub use scrobble::ScrobbleStatus;

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
    PlayFile {
        path: PathBuf,
        device: String,
        bit_perfect: bool,
    },
}

/// What Chilpa tells the frontend.
#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Sources(Vec<SourceId>),
    /// Something started playing, with the output's real quality.
    Playing {
        track: TrackRef,
        quality: AudioQuality,
    },
    Paused,
    /// A file finished playing: how many frames went out, what format the
    /// device was opened with and whether it was bit-perfect.
    Played {
        frames: u64,
        quality: AudioQuality,
        bit_perfect: bool,
    },
    /// Something went wrong, in text the UI can show.
    Failed {
        message: String,
    },
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
    /// The local library album, or zero when this track has no local context.
    pub album_id: i64,
    /// The cover already on disk, ready to show. Empty if there is none.
    pub artwork_path: String,
    pub covers: Vec<String>,
    pub playing: bool,
    pub paused: bool,
    /// Frames delivered and their equivalent in seconds.
    pub frames: u64,
    pub seconds: f64,
    /// Total duration, when it is known.
    pub total_seconds: f64,
    /// What the device was opened with, ready to show.
    pub format: String,
    /// What the music is: the file's format, and the bits that carry the
    /// signal as the engine reports them. Not the device's word width.
    pub quality: Quality,
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
    /// Where it came from: "file", "tags", the name of the plugin that
    /// gave them, or empty.
    pub lyrics_source: String,
    /// Grows when other lyrics arrive; then read [`Chilpa::lyrics_json`].
    pub lyrics_revision: u64,
    /// The last problem, empty if there is none.
    pub error: String,
    /// True when what plays has no end and cannot be sought, like a radio
    /// station: the window shows no seek bar and no time left.
    pub live: bool,
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
pub(crate) fn describe_analysis(bpm: Option<f32>, tone: Option<&str>, lufs: Option<f32>) -> String {
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
    LibraryTrack { id: t.id, path: t.external_id, title: t.title, artist: t.artist, album: t.album, track_number: t.track_number, duration_ms: t.duration_ms, present: t.playable, analysis: String::new() }
}

/// The title a queue shows: the tag's, or the file's name.
fn title_of(path: &std::path::Path) -> String {
    if let Some(title) = chilpa_core::sacd_tracks::title(path) {
        return title;
    }
    // A server's track has no file here to read a title from.
    if chilpa_models::RemoteEntry::is_remote(&path.to_string_lossy()) {
        return chilpa_core::remote::title(path).unwrap_or_default();
    }
    chilpa_tags::read(path).ok().and_then(|info| info.tags.title).unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
}

/// Now, in seconds since the epoch.
fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// How a channel count is named, already plural where it should be.
fn channels(n: u16) -> String {
    match n {
        1 => chilpa_i18n::t("mono"),
        2 => chilpa_i18n::t("stereo"),
        others => chilpa_i18n::tf("{} channel", "{} channels", i64::from(others), &[&others.to_string()]),
    }
}

/// What is known about the playing track, to log it when it ends.
#[derive(Clone, Debug)]
struct InProgress {
    context: listening::ListenContext,
    path: PathBuf,
    resuming: bool,
    carried_ms: u64,
    started_at: i64,
    sample_rate_hz: u32,
    bit_depth: Option<u8>,
    bit_perfect: bool,
    duration_ms: u64,
    /// Frames of it that went out, which is what was heard. The position is
    /// not: it was logged as the time listened, and a seek to the last
    /// second logged the whole track.
    played_frames: u64,
    /// What ListenBrainz is sent once enough of it has been heard; none for
    /// what the file's tags cannot name.
    listen: Option<scrobble::Listen>,
    /// It was already sent: a seek back past the mark does not send it twice.
    scrobbled: bool,
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
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Playlist {
    /// The library's stable cached row id.
    pub id: i64,
    /// The cached source row that owns it.
    pub source_id: i64,
    pub name: String,
    pub description: String,
    pub artwork_path: String,
    /// Up to four cached track covers used by the optional sidebar collage.
    pub covers: Vec<String>,
    pub tags: Vec<String>,
    pub provider: String,
    pub instance: String,
    pub track_count: usize,
    pub play_count: u64, pub last_played_at: Option<i64>,
    pub folder_id: Option<i64>,
    pub custom_position: i64,
    pub visible_sidebar: bool,
    pub updated_at: i64,
    /// Only our own playlists are mixed-source. A source-owned list is
    /// writable only when its source declares that capability.
    pub writable: bool,
    pub mixed: bool,
    /// The plugin it came from, empty when it is one of ours.
    pub source: String,
    /// The id that source gives it, empty when it is one of ours.
    pub external_id: String,
}

/// One folder in the playlist manager.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct PlaylistFolder {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
    pub playlist_count: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize)]
pub struct PlaylistAddResult {
    pub added: usize,
    pub exact: usize,
    pub similar: usize,
    pub rejected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub enum PlaylistManagerKind {
    Folder,
    Playlist,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct PlaylistManagerEntry {
    pub kind: PlaylistManagerKind,
    pub id: i64,
    pub name: String,
    pub detail: String,
    pub source: String,
    pub source_name: String,
    pub artwork_path: String,
    pub covers: Vec<String>,
    pub color: String,
    pub track_count: usize,
    pub play_count: u64,
    pub folder_id: Option<i64>,
    pub local: bool,
    pub visible_sidebar: bool,
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

/// A folder of the library, as the folders page shows it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LibraryFolder {
    pub path: String,
    /// When it was last scanned, in unix seconds.
    pub last_scan: Option<i64>,
    /// False when it is not there now: an unmounted disk, a moved folder.
    pub reachable: bool,
}

/// A named batch added with "Play later". Its positions stay stable because
/// removing queue entries marks them instead of shifting the array.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct QueueGroup {
    pub key: u64,
    pub label: String,
    pub tracks: Vec<usize>,
    pub collapsed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
enum QueuePlacement {
    #[default]
    Original,
    Next,
    Later,
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
    /// The background analyzer changed generated playlists.
    pub playlists_updated: bool,
    /// Mood analysis is running or supplied the latest analysis result.
    pub mood_requested: bool,
    /// Asked for while a scan runs, for that folder: it starts when the scan
    /// ends.
    pub waiting: Option<String>,
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
    /// Found and not readable: links that point nowhere, mostly.
    pub unreadable: usize,
    pub unreachable: bool,
}

/// The Chilpa instance a frontend talks to.
pub struct Chilpa {
    core: chilpa_core::Core,
    library: Option<chilpa_library::Library>,
    /// What is needed to log the play when it ends.
    in_progress: Option<InProgress>,
    /// Sends listens to ListenBrainz; started by the window, never by a test.
    scrobbler: Option<scrobble::Scrobbler>,
    /// The current queue's paths, whole and in the order they were queued.
    /// Going back does not trim it.
    queue: Vec<String>,
    queue_work: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    queue_origins: Vec<Option<queue_origin::QueueOrigin>>,
    browse_origin: Option<queue_origin::QueueOrigin>,
    listen: listen::Workspace,
    media: Option<open_media::Session>,
    /// Which position of `queue` was the first one handed to the player.
    /// Going back restarts from there, and the position the view sees is
    /// counted over the whole queue.
    base: usize,
    shuffle: bool,
    repeat: Repeat,
    /// The queue is walked in the mix's order.
    mix: bool,
    album_sort: AlbumSort,
    /// The artist the grid is filtered to, empty if there is no filter.
    artist: String,
    /// Which positions of `queue` were removed, so the view greys them out.
    removed: Vec<bool>,
    queue_placement: Vec<QueuePlacement>,
    queue_groups: Vec<QueueGroup>,
    next_queue_group: u64,
    /// Position within the first restored track. A restored queue is shown
    /// paused and only opens an output when the listener resumes it.
    restored_at: Option<f64>,
    /// A position in the whole queue after which playback ends.
    stop_after: Option<usize>,
    /// A wall-clock deadline that asks the player to stop from `poll`.
    sleep_until: Option<std::time::Instant>,
    /// The lyrics of what is playing, and the thread that finds them.
    lyrics: lyrics_service::Lyrics,
    desktop: desktop::Desktop,
    ab_loop: Option<ab_loop::AbLoop>,
    keys: Option<hotkeys::Keys>,
    source_favorites: source_favorites::SourceFavorites,
    /// The plugin sandbox, built the first time it is asked for. It is
    /// shared because search queries it from its own thread: a slow source
    /// must not leave the window waiting.
    plugins: Option<std::sync::Arc<std::sync::Mutex<plugins::Plugins>>>,
    plugin_center: Option<plugin_center::PluginCenter>,
    dsp: dsp::Dsp,
    appimage_updates: Option<appimage_updates::Updates>,
    plugin_startup: plugin_startup::Startup,
    /// Process plugins, reached without the sandbox lock.
    natives: std::sync::Arc<natives::Natives>,
    /// The music link being opened, resolved on a thread of its own.
    open_link: open_link::Shared,
    /// What actually goes out the device, for the visualizers.
    tap: chilpa_core::tap::Tap,
    visualizer: visualizer::Visualizer,
    seek_waveform: seek_waveform::SeekWaveform,
    /// The background scan, while it runs.
    scan: Option<chilpa_library::ScanJob>,
    scan_status: ScanStatus,
    analysis: Option<chilpa_core::analysis_job::AnalysisJob>,
    mood_analysis: Option<mood::Job>,
    analysis_folder: Option<String>,
    analysis_status: AnalysisStatus,
    player: Option<chilpa_core::player::Player>,
    /// The search in progress: what is local, what the sources contributed
    /// and what the view draws.
    search: search::SearchState,
    /// Where the window stands in a source that is browsed.
    sources: sources::SourcesState,
    playlist_import: playlist_import::State,
    /// The records, lists and tracks of sources, as the library's own.
    catalog: std::sync::Arc<source_catalog::Catalog>,
    /// What MusicBrainz and Last.fm know of the artists whose pages opened.
    artist_network: artist_page::Network,
    artist_library: artist_library::ArtistLibrary,
    artist_scene: artist_scene::Scene,
    /// The account being connected, between the browser opening and the
    /// user coming back.
    connecting: Option<plugins::Connecting>,
    now: NowPlaying,
    home: home::Home,
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
    /// The metadata editor, and the record open in it if there is one.
    metadata: metadata::Editor,
    /// My servers: adding, syncing, watching and removing them (ADR-017).
    servers: servers::Servers,
    casting: casting::Casting,
    /// The artists of the credits the window asked about, and the state of
    /// the library they were read at.
    credits: std::collections::HashMap<String, Vec<String>>,
    credits_read_at: (i64, u64),
    /// The covers and pictures the listener chose, and the one over what plays.
    artwork: chosen_artwork::State,
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
        chilpa_core::network::configure(privacy::policy(&saved));
        storage_cache::start(saved.window.get("storage.remoteLimitMb").map(String::as_str));
        #[cfg(not(test))]
        chilpa_i18n::set_language(if saved.language.is_empty() { chilpa_i18n::from_environment() } else { &saved.language });
        #[cfg(test)]
        chilpa_i18n::set_language("en");
        let on = |key| saved.window.get(key).is_some_and(|v| v == "true");
        chilpa_core::backends::set_reserving(on(outputs::RESERVE));
        chilpa_core::backends::set_forcing_rate(on(outputs::FORCE_RATE));
        chilpa_core::backends::warm_up();
        let catalog = source_catalog::Catalog::new();
        let tap = chilpa_core::tap::Tap::new();
        let mut chilpa = Self {
            core: chilpa_core::Core::new(),
            library: None,
            in_progress: None,
            scrobbler: None,
            queue: Vec::new(),
            queue_work: Default::default(),
            queue_origins: Vec::new(),
            browse_origin: None,
            listen: listen::Workspace::default(),
            media: None,
            base: 0,
            shuffle: false,
            repeat: Repeat::Off,
            mix: false,
            album_sort: AlbumSort::from_name(albums_view::turned_order(&saved.album_sort).0),
            artist: String::new(),
            removed: Vec::new(),
            queue_placement: Vec::new(),
            queue_groups: Vec::new(),
            next_queue_group: 1,
            restored_at: None,
            stop_after: None,
            sleep_until: None,
            lyrics: Default::default(),
            desktop: Default::default(),
            ab_loop: None,
            keys: None,
            source_favorites: Default::default(),
            plugins: None,
            plugin_center: None,
            dsp: Default::default(),
            appimage_updates: None,
            plugin_startup: plugin_startup::Startup::default(),
            natives: Default::default(),
            open_link: Default::default(),
            visualizer: visualizer::Visualizer::new(tap.clone()),
            seek_waveform: seek_waveform::SeekWaveform::default(),
            tap,
            scan: None,
            scan_status: ScanStatus::default(),
            analysis: None,
            mood_analysis: None,
            analysis_folder: None,
            analysis_status: AnalysisStatus::default(),
            player: None,
            search: search::SearchState::default(),
            sources: sources::SourcesState::default(),
            playlist_import: playlist_import::State::default(),
            artist_network: artist_page::Network::new(catalog.clone()),
            artist_library: Default::default(),
            artist_scene: artist_scene::Scene::new(),
            catalog,
            connecting: None,
            now: NowPlaying::default(),
            home: home::Home::default(),
            sample_rate: 0,
            relaunching: None,
            outputs_generation: String::new(),
            metadata: metadata::Editor::default(),
            servers: servers::Servers::default(),
            casting: casting::Casting::default(),
            credits: Default::default(),
            credits_read_at: (0, 0),
            artwork: Default::default(),
        };
        // Two starts in a row died before the window was shown: the kept
        // queue is left on disk for a start that reaches the window.
        if crate::startup_chain::level() >= 3 {
            eprintln!("startup: the queue restore is bypassed this start (crash chain); the queue stays on disk");
        } else if !chilpa.restore_buffers() {
            chilpa.restore_queue_session();
        }
        chilpa
    }

    /// The library, opened the first time it is used.
    pub(crate) fn library(&mut self) -> Option<&chilpa_library::Library> {
        if self.library.is_none() {
            match chilpa_core::open_library() {
                Ok(lib) => {
                    self.library = Some(lib);
                    self.load_chosen_remote_art();
                }
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
        let Ok(source) = lib.source_id("local", "default") else {
            return;
        };
        let Some(id) = lib.track_by_path(source, path).ok().flatten() else {
            return;
        };
        let Some((bpm, tone, loudness)) = lib.track_features(id, chilpa_core::analysis_job::ANALYZER).ok().flatten() else {
            return;
        };
        self.now.analysis = describe_analysis(bpm, tone.as_deref(), loudness);
    }

    /// Finds the stable album route for the playing file. A source URL or an
    /// unscanned file has no local record page and deliberately returns zero.
    fn load_album_context(&mut self) {
        let path = self.now.path.clone();
        if crate::source_opener::of_plugin(&path).is_some() {
            self.now.album_id = self.source_album_of_path(&path);
            return;
        }
        self.now.album_id = self.library().and_then(|library| library.tracks_by_paths(&[path]).ok()).and_then(|mut rows| rows.pop().flatten()).and_then(|track| track.album_id).unwrap_or_default();
    }

    /// Which line of the lyrics belongs to that second.
    fn line_at(&self, seconds: f64) -> i32 {
        let Some(lyrics) = &self.lyrics.document else {
            return -1;
        };
        let ms = (seconds.max(0.0) * 1000.0).min(u32::MAX as f64) as u32;
        chilpa_lyrics::timeline::current(lyrics, ms).map(|i| i as i32).unwrap_or(-1)
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
        self.replace_queue(vec![format!("cdda:{unit}")]);
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
        self.replace_queue(vec![url.clone()]);
        self.spawn(0, device, bit_perfect);
    }

    /// What Chilpa remembers: the last output and the last folder opened.
    pub fn settings(&self) -> chilpa_core::settings::Settings {
        chilpa_core::settings::load()
    }

    /// Chooses the output, remembers it and moves what is playing there at
    /// the same place. The retired boolean is ignored for source compatibility.
    pub fn use_output(&mut self, output: &str, _legacy_bit_perfect: bool) {
        self.remember(output, false);
        self.apply_playback_options(false);
        self.move_output(output, false);
    }

    /// Remembers the chosen output. The retired boolean is ignored for source
    /// compatibility with callers that have not migrated yet.
    pub fn remember(&mut self, output: &str, _legacy_bit_perfect: bool) {
        let mut s = chilpa_core::settings::load();
        s.output = output.to_string();
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
        if !language.is_empty() && !self.languages().contains(&language) { return; }
        let code = if language.is_empty() { chilpa_i18n::from_environment() } else { language };
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

    /// Starts a file playing. It returns immediately: whatever happens next
    /// arrives through [`Chilpa::poll`].
    pub fn play_file(&mut self, path: &std::path::Path, device: &str, bit_perfect: bool) {
        // A lone file is a one track queue too: that way it goes down the
        // same path and inherits the tap, the repeat and the order.
        // A disc image goes in as its tracks, one entry each.
        let tracks = chilpa_core::sacd_tracks::expand(&[path.to_path_buf()]);
        self.replace_queue(tracks.iter().map(|p| p.display().to_string()).collect());
        self.spawn(0, device, bit_perfect);
        self.now.path = self.queue[0].clone();
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
        let mono: Vec<f32> = snapshot.samples.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect();
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
    pub fn play_folder(&mut self, folder: &std::path::Path, device: &str, bit_perfect: bool) -> Vec<String> {
        let tracks = chilpa_core::sacd_tracks::expand(&audio_files_in(folder));
        if tracks.is_empty() {
            self.now.error = chilpa_i18n::t_args("there is nothing Chilpa can play in {}", &[&folder.display().to_string()]);
            return Vec::new();
        }
        // The folder goes into the queue too: otherwise saving it or going
        // back would have nothing to read from.
        self.replace_queue(tracks.iter().map(|p| p.display().to_string()).collect());
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    /// Pauses or resumes what is playing.
    pub fn set_paused(&mut self, paused: bool) {
        if paused { self.casting.local_start = None; self.persist_progress(true); }
        if self.casting.active() { self.cast_pause(paused); return; }
        if !paused && self.player.as_ref().is_none_or(|p| p.idle()) && !self.queue.is_empty() {
            let restored = self.restored_at.take();
            let seconds = restored.unwrap_or(self.now.seconds).max(0.0);
            let index = self.now.index.min(self.queue.len() - 1);
            let (device, bit_perfect) = self.chosen_output();
            self.spawn_from(index, &device, bit_perfect, Some(std::time::Duration::from_secs_f64(seconds)), Relaunch::Track);
            return;
        }
        if let Some(p) = &self.player {
            p.set_paused(paused);
            self.now.paused = paused;
        }
    }

    /// Seeks to that second of the playing track.
    pub fn seek(&mut self, seconds: f64) {
        if !seconds.is_finite() { return; }
        self.desktop.seeked = Some(seconds);
        self.ab_jumped(seconds);
        if self.casting.active() { self.cast_seek(seconds); return; }
        if let Some(p) = &self.player {
            let safe = seconds.max(0.0);
            p.seek(std::time::Duration::from_secs_f64(safe));
        }
    }

    /// Skips to the next track in the queue.
    pub fn next_track(&mut self) {
        if self.casting.active() { self.cast_next(false); return; }
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
    /// Previous follows the retained listening order, including shuffle.
    pub fn previous_track(&mut self, device: &str, bit_perfect: bool) {
        if self.casting.active() { self.cast_previous(); return; }
        let order = self.full_queue_order();
        let at = order.iter().position(|&i| i == self.now.index).unwrap_or(0);
        if let Some(index) = order[..at].iter().rev().copied().find(|&i| !self.removed.get(i).copied().unwrap_or(false)) {
            self.spawn(index, device, bit_perfect);
        } else { self.seek(0.0); }
    }

    /// Stops whatever is playing.
    pub fn stop(&mut self) {
        self.listen.pending = None;
        self.casting.local_start = None;
        if self.casting.active() { self.cast_stop(); return; }
        // An episode stopped halfway is picked up from here.
        self.keep_place(true);
        if let Some(p) = &self.player {
            p.stop();
        }
    }

    /// Collects whatever has happened and updates the state the view sees.
    pub fn poll(&mut self) -> &NowPlaying {
        use chilpa_core::player::PlayerEvent;
        if self.sleep_until.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            self.sleep_until = None;
            self.stop();
        }
        self.advance_ab();
        self.advance_desktop();
        self.advance_source_favorites();
        self.advance_plugins();
        self.advance_dsp();
        self.advance_search();
        self.advance_sources();
        self.advance_catalog();
        self.catalog.refresh_personal_libraries();
        self.advance_lyrics();
        self.resolve_next_ahead();
        self.keep_place(false);
        self.advance_connect();
        self.poll_servers();
        self.poll_cast();
        if let Some(player) = &self.player {
            self.now.paused = player.is_paused();
        }
        self.follow_playing_documents();
        let mut started = false;
        let events = self.player.as_ref().map(|p| p.poll()).unwrap_or_default();
        for event in events {
            if self.listen.active == 0 { continue; }
            match event {
                PlayerEvent::Started { path, opened, channels: music_channels, bits, bit_perfect, total_frames, tags, artwork, index, queue_len, dsd_multiple, dsd_passthrough } => {
                    self.dsp.passthrough = dsd_passthrough;
                    // What was playing until now is left where it was.
                    self.keep_place(true);
                    started = true;
                    self.sample_rate = opened.sample_rate_hz;
                    self.now.path = path.display().to_string();
                    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    self.now.artwork_path = artwork.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
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
                    self.now.live = self.name_from_entry(&path);
                    self.load_album_context();
                    if dsd_multiple.is_none() && crate::source_opener::of_plugin(&self.now.path).is_some() {
                        // What a source's track turned out to be, measured, for its row.
                        self.catalog.measured(&self.now.path, chilpa_core::remote::opened_codec(&self.now.path), bits, opened.sample_rate_hz);
                    }
                    self.now.playing = true;
                    self.now.bit_perfect = bit_perfect;
                    // On DSD the rate that names it is the one-bit source
                    // rate, not the DoP carrier or native U32 frame rate.
                    let rate = match dsd_multiple {
                        Some(multiple) => multiple * 44_100,
                        None => opened.sample_rate_hz,
                    };
                    let codec = self.remote_codec(&self.now.path.clone());
                    self.now.quality = Quality::of_track(&self.now.path, codec.as_deref(), bits, Some(rate));
                    self.load_lyrics(&path, lyrics_service::length_ms(total_frames, opened.sample_rate_hz));
                    self.load_analysis(&path);
                    // The player counts over what it was given; the view
                    // counts over the whole queue.
                    self.now.index = self.base + index;
                    self.now.queue_len = if self.queue.is_empty() { queue_len } else { self.queue.len() };
                    // Every start closes the previous one: in a queue, the
                    // track that starts is the proof the one before it
                    // finished.
                    let started_at = now_unix();
                    let duration_ms = match (total_frames, opened.sample_rate_hz) {
                        (Some(f), rate) if rate > 0 => f * 1000 / u64::from(rate),
                        _ => 0,
                    };
                    if !self.resume_listen(&path, opened.sample_rate_hz, bits, bit_perfect) {
                        self.log_play(false);
                        let listen = self.announce(tags.as_ref(), duration_ms, started_at);
                        let context = self.context_at_start(&path, tags.as_ref(), started_at);
                        self.home.changed();
                        self.in_progress = Some(InProgress {
                            context,
                            path: path.clone(),
                            resuming: false,
                            carried_ms: 0,
                            started_at,
                            sample_rate_hz: opened.sample_rate_hz,
                            // I log signal bits, not the DAC's wider transport word.
                            bit_depth: bits,
                            bit_perfect,
                            duration_ms,
                            played_frames: 0,
                            listen,
                            scrobbled: false,
                        });
                    }
                    self.now.total_seconds = match (total_frames, opened.sample_rate_hz) {
                        (Some(f), rate) if rate > 0 => f as f64 / f64::from(rate),
                        _ => 0.0,
                    };
                    // The channels are the music's: a mono track that goes
                    // out in both channels of a stereo DAC is still mono.
                    self.now.format = match (dsd_multiple, opened.sample_format.is_dsd(), dsd_passthrough) {
                        (Some(multiple), true, _) => chilpa_i18n::t_args("DSD{} · Native at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
                        // On DSD what matters is the DSD rate; the DoP
                        // carrier is stated separately, because it is the
                        // format the device was opened with.
                        (Some(multiple), false, true) => chilpa_i18n::t_args("DSD{} · DoP at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
                        (Some(multiple), false, false) => chilpa_i18n::t_args("DSD{} · PCM at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
                        (None, _, _) => match bits {
                            Some(bits) => format!("{} Hz · {} bits · {}", opened.sample_rate_hz, bits, channels(music_channels)),
                            // A lossy track has no bit depth, and the word it
                            // was rounded into is not one: "32 bits" on an
                            // MP3 reads as more than it is.
                            None => format!("{} Hz · {}", opened.sample_rate_hz, channels(music_channels)),
                        },
                    };
                }
                PlayerEvent::StreamTitle { title } => {
                    // On a radio the title is sent by the station and
                    // changes with every song. A station that was browsed
                    // keeps its name beside the song, and gets it back when
                    // the station sends an empty title between songs.
                    match self.station_on() {
                        Some(station) if title.trim().is_empty() => self.now.title = station,
                        Some(station) => {
                            self.song_changed(&title);
                            self.now.title = title;
                            self.now.artist = station;
                        }
                        None => self.now.title = title,
                    }
                }
                PlayerEvent::Progress { frames, played } => {
                    self.now.frames = frames;
                    self.set_played(played);
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.now.lyrics_line = self.line_at(self.now.seconds);
                }
                PlayerEvent::Finished { frames } => {
                    // What the renderer counted: what went out, not where
                    // the track was.
                    self.set_played(frames);
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.keep_place(true);
                    self.now.playing = false;
                    self.stop_after = None;
                    self.sleep_until = None;
                    self.log_play(false);
                }
                PlayerEvent::Stopped { frames } => {
                    self.set_played(frames);
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
                        // I carry the delivered time into the next output segment.
                        Some(Relaunch::Output) => {
                            if let Some(current) = self.in_progress.as_mut() { current.resuming = true; }
                        }
                        None => {
                            self.now.playing = false;
                            self.stop_after = None;
                            self.sleep_until = None;
                            // Stopping mid track counts as a skip.
                            self.log_play(true);
                        }
                    }
                }
                PlayerEvent::Failed { message } => {
                    self.now.playing = false;
                    self.stop_after = None;
                    self.sleep_until = None;
                    self.now.error = message;
                    // A device failure is not a listen, but whatever did
                    // play is logged, marked as interrupted.
                    self.log_play(true);
                }
            }
        }
        // A new track's verdict cannot see the volume moved after it. The
        // level the player holds, not the settings: this runs every tick.
        let gain = self.player.as_ref().map_or(1.0, |p| p.volume());
        self.mark_scaled(gain);
        if self.player.as_ref().is_some_and(|p| p.dsp().touched()) { self.now.bit_perfect = false; }
        self.finish_tick(started);
        &self.now
    }

    /// Records in the log how much of the current track had played.
    ///
    /// The log is append-only: the stats, the smart lists and the
    /// play count sync with a DAP all come out of here.
    fn set_played(&mut self, frames: u64) {
        if let Some(current) = self.in_progress.as_mut() {
            current.played_frames = frames;
        }
        self.scrobble_if_heard();
    }

    /// The current state, without collecting events.
    pub fn now_playing(&self) -> &NowPlaying {
        &self.now
    }

    /// Runs a command and returns what the frontend should know.
    pub fn dispatch(&mut self, command: Command) -> Event {
        match command {
            Command::ListSources => Event::Sources(self.core.sources().to_vec()),
            Command::PlayFile { path, device, bit_perfect } => match self.core.play_local(&path, &device, bit_perfect) {
                Ok(played) => Event::Played { frames: played.frames, quality: played.quality, bit_perfect: played.bit_perfect },
                Err(message) => Event::Failed { message },
            },
            Command::Play(_) | Command::Pause | Command::Resume => Event::Failed { message: "the player is not behaving yet".into() },
        }
    }
}

impl Drop for Chilpa {
    fn drop(&mut self) {
        self.appimage_updates.take();
        self.persist_queue_session();
        if let Some(player) = self.player.as_mut() { player.shutdown(); }
        if self.player.is_some() { self.poll(); }
        self.log_play(true);
        self.shutdown_cast();
        if let Some(natives) = self.catalog.natives() { natives.flush_playback(); }
    }
}

#[cfg(test)]
mod lib_tests;
