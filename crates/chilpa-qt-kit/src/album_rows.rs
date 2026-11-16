//! I keep album snapshot pages bounded and read them on demand.
use crate::app;
use chilpa_api::Album;
use std::{cell::RefCell, rc::Rc};
const PAGE: usize = 128;
const PAGES_KEPT: usize = 24;
/// Limits visible source cards and their group headings together.
pub fn source_snapshot(key: &str, order: &str, group: &str, filter: &str, limit: i32) -> chilpa_api::SourceShelfSnapshot {
    let mut snapshot = app::with(|api| api.source_shelf_snapshot(key, order, group, filter));
    if limit > 0 {
        snapshot.albums.truncate(limit as usize);
        snapshot.groups.retain(|(at, _)| *at < snapshot.albums.len());
    }
    snapshot
}

/// A visible source card requests missing artwork even when its snapshot is cached.
pub fn artwork(album: &Album, visible: bool) -> cxx_qt_lib::QUrl {
    if album.id < 0 {
        crate::format::art_url(&app::with(|api| api.source_album_artwork(album.id, visible)))
    } else {
        crate::format::art_url(&album.artwork_path)
    }
}
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
    /// Keeps selection attached to album identity when a refreshed list moves.
    pub fn retained_index(&self, old: &Self, at: Option<i32>, selected_version: bool) -> Option<i32> {
        let id = old.with(usize::try_from(at?).ok()?, |album| album.id)?;
        let id = if selected_version { app::with(|api| api.selected_album_version(id)) } else { id };
        self.position(id).and_then(|at| i32::try_from(at).ok())
    }

    /// Resolves a click or range to album ids without reading unrelated pages.
    pub fn selection(&self, anchor: Option<i32>, index: i32, modifiers: i32) -> (Vec<i64>, bool, Option<i32>) {
        let range = modifiers & 2 != 0;
        let from = if range { anchor.unwrap_or(index) } else { index };
        let ids = (from.min(index)..=from.max(index))
            .filter_map(|at| usize::try_from(at).ok().and_then(|at| self.with(at, |album| album.id))).collect();
        (ids, !range, if range { anchor } else { Some(index) })
    }

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
