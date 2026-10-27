//! The library as the facade sees it.
//!
//! Scanning, analysing and asking the database. Everything returned here
//! already comes in the view's types: whoever draws does not know rusqlite.

use super::*;

impl Chilpa {
    /// A read-only, paged inventory of local tracks and their stored analyses.
    pub fn analysis_inventory(&mut self, filter: &str, search: &str, offset: usize) -> serde_json::Value {
        self.analysis_inventory_for_run(filter, search, offset, 0)
    }

    pub fn analysis_inventory_for_run(&mut self, filter: &str, search: &str, offset: usize, run: i64) -> serde_json::Value {
        let running = self.analysis_status().running;
        let active: Vec<i64> = self.analysis.as_ref().map(|job| job.run_id()).into_iter()
            .chain(self.mood_analysis.as_ref().map(|job| job.run_id())).filter(|id| *id > 0).collect();
        let Some(lib) = self.library() else {
            return serde_json::json!({"error": "The library could not be opened."});
        };
        let runs = match lib.analysis_runs() {
            Ok(runs) => runs,
            Err(error) => return serde_json::json!({"error":error.to_string()}),
        };
        match lib.analysis_inventory_for_run(chilpa_core::analysis_job::VERSION, filter, search, offset, run) {
            Ok(inventory) => serde_json::json!({
                "total": inventory.total,
                "standard": inventory.standard,
                "mood": inventory.mood,
                "matched": inventory.matched,
                "active": active,
                "running":running,
                "runs": runs.into_iter().map(|run| serde_json::json!({
                    "id":run.id, "kind":if run.analyzer == chilpa_library::store::mood::ANALYZER { "mood" } else { "standard" },
                    "version":run.version, "folder":run.folder, "started_at":run.started_at, "finished_at":run.finished_at,
                    "state":if run.state == "running" && !active.contains(&run.id) { "interrupted" } else { &run.state },
                    "total":run.total, "previous":run.previous, "processed":run.processed, "saved":run.saved,
                    "failed":run.failed, "error":run.error, "last":run.last,
                })).collect::<Vec<_>>(),
                "tracks": inventory.tracks.into_iter().map(|track| serde_json::json!({
                    "id": track.id, "title": track.title, "artist": track.artist, "album": track.album,
                    "standard": track.standard, "mood": track.mood,
                    "standard_at":track.standard_at, "mood_at":track.mood_at,
                    "result":track.result, "processed_at":track.processed_at,
                })).collect::<Vec<_>>(),
            }),
            Err(error) => serde_json::json!({"error": error.to_string()}),
        }
    }

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

    /// Starts the analysis of whatever is missing, in the background.
    pub fn start_analysis(&mut self) {
        self.start_analysis_under(None);
    }

    /// The same, only for what is missing under `folder` when one is given.
    pub(crate) fn start_analysis_under(&mut self, folder: Option<String>) {
        if self.analysis.is_some() || self.mood_analysis.is_some() {
            self.now.error = chilpa_i18n::t("an analysis is already running");
            return;
        }
        self.analysis = Some(chilpa_core::analysis_job::AnalysisJob::start_under(
            chilpa_library::Library::default_path(),
            folder.clone(),
        ));
        self.analysis_folder = folder;
        self.analysis_status = AnalysisStatus { running: true, ..AnalysisStatus::default() };
    }

    /// Generates Mood playlists from local audio without rerunning tempo analysis.
    pub fn start_mood_analysis(&mut self) {
        if self.analysis.is_some() || self.mood_analysis.is_some() {
            self.now.error = chilpa_i18n::t("an analysis is already running");
            return;
        }
        if !self.natives.is_native(chilpa_library::store::mood::ANALYZER) {
            self.analysis_status = AnalysisStatus {
                mood_requested: true,
                last: chilpa_i18n::t("Install and enable the Mood plugin first."),
                ..AnalysisStatus::default()
            };
            return;
        }
        self.mood_analysis = Some(crate::mood::Job::start(
            chilpa_library::Library::default_path(), None, self.natives.clone(),
        ));
        self.analysis_status = AnalysisStatus {
            running: true,
            mood_requested: true,
            last: chilpa_i18n::t("Preparing Mood analysis"),
            ..AnalysisStatus::default()
        };
    }

    /// Analyzes what `folder` holds once the scan that runs now ends, or at
    /// once when none runs: a folder just added is read first, and nothing
    /// else of the library is analyzed with it.
    pub fn analyze_after_scan(&mut self, folder: &str) {
        if self.scan.is_some() && self.analysis.is_none() {
            self.analysis_status.waiting = Some(folder.to_owned());
        } else {
            self.start_analysis_under(Some(folder.to_owned()));
        }
    }

    /// Asks the analysis to stop, or not to start after the scan.
    pub fn cancel_analysis(&mut self) {
        self.analysis_status.waiting = None;
        if let Some(j) = &self.analysis {
            j.cancel();
        }
        if let Some(j) = &self.mood_analysis { j.cancel(); }
    }

    /// How the analysis is going.
    pub fn analysis_status(&mut self) -> &AnalysisStatus {
        use chilpa_core::analysis_job::AnalysisProgress;
        let events = self.analysis.as_ref().map(|j| j.poll()).unwrap_or_default();
        let mut finished = false;
        let mut standard_done = false;
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
                    standard_done = true;
                }
                AnalysisProgress::Failed(e) => {
                    self.analysis_status.running = false;
                    self.now.error = chilpa_i18n::t_args("the analysis stopped: {}", &[&e]);
                    finished = true;
                }
            }
        }
        if finished {
            let cancelled = self.analysis.as_ref().is_some_and(|job| job.is_cancelled());
            self.analysis = None;
            if standard_done && !cancelled && self.natives.is_native(chilpa_library::store::mood::ANALYZER) {
                self.mood_analysis = Some(crate::mood::Job::start(
                    chilpa_library::Library::default_path(),
                    self.analysis_folder.take(),
                    self.natives.clone(),
                ));
                self.analysis_status.running = true;
                self.analysis_status.mood_requested = true;
                self.analysis_status.last = chilpa_i18n::t("Preparing Mood analysis");
            } else {
                self.analysis_folder = None;
            }
        }
        let mood_events = self.mood_analysis.as_ref().map(|job| job.poll()).unwrap_or_default();
        let mut mood_finished = false;
        for event in mood_events {
            match event {
                crate::mood::Progress::Skipped => {
                    self.analysis_status.running = false;
                    self.analysis_status.last = chilpa_i18n::t("Mood is off. Enable it in Mood models.");
                    mood_finished = true;
                }
                crate::mood::Progress::Cancelled => {
                    self.analysis_status.running = false;
                    self.analysis_status.last = chilpa_i18n::t("Mood analysis cancelled.");
                    mood_finished = true;
                }
                crate::mood::Progress::Counted { total } => {
                    self.analysis_status.done = 0;
                    self.analysis_status.total = total;
                    self.analysis_status.last = chilpa_i18n::t("Analyzing Mood");
                }
                crate::mood::Progress::Advanced { done, total, title } => {
                    self.analysis_status.done = done;
                    self.analysis_status.total = total;
                    self.analysis_status.last = format!("Mood · {title}");
                }
                crate::mood::Progress::Done { analyzed, failed, playlists } => {
                    self.analysis_status.running = false;
                    self.analysis_status.failed += failed;
                    self.analysis_status.playlists_updated = true;
                    self.analysis_status.last = chilpa_i18n::t_args("Mood: {} tracks analyzed, {} playlists updated",
                        &[&analyzed.to_string(), &playlists.to_string()]);
                    mood_finished = true;
                }
                crate::mood::Progress::Failed(error) => {
                    self.analysis_status.running = false;
                    self.analysis_status.last = chilpa_i18n::t_args("Mood analysis stopped: {}", &[&error]);
                    self.now.error = chilpa_i18n::t_args("Mood analysis stopped: {}", &[&error]);
                    mood_finished = true;
                }
            }
        }
        if mood_finished {
            self.mood_analysis = None;
        }
        &self.analysis_status
    }

    /// The folders that make up the library.
    pub fn roots(&mut self) -> Vec<std::path::PathBuf> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.roots().unwrap_or_default().into_iter().map(|r| r.path).collect()
    }

    /// The folders that make up the library, with when each was last
    /// scanned and whether it can be reached now. A folder on a disk that is
    /// not mounted is kept and marked, never pruned.
    pub fn library_folders(&mut self) -> Vec<LibraryFolder> {
        self.ensure_scan_runtime();
        let Some(lib) = self.library() else { return Vec::new() };
        let roots = lib.roots().unwrap_or_default();
        roots.into_iter().map(|r| LibraryFolder {
                scan_policy: self.scan_runtime.policies.get(r.path.to_string_lossy().as_ref()).copied().unwrap_or_default(),
                reachable: r.reachable,
                path: r.path.to_string_lossy().into_owned(),
                last_scan: r.last_scan,
            })
            .collect()
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

    /// The library's own buttons in Settings: what each does, and what its
    /// row says afterwards.
    pub(crate) fn library_maintenance(&mut self, key: &str) -> Option<String> {
        match key {
            "library.mood.generate" => {
                if self.mood_analysis.is_some() {
                    self.cancel_analysis();
                    Some(String::new())
                } else if self.analysis.is_some() {
                    Some(chilpa_i18n::t("Finish or cancel the current analysis first."))
                } else {
                    self.start_mood_analysis();
                    Some(String::new())
                }
            }
            "library.repair" if self.repair_library() => Some(chilpa_i18n::t("Repaired. Nothing was lost.")),
            "library.repair" => Some(self.now.error.clone()),
            _ => None,
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
        if self.scan.is_some() { self.scan_error(chilpa_i18n::t("a scan is already running")); return false; }
        if !self.forget_folder_scan_state(folder) { return false; }
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

    /// Scans a folder into the library, waiting for it to finish.
    pub fn scan(&mut self, folder: &std::path::Path) -> ScanSummary {
        let Some(lib) = self.library() else { return ScanSummary::default() };
        match chilpa_core::scan_into_library(lib, folder) {
            Ok(r) => ScanSummary {
                seen: r.seen,
                read: r.read,
                unchanged: r.unchanged,
                missing: r.missing,
                unreadable: r.unreadable,
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
        let pictures = lib.artist_pictures().unwrap_or_default();
        lib.presented_artists(limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|a| crate::browse::artist_of(a, &pictures))
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

    /// The Albums screen's order as it was chosen, its direction included
    /// ("year-asc"), for the window to show after a restart.
    pub fn album_order(&self) -> String {
        let saved = chilpa_core::settings::load().album_sort;
        if saved.is_empty() { self.album_sort.name().to_string() } else { saved }
    }

    /// Chooses the Albums screen's order and its direction, and remembers
    /// both for the next session.
    pub fn set_album_order(&mut self, order: &str) {
        self.album_sort = AlbumSort::from_name(crate::albums_view::turned_order(order).0);
        let mut s = chilpa_core::settings::load();
        s.album_sort = order.to_string();
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
        let Some(lib) = self.library() else { return Vec::new() };
        let mut ids = lib.album_ids(order).unwrap_or_default();
        if !artist.is_empty() {
            if let Ok(catalog) = lib.version_catalog() {
                ids.retain(|id| catalog.versions.get(id).is_some_and(|v| v.artist.eq_ignore_ascii_case(&artist)));
            }
        }
        let ids = lib.grouped_album_ids(&ids).unwrap_or(ids);
        let page: Vec<i64> = ids.into_iter().skip(offset).take(limit).collect();
        page.chunks(500).flat_map(|ids| lib.albums_by_ids(ids).unwrap_or_default()).map(crate::browse::album_of).collect()
    }

    /// An album by its id, with its cover and its play count.
    pub fn album(&mut self, album_id: i64) -> Option<Album> {
        if album_id < 0 {
            return self.source_album(album_id);
        }
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
            explicit: a.explicit,
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
