//! Counts and at most four chosen artist portraits per visible group.
//! Stream one release at a time, retaining one (id, 61-bit membership) pair
//! per matched album. Version grouping holds only one parent's ids at a time;
//! no per-parent album sets or image cache survive this read.
//! The source pass sorts one borrowed pointer per resident index entry; the
//! request releases them. Live collection roots may exceed the pruning target.
use super::genres::Inventory;
use crate::Reader;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Summary { pub albums: usize, pub loose_tracks: usize, pub images: Vec<String> }

struct Counter {
    slots: HashMap<String, Vec<usize>>,
    rows: Vec<Summary>,
    last: Option<(bool, i64)>,
    seen: HashSet<usize>,
    mask: u64,
    memberships: Vec<(i64, u64)>,
    image: Option<Option<String>>,
}
impl Counter {
    fn new(names: &[String], inventory: &Inventory) -> Self {
        let mut slots = HashMap::<_, Vec<_>>::new();
        for (at, name) in names.iter().enumerate() {
            for tag in inventory.matching(name) { slots.entry(tag.to_lowercase()).or_default().push(at); }
        }
        assert!(names.len() <= 61);
        Self { slots, rows:names.iter().map(|_| Summary::default()).collect(), last:None, seen:HashSet::new(), mask:0, memberships:Vec::new(), image:None }
    }

    fn flush(&mut self) {
        if let Some((false, id)) = self.last {
            if self.mask != 0 { self.memberships.push((id, self.mask)); }
        }
        self.mask = 0;
    }

    fn accept(&mut self, loose: bool, release: i64, tag: &str, image: impl FnOnce() -> Option<String>) {
        if self.last != Some((loose, release)) { self.flush(); self.seen.clear(); self.image = None; self.last = Some((loose, release)); }
        let Some(slots) = self.slots.get(&tag.to_lowercase()) else { return };
        if self.image.is_none() && slots.iter().any(|at| self.rows[*at].images.len() < 4) {
            self.image = Some(image().filter(|image| !image.is_empty()));
        }
        let image = self.image.as_ref().and_then(Option::as_ref);
        for at in slots {
            let row = &mut self.rows[*at];
            if self.seen.insert(*at) {
                if loose { row.loose_tracks += 1; } else { self.mask |= 1 << at; }
            }
            if row.images.len() < 4 {
                if let Some(image) = image {
                    if !row.images.contains(image) { row.images.push(image.clone()); }
                }
            }
        }
    }
}

impl Reader {
    pub(super) fn genre_summaries(&self, names: &[String], inventory: &Inventory) -> Result<Vec<Summary>, String> {
        let mut counter = Counter::new(names, inventory);
        if counter.slots.is_empty() { return Ok(counter.rows); }
        let pictures = self.lib.artist_pictures().map_err(|error| error.to_string())?;
        self.lib.visit_genre_releases(|loose, id, tag, artist| {
            counter.accept(loose, id, tag, || pictures.of(artist).map(str::to_string));
        }).map_err(|error| error.to_string())?;
        // Index ids belong to the catalog; start a separate counting pass.
        counter.flush();
        counter.last = None;
        let (_, index) = self.source_index();
        let mut items: Vec<_> = index.items.iter().collect();
        items.sort_unstable_by_key(|item| (item.album == 0, if item.album != 0 { item.album } else { item.track }));
        for item in items {
            let loose = item.album == 0;
            let id = if loose { item.track } else { item.album };
            for tag in &item.genres {
                counter.accept(loose, id, tag, || pictures.of(&item.album_artist).map(str::to_string));
            }
        }
        counter.flush();
        let grouping = (!index.items.is_empty()).then(|| self.copy_grouping()).flatten();
        let versions = if counter.memberships.iter().any(|(id, _)| *id > 0) && self.lib.group_album_versions() {
            Some(self.lib.version_catalog().map_err(|error| error.to_string())?)
        } else { None };
        for (at, row) in counter.rows.iter_mut().enumerate() {
            let eligible = || counter.memberships.iter().filter(|(_, mask)| mask & (1 << at) != 0).map(|(id, _)| *id);
            let local: Vec<_> = eligible().filter(|id| *id > 0).collect();
            let mut ids = if let Some(versions) = &versions { versions.collapse(&local) } else { local };
            ids.extend(eligible().filter(|id| *id < 0));
            let dropped = grouping.as_ref().map(|(library, versions, chosen)| crate::library_sources::dropped_copies(&ids,
                crate::library_sources::Grouping { library, versions:*versions, chosen }, &index.versions)).unwrap_or_default();
            row.albums = ids.iter().filter(|id| !dropped.contains(id)).count();
        }
        Ok(counter.rows)
    }
}
