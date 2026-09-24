//! What the window asks about the library.
//!
//! The album grid and its views, the artists, the saved lists, the stats and
//! the scan. Every method is one line that calls the facade and arranges
//! what it answers into properties QML reads.

use super::{Chilpa, VIEWS};
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;
use super::{long_duration, short_date};

impl Chilpa {

    pub fn scan_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let path = folder.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        // The scan runs on its own thread: a large library would leave the
        // window frozen if it happened here.
        self.as_mut().remember_folder(&format!("{path}/."));
        self.as_mut().rust_mut().api.start_scan(std::path::Path::new(&path));
        self.as_mut().set_scanning(true);
        self.as_mut().set_scan_progress(0.0);
        let scanning = self.api.t("scanning...");
        self.as_mut().set_library_status(QString::from(&scanning));
    }

    pub fn analyze_library(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.start_analysis();
        self.as_mut().set_analyzing(true);
        self.as_mut().set_analysis_progress(0.0);
        let running = self.api.t("analysing...");
        self.as_mut().set_analysis_status(QString::from(&running));
    }

    pub fn cancel_analysis(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.cancel_analysis();
    }

    pub fn rescan(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.rescan();
        self.as_mut().set_scanning(true);
        self.as_mut().set_scan_progress(0.0);
        let scanning = self.api.t("scanning...");
        self.as_mut().set_library_status(QString::from(&scanning));
    }
    pub fn album_view(&self) -> i32 {
        self.album_view
    }

    /// It remembers where the user was: what gets opened is, almost always,
    /// next to the last thing opened.
    pub(super) fn remember_folder(mut self: Pin<&mut Self>, path: &str) {
        let folder = std::path::Path::new(path)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        if folder.is_empty() {
            return;
        }
        self.as_mut().set_last_folder(QString::from(&folder));
        self.as_mut().rust_mut().api.remember_folder(&folder);
    }

    pub fn choose_album_view(mut self: Pin<&mut Self>, view: i32) {
        let Some((order, _)) = VIEWS.get(view.max(0) as usize) else { return };
        let order = *order;
        self.as_mut().rust_mut().album_view = view;
        self.as_mut().rust_mut().api.set_album_sort(order);
        self.as_mut().album_view_changed();
        self.as_mut().refresh_albums();
    }

    pub fn refresh_playlists(mut self: Pin<&mut Self>) {
        let genres = self.as_mut().rust_mut().api.genres(40);
        let mut g = QList::<QString>::default();
        for (name, n) in &genres {
            g.append(QString::from(&format!("{name}\t{n}")));
        }
        self.as_mut().set_genres(QStringList::from(&g));

        let smart = self.as_mut().rust_mut().api.smart_lists();
        let mut out = QList::<QString>::default();
        for l in &smart {
            out.append(QString::from(&format!("{}\t{}", l.name, l.track_count)));
        }
        self.as_mut().set_smart_lists(QStringList::from(&out));

        let lists = self.as_mut().rust_mut().api.playlists();
        let mut l = QList::<QString>::default();
        for p in &lists {
            // The source and its id go along so a source's list can be asked
            // for again: it has no row in the library to look it up by.
            l.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                p.id, p.name, p.provider, p.track_count, p.source, p.external_id
            )));
        }
        self.as_mut().set_playlists(QStringList::from(&l));
    }

    pub fn play_genre(mut self: Pin<&mut Self>, genre: &QString) {
        let name = genre.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        let titles = self.as_mut().rust_mut().api.play_genre(&name, &device, bit_perfect);
        self.as_mut().refresh_queue(&titles);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titles.is_empty());
    }

    pub fn play_smart_list(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        let titles =
            self.as_mut().rust_mut().api.play_smart_list(&name, &device, bit_perfect);
        self.as_mut().refresh_queue(&titles);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titles.is_empty());
    }

    pub fn save_queue_as(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let saved = self.as_mut().rust_mut().api.save_queue_as(&name).is_some();
        if saved {
            let saved = self.api.t_args("playlist \"{}\" saved", &[&name]);
            self.as_mut().set_library_status(QString::from(&saved));
            self.as_mut().refresh_playlists();
        }
    }

    /// Opens a source's list: its tracks become the results, and playing
    /// one is the same click as playing a search result.
    pub fn open_source_playlist(
        mut self: Pin<&mut Self>,
        source: &QString,
        id: &QString,
        name: &QString,
    ) {
        let source = source.to_string();
        let id = id.to_string();
        let found = self.as_mut().rust_mut().api.open_source_playlist(&source, &id);
        // The name comes from the row the user clicked: an open album says
        // what it is, and a list has no reason to be the exception. Empty
        // when nothing came back, so the panel does not open on nothing.
        let title = if found > 0 { name.clone() } else { QString::from("") };
        self.as_mut().set_open_list(title);
        self.as_mut().refresh_results();
    }

    pub fn close_list(mut self: Pin<&mut Self>) {
        self.as_mut().set_open_list(QString::from(""));
        self.as_mut().set_results(QStringList::default());
        self.as_mut().rust_mut().result_paths.clear();
    }

    pub fn play_saved_playlist(mut self: Pin<&mut Self>, playlist_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let titles = self
            .as_mut()
            .rust_mut()
            .api
            .play_saved_playlist(playlist_id, &device, bit_perfect);
        self.as_mut().refresh_queue(&titles);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titles.is_empty());
    }

    pub fn delete_playlist(mut self: Pin<&mut Self>, playlist_id: i64) {
        self.as_mut().rust_mut().api.delete_playlist(playlist_id);
        self.as_mut().refresh_playlists();
    }

    pub fn refresh_stats(mut self: Pin<&mut Self>) {
        let s = self.as_mut().rust_mut().api.stats();
        let mut summary = QList::<QString>::default();
        let mut line = |tag: String, value: String| {
            summary.append(QString::from(&format!("{tag}\t{value}")));
        };
        line(self.api.t("Plays"), s.plays.to_string());
        line(self.api.t("Distinct tracks"), s.distinct_tracks.to_string());
        line(self.api.t("Time listened"), long_duration(s.listened_ms));
        line(self.api.t("Skips"), s.skips.to_string());
        line(
            self.api.t("Bit-perfect"),
            if s.plays + s.skips == 0 {
                "—".to_string()
            } else {
                self.api.t_args(
                    "{} of {}",
                    &[&s.bit_perfect.to_string(), &(s.plays + s.skips).to_string()],
                )
            },
        );
        line(self.api.t("Since"), s.since.map(short_date).unwrap_or_else(|| "—".into()));
        self.as_mut().set_stats(QStringList::from(&summary));

        let to_lines = |tracks: Vec<(chilpa_api::LibraryTrack, usize)>| {
            let mut l = QList::<QString>::default();
            for (t, times) in tracks {
                l.append(QString::from(&format!(
                    "{}\t{}\t{}\t{}\t{}",
                    t.title, t.artist, t.album, times, t.path
                )));
            }
            QStringList::from(&l)
        };
        let top = self.as_mut().rust_mut().api.top_tracks(20);
        self.as_mut().set_top_tracks(to_lines(top));
        let recent = self.as_mut().rust_mut().api.recently_played(20);
        self.as_mut().set_recent_tracks(to_lines(recent));
    }

    pub fn refresh_artists(mut self: Pin<&mut Self>) {
        let artists = self.as_mut().rust_mut().api.artists(1000, 0);
        let mut list = QList::<QString>::default();
        for a in &artists {
            list.append(QString::from(&format!(
                "{}\t{}\t{}\t{}",
                a.name, a.album_count, a.track_count, a.artwork_path
            )));
        }
        self.as_mut().set_artists(QStringList::from(&list));
    }

    pub fn open_artist_albums(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().rust_mut().api.set_open_artist(&owned);
        self.as_mut().set_open_artist(name.clone());
        self.as_mut().refresh_albums();
    }

    pub fn refresh_albums(mut self: Pin<&mut Self>) {
        let albums = self.as_mut().rust_mut().api.albums(500, 0);
        let mut list = QList::<QString>::default();
        for a in &albums {
            let year = a.year.map(|y| y.to_string()).unwrap_or_default();
            // One line per album, tab separated: QML splits it and builds its
            // model without the bridge inventing a new type.
            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                a.id, a.title, a.artist, year, a.track_count, a.artwork_path, a.play_count
            );
            list.append(QString::from(&line));
        }
        self.as_mut().set_albums(QStringList::from(&list));
    }

    pub fn open_album_tracks(mut self: Pin<&mut Self>, album_id: i64, title: &QString) {
        let tracks = self.as_mut().rust_mut().api.album_tracks(album_id);
        let mut list = QList::<QString>::default();
        let mut paths = Vec::with_capacity(tracks.len());
        for t in &tracks {
            let seconds = t.duration_ms / 1000;
            let number = t.track_number.map(|n| format!("{n}. ")).unwrap_or_default();
            list.append(QString::from(&format!(
                "{}\t{}{}\t{}\t{}\t{}:{:02}\t{}",
                t.path,
                number,
                t.title,
                t.artist,
                t.album,
                seconds / 60,
                seconds % 60,
                t.analysis
            )));
            paths.push(t.path.clone());
        }
        self.as_mut().set_results(QStringList::from(&list));
        self.as_mut().rust_mut().result_paths = paths;
        self.as_mut().set_open_album(title.clone());
        self.as_mut().set_open_album_id(album_id);

        // The album's header: cover, artist and one line with year, tracks,
        // duration and plays.
        let duration: u64 = tracks.iter().map(|t| t.duration_ms).sum();
        match self.as_mut().rust_mut().api.album(album_id) {
            Some(a) => {
                let cover = if a.artwork_path.is_empty() {
                    String::new()
                } else {
                    format!("file://{}", a.artwork_path)
                };
                let mut parts: Vec<String> = Vec::new();
                if let Some(year) = a.year {
                    parts.push(year.to_string());
                }
                parts.push(self.api.tf(
                    "{} track",
                    "{} tracks",
                    i64::try_from(a.track_count).unwrap_or_default(),
                    &[&a.track_count.to_string()],
                ));
                parts.push(long_duration(duration));
                if a.play_count > 0 {
                    parts.push(self.api.tf(
                        "{} play",
                        "{} plays",
                        i64::try_from(a.play_count).unwrap_or_default(),
                        &[&a.play_count.to_string()],
                    ));
                }
                // The title comes from the library, not from whoever opened
                // the view: that way it always says what the disc says.
                self.as_mut().set_open_album(QString::from(&a.title));
                self.as_mut().set_open_album_artist(QString::from(&a.artist));
                self.as_mut().set_open_album_art(QString::from(&cover));
                self.as_mut().set_open_album_line(QString::from(&parts.join(" · ")));
            }
            None => {
                self.as_mut().set_open_album_artist(QString::from(""));
                self.as_mut().set_open_album_art(QString::from(""));
                self.as_mut().set_open_album_line(QString::from(""));
            }
        }
    }

    pub fn close_album(mut self: Pin<&mut Self>) {
        self.as_mut().set_open_list(QString::from(""));
        self.as_mut().set_open_album(QString::from(""));
        self.as_mut().set_open_album_id(0);
        self.as_mut().set_open_album_artist(QString::from(""));
        self.as_mut().set_open_album_art(QString::from(""));
        self.as_mut().set_open_album_line(QString::from(""));
        self.as_mut().set_results(QStringList::default());
        self.as_mut().rust_mut().result_paths.clear();
    }

    pub fn enqueue_album(mut self: Pin<&mut Self>, album_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.enqueue_album(album_id, &device, bit_perfect);
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_playing(true);
    }

    pub fn play_album(mut self: Pin<&mut Self>, album_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let tracks = self.as_mut().rust_mut().api.play_album(album_id, &device, bit_perfect);
        let mut list = QList::<QString>::default();
        for track in &tracks {
            list.append(QString::from(track));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!tracks.is_empty());
    }

    pub fn play_playlist(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().remember_folder(&path);
        let titles = self.as_mut().rust_mut().api.play_playlist(
            std::path::Path::new(&path),
            &device,
            bit_perfect,
        );
        let mut list = QList::<QString>::default();
        for t in &titles {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titles.is_empty());
    }
}
