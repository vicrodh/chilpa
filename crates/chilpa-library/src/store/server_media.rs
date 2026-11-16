//! The local identity of music owned by one server, and confirmed removals.

use super::Library;
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub source: i64,
    pub local: i64,
    pub album: bool,
    /// Album: its server's id. Track: its complete remote entry.
    pub reference: String,
    pub title: String,
}

impl Library {
    /// No title matching: an album must be a source edition belonging to
    /// exactly one server. Local files and mixed-source records are refused.
    pub fn server_media_target(&self, album: bool, id: i64) -> rusqlite::Result<Option<Target>> {
        if id <= 0 { return Ok(None); }
        if !album {
            return self.db.query_row("SELECT t.source_id, t.external_id, t.title FROM tracks t
                JOIN servers s ON s.source_id = t.source_id WHERE t.id = ?1 AND t.present = 1", [id], |row|
                Ok(Target { source: row.get(0)?, local: id, album, reference: row.get(1)?, title: row.get(2)? })).optional();
        }
        let found: Option<(i64, String, String)> = self.db.query_row(
            "SELECT min(t.source_id), a.version_key, a.title FROM albums a JOIN tracks t ON t.album_id = a.id
             WHERE a.id = ?1 AND t.present = 1 GROUP BY a.id HAVING count(DISTINCT t.source_id) = 1
             AND min(t.source_id) IN (SELECT source_id FROM servers)", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
        Ok(found.and_then(|(source, key, title)| {
            let reference = key.strip_prefix(&format!("{source}:"))?.to_string();
            (!reference.is_empty()).then_some(Target { source, local: id, album, reference, title })
        }))
    }

    /// Only after the server accepted deletion. Ratings, overlays, history
    /// and playlist references stay; the rows become absent as in a full sync.
    pub fn server_media_deleted(&self, target: &Target) -> rusqlite::Result<usize> {
        if self.server_media_target(target.album, target.local)?.as_ref() != Some(target) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let tx = self.own_transaction()?;
        let column = if target.album { "album_id" } else { "id" };
        let changed = tx.execute(&format!("UPDATE tracks SET present = 0, reachable = 0 WHERE source_id = ?1 AND {column} = ?2 AND present = 1"),
            params![target.source, target.local])?;
        tx.finish()?;
        self.match_credits()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewServer, ServerTrack, Filter, TrackOrder};

    fn fixture() -> (Library, i64, i64, i64) {
        let lib = Library::in_memory().unwrap();
        let source = lib.add_server(&NewServer { provider:"jellyfin".into(),instance:"fixture".into(),
            label:"Server".into(),address:"http://fixture.invalid".into(),libraries:"[]".into(),details:"{}".into(),..Default::default() }).unwrap();
        let generation = lib.begin_server_sync(source).unwrap();
        let tracks: Vec<_> = ["one","two"].into_iter().map(|id| ServerTrack {
            external_id:format!("remote:jellyfin/fixture/{id}"),title:id.into(),artist:"Artist".into(),album:"Album".into(),
            album_artist:"Artist".into(),album_key:"remote-album".into(),..Default::default()
        }).collect();
        lib.save_server_tracks(source,generation,&tracks).unwrap();
        lib.finish_server_sync(source,generation).unwrap();
        let (id,_) = lib.track_by_entry("remote:jellyfin/fixture/one").unwrap().unwrap();
        let album = lib.browse_ids(&Filter::default(),TrackOrder::Album).unwrap().album_ids[0];
        (lib,source,id,album)
    }
    #[test]
    fn an_album_is_identified_by_its_source_edition_and_not_its_title() {
        let (lib,source,id,album) = fixture();
        let target = lib.server_media_target(true,album).unwrap().unwrap();
        assert_eq!(target.reference,"remote-album");
        assert_eq!(target.source,source);
        assert_eq!(lib.server_media_deleted(&target).unwrap(),2);
        assert_eq!(lib.track_count(false).unwrap(),0);
        assert_eq!(lib.track_count(true).unwrap(),2,"absent rows keep their history and references");
        assert!(lib.server_media_target(false,id).unwrap().is_none());
    }
    #[test]
    fn deleting_one_track_keeps_the_other_and_refuses_a_changed_identity() {
        let (lib,_,id,_) = fixture();
        let mut target = lib.server_media_target(false,id).unwrap().unwrap();
        target.reference = "another item".into();
        assert!(lib.server_media_deleted(&target).is_err());
        assert_eq!(lib.track_count(false).unwrap(),2);
        let target = lib.server_media_target(false,id).unwrap().unwrap();
        assert_eq!(lib.server_media_deleted(&target).unwrap(),1);
        assert_eq!(lib.track_count(false).unwrap(),1);
        assert_eq!(lib.track_count(true).unwrap(),2);
    }
}
