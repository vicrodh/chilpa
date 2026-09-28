//! I rebuild local releases from persisted part evidence, independently of scans.

mod parts;
#[cfg(test)]
mod migration;
pub(crate) use parts::location;
use super::Library;
use parts::{Track, collect, assemble};
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

impl Library {
    /// I never open media here. Running again with the same evidence makes no writes.
    pub fn assemble_local_releases(&self) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        let tracks = {
            let mut stmt = tx.prepare("SELECT t.id, t.album_id, t.source_id, t.external_id,
                CASE WHEN t.album_title = '' THEN a.title ELSE t.album_title END,
                t.artist, t.album_credit, t.disc_number, t.disc_total, t.present,
                CASE WHEN t.release_part = '' THEN coalesce(t.release_year, a.year) ELSE t.release_year END,
                CASE WHEN t.release_part = '' THEN coalesce(t.release_artwork, a.artwork_path) ELSE t.release_artwork END
                FROM tracks t JOIN albums a ON a.id = t.album_id JOIN sources s ON s.id = t.source_id
                WHERE s.provider = 'local' ORDER BY t.id")?;
            let rows = stmt.query_map([], |r| Ok(Track { id: r.get(0)?, album: r.get(1)?, source: r.get(2)?, path: r.get(3)?,
                title: r.get(4)?, artist: r.get(5)?, credit: r.get(6)?, disc: r.get(7)?, total: r.get(8)?,
                present: r.get(9)?, year: r.get(10)?, artwork: r.get(11)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?.into_iter()
                .filter(|t| t.path.starts_with("sacd:") || Path::new(&t.path).is_absolute()).collect();
            rows
        };
        let parts = collect(tracks);
        let releases = assemble(&parts);
        let mut used = BTreeSet::new();
        let mut moved = BTreeMap::<i64, BTreeSet<i64>>::new();
        let mut metadata = Vec::new();
        let mut changed = false;
        for ids in releases {
            let first = &parts[ids[0]];
            let key = format!("release:{}:{}:{}{}", first.source, first.title.len(), first.title, first.location);
            let old: BTreeSet<_> = ids.iter().flat_map(|i| parts[*i].tracks.iter().map(|t| t.album)).collect();
            let target = if let Some(id) = old.iter().find(|id| !used.contains(*id)) {
                *id
            } else {
                let next: i64 = tx.query_row("SELECT coalesce(max(id), 0) + 1 FROM albums", [], |r| r.get(0))?;
                tx.execute("INSERT INTO albums(title, album_artist, folder, version_key) VALUES (?1, ?2, ?3, ?4)",
                    params![first.title, first.artist, first.location, format!("pending:{next}:{key}")])?;
                changed = true;
                tx.last_insert_rowid()
            };
            used.insert(target);
            for old in old { moved.entry(old).or_default().insert(target); }
            let artwork = ids.iter().flat_map(|i| &parts[*i].tracks).find_map(|t| t.artwork.clone());
            let year = ids.iter().flat_map(|i| &parts[*i].tracks).find_map(|t| t.year);
            metadata.push((target, first.title.clone(), first.artist.clone(), first.location.clone(), key, artwork, year));
            for i in ids {
                let part = &parts[i];
                for track in &part.tracks {
                    changed |= tx.prepare_cached("UPDATE tracks SET album_id = ?2, release_part = ?3,
                        album_title = ?4, release_disc = ?5, release_year = ?6, release_artwork = ?7 WHERE id = ?1 AND
                        (album_id IS NOT ?2 OR release_part <> ?3 OR album_title <> ?4 OR release_disc IS NOT ?5
                         OR release_year IS NOT ?6 OR release_artwork IS NOT ?7)")?.execute(
                        params![track.id, target, part.location, track.title, part.folder_disc, track.year, track.artwork])? > 0;
                }
            }
        }
        // I copy marks before deleting fragments; track ids and their references never change.
        for (old, targets) in &moved {
            for target in targets {
                if old != target {
                    changed |= tx.execute("INSERT OR IGNORE INTO album_favorites(album_id, added_at)
                        SELECT ?2, added_at FROM album_favorites WHERE album_id = ?1", params![old, target])? > 0;
                }
            }
            if !targets.contains(old) {
                changed |= tx.execute("UPDATE album_preferences SET album_id = ?2 WHERE album_id = ?1",
                    params![old, targets.first()])? > 0;
            }
        }
        changed |= tx.execute("DELETE FROM albums WHERE NOT EXISTS (SELECT 1 FROM tracks WHERE album_id = albums.id)", [])? > 0;
        for (id, _, _, _, key, _, _) in &metadata {
            changed |= tx.prepare_cached("UPDATE albums SET version_key = ?2 WHERE id = ?1 AND version_key <> ?3")?.execute(
                params![id, format!("pending:{id}:{key}"), key])? > 0;
        }
        for (id, title, artist, folder, key, artwork, year) in metadata {
            changed |= tx.prepare_cached("INSERT OR IGNORE INTO album_preferences(group_key, album_id)
                SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM album_preferences WHERE album_id = ?2)")?.execute(
                params![super::versions::album_group(&title, &artist), id])? > 0;
            changed |= tx.prepare_cached("UPDATE albums SET title = ?2, album_artist = ?3, folder = ?4,
                version_key = ?5, artwork_path = ?6, year = ?7 WHERE id = ?1 AND
                (title <> ?2 OR album_artist <> ?3 OR folder <> ?4 OR version_key <> ?5
                 OR artwork_path IS NOT ?6 OR year IS NOT ?7)")?.execute(params![id, title, artist, folder, key, artwork, year])? > 0;
        }
        if changed { self.rebuild_rollups()?; }
        tx.finish()
    }
}
