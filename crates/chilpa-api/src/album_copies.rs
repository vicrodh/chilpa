//! A record the library and the sources both keep, as one card: the sources
//! its badges show, the copies its page offers, and the one the listener
//! chose. `library_sources::versions` says which copies are one card.

use crate::album_versions::{AlbumSource, AlbumVersion};
use crate::library_sources::{Clusters, SourceVersion};
use crate::source_catalog::SourceRecord;
use crate::Chilpa;
use chilpa_library::store::versions::VersionCatalog;
use std::collections::HashMap;
use std::sync::Arc;

/// The library's editions, the catalog's revision and "Group album
/// versions": while they hold, the cards hold.
type Stamp = (usize, i32, bool);

/// Every copy the library and the integrated sources keep, gathered into
/// cards. Built on the first card that asks and kept until something it
/// was built from changes.
pub(crate) struct Index {
    stamp: Stamp,
    clusters: Clusters,
    library: Arc<VersionCatalog>,
    records: HashMap<i64, SourceRecord>,
}

#[derive(Default)]
pub(crate) struct Held {
    index: Option<Arc<Index>>,
    /// "Group same album from different sources", read once.
    on: Option<bool>,
}

impl Held {
    /// What was integrated changed: the cards are read again.
    pub(crate) fn forget(&mut self) {
        self.index = None;
    }
}

impl Chilpa {
    pub fn group_source_copies(&mut self) -> bool {
        *self.copies.on.get_or_insert_with(crate::library_sources::grouping_sources)
    }

    pub fn set_group_source_copies(&mut self, on: bool) {
        self.remember_preference(crate::library_sources::GROUP_SOURCES, if on { "true" } else { "false" });
        self.copies = Held { index: None, on: Some(on) };
    }

    fn copy_index(&mut self) -> Option<Arc<Index>> {
        if !self.group_source_copies() { return None; }
        let lib = self.library()?;
        let library = lib.version_catalog().ok()?;
        let versions = lib.group_album_versions();
        let stamp = (Arc::as_ptr(&library) as usize, self.catalog.revisions().0, versions);
        if let Some(index) = self.copies.index.as_ref().filter(|index| index.stamp == stamp) {
            return Some(index.clone());
        }
        let integrated = crate::library_sources::integrated_now(&self.natives);
        let records: HashMap<i64, SourceRecord> = self.catalog.integrated_records(&integrated).into_iter().map(|r| (r.id, r)).collect();
        let sources: HashMap<i64, SourceVersion> = records.iter().map(|(id, record)| (*id, record.version())).collect();
        let copies = crate::library_sources::copies(library.versions.keys().copied().chain(sources.keys().copied()), &library, &sources);
        let clusters = Clusters::build(&copies, versions);
        drop(copies);
        let index = Arc::new(Index { stamp, clusters, library, records });
        self.copies.index = Some(index.clone());
        Some(index)
    }

    /// `own` and the sources of every other copy on the card of `id`.
    pub(crate) fn copy_sources(&mut self, id: i64, mut own: Vec<AlbumSource>) -> Vec<AlbumSource> {
        let Some(index) = self.copy_index() else { return own };
        for member in index.clusters.members_of(id).iter().copied().filter(|m| *m != id) {
            let source = if member > 0 { index.library.versions.get(&member).map(AlbumSource::of) }
                else { index.records.get(&member).map(|record| self.source_of(&record.plugin)) };
            if let Some(source) = source.filter(|s| !own.iter().any(|held| held.key == s.key)) { own.push(source); }
        }
        own
    }

    /// `versions` and every other copy on the card of `id`, which its page
    /// offers to choose.
    pub fn album_copies(&mut self, id: i64, mut versions: Vec<AlbumVersion>) -> Vec<AlbumVersion> {
        let Some(index) = self.copy_index() else { return versions };
        for member in index.clusters.members_of(id).iter().copied() {
            if versions.iter().any(|v| v.id == member) { continue; }
            if member > 0 {
                versions.extend(index.library.versions.get(&member).cloned().map(|v| crate::album_versions::version_labels(vec![v])).into_iter().flatten());
            } else if let Some(record) = index.records.get(&member) {
                let source = self.source_of(&record.plugin);
                let join = |a: &str, b: &str| if b.is_empty() { a.to_string() } else { format!("{a} · {b}") };
                versions.push(AlbumVersion { id: member, label: join(&source.label, &record.quality.long()),
                    brief: join(&source.label, &record.quality.brief()), detail: record.title.clone(), source });
            }
        }
        versions
    }

    /// Remembers which copy of a record its card shows: a source's, or with
    /// one of the library's, none of the sources'.
    pub fn choose_album_copy(&mut self, version: i64) {
        let Some(index) = self.copy_index() else { return };
        let key = |record: &SourceRecord| SourceVersion::key_of(&record.plugin, &record.identity);
        let mut chosen = crate::library_sources::chosen_copies();
        for member in index.clusters.members_of(version) {
            if let Some(record) = index.records.get(member) { chosen.remove(&key(record)); }
        }
        chosen.extend(index.records.get(&version).map(key));
        let mut chosen: Vec<String> = chosen.into_iter().collect();
        chosen.sort();
        self.remember_preference(crate::library_sources::CHOSEN_COPIES, &serde_json::to_string(&chosen).unwrap_or_default());
    }
}
