//! I resolve a search selection off the window thread and retain its identities.
use crate::{Chilpa, FoundRow, Reader};
use crate::source_catalog::{Catalog, SelectionHold};
use std::sync::{Arc, mpsc::{channel, Receiver, TryRecvError}};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(60);

enum Target { Id(i64), Path(String) }
enum Load {
    Card(i64),
    Reader(Receiver<Result<Vec<Target>, String>>),
    Tracks(Vec<Target>),
}

/// I keep the request alive until its ready ids have reached the picker.
pub struct FoundSelection {
    catalog: Arc<Catalog>,
    hold: SelectionHold,
    load: Load,
    started: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SelectionState {
    Pending,
    Ready(Vec<i64>),
    Failed(String),
}

fn failed() -> String { chilpa_i18n::t("The selected tracks could not be read.") }

impl FoundSelection {
    pub fn poll(&mut self) -> SelectionState {
        if self.started.elapsed() >= PATIENCE {
            return SelectionState::Failed(chilpa_i18n::t("Reading the selected tracks timed out. Please try again."));
        }
        if let Load::Reader(receiver) = &self.load {
            match receiver.try_recv() {
                Ok(Ok(tracks)) => {
                    self.hold.add_tracks(tracks.iter().filter_map(|track| match track { Target::Path(path) => Some(path.clone()), _ => None }));
                    let ids: Vec<_> = tracks.iter().filter_map(|track| match track { Target::Id(id) if *id < 0 => Some(*id), _ => None }).collect();
                    self.hold.add_tracks(self.catalog.tracks_by_ids(&ids).into_iter().map(|(track, _)| track.path));
                    self.load = Load::Tracks(tracks);
                }
                Ok(Err(error)) => return SelectionState::Failed(error),
                Err(TryRecvError::Disconnected) => return SelectionState::Failed(failed()),
                Err(TryRecvError::Empty) => return SelectionState::Pending,
            }
        }
        let ids = match &self.load {
            Load::Card(id) => {
                let Some(card) = self.catalog.card(*id) else { return SelectionState::Failed(failed()) };
                if card.metadata["available"] == false { return SelectionState::Failed(chilpa_i18n::t("This record cannot be played now.")); }
                match self.catalog.tracks(*id) {
                    Some(tracks) => tracks.into_iter().filter(|track| track.playable).map(|track| track.id).collect(),
                    None => return self.catalog.card_error(*id).map(SelectionState::Failed).unwrap_or(SelectionState::Pending),
                }
            }
            Load::Tracks(tracks) => {
                let mut ids = Vec::with_capacity(tracks.len());
                for target in tracks {
                    match target {
                        Target::Id(id) if *id != 0 => ids.push(*id),
                        Target::Path(path) => match self.catalog.track_by_path(path) {
                            Some((track, _)) if track.playable => ids.push(track.id),
                            Some(_) => return SelectionState::Failed(failed()),
                            None if crate::source_opener::of_plugin(path).is_some() => {
                                self.catalog.want_track(path);
                                return SelectionState::Pending;
                            }
                            None => return SelectionState::Failed(failed()),
                        },
                        _ => return SelectionState::Failed(failed()),
                    }
                }
                ids
            }
            Load::Reader(_) => return SelectionState::Pending,
        };
        if ids.is_empty() { SelectionState::Failed(chilpa_i18n::t("No playable tracks were found.")) }
        else { SelectionState::Ready(ids) }
    }
}

impl Chilpa {
    pub fn found_selection(&mut self, row: &FoundRow) -> Result<FoundSelection, String> {
        let catalog = self.catalog.clone();
        let (hold, load) = match row {
            FoundRow::Remote(row) => {
                if row.no_audio || row.id.is_empty() || row.source.is_empty() || row.item.as_ref().is_some_and(|item| item["available"] == false) {
                    return Err(failed());
                }
                if row.collection && !matches!(row.entity.as_str(), "" | "album" | "playlist") { return Err(failed()); }
                let identity = row.item.as_ref().and_then(|item| item["ref"].as_str()).map(str::to_string)
                    .unwrap_or_else(|| if row.collection { format!("handle:{}", row.id) } else { row.id.clone() });
                let cards = row.collection.then(|| (row.source.clone(), identity.clone()));
                let tracks = (!row.collection).then(|| crate::source_opener::entry_of(&row.source, &identity));
                let hold = catalog.hold(cards, tracks);
                let epoch = self.plugins.as_ref().and_then(|plugins| plugins.try_lock().ok().map(|plugins| plugins.epoch(&row.source)));
                let id = catalog.adopt(&row.source, epoch, row);
                if id == 0 { return Err(failed()); }
                let load = if row.collection { catalog.request_selection_tracks(id); Load::Card(id) }
                    else { Load::Tracks(vec![Target::Id(id)]) };
                (hold, load)
            }
            FoundRow::Track(track) => (catalog.hold([], [track.path.clone()]), Load::Tracks(vec![Target::Id(track.id)])),
            FoundRow::Album(_) | FoundRow::Artist(_) | FoundRow::Top(_) | FoundRow::Playlist(_) => {
                let (sender, receiver) = channel();
                let row = row.clone();
                let worker = self.query_worker().ok_or_else(failed)?;
                if !worker.submit(move |reader| { let _ = sender.send(reader.selection_targets(&row)); }) { return Err(failed()); }
                (catalog.hold([], []), Load::Reader(receiver))
            }
            _ => return Err(failed()),
        };
        Ok(FoundSelection { catalog, hold, load, started: Instant::now() })
    }

    pub fn hold_found_menu(&self, row: &FoundRow) -> SelectionHold {
        let mut cards = Vec::new();
        let mut tracks = Vec::new();
        match row {
            FoundRow::Remote(row) if row.collection => {
                let identity = row.item.as_ref().and_then(|item| item["ref"].as_str()).map(str::to_string).unwrap_or_else(|| format!("handle:{}", row.id));
                cards.push((row.source.clone(), identity));
            }
            FoundRow::Remote(row) => tracks.push(crate::source_opener::entry_of(&row.source, &row.id)),
            FoundRow::Track(track) => tracks.push(track.path.clone()),
            _ => {}
        }
        self.catalog.hold(cards, tracks)
    }

    pub fn hold_playlist_tracks(&self, ids: &[i64]) -> SelectionHold {
        let paths = self.catalog.tracks_by_ids(ids).into_iter().map(|(track, _)| track.path);
        self.catalog.hold([], paths)
    }
}

impl Reader {
    fn selection_targets(&self, row: &FoundRow) -> Result<Vec<Target>, String> {
        let filter = match row {
            FoundRow::Album(album) => crate::Filter { album: Some(album.id), ..Default::default() },
            FoundRow::Artist(artist) | FoundRow::Top(artist) => crate::Filter { performer: Some(artist.name.clone()), ..Default::default() },
            FoundRow::Playlist(playlist) => return self.lib.playlist_entries(playlist.id)
                .map(|entries| entries.into_iter().map(|entry| entry.track_id.map(Target::Id).unwrap_or(Target::Path(entry.external_id))).collect())
                .map_err(|_| failed()),
            _ => return Err(failed()),
        };
        let order = if filter.performer.is_some() { crate::TrackOrder::Artist } else { crate::TrackOrder::Disc };
        self.lib.browse_ids(&filter, order).map(|snapshot| snapshot.ids.into_iter().map(Target::Id).collect()).map_err(|_| failed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_library::Library;

    fn ids(targets: Vec<Target>) -> Vec<i64> {
        targets.into_iter().map(|target| match target { Target::Id(id) => id, Target::Path(_) => panic!("expected library id") }).collect()
    }

    #[test]
    fn a_selection_keeps_the_clicked_edition_and_compilation_performances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let lib = Library::open(&path).unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let server = lib.source_id("server", "home").unwrap();
        let first = lib.source_edition(local, "box", "Box", "Artist", Some(2000)).unwrap();
        let second = lib.source_edition(server, "box", "Box [Remastered]", "Artist", Some(2000)).unwrap();
        let compilation = lib.source_edition(local, "various", "Compilation", "Various Artists", Some(2001)).unwrap();
        for (id, album, source, artist) in [(11, first, local, "Artist"), (12, second, server, "Artist"), (13, compilation, local, "Artist"), (14, compilation, local, "Other")] {
            lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at)
                VALUES (?1, ?2, ?3, 'Song', ?4, ?5, 1)", rusqlite::params![id, source, format!("/music/{id}.flac"), artist, album]).unwrap();
        }
        lib.select_album_version(first, second).unwrap();
        let reader = Reader::open(&path).unwrap();
        let album = crate::Album { id:first, title:"Box".into(), artist:"Artist".into(), year:Some(2000), track_count:1, artwork_path:String::new(), play_count:0 };
        assert_eq!(ids(reader.selection_targets(&FoundRow::Album(album)).unwrap()), [11], "picking does not substitute the preferred edition");
        let artist = crate::Artist { name:"Artist".into(), album_count:2, track_count:3, artwork_path:String::new() };
        let selected = ids(reader.selection_targets(&FoundRow::Artist(artist)).unwrap());
        let expected = lib.browse_ids(&crate::Filter { performer:Some("Artist".into()), ..Default::default() }, crate::TrackOrder::Artist).unwrap().ids;
        assert_eq!(selected, expected, "the same performer query orders play and pick");
        assert!(selected.contains(&13), "a credited compilation performance is retained");
        assert!(!selected.contains(&14), "another performer's compilation track is excluded");
    }

    #[test]
    fn a_reader_selection_distinguishes_pending_failure_empty_and_timeout() {
        let catalog = Catalog::new();
        let (sender, receiver) = channel();
        let mut selection = FoundSelection { hold:catalog.hold([], []), catalog:catalog.clone(), load:Load::Reader(receiver), started:Instant::now() };
        assert_eq!(selection.poll(), SelectionState::Pending);
        sender.send(Err("Reader failed".into())).unwrap();
        assert_eq!(selection.poll(), SelectionState::Failed("Reader failed".into()));
        let (sender, receiver) = channel();
        selection.load = Load::Reader(receiver);
        sender.send(Ok(Vec::new())).unwrap();
        assert!(matches!(selection.poll(), SelectionState::Failed(_)), "an empty collection never opens a new-playlist form");
        selection.started = Instant::now() - PATIENCE;
        assert!(matches!(selection.poll(), SelectionState::Failed(error) if error.contains("timed out")));
    }
}
