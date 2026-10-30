//! What is laid over a server's records without touching the server.
//!
//! The document is the sidecar's (`sidecar::document`): it carries only
//! what somebody changed, and a field it names as null is empty. It is kept
//! here and not beside the music because a record on a server has no folder
//! on this machine. A sync rewrites the mirror from what the server says
//! and lays the overlay back over it, so a field changed here wins and a
//! field left alone follows the server. A record the server stops listing
//! keeps its overlay: it is counted, never taken out.

use crate::sidecar::{Album, Home, Override, Scope, Sidecar, Track, VERSION};
use crate::store::Library;
use chilpa_models::{RemoteEntry, SourceId};
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// What writing an overlay left behind.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredOverlay {
    /// Where the record's cover is, when the overlay names one.
    pub cover: Option<PathBuf>,
    /// The tracks the overlay was laid over.
    pub tracks: Vec<i64>,
}

/// The overlays of one source, by what they go under.
type Overlays = HashMap<(Scope, String), Sidecar>;

fn parts(home: &Home) -> Option<(i64, Scope, &str)> {
    match home {
        Home::Hosted { source_id, scope, key } => Some((*source_id, *scope, key)),
        _ => None,
    }
}

/// The year a date starts with, as the library keeps it.
fn year_of(date: &str) -> Option<i32> {
    let head = date.get(..4)?;
    head.bytes().all(|b| b.is_ascii_digit()).then(|| head.parse().ok()).flatten()
}

/// A text override as a column takes it: cleared is empty.
fn text(over: &Override<String>) -> Option<String> {
    match over {
        Override::Absent => None,
        Override::Cleared => Some(String::new()),
        Override::Value(v) => Some(v.clone()),
    }
}

/// A text override as a nullable column takes it.
fn nullable(over: &Override<String>) -> Option<Option<String>> {
    match over {
        Override::Absent => None,
        Override::Cleared => Some(None),
        Override::Value(v) => Some(Some(v.clone())),
    }
}

fn number(over: &Override<u32>) -> Option<Option<u16>> {
    match over {
        Override::Absent => None,
        Override::Cleared => Some(None),
        // A number that does not fit the column is not one.
        Override::Value(n) => u16::try_from(*n).ok().map(Some),
    }
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// A server's id as a file name: letters, digits, dot, dash and underscore
/// as they are; every other byte as `%XX`, so a slash or a space in it
/// cannot leave the folder.
fn file_safe(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    // A name that is no file of its own.
    if matches!(out.as_str(), "" | "." | "..") {
        out.insert(0, '_');
    }
    out
}

impl Library {
    /// Where the covers chosen here for a server's records are kept: in the
    /// folder the library's file is in, never beside anything of the
    /// server's, and taken out with the server. `None` for a library that
    /// is not a file.
    pub fn overlay_covers_dir(&self, source_id: i64) -> Option<PathBuf> {
        let db = self.db.path().filter(|path| !path.is_empty())?;
        Some(Path::new(db).parent()?.join("source-covers").join(source_id.to_string()))
    }

    /// Where a record's cover goes, by its kind: "jpg" or "png". A track
    /// without a record has no cover of its own here.
    fn overlay_cover_at(&self, home: &Home, extension: &str) -> Option<PathBuf> {
        let Home::Hosted { source_id, scope: Scope::Album, key } = home else { return None };
        Some(self.overlay_covers_dir(*source_id)?.join(format!("{}.{extension}", file_safe(key))))
    }

    /// The overlay in force for a home on a server. `None` when nothing
    /// was laid over it; `Err` says why it could not be read.
    pub fn overlay(&self, home: &Home) -> Result<Option<Sidecar>, String> {
        let Some((source, scope, key)) = parts(home) else { return Ok(None) };
        let document: Option<String> = self
            .db
            .query_row(
                "SELECT document FROM source_overlay WHERE source_id = ?1 AND scope = ?2 AND key = ?3",
                params![source, scope.name(), key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match document {
            None => Ok(None),
            Some(json) => Sidecar::from_json(json.as_bytes()).map(Some).map_err(|e| e.to_string()),
        }
    }

    /// The overlay laid over a server's track, found by its queue entry
    /// (`chilpa_models::RemoteEntry`): its record's, or its own when it has
    /// no record there. `None` for an entry that is not a server's track.
    pub fn overlay_of_entry(&self, entry: &str) -> rusqlite::Result<Option<Sidecar>> {
        let found: Option<(i64, Option<String>)> = self
            .db
            .query_row(
                "SELECT t.source_id, a.version_key FROM tracks t JOIN sources s ON s.id = t.source_id
                 LEFT JOIN albums a ON a.id = t.album_id WHERE t.external_id = ?1 AND s.kind = 'server'",
                params![entry],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((source, version_key)) = found else { return Ok(None) };
        let album_key = version_key.as_deref().and_then(|key| key.strip_prefix(&format!("{source}:")));
        Ok(self.overlay(&Home::hosted(source, album_key, entry)).ok().flatten())
    }

    /// Every overlay of a source, read once. One that cannot be read is
    /// left out: it is kept as it is and lays nothing over.
    fn overlays_of(&self, source_id: i64) -> rusqlite::Result<Overlays> {
        let mut stmt = self.db.prepare_cached("SELECT scope, key, document FROM source_overlay WHERE source_id = ?1")?;
        let rows = stmt.query_map(params![source_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = Overlays::new();
        for row in rows {
            let (scope, key, document) = row?;
            if let (Some(scope), Ok(sidecar)) = (Scope::from_name(&scope), Sidecar::from_json(document.as_bytes())) {
                out.insert((scope, key), sidecar);
            }
        }
        Ok(out)
    }

    /// Writes an overlay down, with its cover when one is given -- the
    /// image's bytes as they are and "jpg" or "png" -- and lays it over the
    /// mirror. One that lays nothing over is taken out, with its cover.
    pub fn store_overlay(&self, home: &Home, sidecar: &Sidecar, cover: Option<(&[u8], &str)>, now: i64) -> Result<StoredOverlay, String> {
        let Some((source, scope, key)) = parts(home) else { return Err("not a home on a server".into()) };
        if sidecar.version > VERSION {
            return Err(format!("version {}", sidecar.version));
        }
        let mut sidecar = sidecar.clone();
        sidecar.version = VERSION;
        sidecar.updated_at = now;
        sidecar.imported = false;

        let io = |e: std::io::Error| e.to_string();
        match cover {
            Some((bytes, extension)) => {
                let Some(at) = self.overlay_cover_at(home, extension) else {
                    return Err("there is no place for this cover in the library".into());
                };
                if let Some(dir) = at.parent() {
                    std::fs::create_dir_all(dir).map_err(io)?;
                }
                // Written beside and moved over: a cover half written is
                // never shown.
                let mut pending = at.clone().into_os_string();
                pending.push(".tmp");
                let pending = PathBuf::from(pending);
                std::fs::write(&pending, bytes).and_then(|()| std::fs::rename(&pending, &at)).map_err(|e| {
                    let _ = std::fs::remove_file(&pending);
                    io(e)
                })?;
                if let Some(other) = self.overlay_cover_at(home, if extension == "png" { "jpg" } else { "png" }) {
                    remove(&other).map_err(io)?;
                }
                let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                sidecar.album.cover = Override::Value(name);
            }
            None => {
                // A cover nothing names is a file nobody will find.
                if sidecar.album.cover.value().is_none() {
                    sidecar.album.cover = Override::Absent;
                    for extension in ["jpg", "png"] {
                        if let Some(at) = self.overlay_cover_at(home, extension) {
                            remove(&at).map_err(io)?;
                        }
                    }
                }
            }
        }
        sidecar.tracks.retain(|_, track| !track.is_empty());

        let written = if sidecar.is_empty() {
            self.db.execute(
                "DELETE FROM source_overlay WHERE source_id = ?1 AND scope = ?2 AND key = ?3",
                params![source, scope.name(), key],
            )
        } else {
            let document = String::from_utf8(sidecar.to_json()).unwrap_or_default();
            self.db.execute(
                "INSERT INTO source_overlay (source_id, scope, key, document, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source_id, scope, key) DO UPDATE SET document = excluded.document, updated_at = excluded.updated_at",
                params![source, scope.name(), key, document, now],
            )
        };
        written.map_err(|e| e.to_string())?;
        let tracks = self.lay_overlay(home).map_err(|e| e.to_string())?;
        let cover = self.overlay_cover(home, &sidecar.album);
        Ok(StoredOverlay { cover, tracks })
    }

    /// The provider and the instance of a source, which its tracks' entries
    /// are built from.
    fn source_of(&self, source_id: i64) -> rusqlite::Result<Option<SourceId>> {
        self.db
            .query_row("SELECT provider, instance FROM sources WHERE id = ?1", params![source_id], |r| {
                Ok(SourceId::new(r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()
    }

    /// Lays a home's overlay over every track of it the library holds, and
    /// over its record. Returns the tracks reached. Nothing to lay leaves
    /// the mirror as it is.
    pub fn lay_overlay(&self, home: &Home) -> rusqlite::Result<Vec<i64>> {
        let Some((source, scope, key)) = parts(home) else { return Ok(Vec::new()) };
        let Some(sidecar) = self.overlay(home).ok().flatten() else { return Ok(Vec::new()) };
        let tx = self.own_transaction()?;
        let mut reached = Vec::new();
        match scope {
            Scope::Album => {
                let album: Option<i64> = tx
                    .query_row("SELECT id FROM albums WHERE version_key = ?1", params![format!("{source}:{key}")], |r| r.get(0))
                    .optional()?;
                let Some(album) = album else { return Ok(reached) };
                let tracks: Vec<(i64, String)> = {
                    let mut stmt = tx.prepare("SELECT id, external_id FROM tracks WHERE source_id = ?1 AND album_id = ?2")?;
                    let rows = stmt.query_map(params![source, album], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                self.lay_album(home, &sidecar.album, album)?;
                for (id, external_id) in tracks {
                    self.lay_track(&sidecar.album, sidecar.tracks.get(&home.key(&external_id)), id)?;
                    reached.push(id);
                }
            }
            Scope::Track => {
                let Some(of) = self.source_of(source)? else { return Ok(reached) };
                let entry = RemoteEntry::new(of, key).to_entry();
                let track: Option<i64> =
                    tx.query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![source, entry], |r| r.get(0)).optional()?;
                if let Some(id) = track {
                    self.lay_track(&sidecar.album, sidecar.tracks.get(key), id)?;
                    reached.push(id);
                }
            }
        }
        tx.finish()?;
        Ok(reached)
    }

    /// Lays the source's overlays over tracks a sync just wrote as the
    /// server says them, each with its record's id on the server. Inside
    /// the sync's transaction.
    pub(crate) fn lay_overlays_over(&self, source_id: i64, written: &[(i64, Option<i64>, String, String)]) -> rusqlite::Result<()> {
        let overlays = self.overlays_of(source_id)?;
        if overlays.is_empty() {
            return Ok(());
        }
        let mut albums_laid: HashSet<i64> = HashSet::new();
        for (track_id, album_id, album_key, external_id) in written {
            let (home, sidecar) = match album_id {
                Some(_) => {
                    let Some(sidecar) = overlays.get(&(Scope::Album, album_key.clone())) else { continue };
                    (Home::hosted(source_id, Some(album_key.as_str()), external_id), sidecar)
                }
                None => {
                    let home = Home::hosted(source_id, None, external_id);
                    let Some(sidecar) = overlays.get(&(Scope::Track, home.key(external_id))) else { continue };
                    (home, sidecar)
                }
            };
            if let Some(album) = album_id {
                if albums_laid.insert(*album) {
                    self.lay_album(&home, &sidecar.album, *album)?;
                }
            }
            self.lay_track(&sidecar.album, sidecar.tracks.get(&home.key(external_id)), *track_id)?;
        }
        Ok(())
    }

    /// What the record's overlay says of the record itself.
    fn lay_album(&self, home: &Home, album: &Album, album_id: i64) -> rusqlite::Result<()> {
        if let Some(title) = text(&album.title) {
            self.db.execute("UPDATE albums SET title = ?2 WHERE id = ?1", params![album_id, title])?;
            self.db.execute("UPDATE tracks SET album_title = ?2 WHERE album_id = ?1", params![album_id, title])?;
            // The search keeps a copy of the record's title on each track's
            // row, refreshed when the track's record is set.
            self.db.execute("UPDATE tracks SET album_id = album_id WHERE album_id = ?1", params![album_id])?;
        }
        if let Some(artist) = text(&album.album_artist) {
            self.db.execute("UPDATE albums SET album_artist = ?2 WHERE id = ?1", params![album_id, artist])?;
            self.db.execute("UPDATE tracks SET album_credit = ?2 WHERE album_id = ?1", params![album_id, artist])?;
        }
        if let Some(date) = nullable(&album.date) {
            let year = date.as_deref().and_then(year_of);
            self.db.execute("UPDATE albums SET year = ?2 WHERE id = ?1", params![album_id, year])?;
            self.db.execute("UPDATE tracks SET release_year = ?2 WHERE album_id = ?1", params![album_id, year])?;
        }
        if let Some(cover) = self.overlay_cover(home, album) {
            let cover = cover.display().to_string();
            self.db.execute("UPDATE albums SET artwork_path = ?2 WHERE id = ?1", params![album_id, cover])?;
            self.db.execute("UPDATE tracks SET release_artwork = ?2 WHERE album_id = ?1", params![album_id, cover])?;
        }
        Ok(())
    }

    /// The cover a record's overlay names, when it is there. A name and
    /// nothing else: an overlay does not point outside its folder.
    fn overlay_cover(&self, home: &Home, album: &Album) -> Option<PathBuf> {
        let name = album.cover.value()?;
        if name.contains(['/', '\\']) {
            return None;
        }
        let Home::Hosted { source_id, scope: Scope::Album, .. } = home else { return None };
        Some(self.overlay_covers_dir(*source_id)?.join(name)).filter(|path| path.is_file())
    }

    /// What the overlay says of one track: the record's fields the library
    /// keeps per track, and the track's own.
    fn lay_track(&self, album: &Album, track: Option<&Track>, id: i64) -> rusqlite::Result<()> {
        match &album.genres {
            Override::Absent => {}
            Override::Cleared => self.set_track_genres(id, &[])?,
            Override::Value(genres) => self.set_track_genres(id, genres)?,
        }
        if let Some(label) = nullable(&album.label) {
            self.db.execute("UPDATE tracks SET label = ?2 WHERE id = ?1", params![id, label.filter(|l| !l.trim().is_empty())])?;
        }
        let Some(track) = track else { return Ok(()) };
        if let Some(title) = text(&track.title) {
            self.db.execute("UPDATE tracks SET title = ?2 WHERE id = ?1", params![id, title.trim()])?;
        }
        if let Some(artist) = text(&track.artist_credit) {
            self.db.execute("UPDATE tracks SET artist = ?2 WHERE id = ?1", params![id, artist.trim()])?;
        }
        if let Some(n) = number(&track.track_number) {
            self.db.execute("UPDATE tracks SET track_number = ?2 WHERE id = ?1", params![id, n])?;
        }
        if let Some(n) = number(&track.disc_number) {
            self.db.execute("UPDATE tracks SET disc_number = ?2 WHERE id = ?1", params![id, n])?;
        }
        if let Some(subtitle) = text(&track.disc_subtitle) {
            self.db.execute("UPDATE tracks SET disc_subtitle = ?2 WHERE id = ?1", params![id, subtitle.trim()])?;
        }
        if let Some(work) = text(&track.work) {
            self.db.execute("UPDATE tracks SET work = ?2 WHERE id = ?1", params![id, work.trim()])?;
        }
        if let Some(recording) = nullable(&track.musicbrainz_recording_id) {
            self.db.execute("UPDATE tracks SET recording_mbid = ?2 WHERE id = ?1", params![id, recording.filter(|r| !r.is_empty())])?;
        }
        match &track.composers {
            Override::Absent => {}
            Override::Cleared => self.set_track_composers(id, &[])?,
            Override::Value(composers) => self.set_track_composers(id, composers)?,
        }
        match track.explicit {
            Override::Absent => {}
            Override::Cleared => {
                self.db.execute("UPDATE tracks SET explicit = NULL WHERE id = ?1", params![id])?;
            }
            Override::Value(flag) => {
                self.db.execute("UPDATE tracks SET explicit = ?2 WHERE id = ?1", params![id, flag])?;
            }
        }
        Ok(())
    }

    /// The cover an overlay keeps for a record, when the record is a
    /// server's and its overlay names one that is there.
    pub(crate) fn overlay_cover_of_album(&self, album_id: i64) -> rusqlite::Result<Option<PathBuf>> {
        let keyed: Option<(String, i64)> = self
            .db
            .query_row(
                "SELECT a.version_key, t.source_id FROM albums a JOIN tracks t ON t.album_id = a.id
                 JOIN sources s ON s.id = t.source_id WHERE a.id = ?1 AND s.kind = 'server' LIMIT 1",
                params![album_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((version_key, source)) = keyed else { return Ok(None) };
        let Some(key) = version_key.strip_prefix(&format!("{source}:")) else { return Ok(None) };
        let home = Home::hosted(source, Some(key), "");
        let Some(sidecar) = self.overlay(&home).ok().flatten() else { return Ok(None) };
        Ok(self.overlay_cover(&home, &sidecar.album))
    }

    /// How many of a server's overlays name a record or a track it no
    /// longer lists. They are kept: the server may list them again.
    pub fn orphan_overlays(&self, source_id: i64) -> rusqlite::Result<usize> {
        let Some(of) = self.source_of(source_id)? else { return Ok(0) };
        let kept: Vec<(String, String)> = {
            let mut stmt = self.db.prepare("SELECT scope, key FROM source_overlay WHERE source_id = ?1")?;
            let rows = stmt.query_map(params![source_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut orphans = 0;
        for (scope, key) in kept {
            let listed: bool = match Scope::from_name(&scope) {
                Some(Scope::Album) => self.db.query_row(
                    "SELECT EXISTS (SELECT 1 FROM albums a JOIN tracks t ON t.album_id = a.id
                     WHERE a.version_key = ?1 AND t.source_id = ?2 AND t.present = 1)",
                    params![format!("{source_id}:{key}"), source_id],
                    |r| r.get(0),
                )?,
                Some(Scope::Track) => self.db.query_row(
                    "SELECT EXISTS (SELECT 1 FROM tracks WHERE source_id = ?1 AND external_id = ?2 AND present = 1)",
                    params![source_id, RemoteEntry::new(of.clone(), key).to_entry()],
                    |r| r.get(0),
                )?,
                None => true,
            };
            if !listed {
                orphans += 1;
            }
        }
        Ok(orphans)
    }
}

#[cfg(test)]
mod tests;
