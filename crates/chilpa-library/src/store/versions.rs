//! Physical editions stay separate. Groups and a remembered choice only
//! affect presentation; track identities and playlists never change.

use super::Library;
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AlbumVersion {
    pub id: i64,
    pub group: String,
    pub title: String,
    pub artist: String,
    pub location: String,
    pub source: String,
    pub source_label: String,
    pub source_icon: String,
    pub local: bool,
    pub path: String,
    pub bits: Option<u8>,
    pub rate: Option<u32>,
    /// What says its format: a file's extension, a server track's codec.
    pub codec: String,
    pub tracks: usize,
    /// Some track of it can be played now. An edition whose folder is not
    /// mounted or whose server does not answer is offered dimmed and never
    /// chosen for me while another one can play (ADR-017).
    pub reachable: bool,
    rank: (u8, u8, u32, usize, bool),
}

pub(crate) struct CachedVersions {
    stamp: i64,
    catalog: Arc<VersionCatalog>,
}

/// What a remembered choice's group key starts with when it marks a record
/// kept apart from the other editions of its name. Such a record is a group
/// of its own, and the row says its edition is itself; being a remembered
/// choice, it follows the record when a scan renumbers it and goes with it.
pub(crate) const APART: &str = "apart:";

#[derive(Default)]
pub struct VersionCatalog {
    pub versions: HashMap<i64, AlbumVersion>,
    pub artists: HashMap<i64, Vec<(String, usize)>>,
    pub(crate) tracks: HashMap<i64, (i64, u64)>,
    groups: HashMap<String, Vec<i64>>,
    /// Every edition under the group its name gives, kept apart or not: a
    /// link asks for a record by name.
    named: HashMap<String, Vec<i64>>,
    preferred: HashMap<String, i64>,
}

/// Normalize edition annotations, keeping meaningful subtitles, live dates
/// and numbers that distinguish records (including an album named 1984).
pub fn album_group(title: &str, artist: &str) -> String {
    fn words(text: &str) -> String {
        text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
    }
    fn annotation(text: &str) -> bool {
        let lower = words(text);
        if lower.split_whitespace().any(|w| matches!(w, "live" | "concert" | "remix" | "remixes")) { return false; }
        lower.split_whitespace().any(|w| matches!(w,
            "remaster" | "remastered" | "remastering" | "deluxe" | "expanded" |
            "anniversary" | "edition" | "reissue" | "bonus" | "mono" | "stereo" |
            "sacd" | "dsd" | "dsf" | "dff" | "vinyl" | "flac" | "alac" | "mp3" | "bit" | "hires"))
    }
    let mut base = title.trim().to_string();
    loop {
        let mut changed = false;
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if let Some(start) = base.find(open) {
                if let Some(end) = base[start..].find(close).map(|end| start + end) {
                    if annotation(&base[start + 1..end]) {
                        base.replace_range(start..=end, " ");
                        changed = true;
                        break;
                    }
                }
            }
        }
        if !changed { break; }
    }
    for separator in [" - ", " – ", " — "] {
        if let Some((head, tail)) = base.rsplit_once(separator) {
            if annotation(tail) { base = head.to_string(); }
        }
    }
    let mut name = words(&base);
    let tokens: Vec<&str> = name.split_whitespace().collect();
    if tokens.len() > 1 && tokens[0].len() == 4 && tokens[0].parse::<u32>().is_ok_and(|year| (1900..=2099).contains(&year)) {
        name = tokens[1..].join(" ");
    }
    let credit = words(artist);
    if let Some(tail) = name.strip_prefix(&format!("{credit} ")) {
        if !tail.is_empty() { name = tail.to_string(); }
    }
    if name.is_empty() { name = words(title); }
    if let Some(tail) = name.strip_prefix("the ") { name = tail.to_string(); }
    format!("{}:{credit}{name}", credit.len())
}

/// I identify only the physical part here; evidence assembles its release.
pub fn local_location(path: &str) -> String {
    super::releases::location(path)
}

impl AlbumVersion {
    /// Its tier as a grid ranks it: 4 DSD, 3 hi-res, 2 lossless, 1 lossy.
    pub fn tier(&self) -> u8 {
        self.rank.0
    }
}

impl VersionCatalog {
    pub fn choices(&self, id: i64) -> Vec<AlbumVersion> {
        self.versions.get(&id).and_then(|v| self.groups.get(&v.group))
            .into_iter().flatten().filter_map(|id| self.versions.get(id).cloned()).collect()
    }

    pub fn selected(&self, id: i64) -> i64 {
        let Some(version) = self.versions.get(&id) else { return id };
        let ids = self.groups.get(&version.group).map(Vec::as_slice).unwrap_or(&[]);
        self.pick(&version.group, ids).unwrap_or(id)
    }

    /// The best edition the library has of a record named `title` by
    /// `artist`: the first of its group, which is the best one that can
    /// play. A remembered choice does not count here: a link asks for the
    /// record, and the record sounds best in this edition.
    pub fn best_of(&self, title: &str, artist: &str) -> Option<i64> {
        self.named.get(&album_group(title, artist))?.first().copied()
    }

    /// The remembered edition while it can play, or while nothing else can;
    /// otherwise the best one that can. `ids` come in the group's order,
    /// which puts what can play first.
    fn pick(&self, group: &str, ids: &[i64]) -> Option<i64> {
        let playable = |id: &i64| self.versions.get(id).is_some_and(|v| v.reachable);
        let preferred = self.preferred.get(group).filter(|id| ids.contains(id)).copied();
        match preferred {
            Some(id) if playable(&id) || !ids.iter().any(playable) => Some(id),
            _ => ids.first().copied(),
        }
    }

    /// Pick only among eligible editions, so excluding a source or quality
    /// cannot bring it back through a group's preferred version.
    pub fn collapse(&self, ids: &[i64]) -> Vec<i64> {
        let eligible: HashSet<i64> = ids.iter().copied().collect();
        let mut seen = HashSet::new();
        ids.iter().filter_map(|id| {
            let Some(v) = self.versions.get(id) else { return Some(*id) };
            if !seen.insert(&v.group) { return None; }
            let candidates: Vec<i64> = self.groups[&v.group].iter().filter(|id| eligible.contains(id)).copied().collect();
            self.pick(&v.group, &candidates)
        }).collect()
    }
}

impl Library {
    /// Adapter-owned presentation, independent of a provider's protocol.
    pub fn set_source_icon(&self, source: i64, icon: &str) -> rusqlite::Result<()> {
        self.db.execute("UPDATE sources SET icon = ?2 WHERE id = ?1", params![source, icon])?;
        Ok(())
    }

    pub fn grouped_snapshot(&self, filter: &super::Filter, order: super::TrackOrder) -> rusqlite::Result<super::Snapshot> {
        if !self.group_album_versions() { return self.browse_ids(filter, order); }
        let catalog = self.version_catalog()?;
        let mut filter = filter.clone();
        let mut albums = filter.albums.clone();
        albums.extend(filter.album);
        if !albums.is_empty() {
            filter.album = None;
            filter.albums = albums.iter().flat_map(|id| catalog.choices(*id)).map(|v| v.id).collect();
            if filter.albums.is_empty() { return Ok(super::Snapshot::default()); }
        }
        let mut snapshot = self.browse_ids(&filter, order)?;
        snapshot.album_ids = catalog.collapse(&snapshot.album_ids);
        let chosen: HashSet<i64> = snapshot.album_ids.iter().copied().collect();
        let mut removed_ms = 0;
        snapshot.ids.retain(|id| match catalog.tracks.get(id) {
            Some((album, duration)) if !chosen.contains(album) => { removed_ms += duration; false }
            _ => true,
        });
        snapshot.totals.albums = chosen.len();
        snapshot.totals.tracks = snapshot.ids.len();
        snapshot.totals.duration_ms = snapshot.totals.duration_ms.saturating_sub(removed_ms);
        Ok(snapshot)
    }

    pub fn group_album_versions(&self) -> bool {
        self.db.query_row("SELECT grouped FROM album_presentation WHERE id = 1", [], |r| r.get(0)).unwrap_or(true)
    }

    pub fn group_artist_versions(&self) -> bool {
        self.db.query_row("SELECT artist_grouped FROM album_presentation WHERE id = 1", [], |r| r.get(0)).unwrap_or(true)
    }

    pub fn set_group_artist_versions(&self, on: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE album_presentation SET artist_grouped = ?1 WHERE id = 1", [on])?;
        Ok(())
    }

    pub fn set_group_album_versions(&self, on: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE album_presentation SET grouped = ?1 WHERE id = 1", [on])?;
        Ok(())
    }

    pub fn select_album_version(&self, album: i64, version: i64) -> rusqlite::Result<bool> {
        let catalog = self.version_catalog()?;
        let Some(a) = catalog.versions.get(&album) else { return Ok(false) };
        if !catalog.versions.get(&version).is_some_and(|v| v.group == a.group) { return Ok(false); }
        self.db.execute("INSERT INTO album_preferences(group_key, album_id) VALUES (?1, ?2)
            ON CONFLICT(group_key) DO UPDATE SET album_id = excluded.album_id", params![a.group, version])?;
        Ok(true)
    }

    pub fn selected_album(&self, album: i64) -> i64 {
        if !self.group_album_versions() { return album; }
        self.version_catalog().map(|c| c.selected(album)).unwrap_or(album)
    }

    pub fn grouped_album_ids(&self, ids: &[i64]) -> rusqlite::Result<Vec<i64>> {
        if !self.group_album_versions() { return Ok(ids.to_vec()); }
        Ok(self.version_catalog()?.collapse(ids))
    }

    /// Whether a record is kept apart from the other editions of its name:
    /// `Some(true)` kept apart, `Some(false)` grouped with another, `None`
    /// when it has none to be grouped with.
    pub fn album_apart(&self, album: i64) -> rusqlite::Result<Option<bool>> {
        let catalog = self.version_catalog()?;
        let Some(version) = catalog.versions.get(&album) else { return Ok(None) };
        if version.group.starts_with(APART) { return Ok(Some(true)); }
        Ok((catalog.groups.get(&version.group).map_or(0, Vec::len) > 1).then_some(false))
    }

    /// Keeps a record apart from the other editions of its name, or groups
    /// it with them again. Returns whether anything changed.
    pub fn set_album_apart(&self, album: i64, apart: bool) -> rusqlite::Result<bool> {
        let marked = format!("album_id = ?1 AND group_key LIKE '{APART}%'");
        if !apart {
            return Ok(self.db.execute(&format!("DELETE FROM album_preferences WHERE {marked}"), [album])? > 0);
        }
        if self.db.query_row(&format!("SELECT count(*) FROM album_preferences WHERE {marked}"), [album], |r| r.get::<_, i64>(0))? > 0 {
            return Ok(false);
        }
        // A record a scan renumbered keeps the key it was marked with, which
        // may be this record's id.
        let taken = |key: &str| self.db.query_row("SELECT count(*) FROM album_preferences WHERE group_key = ?1", [key], |r| r.get::<_, i64>(0));
        let mut key = format!("{APART}{album}");
        let mut again = 0;
        while taken(&key)? > 0 {
            again += 1;
            key = format!("{APART}{album}.{again}");
        }
        self.db.execute("INSERT INTO album_preferences(group_key, album_id) VALUES (?1, ?2)", params![key, album])?;
        Ok(true)
    }

    /// I rebuild only when catalog inputs change, shared by every read on a connection.
    /// Scroll-page reads never scan the library again.
    pub fn version_catalog(&self) -> rusqlite::Result<Arc<VersionCatalog>> {
        let stamp = self.db.query_row("SELECT revision FROM version_catalog_revision WHERE id = 1", [], |r| r.get(0))?;
        let cacheable = self.db.is_autocommit() || self.db.is_readonly("main")?;
        if let Some(cache) = self.versions.borrow().as_ref().filter(|cache| cacheable && cache.stamp == stamp) {
            return Ok(Arc::clone(&cache.catalog));
        }
        let mut catalog = VersionCatalog::default();
        let apart: HashSet<i64> = self.db.prepare(&format!("SELECT album_id FROM album_preferences WHERE group_key LIKE '{APART}%'"))?
            .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.db.prepare("SELECT a.id, a.title, a.album_artist, a.folder,
            s.provider || ':' || s.instance, t.external_id, t.bit_depth, t.sample_rate_hz, t.id, t.duration_ms,
            s.label, s.icon, s.provider = 'local', t.reachable, coalesce(t.codec, ''), s.kind = 'server'
            FROM albums a JOIN tracks t ON t.album_id = a.id JOIN sources s ON s.id = t.source_id
            WHERE t.present = 1 AND s.enabled = 1 ORDER BY a.id, t.id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id = row.get(0)?;
            catalog.tracks.insert(row.get(8)?, (id, row.get::<_, i64>(9)?.max(0) as u64));
            let title: String = row.get(1)?;
            let path: String = row.get(5)?;
            let bits: Option<u8> = row.get(6)?;
            let rate: Option<u32> = row.get(7)?;
            let location: String = row.get(3)?;
            // A server's track has an id, not a file name: its codec says
            // what the extension would have.
            let ext = Path::new(&path).extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            let ext = if row.get::<_, bool>(15)? { row.get::<_, String>(14)?.to_lowercase() } else { ext };
            let reachable = row.get::<_, i64>(13)? == 1;
            let dsd = path.starts_with("sacd:") || matches!(ext.as_str(), "dsf" | "dff" | "dsd") || rate.is_some_and(|r| r >= 2_822_400);
            let lossy = matches!(ext.as_str(), "mp3" | "ogg" | "oga" | "opus" | "aac" | "vorbis" | "wma") || (matches!(ext.as_str(), "m4a" | "mp4") && bits.is_none());
            let tier = if dsd { 4 } else if lossy { 1 } else if bits.is_some_and(|b| b >= 24) || rate.is_some_and(|r| r > 48_000) { 3 } else if bits.is_some() { 2 } else { 0 };
            let quality = (tier, if lossy || dsd { 0 } else { bits.unwrap_or(0) }, rate.unwrap_or(0));
            let edition = format!("{title} {location}").to_lowercase();
            let enhanced = ["remaster", "deluxe", "expanded", "anniversary", "reissue"].iter().any(|word| edition.contains(word));
            let v = catalog.versions.entry(id).or_insert_with(|| AlbumVersion {
                id, group: album_group(&title, &row.get::<_, String>(2).unwrap_or_default()),
                rank: (quality.0, quality.1, quality.2, 0, enhanced),
                title, artist: row.get(2).unwrap_or_default(), location, source: row.get(4).unwrap_or_default(),
                source_label: row.get(10).unwrap_or_default(), source_icon: row.get(11).unwrap_or_default(), local: row.get(12).unwrap_or(false),
                path: path.clone(), bits, rate, codec: ext.clone(), tracks: 0, reachable: false,
            });
            v.tracks += 1;
            v.reachable |= reachable;
            if quality < (v.rank.0, v.rank.1, v.rank.2) {
                v.rank.0 = quality.0; v.rank.1 = quality.1; v.rank.2 = quality.2;
                v.bits = bits; v.rate = rate; v.path = path; v.codec = ext;
            }
            v.rank.3 = v.tracks;
        }
        for version in catalog.versions.values_mut() {
            catalog.named.entry(version.group.clone()).or_default().push(version.id);
            if apart.contains(&version.id) { version.group = format!("{APART}{}", version.id); }
            catalog.groups.entry(version.group.clone()).or_default().push(version.id);
        }
        for ids in catalog.groups.values_mut().chain(catalog.named.values_mut()) {
            ids.sort_by_key(|id| (!catalog.versions[id].reachable, std::cmp::Reverse(catalog.versions[id].rank), *id));
        }
        let mut stmt = self.db.prepare("SELECT group_key, album_id FROM album_preferences")?;
        catalog.preferred = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.db.prepare("SELECT album_id, artist, tracks FROM album_artist_rollup")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as usize)))? {
            let (album, artist, tracks) = row?;
            catalog.artists.entry(album).or_default().push((artist, tracks));
        }
        let catalog = Arc::new(catalog);
        if cacheable { *self.versions.borrow_mut() = Some(CachedVersions { stamp, catalog: Arc::clone(&catalog) }); }
        Ok(catalog)
    }

    /// I retain this entry point for callers upgrading the old identity.
    pub fn separate_local_editions(&self) -> rusqlite::Result<()> {
        self.assemble_local_releases()
    }

    pub(crate) fn local_edition(&self, source: i64, title: &str, artist: &str, location: &str, year: Option<u32>) -> rusqlite::Result<i64> {
        let key = format!("{source}:{location}");
        let found: Option<i64> = self.db.query_row("SELECT id FROM albums WHERE title = ?1 AND version_key = ?2", params![title, key], |r| r.get(0)).optional()?;
        if let Some(id) = found {
            self.db.execute("UPDATE albums SET album_artist = ?3 WHERE id = ?1 AND album_artist <> ?2 COLLATE NOCASE", params![id, artist, crate::VARIOUS_ARTISTS])?;
            return Ok(id);
        }
        self.db.execute("INSERT INTO albums(title, album_artist, year, folder, version_key) VALUES (?1, ?2, ?3, ?4, ?5)", params![title, artist, year, location, key])?;
        Ok(self.db.last_insert_rowid())
    }

    /// Adapters provide a stable release id scoped to their source instance.
    /// Grouping compares metadata across sources, never opaque release ids.
    pub fn source_edition(&self, source: i64, release_id: &str, title: &str, artist: &str, year: Option<u32>) -> rusqlite::Result<i64> {
        let key = format!("{source}:{release_id}");
        let found: Option<i64> = self.db.query_row("SELECT id FROM albums WHERE version_key = ?1", [&key], |r| r.get(0)).optional()?;
        if let Some(id) = found {
            self.db.execute("UPDATE albums SET title = ?2, album_artist = ?3, year = ?4 WHERE id = ?1", params![id, title, artist, year])?;
            return Ok(id);
        }
        self.local_edition(source, title, artist, release_id, year)
    }

    pub fn presented_artists(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<super::StoredArtist>> {
        let mut artists = self.artists(limit, offset)?;
        if !self.group_album_versions() { return Ok(artists); }
        let catalog = self.version_catalog()?;
        for artist in &mut artists {
            let contributions: HashMap<i64, usize> = catalog.artists.iter().filter_map(|(id, members)|
                members.iter().find(|(name, _)| name.eq_ignore_ascii_case(&artist.name)).map(|(_, tracks)| (*id, *tracks))).collect();
            let ids: Vec<i64> = contributions.keys().copied().collect();
            let chosen = catalog.collapse(&ids);
            artist.album_count = chosen.len();
            artist.track_count = chosen.iter().map(|id| contributions[id]).sum();
        }
        Ok(artists)
    }
}
