//! Saving: deciding what a save does, doing it, and telling the library.
//!
//! Everything is decided before anything is written. What is written is
//! written beside where it goes, and moved over only when all of it is
//! ready; so a save that fails before that moment leaves no trace.

use super::artwork::Waiting;
use super::draft::{Cell, Draft, Row};
use super::fields::{self, ALBUM, TRACK};
use super::worker::{Done, Job};
use super::{Mode, State};
use crate::Chilpa;
use chilpa_library::sidecar::{split, stage, Home, Override, Sidecar, SidecarError, StagedHome, VERSION};
use chilpa_library::{AlbumEdit, Put, TrackEdit};
use chilpa_tags::edit::{self, Change, Cover, FileChanges, Options, Refusal, WriteError};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// What a save does to one home.
pub(crate) struct HomePlan {
    pub home: Home,
    /// A track of it.
    pub of: String,
    /// The sidecar as it has to end. Empty means: remove it.
    pub sidecar: Sidecar,
    /// The cover in wait goes beside it.
    pub cover: bool,
    /// The tracks that live in it.
    pub tracks: Vec<i64>,
}

/// Everything a save is going to do, decided before it does any of it.
pub(crate) struct Plan {
    pub files: Vec<FileChanges>,
    /// The track each of `files` is, in the same order.
    pub file_tracks: Vec<i64>,
    pub options: Options,
    pub homes: Vec<HomePlan>,
    pub cover: Option<Waiting>,
    pub now: i64,
}

/// What a save left on disk.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Written {
    /// The tracks whose files were replaced.
    pub files: Vec<i64>,
    /// The tracks whose home's sidecar was written or removed.
    pub sidecars: Vec<i64>,
    /// The homes that changed, each with a track of it.
    pub homes: Vec<(Home, String)>,
    /// Where the record's cover is now. Empty if it did not change.
    pub cover: String,
}

impl Written {
    pub fn nothing(&self) -> bool {
        self.files.is_empty() && self.sidecars.is_empty()
    }
}

/// What a field's value is laid over a file as.
fn over(value: &str, file: &str) -> Override<String> {
    if value == file {
        Override::Absent
    } else if value.is_empty() {
        Override::Cleared
    } else {
        Override::Value(value.to_string())
    }
}

/// What a field is called where tags are written. The record's title is
/// the file's album.
fn tag_name(of_the_record: bool, name: &'static str) -> &'static str {
    if of_the_record && name == "title" {
        "album"
    } else {
        name
    }
}

/// Puts one field's value into a file's changes.
fn put(changes: &mut FileChanges, of_the_record: bool, name: &str, value: &str) {
    let text = || if value.is_empty() { Change::Clear } else { Change::Set(value.to_string()) };
    let list = || {
        let values = split(value);
        if values.is_empty() {
            Change::Clear
        } else {
            Change::Set(values)
        }
    };
    let number = || value.parse::<u32>().map_or(Change::Clear, Change::Set);
    match (of_the_record, name) {
        (true, "title") => changes.album = text(),
        (true, "album_artist") => changes.album_artist = text(),
        (true, "album_artists") => changes.album_artists = list(),
        (true, "compilation") => changes.compilation = Change::Set(value == "true"),
        (true, "date") => changes.date = text(),
        (true, "genres") => changes.genres = list(),
        (true, "catalog_number") => changes.catalog_number = text(),
        (true, "musicbrainz_release_id") => changes.musicbrainz_release_id = text(),
        (true, "musicbrainz_release_group_id") => changes.musicbrainz_release_group_id = text(),
        (true, "musicbrainz_album_artist_ids") => changes.musicbrainz_album_artist_ids = list(),
        (false, "title") => changes.title = text(),
        (false, "artist_credit") => changes.artist_credit = text(),
        (false, "artists") => changes.artists = list(),
        (false, "track_number") => changes.track_number = number(),
        (false, "disc_number") => changes.disc_number = number(),
        (false, "composers") => changes.composers = list(),
        (false, "performers") => changes.performers = list(),
        (false, "musicbrainz_recording_id") => changes.musicbrainz_recording_id = text(),
        (false, "musicbrainz_track_id") => changes.musicbrainz_track_id = text(),
        (false, "musicbrainz_artist_ids") => changes.musicbrainz_artist_ids = list(),
        // The Discogs release has no tag to go in.
        _ => {}
    }
}

/// Whether a save writes this field.
fn goes(cell: &Cell, mode: Mode) -> bool {
    match mode {
        Mode::Sidecar => cell.dirty(),
        Mode::Files => cell.materializes(),
    }
}

/// Decides what a save does. Nothing is read and nothing is written.
pub(crate) fn plan(draft: &Draft, mode: Mode, options: Options, cover: Option<Waiting>, now: i64) -> Plan {
    let mut files: Vec<FileChanges> = draft.rows.iter().map(|row| FileChanges::none(PathBuf::from(&row.track.external_id))).collect();
    let mut sidecars: Vec<Sidecar> = draft.homes.iter().map(|h| h.sidecar.clone().unwrap_or_else(|| Sidecar::new(now))).collect();
    let in_home = |home: usize| draft.rows.iter().enumerate().filter(move |(_, row)| row.home == home);
    let to_the_file = |row: &Row, of_the_record: bool, name: &'static str| mode == Mode::Files && row.placed.contains(&tag_name(of_the_record, name));

    for (at, field) in ALBUM.iter().enumerate() {
        let cell = &draft.album[at];
        if !goes(cell, mode) {
            continue;
        }
        for (home, sidecar) in sidecars.iter_mut().enumerate() {
            // What every file of the home ends up saying is not laid over
            // any of them.
            let mut laid = Override::Absent;
            for (i, row) in in_home(home) {
                if to_the_file(row, true, field.name) {
                    if cell.now != row.album_file[at] {
                        put(&mut files[i], true, field.name, &cell.now);
                    }
                } else if laid.is_absent() {
                    laid = over(&cell.now, &row.album_file[at]);
                }
            }
            sidecar.album.put(field.name, laid);
        }
    }
    for (i, row) in draft.rows.iter().enumerate() {
        for (at, field) in TRACK.iter().enumerate() {
            let cell = &row.cells[at];
            if !goes(cell, mode) {
                continue;
            }
            let laid = if to_the_file(row, false, field.name) {
                if cell.now != row.file[at] {
                    put(&mut files[i], false, field.name, &cell.now);
                }
                Override::Absent
            } else {
                over(&cell.now, &row.file[at])
            };
            // A track the sidecar did not name is not named to say nothing.
            let sidecar = &mut sidecars[row.home];
            if !laid.is_absent() || sidecar.tracks.contains_key(&row.key) {
                sidecar.track_mut(&row.key).put(field.name, laid);
            }
        }
    }
    for sidecar in &mut sidecars {
        sidecar.tracks.retain(|_, track| !track.is_empty());
    }

    if let Some(waiting) = &cover {
        match mode {
            Mode::Sidecar => {}
            Mode::Files => {
                for changes in &mut files {
                    changes.cover = Change::Set(Cover { bytes: waiting.bytes.clone(), mime: waiting.mime });
                }
                // The files carry the cover now: a sidecar that named one
                // stops naming it.
                for sidecar in &mut sidecars {
                    sidecar.album.cover = Override::Absent;
                }
            }
        }
    }

    let beside = cover.is_some() && mode == Mode::Sidecar;
    let homes = draft
        .homes
        .iter()
        .zip(sidecars)
        .enumerate()
        // A home nothing changes in is not written again.
        .filter(|(_, (state, sidecar))| beside || state.sidecar.as_ref().map_or(!sidecar.is_empty(), |was| was != sidecar))
        .map(|(at, (state, sidecar))| HomePlan {
            home: state.home.clone(),
            of: state.of.clone(),
            sidecar,
            cover: beside,
            tracks: in_home(at).map(|(_, row)| row.track.id).collect(),
        })
        .collect();

    let file_tracks = draft.rows.iter().map(|row| row.track.id).collect();
    Plan { files, file_tracks, options, homes, cover, now }
}

fn refusal(why: Refusal) -> String {
    match why {
        Refusal::Missing => chilpa_i18n::t("it is not where it was"),
        Refusal::ReadOnly => chilpa_i18n::t("it cannot be written to"),
        Refusal::Dsd => chilpa_i18n::t("it is a DSD file"),
        Refusal::Unreadable | Refusal::Format => chilpa_i18n::t("it cannot take tags"),
    }
}

/// What went wrong writing the files, in the user's language, and what
/// was replaced before it did.
fn said(error: WriteError) -> (String, Vec<PathBuf>) {
    let name = |path: &PathBuf| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match error {
        WriteError::Refused { path, why } => (chilpa_i18n::t_args("{} could not be written: {}", &[&name(&path), &refusal(why)]), Vec::new()),
        WriteError::Copy { path, error } => (chilpa_i18n::t_args("{} could not be copied: {}", &[&name(&path), &error.to_string()]), Vec::new()),
        WriteError::Write { path, reason } => (chilpa_i18n::t_args("{} could not be written: {}", &[&name(&path), &reason]), Vec::new()),
        WriteError::Verify { path, field } => (
            chilpa_i18n::t_args("{} was written and does not say what it should in \"{}\"; nothing was replaced", &[&name(&path), field]),
            Vec::new(),
        ),
        WriteError::Audio { path } => (chilpa_i18n::t_args("{} came out with different audio; nothing was replaced", &[&name(&path)]), Vec::new()),
        WriteError::NoPlace { path, field } => (chilpa_i18n::t_args("{} has no place for \"{}\"", &[&name(&path), field]), Vec::new()),
        WriteError::Replace { path, error, replaced } => (chilpa_i18n::t_args("{} could not be replaced: {}", &[&name(&path), &error.to_string()]), replaced),
    }
}

fn sidecar_said(error: SidecarError) -> String {
    match error {
        SidecarError::Newer(_) => chilpa_i18n::t("a newer Chilpa wrote this sidecar; update Chilpa to change it"),
        other => chilpa_i18n::t_args("the sidecar could not be written: {}", &[&other.to_string()]),
    }
}

/// Does what was planned. Runs on the worker.
pub(crate) fn run(session: u64, plan: Plan, finished: &Sender<Done>) -> Done {
    let failed = |reason: String, written: Written| Done::SaveFailed { session, reason, written };
    let mut written = Written::default();

    // 1. The files, written beside themselves and checked.
    let mut progress = |done: usize, total: usize| {
        let _ = finished.send(Done::Progress { session, done, total });
    };
    let staged = match edit::stage(&plan.files, plan.options, &mut progress) {
        Ok(staged) => staged,
        Err(e) => return failed(said(e).0, written),
    };

    // 2. The sidecars and their covers, written beside themselves.
    let cover = plan.cover.as_ref().map(|w| (w.bytes.as_slice(), w.extension));
    let mut homes: Vec<(StagedHome, &HomePlan)> = Vec::with_capacity(plan.homes.len());
    for home in &plan.homes {
        match stage(&home.home, home.sidecar.clone(), cover.filter(|_| home.cover), plan.now) {
            Ok(staged) => homes.push((staged, home)),
            // What was staged is dropped here, and dropping it removes it.
            Err(e) => return failed(sidecar_said(e), written),
        }
    }

    // 3. From here on, things change. The files first.
    let changed: Vec<PathBuf> = plan.files.iter().filter(|f| !f.is_empty()).map(|f| f.path.clone()).collect();
    let track_of = |path: &PathBuf| plan.files.iter().position(|f| f.path == *path).and_then(|at| plan.file_tracks.get(at).copied());
    match staged.commit() {
        Ok(replaced) => written.files = replaced.iter().filter_map(track_of).collect(),
        Err(e) => {
            let (reason, replaced) = said(e);
            written.files = replaced.iter().filter_map(track_of).collect();
            let reason = chilpa_i18n::t_args(
                "{} of {} files were changed before it failed: {}",
                &[&written.files.len().to_string(), &changed.len().to_string(), &reason],
            );
            return failed(reason, written);
        }
    }

    // 4. Then the sidecars.
    for (staged, home) in homes {
        let cover_at = staged.cover().map(|p| p.to_string_lossy().into_owned());
        if let Err(e) = staged.land() {
            return failed(sidecar_said(e), written);
        }
        written.sidecars.extend(&home.tracks);
        written.homes.push((home.home.clone(), home.of.clone()));
        if written.cover.is_empty() && home.cover {
            written.cover = cover_at.unwrap_or_default();
        }
    }

    // 5. A cover that went into the files is shown from the first of them.
    if plan.cover.is_some() && written.cover.is_empty() {
        if let Some(first) = changed.first() {
            let kept = chilpa_tags::extract_artwork(first, &chilpa_core::artwork_cache()).ok().flatten();
            written.cover = kept.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        }
    }
    Done::Saved { session, written }
}

/// What the library is told, for the tracks a save reached.
pub(crate) fn project(draft: &Draft, reached: &[i64], cover: &str) -> AlbumEdit {
    let went = |cell: &Cell| cell.materializes();
    let set = |cell: &Cell| if went(cell) { Put::Set(cell.now.clone()) } else { Put::Keep };
    let number = |cell: &Cell| if went(cell) { Put::Set(cell.now.parse::<u16>().ok()) } else { Put::Keep };
    let track_cell = |row: &Row, name: &str| row.cells[fields::track(name).unwrap_or(0)].clone();

    let genres = draft.album_cell("genres");
    let date = draft.album_cell("date");
    let tracks = draft
        .rows
        .iter()
        .filter(|row| reached.contains(&row.track.id))
        .map(|row| {
            let recording = track_cell(row, "musicbrainz_recording_id");
            TrackEdit {
                id: row.track.id,
                title: set(&track_cell(row, "title")),
                artist: set(&track_cell(row, "artist_credit")),
                track_number: number(&track_cell(row, "track_number")),
                disc_number: number(&track_cell(row, "disc_number")),
                recording_mbid: if went(&recording) { Put::Set((!recording.now.is_empty()).then(|| recording.now.clone())) } else { Put::Keep },
                genres: if went(genres) { Put::Set(split(&genres.now)) } else { Put::Keep },
            }
        })
        .collect();
    AlbumEdit {
        album_id: draft.album_id,
        title: draft.album_cell("title").now.clone(),
        album_artist: draft.album_cell("album_artist").now.clone(),
        year: if went(date) { Put::Set(fields::year_of(&date.now)) } else { Put::Keep },
        artwork: if cover.is_empty() { Put::Keep } else { Put::Set(cover.to_string()) },
        tracks,
    }
}

impl Chilpa {
    /// Saves what the form holds. False when it is refused; why is in the
    /// view.
    pub fn metadata_save(&mut self) -> bool {
        if self.metadata.state != State::Ready {
            return false;
        }
        let scanning = self.scan.is_some();
        // `now.path` keeps the last track after a stop, for the player bar;
        // only a track playing or paused holds its file open.
        let loaded = if self.now.playing || self.now.paused { self.now.path.clone() } else { String::new() };
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref() else { return false };

        if scanning {
            return editor.refuse(chilpa_i18n::t("a scan is running; save when it is done"));
        }
        let changed = draft.touched && draft.album.iter().chain(draft.rows.iter().flat_map(|r| r.cells.iter())).any(Cell::dirty);
        if !changed && editor.waiting.is_none() {
            return editor.refuse(chilpa_i18n::t("there is nothing to save"));
        }
        if let Some(why) = draft.first_invalid() {
            return editor.refuse(why);
        }
        if editor.mode == Mode::Files {
            if !loaded.is_empty() && draft.rows.iter().any(|row| row.track.external_id == loaded) {
                return editor.refuse(chilpa_i18n::t("stop what is playing before writing to its file, or save to a sidecar"));
            }
            let refused = super::view::files_refused(draft, &editor.layers);
            if !refused.is_empty() {
                return editor.refuse(refused);
            }
        }
        let made = plan(draft, editor.mode, Options { id3: editor.id3, secondary_layers: editor.secondary_layers }, editor.waiting.clone(), crate::now_unix());
        for home in &made.homes {
            let state = draft.homes.iter().find(|h| h.home == home.home);
            if let Some(why) = state.filter(|h| !h.unreadable.is_empty()) {
                let name = why.home.sidecar().to_string_lossy().into_owned();
                return editor.refuse(chilpa_i18n::t_args("{} is not a sidecar Chilpa can read; move it away or fix it", &[&name]));
            }
            if home.sidecar.version > VERSION {
                return editor.refuse(chilpa_i18n::t("a newer Chilpa wrote this sidecar; update Chilpa to change it"));
            }
        }

        editor.state = State::Saving;
        editor.progress = (0, made.files.iter().filter(|f| !f.is_empty()).count());
        editor.error.clear();
        editor.notice.clear();
        // What could still come back for this record comes back to nobody.
        editor.lookup_turn += 1;
        editor.artwork_turn += 1;
        editor.searching = false;
        editor.loading = false;
        editor.artwork_searching = false;
        editor.artwork_loading = false;
        let session = editor.session;
        if !editor.ask(Job::Save { session, plan: made }) {
            editor.state = State::Ready;
            return editor.refuse(chilpa_i18n::t("the editor could not be started"));
        }
        editor.refresh();
        true
    }

    /// What a save left is told to the library, and the editor is left as
    /// it has to be after it.
    pub(super) fn metadata_saved(&mut self, written: Written, failed: Option<String>) {
        let reached: Vec<i64> = written.files.iter().chain(&written.sidecars).copied().collect();
        let edit = self.metadata.draft.as_ref().filter(|_| !reached.is_empty()).map(|draft| project(draft, &reached, &written.cover));
        let now = crate::now_unix();

        let mut album_id = self.metadata.album_id;
        let mut told = None;
        if let (Some(edit), Some(lib)) = (edit, self.library()) {
            match lib.apply_edit(&edit) {
                Ok(edited) => {
                    album_id = edited.album_id;
                    // Only once the library says what the sidecars say: a
                    // sidecar written down as seen is one the next scan
                    // skips, and it would never catch the library up.
                    for (home, of) in &written.homes {
                        let _ = lib.note_home(home, of, now);
                    }
                }
                Err(e) => told = Some(chilpa_i18n::t_args("what was saved could not be written down in the library: {}; rescan to see it", &[&e.to_string()])),
            }
        }

        let editor = &mut self.metadata;
        match failed {
            // Nothing was written: the form is as it was, and so is the record.
            Some(reason) if written.nothing() => {
                editor.state = State::Ready;
                editor.progress = (0, 0);
                editor.error = reason;
                editor.notice.clear();
            }
            outcome => {
                // The picture the form was taken from is no longer true.
                editor.forget();
                editor.album_id = album_id;
                editor.notice = if outcome.is_none() && told.is_none() { chilpa_i18n::t("the changes were saved") } else { String::new() };
                editor.error = outcome.or(told).unwrap_or_default();
            }
        }
        editor.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::super::artwork::testing::picture;
    use super::super::draft::testing::*;
    use super::*;

    fn options() -> Options {
        Options::default()
    }

    fn waiting() -> Waiting {
        Waiting {
            bytes: picture(8, image::ImageFormat::Png),
            extension: "png",
            mime: "image/png",
            width: 8,
            height: 8,
            preview: PathBuf::from("/nowhere/preview.png"),
            from: "file",
        }
    }

    #[test]
    fn a_save_to_a_sidecar_lays_over_only_what_changed() {
        let mut draft = plain();
        draft.set_album("genres", "Blues; Soul");
        draft.set_track(2, "title", "Another");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        assert!(made.files.iter().all(FileChanges::is_empty), "no file is written to");
        assert_eq!(made.homes.len(), 1);
        let sidecar = &made.homes[0].sidecar;
        assert_eq!(sidecar.album.genres, Override::Value(vec!["Blues".into(), "Soul".into()]));
        assert_eq!(sidecar.album.title, Override::Absent, "the title was not touched");
        assert_eq!(sidecar.album.date, Override::Absent);
        assert_eq!(sidecar.tracks.len(), 1);
        assert_eq!(sidecar.tracks["02.flac"].title, Override::Value("Another".into()));
        assert_eq!(made.homes[0].tracks, [1, 2]);
    }

    #[test]
    fn a_field_typed_back_to_what_the_file_says_stops_being_overridden() {
        let mut sidecar = Sidecar::new(0);
        sidecar.track_mut("01.flac").title = Override::Value("Laid over".into());
        sidecar.album.catalog_number = Override::Value("CAT-1".into());
        let mut draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(Some(sidecar))]);
        draft.set_track(1, "title", "Track 1");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        let sidecar = &made.homes[0].sidecar;
        assert!(sidecar.tracks.get("01.flac").is_none_or(|t| t.is_empty()));
        assert_eq!(sidecar.album.catalog_number, Override::Value("CAT-1".into()), "what was not touched stays laid over");
    }

    #[test]
    fn clearing_a_field_lays_nothing_over_it() {
        let mut draft = plain();
        draft.set_album("date", "");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        assert_eq!(made.homes[0].sidecar.album.date, Override::Cleared);
    }

    #[test]
    fn a_sidecar_with_nothing_left_in_it_is_planned_empty() {
        let mut sidecar = Sidecar::new(0);
        sidecar.track_mut("01.flac").title = Override::Value("Laid over".into());
        let mut draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(Some(sidecar))]);
        draft.set_track(1, "title", "Track 1");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        assert_eq!(made.homes.len(), 1, "it changed, so it is written: as nothing");
        assert!(made.homes[0].sidecar.is_empty());
    }

    #[test]
    fn a_save_to_the_files_writes_what_changed_and_what_was_laid_over() {
        let mut sidecar = Sidecar::new(0);
        sidecar.album.catalog_number = Override::Value("CAT-1".into());
        sidecar.track_mut("01.flac").title = Override::Value("Laid over".into());
        let mut draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(Some(sidecar))]);
        draft.set_album("date", "1984-09-03");
        let made = plan(&draft, Mode::Files, options(), None, 5);

        assert_eq!(made.files[0].title, Change::Set("Laid over".into()), "what the sidecar laid over goes into the file");
        assert_eq!(made.files[1].title, Change::Keep, "a title nobody touched is not rewritten");
        for file in &made.files {
            assert_eq!(file.date, Change::Set("1984-09-03".into()));
            assert_eq!(file.catalog_number, Change::Set("CAT-1".into()));
            assert_eq!(file.genres, Change::Keep);
            assert_eq!(file.album, Change::Keep);
        }
        assert_eq!(made.homes.len(), 1);
        assert!(made.homes[0].sidecar.is_empty(), "and the sidecar is left with nothing to say");
    }

    #[test]
    fn what_a_file_has_no_place_for_stays_in_the_sidecar() {
        let mut first = read(1);
        first.placed.retain(|name| *name != "composers");
        let mut draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![first, read(2)], vec![home(None)]);
        draft.set_track(1, "composers", "Somebody");
        draft.set_track(2, "composers", "Somebody");
        draft.set_album("discogs_release_id", "1001");
        let made = plan(&draft, Mode::Files, options(), None, 5);
        assert_eq!(made.files[0].composers, Change::Keep);
        assert_eq!(made.files[1].composers, Change::Set(vec!["Somebody".into()]));
        let sidecar = &made.homes[0].sidecar;
        assert_eq!(sidecar.tracks["01.flac"].composers, Override::Value(vec!["Somebody".into()]));
        assert!(sidecar.tracks.get("02.flac").is_none_or(|t| t.is_empty()));
        assert_eq!(sidecar.album.discogs_release_id, Override::Value("1001".into()), "no tag holds it");
    }

    #[test]
    fn a_record_in_two_folders_gets_a_sidecar_in_each() {
        let mut second = track(2, 2);
        second.external_id = "/m/A record/CD2/02.flac".into();
        let other = super::super::draft::HomeState {
            home: Home::of("/m/A record/CD2/02.flac", None),
            of: "/m/A record/CD2/02.flac".into(),
            sidecar: None,
            unreadable: String::new(),
        };
        let mut draft = Draft::open(&album(), vec![track(1, 1), second], vec![read(1), read(2)], vec![home(None), other]);
        draft.set_album("title", "Renamed");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        assert_eq!(made.homes.len(), 2);
        assert_eq!(made.homes[0].tracks, [1]);
        assert_eq!(made.homes[1].tracks, [2]);
        assert!(made.homes.iter().all(|h| h.sidecar.album.title == Override::Value("Renamed".into())));
    }

    #[test]
    fn a_home_nothing_changes_in_is_not_written_again() {
        let mut second = track(2, 2);
        second.external_id = "/m/A record/CD2/02.flac".into();
        let other = super::super::draft::HomeState {
            home: Home::of("/m/A record/CD2/02.flac", None),
            of: "/m/A record/CD2/02.flac".into(),
            sidecar: None,
            unreadable: String::new(),
        };
        let mut draft = Draft::open(&album(), vec![track(1, 1), second], vec![read(1), read(2)], vec![home(None), other]);
        draft.set_track(2, "title", "Another");
        let made = plan(&draft, Mode::Sidecar, options(), None, 5);
        assert_eq!(made.homes.len(), 1);
        assert_eq!(made.homes[0].tracks, [2]);
    }

    #[test]
    fn a_cover_goes_beside_the_sidecar_or_into_the_files() {
        let draft = plain();
        let beside = plan(&draft, Mode::Sidecar, options(), Some(waiting()), 5);
        assert!(beside.files.iter().all(FileChanges::is_empty));
        assert_eq!(beside.homes.len(), 1);
        assert!(beside.homes[0].cover);

        let inside = plan(&draft, Mode::Files, options(), Some(waiting()), 5);
        assert!(inside.files.iter().all(|f| matches!(&f.cover, Change::Set(c) if c.mime == "image/png")));
        assert!(inside.homes.is_empty(), "there was no sidecar and there is none to write");
    }

    #[test]
    fn the_library_is_told_what_went_out_and_nothing_else() {
        let mut draft = plain();
        draft.set_album("date", "1984-09-03");
        draft.set_track(1, "title", "Another");
        draft.set_track(1, "disc_number", "2");
        let edit = project(&draft, &[1, 2], "");
        assert_eq!((edit.title.as_str(), edit.album_artist.as_str()), ("A record", "Someone"));
        assert_eq!(edit.year, Put::Set(Some(1984)));
        assert_eq!(edit.artwork, Put::Keep);
        assert_eq!(edit.tracks.len(), 2);
        assert_eq!(edit.tracks[0].title, Put::Set("Another".into()));
        assert_eq!(edit.tracks[0].disc_number, Put::Set(Some(2)));
        assert_eq!(edit.tracks[0].artist, Put::Keep);
        assert_eq!(edit.tracks[0].genres, Put::Keep);
        assert_eq!(edit.tracks[1].title, Put::Keep);

        let only = project(&draft, &[2], "/covers/new.png");
        assert_eq!(only.tracks.len(), 1, "a track the save did not reach is not told");
        assert_eq!(only.artwork, Put::Set("/covers/new.png".into()));
    }
}
