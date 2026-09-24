//! The library as the facade sees it.
//!
//! Scanning, analysing and asking the database. Everything returned here
//! already comes in the view's types: whoever draws does not know rusqlite.

use super::*;

impl Chilpa {
    /// Attaches what the analysis found to each track, in a single query.
    fn with_analysis(&mut self, mut tracks: Vec<LibraryTrack>) -> Vec<LibraryTrack> {
        if tracks.is_empty() {
            return tracks;
        }
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
        let Some(lib) = self.library() else { return tracks };
        let Ok(map) = lib.features_for(&ids, chilpa_core::analysis_job::ANALYZER) else {
            return tracks;
        };
        for track in tracks.iter_mut() {
            let Some((bpm, tone, loudness)) = map.get(&track.id) else { continue };
            track.analysis = describe_analysis(*bpm, tone.as_deref(), *loudness);
        }
        tracks
    }

    /// Starts a background scan. The interface does not block.
    pub fn start_scan(&mut self, folder: &std::path::Path) {
        if self.scan.is_some() {
            self.now.error = chilpa_i18n::t("a scan is already running");
            return;
        }
        // The interface's connection is let go: the scan opens its own, and
        // two connections to the same WAL database coexist without fighting.
        self.scan = Some(chilpa_library::ScanJob::start(
            chilpa_library::Library::default_path(),
            folder.to_path_buf(),
            Some(chilpa_core::artwork_cache()),
        ));
        self.scan_status = ScanStatus { running: true, ..ScanStatus::default() };
    }

    /// Rescans every folder already in the library.
    ///
    /// It is the usual case after day one: the music lives where it lives,
    /// and going to find it by hand every time is work for the user that
    /// the library has already written down.
    pub fn rescan(&mut self) {
        if self.scan.is_some() {
            self.now.error = chilpa_i18n::t("a scan is already running");
            return;
        }
        let folders = self.roots();
        if folders.is_empty() {
            self.now.error = chilpa_i18n::t("there is no folder in the library yet");
            return;
        }
        self.scan = Some(chilpa_library::ScanJob::start_all(
            chilpa_library::Library::default_path(),
            folders,
            Some(chilpa_core::artwork_cache()),
        ));
        self.scan_status = ScanStatus { running: true, ..ScanStatus::default() };
    }

    /// Starts the analysis of whatever is missing, in the background.
    pub fn start_analysis(&mut self) {
        if self.analysis.is_some() {
            self.now.error = chilpa_i18n::t("an analysis is already running");
            return;
        }
        self.analysis = Some(chilpa_core::analysis_job::AnalysisJob::start(
            chilpa_library::Library::default_path(),
        ));
        self.analysis_status = AnalysisStatus { running: true, ..AnalysisStatus::default() };
    }

    /// Asks the analysis to stop.
    pub fn cancel_analysis(&mut self) {
        if let Some(j) = &self.analysis {
            j.cancel();
        }
    }

    /// How the analysis is going.
    pub fn analysis_status(&mut self) -> &AnalysisStatus {
        use chilpa_core::analysis_job::AnalysisProgress;
        let events = self.analysis.as_ref().map(|j| j.poll()).unwrap_or_default();
        let mut finished = false;
        for event in events {
            match event {
                AnalysisProgress::Counted { total } => self.analysis_status.total = total,
                AnalysisProgress::Advanced { done, total, title, bpm, key } => {
                    self.analysis_status.done = done;
                    self.analysis_status.total = total;
                    let tempo = match bpm {
                        Some(b) => format!("{b:.0} BPM"),
                        None => chilpa_i18n::t("no pulse"),
                    };
                    self.analysis_status.last = match key {
                        Some(k) => format!("{title} · {tempo} · {k}"),
                        None => format!("{title} · {tempo}"),
                    };
                }
                AnalysisProgress::Done { analyzed, without_pulse, failed } => {
                    self.analysis_status.running = false;
                    self.analysis_status.analyzed = analyzed;
                    self.analysis_status.without_pulse = without_pulse;
                    self.analysis_status.failed = failed;
                    finished = true;
                }
                AnalysisProgress::Failed(e) => {
                    self.analysis_status.running = false;
                    self.now.error = chilpa_i18n::t_args("the analysis stopped: {}", &[&e]);
                    finished = true;
                }
            }
        }
        if finished {
            self.analysis = None;
        }
        &self.analysis_status
    }

    /// The folders that make up the library.
    pub fn roots(&mut self) -> Vec<std::path::PathBuf> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.roots().unwrap_or_default().into_iter().map(|r| r.path).collect()
    }

    /// Checks the library and says what is wrong, in one line.
    ///
    /// Empty when there is nothing to say. It reads the whole file, so it is
    /// something someone asks for, not something that runs on its own.
    pub fn check_library(&mut self) -> String {
        let Some(lib) = self.library() else { return String::new() };
        match lib.check() {
            Ok(found) if found.is_empty() => String::new(),
            Ok(found) => found.join(" · "),
            Err(e) => chilpa_i18n::t_args("the library could not be checked: {}", &[&e.to_string()]),
        }
    }

    /// Rebuilds what is derived: the counts and the search index.
    ///
    /// The tracks and the play log are not touched, because they are what
    /// everything else is rebuilt from. Nothing is lost by running it.
    pub fn repair_library(&mut self) -> bool {
        let Some(lib) = self.library() else { return false };
        match lib.repair() {
            Ok(()) => true,
            Err(e) => {
                self.now.error =
                    chilpa_i18n::t_args("the library could not be repaired: {}", &[&e.to_string()]);
                false
            }
        }
    }

    /// Forgets a folder: it stops being rescanned and what came from it
    /// stops counting as present.
    ///
    /// Nothing is deleted. Adding the folder back finds its tracks again,
    /// with the listening history they already had.
    ///
    /// Returns true if there was such a folder to forget.
    pub fn forget_folder(&mut self, folder: &str) -> bool {
        let Some(lib) = self.library() else { return false };
        let Some(root) = lib
            .roots()
            .unwrap_or_default()
            .into_iter()
            .find(|r| r.path.to_string_lossy() == folder)
        else {
            return false;
        };
        match lib.forget_root(root.id) {
            Ok(()) => true,
            Err(e) => {
                self.now.error =
                    chilpa_i18n::t_args("the folder could not be forgotten: {}", &[&e.to_string()]);
                false
            }
        }
    }

    /// How the scan is going. On finishing, it refreshes what the view shows.
    pub fn scan_status(&mut self) -> &ScanStatus {
        let events = self.scan.as_ref().map(|j| j.poll()).unwrap_or_default();
        for event in events {
            match event {
                chilpa_library::ScanProgress::Counted { total } => {
                    self.scan_status.total = total;
                }
                chilpa_library::ScanProgress::Advanced { done, total } => {
                    self.scan_status.done = done;
                    self.scan_status.total = total;
                }
                chilpa_library::ScanProgress::Done(r) => {
                    self.scan_status = ScanStatus {
                        running: false,
                        done: r.seen,
                        total: r.seen,
                        summary: ScanSummary {
                            seen: r.seen,
                            read: r.read,
                            unchanged: r.unchanged,
                            missing: r.missing,
                            unreachable: r.unreachable,
                        },
                    };
                    self.scan = None;
                }
                chilpa_library::ScanProgress::Failed(e) => {
                    self.now.error = chilpa_i18n::t_args("the scan failed: {}", &[&e]);
                    self.scan_status.running = false;
                    self.scan = None;
                }
            }
        }
        &self.scan_status
    }

    /// Scans a folder into the library, waiting for it to finish.
    pub fn scan(&mut self, folder: &std::path::Path) -> ScanSummary {
        let Some(lib) = self.library() else { return ScanSummary::default() };
        match chilpa_core::scan_into_library(lib, folder) {
            Ok(r) => ScanSummary {
                seen: r.seen,
                read: r.read,
                unchanged: r.unchanged,
                missing: r.missing,
                unreachable: r.unreachable,
            },
            Err(e) => {
                self.now.error = chilpa_i18n::t_args("the scan failed: {}", &[&e.to_string()]);
                ScanSummary::default()
            }
        }
    }

    /// The library's albums.
    pub fn albums(&mut self, limit: usize, offset: usize) -> Vec<Album> {
        let sort = self.album_sort;
        self.albums_sorted(sort, limit, offset)
    }

    /// The library's genres, most common first.
    pub fn genres(&mut self, limit: usize) -> Vec<(String, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        let mut g = lib.genres().unwrap_or_default();
        g.truncate(limit);
        g
    }

    /// The smart lists Chilpa ships with, and how many tracks each yields
    /// today.
    ///
    /// A smart list does not keep tracks: it keeps rules and is resolved
    /// again every time. That is why the count changes on its own.
    pub fn smart_lists(&mut self) -> Vec<SmartListInfo> {
        let Some(lib) = self.library() else { return Vec::new() };
        chilpa_library::smart_builtins()
            .into_iter()
            .map(|l| {
                // The panel wants the number, not the tracks: eight lists
                // side by side, each of them resolved, was two hundred
                // milliseconds of opening a tab.
                let n = lib.smart_count(&l).unwrap_or(0);
                SmartListInfo { name: l.name, track_count: n }
            })
            .collect()
    }

    /// The summary of what was listened to.
    pub fn stats(&mut self) -> Stats {
        let Some(lib) = self.library() else { return Stats::default() };
        lib.stats().unwrap_or_default()
    }

    /// The most played tracks.
    pub fn top_tracks(&mut self, limit: usize) -> Vec<(LibraryTrack, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.top_tracks(limit)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (track_of(t.track), t.count))
            .collect()
    }

    /// The last things played, one track per line.
    pub fn recently_played(&mut self, limit: usize) -> Vec<(LibraryTrack, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.recently_played(limit)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (track_of(t.track), t.count))
            .collect()
    }

    /// The library's artists.
    pub fn artists(&mut self, limit: usize, offset: usize) -> Vec<Artist> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.artists(limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|a| Artist {
                name: a.name,
                album_count: a.album_count,
                track_count: a.track_count,
                artwork_path: a.artwork_path.unwrap_or_default(),
            })
            .collect()
    }

    /// The artist being looked at, empty if all of them are shown.
    pub fn open_artist(&self) -> &str {
        &self.artist
    }

    /// Filters the grid to one artist. An empty name clears the filter.
    pub fn set_open_artist(&mut self, name: &str) {
        self.artist = name.to_string();
    }

    /// The quick view the library is being looked at with.
    pub fn album_sort(&self) -> AlbumSort {
        self.album_sort
    }

    /// Changes the view and remembers it for the next session.
    pub fn set_album_sort(&mut self, order: AlbumSort) {
        self.album_sort = order;
        let mut s = chilpa_core::settings::load();
        s.album_sort = order.name().to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// The albums in whatever order is asked for, with their play counts.
    pub fn albums_sorted(
        &mut self,
        order: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> Vec<Album> {
        let artist = self.artist.clone();
        let filter = (!artist.is_empty()).then_some(artist.as_str());
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_filtered(order, filter, limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|(a, listens)| Album {
                id: a.id,
                title: a.title,
                artist: a.album_artist,
                year: a.year,
                track_count: a.track_count,
                artwork_path: a.artwork_path.unwrap_or_default(),
                play_count: listens,
            })
            .collect()
    }

    /// An album by its id, with its cover and its play count.
    pub fn album(&mut self, album_id: i64) -> Option<Album> {
        let lib = self.library()?;
        let (a, listens) = lib.album(album_id).ok().flatten()?;
        Some(Album {
            id: a.id,
            title: a.title,
            artist: a.album_artist,
            year: a.year,
            track_count: a.track_count,
            artwork_path: a.artwork_path.unwrap_or_default(),
            play_count: listens,
        })
    }

    /// An album's tracks, in the disc's order.
    pub fn album_tracks(&mut self, album_id: i64) -> Vec<LibraryTrack> {
        let Some(lib) = self.library() else { return Vec::new() };
        let tracks: Vec<LibraryTrack> =
            lib.album_tracks(album_id).unwrap_or_default().into_iter().map(track_of).collect();
        self.with_analysis(tracks)
    }

    /// Searches the library by title, artist or album.
    pub fn search(&mut self, text: &str, limit: usize) -> Vec<LibraryTrack> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let Some(lib) = self.library() else { return Vec::new() };
        let tracks: Vec<LibraryTrack> =
            lib.search(text.trim(), limit).unwrap_or_default().into_iter().map(track_of).collect();
        self.with_analysis(tracks)
    }
}
