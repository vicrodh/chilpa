//! Track snapshot storage, shared with bridges without owning a Qt object.
use crate::app;
use chilpa_api::{TrackEntry as Entry, TrackRow};
use std::{cell::RefCell, rc::Rc};

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
