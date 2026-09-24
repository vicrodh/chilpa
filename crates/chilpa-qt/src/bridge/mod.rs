//! The bridge between QML and the facade.
//!
//! No logic lives here: the object exposes properties QML reads and orders
//! QML fires, and each one is a line that calls `chilpa-api` (ADR-001). If a
//! view needs something the facade does not have, the facade is widened, not
//! this file.

use cxx_qt_lib::{QList, QString, QStringList};

#[cxx_qt::bridge]
pub mod chilpa_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The outputs that can be chosen, already labelled for display.
        #[qproperty(QStringList, outputs)]
        /// The identifiers, in the same order as the labels.
        #[qproperty(QStringList, output_ids)]
        /// What is playing and its state.
        #[qproperty(QString, track)]
        #[qproperty(QString, title)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, album)]
        /// The cover as a file URL, ready for a QML Image.
        #[qproperty(QString, artwork)]
        /// The queued tracks and which one is playing.
        #[qproperty(QStringList, queue)]
        #[qproperty(i32, queue_index)]
        /// Which positions of the queue were removed, as "0" or "1".
        #[qproperty(QStringList, queue_removed)]
        /// The library: one line per album, ready for the view. Each entry is
        /// "id\ttitle\tartist\tyear\ttracks\tcover\tplays".
        #[qproperty(QStringList, albums)]
        /// The results of the last search or of the open album: one line per
        /// track, with "path, title, artist, album, duration and what the
        /// analysis found".
        #[qproperty(QStringList, results)]
        /// The sources that answered badly, already written for display. Empty
        /// when all of them answered fine.
        #[qproperty(QString, search_problems)]
        /// The source list being looked at, empty if none is open. The
        /// results panel shows it the way it shows an open album: with a
        /// title and a way out.
        #[qproperty(QString, open_list)]
        /// The album being looked at, empty if none is open.
        #[qproperty(QString, open_album)]
        #[qproperty(i64, open_album_id)]
        /// The rest of the open album, for its header.
        #[qproperty(QString, open_album_artist)]
        #[qproperty(QString, open_album_art)]
        #[qproperty(QString, open_album_line)]
        #[qproperty(QString, library_status)]
        /// How the analysis is going: from 0 to 1, whether it runs, and what is
        /// being looked at.
        #[qproperty(f64, analysis_progress)]
        #[qproperty(bool, analyzing)]
        #[qproperty(QString, analysis_status)]
        /// How the scan is going: from 0 to 1, and whether it is still running.
        #[qproperty(f64, scan_progress)]
        #[qproperty(bool, scanning)]
        /// The window's layout. It is the name of a `Layout<name>.qml`, so that
        /// adding a layout is adding a file.
        #[qproperty(QString, layout)]
        /// The layouts the binary carries. `build.rs` builds the list from the
        /// files, it is not written by hand.
        #[qproperty(QStringList, layouts)]
        /// What is typed in the search. It lives here because it decides what
        /// is seen, and the box it is typed in moves with every layout.
        #[qproperty(QString, search_text)]
        /// What the analysis says about what is playing: "128 BPM - 8B".
        #[qproperty(QString, analysis)]
        /// The lyrics of what is playing: one line per entry, which line is
        /// current (-1 if none) and where it came from.
        #[qproperty(QStringList, lyrics)]
        #[qproperty(i32, lyrics_line)]
        #[qproperty(bool, lyrics_synced)]
        #[qproperty(QString, lyrics_source)]
        /// The last folder opened, so the dialogs do not start at home every
        /// time.
        #[qproperty(QString, last_folder)]
        /// The envelope of what goes out: one value from 0 to 1 per point.
        #[qproperty(QStringList, scope)]
        /// Which visualizer is drawn: "wave", "spectrum" or "goniometer".
        #[qproperty(QString, visualizer)]
        /// The spectrum of what goes out: one value from 0 to 1 per band.
        #[qproperty(QStringList, spectrum)]
        /// "left\tright" pairs for the goniometer, and the correlation between
        /// channels from -1 to 1 (999 when there are not two channels).
        #[qproperty(QStringList, pairs)]
        #[qproperty(f64, correlation)]
        /// The level per channel, as "peak\trms".
        #[qproperty(QStringList, levels)]
        /// The installed plugins, as "id\tname\tstate\tproblem".
        #[qproperty(QStringList, plugins)]
        /// The output's diagnosis: one summary line and one line per format, as
        /// "name\tformat\tverdict".
        #[qproperty(QString, wizard_summary)]
        #[qproperty(QStringList, wizard_rows)]
        /// The CD drives on the machine.
        #[qproperty(QStringList, cd_drives)]
        /// The library's genres, as "name\ttracks".
        #[qproperty(QStringList, genres)]
        /// The smart lists, as "name\ttracks".
        #[qproperty(QStringList, smart_lists)]
        /// The saved lists, as "id\tname\tprovider\ttracks\tsource\texternal id".
        #[qproperty(QStringList, playlists)]
        /// The listening summary, already in "label\tvalue" lines.
        #[qproperty(QStringList, stats)]
        /// The most played and the last played, as
        /// "title\tartist\talbum\tcount\tpath".
        #[qproperty(QStringList, top_tracks)]
        #[qproperty(QStringList, recent_tracks)]
        /// The library's artists: one line with
        /// "name\talbums\ttracks\tcover".
        #[qproperty(QStringList, artists)]
        /// The artist the grid is filtered to, empty if all are shown.
        #[qproperty(QString, open_artist)]
        /// The library's quick views and which one is set.
        #[qproperty(QStringList, album_views)]
        #[qproperty(i32, album_view, READ = album_view, WRITE = choose_album_view, NOTIFY = album_view_changed)]
        /// How the queue is traversed: shuffled or not, and repeat as
        /// 0 = none, 1 = the whole queue, 2 = the track.
        #[qproperty(bool, shuffle, READ = shuffle, WRITE = shuffle_queue, NOTIFY = shuffle_changed)]
        #[qproperty(i32, repeat)]
        /// What was chosen for playback: the output and whether bit-perfect is
        /// required. It lives here, not in a window control, so any layout can
        /// read it without depending on where the selector ended up.
        #[qproperty(QString, device)]
        #[qproperty(bool, want_bit_perfect)]
        #[qproperty(QString, format)]
        #[qproperty(QString, error)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, paused)]
        #[qproperty(bool, bit_perfect)]
        #[qproperty(f64, seconds)]
        #[qproperty(f64, total_seconds)]
        /// What was asked to be played at startup, if it came on the command
        /// line. Empty when it did not.
        #[qproperty(QString, autoplay_path)]
        #[qproperty(QString, autoplay_device)]
        /// A startup search, for exercising the view without typing.
        #[qproperty(QString, initial_search)]
        /// A startup album, by id, for exercising the view without a mouse.
        #[qproperty(i64, initial_album)]
        /// A source's list to open at startup, as "source id", for the same
        /// reason: exercising the view without a mouse.
        #[qproperty(QString, initial_list)]
        /// Which library tab it starts on: "artists" or empty.
        #[qproperty(QString, initial_tab)]
        /// Which search result to start playing at startup, counting from 1.
        /// For exercising without keyboard or mouse, like the ones above.
        #[qproperty(i32, initial_play)]
        /// Which plugin to connect at startup, for the same reason.
        #[qproperty(QString, initial_connect)]
        /// True to rescan at startup, without touching a button.
        #[qproperty(bool, initial_rescan)]
        /// True to analyse at startup, just as useful for exercising.
        #[qproperty(bool, initial_analysis)]
        /// The language the window is speaking, as a two letter code.
        #[qproperty(QString, language)]
        /// The languages that can be chosen.
        #[qproperty(QStringList, languages)]
        /// Bumped every time the language changes. A QML binding that passes
        /// it to `translate` re-evaluates, which is what makes the window
        /// change language without being rebuilt.
        #[qproperty(i32, tr_rev)]
        /// The folders the library is made of, one per line.
        #[qproperty(QStringList, library_folders)]
        /// The visualizers that ship, as "id\tname".
        #[qproperty(QStringList, visualizers)]
        type Chilpa = super::ChilpaRust;

        /// Translates a msgid, which is the English string itself.
        ///
        /// `rev` is `trRev` and is not used: it is there so the binding
        /// depends on it.
        #[qinvokable]
        fn translate(self: &Chilpa, msgid: &QString, rev: i32) -> QString;

        /// The same for a count, choosing singular or plural.
        #[qinvokable]
        fn translate_n(
            self: &Chilpa,
            singular: &QString,
            plural: &QString,
            n: i32,
            rev: i32,
        ) -> QString;

        /// The common case of a count: one `{}` in the msgid, filled with
        /// `n`.
        #[qinvokable]
        fn translate_count(
            self: &Chilpa,
            singular: &QString,
            plural: &QString,
            n: i32,
            rev: i32,
        ) -> QString;

        /// The same, filling each `{}` with the next argument.
        #[qinvokable]
        fn translate_args(
            self: &Chilpa,
            msgid: &QString,
            args: &QStringList,
            rev: i32,
        ) -> QString;

        /// Chooses the language and remembers it. Empty goes back to what
        /// the environment asks for.
        #[qinvokable]
        fn choose_language(self: Pin<&mut Chilpa>, code: &QString);

        /// Reads the folders the library is made of.
        #[qinvokable]
        fn refresh_library_folders(self: Pin<&mut Chilpa>);

        /// Forgets a folder: it stops being rescanned and what came from it
        /// stops counting as present. Nothing is deleted.
        #[qinvokable]
        fn forget_library_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// Chooses the visualizer and remembers it.
        #[qinvokable]
        fn choose_visualizer(self: Pin<&mut Chilpa>, name: &QString);

        /// Chooses the window's layout and remembers it.
        #[qinvokable]
        fn choose_layout(self: Pin<&mut Chilpa>, name: &QString);

        /// Chooses the output and the policy, and remembers them for next time.
        #[qinvokable]
        fn choose(self: Pin<&mut Chilpa>, device: &QString, bit_perfect: bool);

        /// Reads the machine's outputs again.
        #[qinvokable]
        fn refresh_outputs(self: Pin<&mut Chilpa>);

        /// Starts a file playing on the chosen output.
        #[qinvokable]
        fn play(self: Pin<&mut Chilpa>, path: &QString);

        /// Searches the library.
        #[qinvokable]
        fn search(self: Pin<&mut Chilpa>, text: &QString);

        /// Starts what the search found playing, from that position.
        ///
        /// What is a source's is resolved before playing, so it can take as
        /// long as the plugin takes to answer.
        #[qinvokable]
        fn play_results(self: Pin<&mut Chilpa>, from: i32);

        /// Starts the analysis of whatever is missing, in the background.
        #[qinvokable]
        fn analyze_library(self: Pin<&mut Chilpa>);

        /// Asks the analysis to stop.
        #[qinvokable]
        fn cancel_analysis(self: Pin<&mut Chilpa>);

        /// Rescans the folders already in the library.
        #[qinvokable]
        fn rescan(self: Pin<&mut Chilpa>);

        /// Scans a folder into the library and refreshes the albums.
        #[qinvokable]
        fn scan_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// The getter and setter for `album_view`: changing the view reads
        /// the library again in that order.
        #[cxx_name = "albumView"]
        fn album_view(self: &Chilpa) -> i32;
        #[cxx_name = "setAlbumView"]
        fn choose_album_view(self: Pin<&mut Chilpa>, view: i32);

        #[qsignal]
        #[cxx_name = "albumViewChanged"]
        fn album_view_changed(self: Pin<&mut Chilpa>);

        /// Reads the saved lists again.
        #[qinvokable]
        fn refresh_playlists(self: Pin<&mut Chilpa>);

        /// Starts a genre playing, shuffled.
        #[qinvokable]
        fn play_genre(self: Pin<&mut Chilpa>, genre: &QString);

        /// Starts a smart list playing, by its name.
        #[qinvokable]
        fn play_smart_list(self: Pin<&mut Chilpa>, name: &QString);

        /// Saves the queue as a list under that name.
        #[qinvokable]
        fn save_queue_as(self: Pin<&mut Chilpa>, name: &QString);

        /// Starts a saved list playing.
        #[qinvokable]
        fn play_saved_playlist(self: Pin<&mut Chilpa>, playlist_id: i64);

        /// Opens a source's list: its tracks become the results.
        #[qinvokable]
        fn open_source_playlist(
            self: Pin<&mut Chilpa>,
            source: &QString,
            id: &QString,
            name: &QString,
        );

        /// Deletes a saved list. The tracks are not touched.
        #[qinvokable]
        fn delete_playlist(self: Pin<&mut Chilpa>, playlist_id: i64);

        /// Reads the listening stats again.
        #[qinvokable]
        fn refresh_stats(self: Pin<&mut Chilpa>);

        /// Reads the library's artists again.
        #[qinvokable]
        fn refresh_artists(self: Pin<&mut Chilpa>);

        /// Filters the grid to one artist. An empty name clears the filter.
        #[qinvokable]
        fn open_artist_albums(self: Pin<&mut Chilpa>, name: &QString);

        /// Reads the library's albums again.
        #[qinvokable]
        fn refresh_albums(self: Pin<&mut Chilpa>);

        /// Opens an album: it shows its tracks without playing them.
        #[qinvokable]
        fn open_album_tracks(self: Pin<&mut Chilpa>, album_id: i64, title: &QString);

        /// Closes the open album.
        #[qinvokable]
        fn close_album(self: Pin<&mut Chilpa>);

        /// Closes the open source list.
        #[qinvokable]
        fn close_list(self: Pin<&mut Chilpa>);

        /// Queues what the search found, from that position.
        #[qinvokable]
        fn enqueue_results(self: Pin<&mut Chilpa>, from: i32);

        /// Queues a whole album, without cutting off what is playing.
        #[qinvokable]
        fn enqueue_album(self: Pin<&mut Chilpa>, album_id: i64);

        /// Starts a whole album playing.
        #[qinvokable]
        fn play_album(self: Pin<&mut Chilpa>, album_id: i64);

        /// Starts whatever CD is in that drive playing.
        #[qinvokable]
        fn play_cd(self: Pin<&mut Chilpa>, drive: &QString);

        /// Loads whatever plugins there are and reads the list again.
        #[qinvokable]
        fn load_plugins(self: Pin<&mut Chilpa>);

        /// The user approves what a plugin asks for; until then it does not
        /// run, and its wasm is not compiled (ADR-008).
        #[qinvokable]
        fn approve_plugin(self: Pin<&mut Chilpa>, id: &QString);

        /// Connects a plugin's account: it opens the browser where the service
        /// asks for permission. Whatever it answers arrives on the tick.
        #[qinvokable]
        fn connect_account(self: Pin<&mut Chilpa>, id: &QString);

        /// Stops waiting for the browser.
        #[qinvokable]
        fn cancel_connect(self: Pin<&mut Chilpa>);

        /// Deletes the kept credential. The plugin stays installed.
        #[qinvokable]
        fn disconnect_account(self: Pin<&mut Chilpa>, id: &QString);

        /// Withdraws its approval: it stops running right away.
        #[qinvokable]
        fn revoke_plugin(self: Pin<&mut Chilpa>, id: &QString);

        /// Takes a plugin out of quarantine. Only the user asks for it.
        #[qinvokable]
        fn reactivate_plugin(self: Pin<&mut Chilpa>, id: &QString);

        /// Measures the chosen output and says which formats come out untouched.
        #[qinvokable]
        fn run_wizard(self: Pin<&mut Chilpa>);

        /// Looks again at what CD drives there are.
        #[qinvokable]
        fn refresh_cd_drives(self: Pin<&mut Chilpa>);

        /// Starts a radio, or any stream over HTTP, playing.
        #[qinvokable]
        fn play_url(self: Pin<&mut Chilpa>, url: &QString);

        /// Opens an M3U list and starts it playing.
        #[qinvokable]
        fn play_playlist(self: Pin<&mut Chilpa>, path: &QString);

        /// Saves the current queue as an M3U list.
        #[qinvokable]
        fn save_queue(self: Pin<&mut Chilpa>, path: &QString);

        /// Queues a whole folder.
        #[qinvokable]
        fn play_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// The getter and setter for `shuffle`: QML writes the property and the
        /// setter applies it on the facade.
        #[cxx_name = "shuffle"]
        fn shuffle(self: &Chilpa) -> bool;
        #[cxx_name = "setShuffle"]
        fn shuffle_queue(self: Pin<&mut Chilpa>, shuffle: bool);

        #[qsignal]
        #[cxx_name = "shuffleChanged"]
        fn shuffle_changed(self: Pin<&mut Chilpa>);

        /// Changes repeat: 0 none, 1 the whole queue, 2 the track.
        #[qinvokable]
        fn cycle_repeat(self: Pin<&mut Chilpa>);

        /// Reorders the queue so the tempo changes little between tracks.
        #[qinvokable]
        fn automix(self: Pin<&mut Chilpa>);

        /// Removes the track at that position from the queue.
        #[qinvokable]
        fn remove_from_queue(self: Pin<&mut Chilpa>, index: i32);

        /// Stops and empties the queue.
        #[qinvokable]
        fn clear_queue(self: Pin<&mut Chilpa>);

        /// Skips to the next track in the queue.
        #[qinvokable]
        fn next_track(self: Pin<&mut Chilpa>);

        /// Goes back to the previous one; on the first, it restarts it.
        #[qinvokable]
        fn previous_track(self: Pin<&mut Chilpa>);

        /// Pauses or resumes. It is not called set_paused: that name is already
        /// taken by the setter the `paused` property generates.
        #[qinvokable]
        fn pause(self: Pin<&mut Chilpa>, paused: bool);

        /// Seeks to that second of what is playing.
        #[qinvokable]
        fn seek(self: Pin<&mut Chilpa>, seconds: f64);

        /// Stops whatever is playing.
        #[qinvokable]
        fn stop(self: Pin<&mut Chilpa>);

        /// Collects the player's events and updates the properties. QML calls
        /// it from a timer.
        #[qinvokable]
        fn poll(self: Pin<&mut Chilpa>);
    }
}

use chilpa_bridge::Chilpa;

mod i18n;
mod library;
mod playback;
mod plugins;
mod search;
mod settings;
mod tick;

/// A long span said the way it is said: "3 h 21 min", not 12,060,000 ms.
pub(super) fn long_duration(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes == 0 {
        // A six second test disc does not last "0 min".
        return format!("{} s", ms / 1000);
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if hours == 0 {
        return format!("{minutes} min");
    }
    format!("{hours} h {rest} min")
}

/// A unix date as year-month-day, without dragging in a date library.
pub(super) fn short_date(unix: i64) -> String {
    // Days since 1970 to a civil date (Howard Hinnant). It is four lines and
    // it avoids a dependency for showing one line.
    let days_ago = unix.div_euclid(86_400) + 719_468;
    let era = days_ago.div_euclid(146_097);
    let doe = days_ago - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The library's quick views, with the msgid of their name for the list.
pub(super) const VIEWS: &[(chilpa_api::AlbumSort, &str)] = &[
    (chilpa_api::AlbumSort::Artist, chilpa_api::Chilpa::mark("By artist")),
    (chilpa_api::AlbumSort::Title, chilpa_api::Chilpa::mark("By title")),
    (chilpa_api::AlbumSort::Year, chilpa_api::Chilpa::mark("By year")),
    (chilpa_api::AlbumSort::Added, chilpa_api::Chilpa::mark("Recently added")),
    (chilpa_api::AlbumSort::Played, chilpa_api::Chilpa::mark("Most played")),
    (chilpa_api::AlbumSort::LastPlayed, chilpa_api::Chilpa::mark("Recently played")),
];


/// The list of layouts, generated by `build.rs` from the QML files. Each
/// name goes through `mark`: the file name is what the selector shows.
mod layouts {
    include!(concat!(env!("OUT_DIR"), "/layouts.rs"));
}

/// The state on the Rust side.
pub struct ChilpaRust {
    language: QString,
    languages: QStringList,
    tr_rev: i32,
    library_folders: QStringList,
    visualizers: QStringList,
    outputs: QStringList,
    output_ids: QStringList,
    track: QString,
    title: QString,
    artist: QString,
    album: QString,
    artwork: QString,
    queue: QStringList,
    queue_index: i32,
    queue_removed: QStringList,
    albums: QStringList,
    results: QStringList,
    search_problems: QString,
    open_list: QString,
    result_paths: Vec<String>,
    /// Whether the previous tick already knew the browser was being waited
    /// on, so the plugin list is not rebuilt on every tick.
    connecting_shown: bool,
    open_album: QString,
    open_album_id: i64,
    open_album_artist: QString,
    open_album_art: QString,
    open_album_line: QString,
    library_status: QString,
    analysis_progress: f64,
    analyzing: bool,
    analysis_status: QString,
    scan_progress: f64,
    scanning: bool,
    layout: QString,
    layouts: QStringList,
    analysis: QString,
    lyrics: QStringList,
    /// Which track the assembled lyrics belong to, so they are not rebuilt
    /// on every tick.
    lyrics_of: String,
    lyrics_line: i32,
    lyrics_synced: bool,
    lyrics_source: QString,
    last_folder: QString,
    scope: QStringList,
    visualizer: QString,
    spectrum: QStringList,
    pairs: QStringList,
    correlation: f64,
    levels: QStringList,
    plugins: QStringList,
    wizard_summary: QString,
    wizard_rows: QStringList,
    cd_drives: QStringList,
    genres: QStringList,
    smart_lists: QStringList,
    playlists: QStringList,
    stats: QStringList,
    top_tracks: QStringList,
    recent_tracks: QStringList,
    artists: QStringList,
    open_artist: QString,
    album_views: QStringList,
    album_view: i32,
    shuffle: bool,
    repeat: i32,
    search_text: QString,
    device: QString,
    want_bit_perfect: bool,
    format: QString,
    error: QString,
    playing: bool,
    paused: bool,
    bit_perfect: bool,
    seconds: f64,
    total_seconds: f64,
    autoplay_path: QString,
    autoplay_device: QString,
    initial_search: QString,
    initial_album: i64,
    initial_list: QString,
    initial_tab: QString,
    initial_play: i32,
    initial_connect: QString,
    initial_rescan: bool,
    initial_analysis: bool,
    api: chilpa_api::Chilpa,
}

impl Default for ChilpaRust {
    fn default() -> Self {
        // Building one facade sets the language from the preferences, so
        // everything below already speaks it.
        let api = chilpa_api::Chilpa::new();
        let remembered = api.settings();
        let mut codes = QList::<QString>::default();
        for code in api.languages() {
            codes.append(QString::from(*code));
        }
        let mut drawings = QList::<QString>::default();
        for (id, name) in settings::VISUALIZERS {
            drawings.append(QString::from(&format!("{id}\t{}", api.t(name))));
        }
        Self {
            language: QString::from(api.language()),
            languages: QStringList::from(&codes),
            tr_rev: 0,
            library_folders: QStringList::default(),
            visualizers: QStringList::from(&drawings),
            outputs: QStringList::default(),
            output_ids: QStringList::default(),
            track: QString::from(""),
            title: QString::from(""),
            artist: QString::from(""),
            album: QString::from(""),
            artwork: QString::from(""),
            queue: QStringList::default(),
            queue_index: 0,
            queue_removed: QStringList::default(),
            albums: QStringList::default(),
            results: QStringList::default(),
            search_problems: QString::from(""),
            open_list: QString::from(""),
            result_paths: Vec::new(),
            connecting_shown: false,
            open_album: QString::from(""),
            open_album_id: 0,
            open_album_artist: QString::from(""),
            open_album_art: QString::from(""),
            open_album_line: QString::from(""),
            library_status: QString::from(""),
            analysis_progress: 0.0,
            analyzing: false,
            analysis_status: QString::from(""),
            scan_progress: 0.0,
            scanning: false,
            analysis: QString::from(""),
            lyrics: QStringList::default(),
            lyrics_of: String::new(),
            lyrics_line: -1,
            lyrics_synced: false,
            lyrics_source: QString::from(""),
            last_folder: QString::from(&remembered.last_folder),
            scope: QStringList::default(),
            visualizer: QString::from(&remembered.visualizer),
            spectrum: QStringList::default(),
            pairs: QStringList::default(),
            correlation: 999.0,
            levels: QStringList::default(),
            plugins: QStringList::default(),
            wizard_summary: QString::from(""),
            wizard_rows: QStringList::default(),
            cd_drives: QStringList::default(),
            genres: QStringList::default(),
            smart_lists: QStringList::default(),
            playlists: QStringList::default(),
            stats: QStringList::default(),
            top_tracks: QStringList::default(),
            recent_tracks: QStringList::default(),
            artists: QStringList::default(),
            open_artist: QString::from(""),
            album_views: {
                let mut l = QList::<QString>::default();
                for v in VIEWS {
                    l.append(QString::from(&api.t(v.1)));
                }
                QStringList::from(&l)
            },
            album_view: {
                let sort = api.album_sort();
                VIEWS.iter().position(|v| v.0 == sort).unwrap_or(0) as i32
            },
            shuffle: false,
            repeat: 0,
            layout: QString::from(&remembered.layout),
            layouts: {
                let mut l = QList::<QString>::default();
                for n in layouts::LAYOUTS {
                    l.append(QString::from(*n));
                }
                QStringList::from(&l)
            },
            search_text: QString::from(""),
            device: QString::from(&remembered.output),
            want_bit_perfect: remembered.bit_perfect,
            format: QString::from(""),
            error: QString::from(""),
            playing: false,
            paused: false,
            bit_perfect: false,
            seconds: 0.0,
            total_seconds: 0.0,
            // The window can be started pointing at a file, to exercise it
            // without touching anything: chilpa-qt <file> [output].
            autoplay_path: QString::from(&std::env::args().nth(1).unwrap_or_default()),
            autoplay_device: QString::from(&std::env::args().nth(2).unwrap_or_default()),
            initial_search: QString::from(&std::env::var("CHILPA_SEARCH").unwrap_or_default()),
            initial_album: std::env::var("CHILPA_ALBUM").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
            initial_list: QString::from(
                &std::env::var("CHILPA_LIST").unwrap_or_default(),
            ),
            initial_tab: QString::from(&std::env::var("CHILPA_TAB").unwrap_or_default()),
            // Which plugin to connect at startup. Empty connects nothing.
            initial_connect: QString::from(
                &std::env::var("CHILPA_CONNECT").unwrap_or_default(),
            ),
            // Which result to start playing at startup, counting from 1.
            // With 0 nothing plays, which is the normal case.
            initial_play: std::env::var("CHILPA_PLAY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            initial_rescan: std::env::var("CHILPA_RESCAN").is_ok(),
            initial_analysis: std::env::var("CHILPA_ANALYZE").is_ok(),
            api,
        }
    }
}


/// Opens a URL in the system's browser.
///
/// `xdg-open` on Linux, `open` on macOS, `start` on Windows. It is not
/// waited on: the browser lives its life and Chilpa goes on with its own,
/// which is waiting on its thread for the user to come back.
pub(super) fn open_in_browser(url: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
