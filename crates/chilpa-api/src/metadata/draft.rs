//! What the form holds.
//!
//! Three things are kept apart for every field: what the file says, what a
//! sidecar lays over it, and what the form holds now. A save writes a field
//! when what the form holds is not what was in force, and nothing else.

use super::fields::{self, Field, ALBUM, TRACK};
use chilpa_library::sidecar::{join, Home, Override, Sidecar};
use chilpa_library::{EditTrack, StoredAlbum};
use chilpa_tags::edit::Snapshot;

/// One field of the form.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Cell {
    /// What was in force when the record was opened.
    start: String,
    start_mixed: bool,
    /// What the form holds now.
    pub now: String,
    /// The tracks do not agree, and nobody has typed over it.
    pub mixed: bool,
    /// A sidecar lays something over it.
    pub overridden: bool,
    /// What it shows is written nowhere: the library worked it out. It goes
    /// out with the first save.
    pub unwritten: bool,
}

impl Cell {
    pub fn of(value: String, overridden: bool) -> Self {
        Self { start: value.clone(), now: value, overridden, ..Self::default() }
    }

    pub fn mixed(overridden: bool) -> Self {
        Self { start_mixed: true, mixed: true, overridden, ..Self::default() }
    }

    pub fn dirty(&self) -> bool {
        !self.mixed && (self.unwritten || self.now != self.start)
    }

    /// What a save to the files writes: what changed, and what a sidecar
    /// was laying over them.
    pub fn materializes(&self) -> bool {
        !self.mixed && (self.dirty() || self.overridden)
    }

    pub fn set(&mut self, value: String) {
        self.now = value;
        self.mixed = false;
    }

    pub fn revert(&mut self) {
        self.now = self.start.clone();
        self.mixed = self.start_mixed;
    }
}

/// What a file's tags said when the record was opened.
#[derive(Clone, Debug, Default)]
pub(crate) struct Read {
    pub said: Snapshot,
    /// False when nothing could be read: what the library keeps stands in.
    pub known: bool,
    /// The fields its tag has a place for. Empty for a file that takes none.
    pub placed: Vec<&'static str>,
}

/// A home of the record, and the sidecar in force for it.
#[derive(Clone, Debug)]
pub(crate) struct HomeState {
    pub home: Home,
    /// A track of it, which is what QBZ's sidecars are looked for by.
    pub of: String,
    pub sidecar: Option<Sidecar>,
    /// Why its sidecar could not be read, when it could not. Empty otherwise.
    pub unreadable: String,
}

pub(crate) struct Row {
    pub track: EditTrack,
    /// Which of the draft's homes is its own.
    pub home: usize,
    /// What it goes by in its home.
    pub key: String,
    pub placed: Vec<&'static str>,
    /// What its file says of each of its fields, in `TRACK`'s order.
    pub file: Vec<String>,
    /// And of each of the record's, in `ALBUM`'s.
    pub album_file: Vec<String>,
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn inside_a_disc(&self) -> bool {
        self.track.container.is_some()
    }
}

pub(crate) struct Draft {
    pub album_id: i64,
    pub album: Vec<Cell>,
    pub rows: Vec<Row>,
    pub homes: Vec<HomeState>,
    /// Somebody changed something since it was opened.
    pub touched: bool,
}

/// An override as the form holds it: `None` when there is none, empty when
/// it clears.
fn laid(over: Override<String>) -> Option<String> {
    match over {
        Override::Absent => None,
        Override::Cleared => Some(String::new()),
        Override::Value(value) => Some(value),
    }
}

fn digits(number: Option<u32>) -> String {
    number.map(|n| n.to_string()).unwrap_or_default()
}

/// What a file says of one of its own fields.
fn track_file(field: &Field, read: &Read, track: &EditTrack) -> String {
    let said = &read.said;
    match field.name {
        // A file with no title goes by its name, which is what the library
        // wrote down for it.
        "title" => said.title.clone().unwrap_or_else(|| track.title.clone()),
        // Several artist values are shown the way the library joins them.
        "artist_credit" if said.artist_credit.is_empty() => track.artist.clone(),
        "artist_credit" => said.artist_credit.join(", "),
        "artists" => join(&said.artists),
        "track_number" if read.known => digits(said.track_number),
        "track_number" => digits(track.track_number.map(u32::from)),
        "disc_number" if read.known => digits(said.disc_number.or(track.disc_number.map(u32::from))),
        "disc_number" => digits(track.disc_number.map(u32::from)),
        "composers" => join(&said.composers),
        "performers" => join(&said.performers),
        "musicbrainz_recording_id" if read.known => said.musicbrainz_recording_id.clone().unwrap_or_default(),
        "musicbrainz_recording_id" => track.recording_mbid.clone().unwrap_or_default(),
        "musicbrainz_track_id" => said.musicbrainz_track_id.clone().unwrap_or_default(),
        "musicbrainz_artist_ids" => join(&said.musicbrainz_artist_ids),
        _ => String::new(),
    }
}

/// What a file says of one of its record's fields.
fn album_file(field: &Field, read: &Read, track: &EditTrack, album: &StoredAlbum) -> String {
    let said = &read.said;
    match field.name {
        "title" => said.album.clone().unwrap_or_else(|| album.title.clone()),
        "album_artist" if read.known => said.album_artist.clone().unwrap_or_default(),
        "album_artist" => album.album_artist.clone(),
        "album_artists" => join(&said.album_artists),
        "compilation" => (said.compilation == Some(true)).to_string(),
        "date" if read.known => said.date.clone().unwrap_or_default(),
        "date" => album.year.map(|y| format!("{y:04}")).unwrap_or_default(),
        "genres" if read.known => join(&chilpa_library::split_genres(&said.genres)),
        "genres" => join(&track.genres),
        "catalog_number" => said.catalog_number.clone().unwrap_or_default(),
        "label" => said.label.clone().unwrap_or_default(),
        "musicbrainz_release_id" => said.musicbrainz_release_id.clone().unwrap_or_default(),
        "musicbrainz_release_group_id" => said.musicbrainz_release_group_id.clone().unwrap_or_default(),
        "musicbrainz_album_artist_ids" => join(&said.musicbrainz_album_artist_ids),
        // No tag holds it.
        _ => String::new(),
    }
}

impl Draft {
    /// The form as a record opens: `reads` one per track and in their
    /// order, `homes` with every home any of the tracks lives in.
    pub fn open(album: &StoredAlbum, tracks: Vec<EditTrack>, reads: Vec<Read>, homes: Vec<HomeState>) -> Self {
        let mut rows: Vec<Row> = Vec::with_capacity(tracks.len());
        for (track, read) in tracks.into_iter().zip(reads) {
            let home = Home::of(&track.external_id, track.container.as_deref());
            let at = homes.iter().position(|h| h.home == home).unwrap_or(0);
            let key = home.key(&track.external_id);
            let over = homes.get(at).and_then(|h| h.sidecar.as_ref()).and_then(|s| s.tracks.get(&key));
            let file: Vec<String> = TRACK.iter().map(|f| track_file(f, &read, &track)).collect();
            let cells = TRACK
                .iter()
                .zip(&file)
                .map(|(f, file)| match over.and_then(|o| laid(o.get(f.name))) {
                    Some(laid) => Cell::of(laid, true),
                    None => {
                        let mut cell = Cell::of(file.clone(), false);
                        cell.unwritten = f.name == "disc_number" && read.known && read.said.disc_number.is_none()
                            && track.disc_number.is_some();
                        cell
                    },
                })
                .collect();
            let album_file = ALBUM.iter().map(|f| album_file(f, &read, &track, album)).collect();
            rows.push(Row { track, home: at, key, placed: read.placed, file, album_file, cells });
        }

        let of_home = |row: &Row, name: &str| homes.get(row.home).and_then(|h| h.sidecar.as_ref()).and_then(|s| laid(s.album.get(name)));
        let cells = ALBUM
            .iter()
            .enumerate()
            .map(|(at, field)| {
                let in_force: Vec<String> =
                    rows.iter().map(|row| of_home(row, field.name).unwrap_or_else(|| row.album_file[at].clone())).collect();
                let overridden = rows.iter().any(|row| of_home(row, field.name).is_some());
                match field.name {
                    // The record is what the library says it is: that is
                    // what made these tracks one record.
                    "title" => Cell::of(album.title.clone(), overridden),
                    "album_artist" => {
                        let mut cell = Cell::of(album.album_artist.clone(), overridden);
                        cell.unwritten = in_force.iter().any(String::is_empty);
                        cell
                    }
                    _ if in_force.windows(2).all(|pair| fields::same(field.kind, &pair[0], &pair[1])) => {
                        Cell::of(in_force.into_iter().next().unwrap_or_default(), overridden)
                    }
                    _ => Cell::mixed(overridden),
                }
            })
            .collect();
        Self { album_id: album.id, album: cells, rows, homes, touched: false }
    }

    pub fn row(&self, track_id: i64) -> Option<usize> {
        self.rows.iter().position(|row| row.track.id == track_id)
    }

    /// Sets one of the record's fields. False for a name that is not one.
    pub fn set_album(&mut self, name: &str, value: &str) -> bool {
        let Some(at) = fields::album(name) else { return false };
        self.album[at].set(fields::normalize(ALBUM[at].kind, value));
        self.touched = true;
        true
    }

    /// Sets one of a track's. False for a track or a name that is not one,
    /// and for the number of a track inside a disc.
    pub fn set_track(&mut self, track_id: i64, name: &str, value: &str) -> bool {
        let (Some(row), Some(at)) = (self.row(track_id), fields::track(name)) else { return false };
        if name == "track_number" && self.rows[row].inside_a_disc() {
            return false;
        }
        self.rows[row].cells[at].set(fields::normalize(TRACK[at].kind, value));
        self.touched = true;
        true
    }

    pub fn revert(&mut self) {
        for cell in self.album.iter_mut().chain(self.rows.iter_mut().flat_map(|row| row.cells.iter_mut())) {
            cell.revert();
        }
        self.touched = false;
    }

    pub fn album_cell(&self, name: &str) -> &Cell {
        &self.album[fields::album(name).unwrap_or(0)]
    }

    /// The first field that cannot be saved, in the user's language.
    pub fn first_invalid(&self) -> Option<String> {
        let of_the_record = ALBUM.iter().zip(&self.album).filter(|(_, cell)| !cell.mixed).map(|(f, cell)| fields::invalid(f, false, &cell.now));
        let of_the_tracks = self.rows.iter().flat_map(|row| TRACK.iter().zip(&row.cells).map(|(f, cell)| fields::invalid(f, true, &cell.now)));
        of_the_record.chain(of_the_tracks).find(|why| !why.is_empty())
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub(crate) fn album() -> StoredAlbum {
        StoredAlbum { id: 7, title: "A record".into(), album_artist: "Someone".into(), year: Some(1990), artwork_path: None, track_count: 2 }
    }

    pub(crate) fn track(id: i64, number: u16) -> EditTrack {
        EditTrack {
            id,
            external_id: format!("/m/A record/{number:02}.flac"),
            container: None,
            title: format!("Track {number}"),
            artist: "Someone".into(),
            track_number: Some(number),
            disc_number: None,
            recording_mbid: None,
            genres: vec!["Rock".into()],
        }
    }

    pub(crate) fn read(number: u32) -> Read {
        Read {
            said: Snapshot {
                title: Some(format!("Track {number}")),
                artist_credit: vec!["Someone".into()],
                album: Some("A record".into()),
                album_artist: Some("Someone".into()),
                date: Some("1990-05-01".into()),
                genres: vec!["Rock".into()],
                track_number: Some(number),
                ..Snapshot::default()
            },
            known: true,
            placed: vec![
                "title", "artist_credit", "album", "date", "track_number", "disc_number", "cover", "album_artist",
                "catalog_number", "musicbrainz_release_id", "musicbrainz_release_group_id", "musicbrainz_recording_id",
                "musicbrainz_track_id", "artists", "album_artists", "genres", "composers", "performers",
                "musicbrainz_album_artist_ids", "musicbrainz_artist_ids", "compilation",
            ],
        }
    }

    pub(crate) fn home(sidecar: Option<Sidecar>) -> HomeState {
        HomeState { home: Home::of("/m/A record/01.flac", None), of: "/m/A record/01.flac".into(), sidecar, unreadable: String::new() }
    }

    /// Two tracks in one folder, as their files say.
    pub(crate) fn plain() -> Draft {
        Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(None)])
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn a_record_opens_saying_what_its_files_say_and_clean() {
        let draft = plain();
        assert_eq!(draft.album_cell("title").now, "A record");
        assert_eq!(draft.album_cell("date").now, "1990-05-01", "the date is whole");
        assert_eq!(draft.album_cell("genres").now, "Rock");
        assert_eq!(draft.rows[1].cells[0].now, "Track 2");
        assert!(!draft.touched);
        assert!(draft.album.iter().chain(draft.rows.iter().flat_map(|r| r.cells.iter())).all(|c| !c.dirty()));
        assert_eq!(draft.first_invalid(), None);
    }

    #[test]
    fn a_field_the_tracks_disagree_on_starts_mixed_and_clean() {
        let mut second = read(2);
        second.said.date = Some("1991".into());
        let draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), second], vec![home(None)]);
        let date = draft.album_cell("date");
        assert!(date.mixed && date.now.is_empty());
        assert!(!date.dirty() && !date.materializes());
    }

    #[test]
    fn typing_over_a_mixed_field_makes_it_dirty() {
        let mut second = read(2);
        second.said.date = Some("1991".into());
        let mut draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), second], vec![home(None)]);
        assert!(draft.set_album("date", " 1990 "));
        let date = draft.album_cell("date");
        assert!(!date.mixed && date.dirty());
        assert_eq!(date.now, "1990");
        draft.revert();
        assert!(draft.album_cell("date").mixed, "and reverting leaves it as it opened");
        assert!(!draft.touched);
    }

    #[test]
    fn what_the_sidecar_lays_over_is_what_the_form_opens_with() {
        let mut sidecar = Sidecar::new(0);
        sidecar.album.catalog_number = Override::Value("CAT-1".into());
        sidecar.album.date = Override::Cleared;
        sidecar.track_mut("02.flac").title = Override::Value("Laid over".into());
        let draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(Some(sidecar))]);
        let number = draft.album_cell("catalog_number");
        assert_eq!(number.now, "CAT-1");
        assert!(number.overridden && !number.dirty() && number.materializes());
        assert_eq!(draft.album_cell("date").now, "", "cleared is empty, whatever the file says");
        assert_eq!(draft.rows[1].cells[0].now, "Laid over");
        assert_eq!(draft.rows[1].file[0], "Track 2", "and what the file says is still known");
        assert!(!draft.rows[0].cells[0].overridden);
    }

    #[test]
    fn typing_back_what_was_there_is_not_a_change() {
        let mut draft = plain();
        draft.set_track(1, "title", "Another");
        assert!(draft.rows[0].cells[0].dirty());
        draft.set_track(1, "title", "  Track 1 ");
        assert!(!draft.rows[0].cells[0].dirty());
        assert!(draft.touched, "somebody did type, though");
    }

    #[test]
    fn an_invalid_value_is_kept_and_found() {
        let mut draft = plain();
        draft.set_track(2, "track_number", "twelve");
        assert_eq!(draft.rows[1].cells[3].now, "twelve", "what was typed is not lost");
        assert!(draft.first_invalid().is_some());
        draft.set_track(2, "track_number", "12");
        assert_eq!(draft.first_invalid(), None);
    }

    #[test]
    fn a_track_inside_a_disc_keeps_its_number() {
        let mut held = track(1, 1);
        held.external_id = "sacd:/m/disc.iso#1".into();
        held.container = Some("/m/disc.iso".into());
        let at = HomeState { home: Home::of("", Some("/m/disc.iso")), of: held.external_id.clone(), sidecar: None, unreadable: String::new() };
        let mut draft = Draft::open(&album(), vec![held], vec![Read::default()], vec![at]);
        assert_eq!(draft.rows[0].key, "#1");
        assert!(!draft.set_track(1, "track_number", "5"));
        assert!(draft.set_track(1, "title", "Renamed"));
        assert_eq!(draft.rows[0].file[3], "1", "what could not be read is what the library keeps");
    }

    #[test]
    fn a_field_that_does_not_exist_is_refused() {
        let mut draft = plain();
        assert!(!draft.set_album("mood", "calm"));
        assert!(!draft.set_track(1, "file_path", "/somewhere/else.flac"));
        assert!(!draft.set_track(99, "title", "Nobody's"));
        assert!(!draft.touched);
    }

    #[test]
    fn an_album_artist_the_library_worked_out_goes_out_with_the_first_save() {
        let mut first = read(1);
        first.said.album_artist = None;
        let draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![first, read(2)], vec![home(None)]);
        let artist = draft.album_cell("album_artist");
        assert_eq!(artist.now, "Someone");
        assert!(artist.unwritten && artist.dirty());
        assert!(!draft.touched, "and nobody has changed anything yet");
    }

    #[test]
    fn two_artist_values_are_shown_joined_and_are_not_a_change() {
        let mut first = read(1);
        first.said.artist_credit = vec!["One".into(), "Two".into()];
        let draft = Draft::open(&album(), vec![track(1, 1)], vec![first], vec![home(None)]);
        assert_eq!(draft.rows[0].cells[1].now, "One, Two");
        assert!(!draft.rows[0].cells[1].dirty());
    }
}
