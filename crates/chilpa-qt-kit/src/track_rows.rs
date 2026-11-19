//! Track snapshot storage, shared with bridges without owning a Qt object.
use crate::app;
use chilpa_api::{TrackEntry as Entry, TrackRow};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

// Each model retains at most 32 pages of 128 tracks, released with the model.
const PAGE: usize = 128;
const PAGES_KEPT: usize = 32;

pub enum Rows {
    Loaded(Vec<Entry>),
    Snapshot { ids: Rc<chilpa_api::Snapshot>, pages: Rc<RefCell<chilpa_api::PageCache<Option<TrackRow>>>> },
}

impl Default for Rows {
    fn default() -> Self { Self::Loaded(Vec::new()) }
}

pub enum At<'a> {
    Header(&'a chilpa_api::GroupHeader),
    Track(Rc<Vec<Option<TrackRow>>>, usize),
    Loaded(&'a TrackRow),
    Missing,
}

impl Rows {
    /// Loaded rows only; snapshots keep their bounded page cache.
    pub fn loaded_tracks(&self) -> Vec<&TrackRow> {
        match self {
            Self::Loaded(rows) => rows.iter().filter_map(|entry| if let Entry::Track(track) = entry { Some(track) } else { None }).collect(),
            Self::Snapshot { .. } => Vec::new(),
        }
    }

    /// The first row that lists that id, found without reading a snapshot's pages.
    pub fn row_of(&self, id: i64) -> Option<usize> {
        match self {
            Self::Loaded(rows) => rows.iter().position(|entry| matches!(entry, Entry::Track(track) if track.id == id)),
            Self::Snapshot { ids, .. } => ids.ids.iter().position(|listed| *listed == id),
        }
    }

    /// Drops from a selection the ids these rows no longer list.
    pub fn keep_listed(&self, selected: &mut HashSet<i64>) {
        if selected.is_empty() { return; }
        let listed: HashSet<i64> = match self {
            Self::Loaded(_) => self.loaded_tracks().iter().map(|track| track.id).collect(),
            Self::Snapshot { ids, .. } => ids.ids.iter().copied().collect(),
        };
        selected.retain(|id| listed.contains(id));
    }

    pub fn len(&self) -> usize {
        match self { Self::Loaded(rows) => rows.len(), Self::Snapshot { ids, .. } => ids.ids.len() }
    }

    pub fn at(&self, row: usize) -> Option<At<'_>> {
        match self {
            Self::Loaded(rows) => match rows.get(row)? {
                Entry::Header(header) => Some(At::Header(header)),
                Entry::Track(track) => Some(At::Loaded(track)),
            },
            Self::Snapshot { ids, pages } => {
                let page = row / PAGE;
                let rows = pages.borrow_mut().read(page, PAGES_KEPT, || {
                    let end = ((page + 1) * PAGE).min(ids.ids.len());
                    let started = std::env::var_os("CHILPA_TRACK_TIMINGS").map(|_| std::time::Instant::now());
                    let rows = app::with(|api| api.track_rows(&ids.ids[page * PAGE..end]));
                    if let Some(started) = started { eprintln!("tracks hydrate_page={} rows={} elapsed_us={}", page, rows.len(), started.elapsed().as_micros()); }
                    rows
                });
                match rows.get(row % PAGE) {
                    Some(Some(_)) => Some(At::Track(rows, row % PAGE)),
                    Some(None) => Some(At::Missing),
                    None => None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64) -> TrackRow {
        TrackRow { id, path: format!("source:{id}"), title: "Source track".into(), artist: String::new(), album: String::new(), album_id: 0,
            disc: 0, number: 0, duration_ms: 0, plays: 0, quality: Default::default(), bit_depth: None, artwork_path: String::new(),
            available: true, composer: String::new(), disc_subtitle: String::new(), work: String::new(), composer_target: String::new(),
            groups: None, explicit: None, ai_generated: None, credits: None }
    }

    #[test]
    fn snapshot_pages_keep_holes_and_negative_source_ids_at_their_positions() {
        let ids = Rc::new(chilpa_api::Snapshot { ids: (0..129).map(|at| if at == 126 { -9 } else { at as i64 + 1 }).collect(), ..Default::default() });
        let mut cache = chilpa_api::PageCache::default();
        let mut first = vec![None; PAGE];
        first[126] = Some(track(-9));
        cache.insert(0, first);
        cache.insert(1, vec![None]);
        let rows = Rows::Snapshot { ids, pages: Rc::new(RefCell::new(cache)) };

        assert!(matches!(rows.at(0), Some(At::Missing)));
        assert!(matches!(rows.at(127), Some(At::Missing)));
        assert!(matches!(rows.at(128), Some(At::Missing)));
        assert!(matches!(rows.at(126), Some(At::Track(page, 126)) if page[126].as_ref().is_some_and(|row| row.id == -9)));
    }

    #[test]
    fn a_refresh_keeps_only_listed_selections_and_finds_rows_by_id() {
        let ids = Rc::new(chilpa_api::Snapshot { ids: vec![5, -9, 7], ..Default::default() });
        let snapshot = Rows::Snapshot { ids, pages: Rc::default() };
        let mut selected: HashSet<i64> = [7, 8, -9].into();
        snapshot.keep_listed(&mut selected);
        assert_eq!(selected, [7, -9].into(), "an id the new snapshot no longer lists leaves the selection");
        assert_eq!((snapshot.row_of(7), snapshot.row_of(-9), snapshot.row_of(8)), (Some(2), Some(1), None));
        let header = chilpa_api::GroupHeader { label: "Disc 1".into(), ..Default::default() };
        let loaded = Rows::Loaded(vec![Entry::Header(header), Entry::Track(track(3)), Entry::Track(track(4))]);
        let mut selected: HashSet<i64> = [4, 5].into();
        loaded.keep_listed(&mut selected);
        assert_eq!(selected, [4].into());
        assert_eq!((loaded.row_of(4), loaded.row_of(0)), (Some(2), None), "a header is not a track's row");
    }
}
