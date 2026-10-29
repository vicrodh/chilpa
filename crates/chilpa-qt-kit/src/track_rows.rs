//! Track snapshot storage, shared with bridges without owning a Qt object.
use crate::app;
use chilpa_api::{TrackEntry as Entry, TrackRow};
use std::{cell::RefCell, rc::Rc};

// Each model retains at most 32 pages of 128 tracks, released with the model.
const PAGE: usize = 128;
const PAGES_KEPT: usize = 32;

pub enum Rows {
    Loaded(Vec<Entry>),
    Snapshot { ids: Rc<chilpa_api::Snapshot>, pages: Rc<RefCell<chilpa_api::PageCache<TrackRow>>> },
}

impl Default for Rows {
    fn default() -> Self { Self::Loaded(Vec::new()) }
}

pub enum At<'a> {
    Header(&'a chilpa_api::GroupHeader),
    Track(Rc<Vec<TrackRow>>, usize),
    Loaded(&'a TrackRow),
}

impl Rows {
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
                    app::with(|api| api.track_rows(&ids.ids[page * PAGE..end]))
                });
                (row % PAGE < rows.len()).then(|| At::Track(rows, row % PAGE))
            }
        }
    }
}
