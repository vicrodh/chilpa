//! I keep album snapshot pages bounded and read them on demand.
use crate::app;
use chilpa_api::Album;
use std::{cell::RefCell, rc::Rc};
const PAGE: usize = 128;
const PAGES_KEPT: usize = 24;
pub enum Albums {
    Loaded(Vec<Album>),
    Snapshot { ids: Vec<i64>, pages: Rc<RefCell<chilpa_api::PageCache<Album>>> },
}
impl Default for Albums {
    fn default() -> Self {
        Albums::Loaded(Vec::new())
    }
}
impl Albums {
    pub fn mixed(&self) -> bool {
        match self {
            Albums::Loaded(v) => chilpa_api::mixes_sources(v.iter().map(|a| a.id)),
            Albums::Snapshot { ids, .. } => chilpa_api::mixes_sources(ids.iter().copied()),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Albums::Loaded(v) => v.len(),
            Albums::Snapshot { ids, .. } => ids.len(),
        }
    }
    pub fn position(&self, id: i64) -> Option<usize> {
        match self { Self::Loaded(rows) => rows.iter().position(|row| row.id == id), Self::Snapshot { ids, .. } => ids.iter().position(|row| *row == id) }
    }
    /// I retain old cards until structural notifications finish. At most 1024
    /// cards share this transaction's buffer; I drop its old prefix and retain
    /// at most 512 slots after publishing the edit.
    pub fn stage(&mut self, old: &mut Self, plan: Option<&chilpa_api::list_changes::Changes>) -> usize {
        if plan.is_none_or(|plan| plan.steps.is_empty()) { return 0; }
        if let (Self::Loaded(old), Self::Loaded(new)) = (old, self) {
            let offset = old.len(); old.reserve_exact(new.len()); old.append(new); std::mem::swap(old, new); offset
        } else { 0 }
    }
    pub fn finish(&mut self, offset: usize) {
        if let Self::Loaded(rows) = self { rows.drain(..offset); if offset > 0 { rows.shrink_to(chilpa_api::list_changes::MAX_ROWS); } }
    }
    /// Reads one bounded page by id on a miss; any page costs the same.
    pub fn with<R>(&self, index: usize, f: impl FnOnce(&Album) -> R) -> Option<R> {
        match self {
            Albums::Loaded(v) => v.get(index).map(f),
            Albums::Snapshot { ids, pages } => {
                if index >= ids.len() {
                    return None;
                }
                let page = index / PAGE;
                let rows = pages.borrow_mut().read(page, PAGES_KEPT, || {
                    let end = ((page + 1) * PAGE).min(ids.len());
                    app::with(|api| api.albums_by_ids(&ids[page * PAGE..end]))
                });
                rows.get(index % PAGE).map(f)
            }
        }
    }
}
