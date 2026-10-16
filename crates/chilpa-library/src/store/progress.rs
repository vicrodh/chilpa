//! How far the listener got in something of a source that has an end.
//!
//! An episode is picked up where it was left, and one heard to the end is
//! marked. The row also carries the entry's card, so the player can name
//! what a queue only knows by its id.

use super::{now_unix, Library};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

/// An entry with an end, as the listener left it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heard {
    pub external_id: String,
    pub title: String,
    /// Whose it is: the show, for an episode.
    pub subtitle: String,
    pub image_url: String,
    /// Zero when the source did not say.
    pub duration_ms: u64,
    pub position_ms: u64,
    pub played: bool,
}

impl Library {
    /// Keeps the card of an entry that is about to play. What the listener
    /// already heard of it is left as it was.
    pub fn remember_heard_many(&self, source: &str, cards: &[Heard]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        for card in cards { self.remember_heard(source, card)?; }
        tx.finish()
    }

    pub fn remember_heard(&self, source: &str, card: &Heard) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO source_progress (source, external_id, title, subtitle, image_url, duration_ms, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(source, external_id) DO UPDATE SET
                 title = excluded.title, subtitle = excluded.subtitle, image_url = excluded.image_url,
                 duration_ms = CASE WHEN excluded.duration_ms > 0 THEN excluded.duration_ms ELSE duration_ms END",
            params![source, card.external_id, card.title, card.subtitle, card.image_url, card.duration_ms as i64, now_unix()],
        )?;
        Ok(())
    }

    /// Notes where the listener is. Nothing happens for an entry whose card
    /// was never kept: there is no row to say what it is.
    pub fn set_heard(&self, source: &str, external_id: &str, position_ms: u64, played: bool) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE source_progress SET position_ms = ?3, played = ?4, updated_at = ?5
             WHERE source = ?1 AND external_id = ?2",
            params![source, external_id, position_ms as i64, played, now_unix()],
        )?;
        Ok(())
    }

    pub fn heard(&self, source: &str, external_id: &str) -> rusqlite::Result<Option<Heard>> {
        self.db
            .query_row(
                "SELECT external_id, title, subtitle, image_url, duration_ms, position_ms, played
                 FROM source_progress WHERE source = ?1 AND external_id = ?2",
                params![source, external_id],
                read,
            )
            .optional()
    }

    /// Everything the listener started or finished from that source, by id.
    pub fn heard_of(&self, source: &str) -> rusqlite::Result<HashMap<String, Heard>> {
        let mut found = HashMap::new();
        let mut stmt = self.db.prepare(
            "SELECT external_id, title, subtitle, image_url, duration_ms, position_ms, played
             FROM source_progress WHERE source = ?1 AND (position_ms > 0 OR played = 1)",
        )?;
        for row in stmt.query_map([source], read)? {
            let heard = row?;
            found.insert(heard.external_id.clone(), heard);
        }
        Ok(found)
    }
}

impl Library {
    /// What the listener left halfway from that source, the latest first.
    pub fn unfinished(&self, source: &str) -> rusqlite::Result<Vec<Heard>> {
        self.db
            .prepare(
                "SELECT external_id, title, subtitle, image_url, duration_ms, position_ms, played
                 FROM source_progress WHERE source = ?1 AND played = 0 AND position_ms > 0
                 ORDER BY updated_at DESC, rowid DESC",
            )?
            .query_map([source], read)?
            .collect()
    }
}

fn read(r: &rusqlite::Row) -> rusqlite::Result<Heard> {
    Ok(Heard {
        external_id: r.get(0)?,
        title: r.get(1)?,
        subtitle: r.get(2)?,
        image_url: r.get(3)?,
        duration_ms: r.get::<_, i64>(4)?.max(0) as u64,
        position_ms: r.get::<_, i64>(5)?.max(0) as u64,
        played: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(id: &str) -> Heard {
        Heard { external_id: id.into(), title: "An Episode".into(), subtitle: "A Show".into(), duration_ms: 60_000, ..Heard::default() }
    }

    #[test]
    fn a_position_is_kept_and_the_card_can_be_refreshed_without_losing_it() {
        let lib = Library::in_memory().unwrap();
        lib.remember_heard("shows", &episode("ep:1")).unwrap();
        lib.set_heard("shows", "ep:1", 12_500, false).unwrap();
        // Queued again later, with a better title and no length this time.
        let again = Heard { title: "An Episode (fixed)".into(), duration_ms: 0, ..episode("ep:1") };
        lib.remember_heard("shows", &again).unwrap();
        let heard = lib.heard("shows", "ep:1").unwrap().unwrap();
        assert_eq!(heard.title, "An Episode (fixed)");
        assert_eq!(heard.position_ms, 12_500, "where the listener was is not lost");
        assert_eq!(heard.duration_ms, 60_000, "nor a length it did know");
        assert!(!heard.played);
    }

    #[test]
    fn only_what_was_started_or_finished_is_listed_and_per_source() {
        let lib = Library::in_memory().unwrap();
        for id in ["ep:1", "ep:2", "ep:3"] {
            lib.remember_heard("shows", &episode(id)).unwrap();
        }
        lib.remember_heard("other", &episode("ep:1")).unwrap();
        lib.set_heard("shows", "ep:1", 5_000, false).unwrap();
        lib.set_heard("shows", "ep:2", 0, true).unwrap();
        let heard = lib.heard_of("shows").unwrap();
        assert_eq!(heard.len(), 2);
        assert!(heard["ep:2"].played);
        assert!(lib.heard_of("other").unwrap().is_empty());
    }

    #[test]
    fn what_was_left_halfway_is_listed_and_what_was_finished_is_not() {
        let lib = Library::in_memory().unwrap();
        for id in ["ep:1", "ep:2", "ep:3"] {
            lib.remember_heard("shows", &episode(id)).unwrap();
        }
        lib.set_heard("shows", "ep:1", 5_000, false).unwrap();
        lib.set_heard("shows", "ep:2", 0, true).unwrap();
        let halfway = lib.unfinished("shows").unwrap();
        assert_eq!(halfway.len(), 1);
        assert_eq!((halfway[0].external_id.as_str(), halfway[0].position_ms), ("ep:1", 5_000));
        assert!(lib.unfinished("other").unwrap().is_empty());
    }

    #[test]
    fn a_position_for_something_never_kept_changes_nothing() {
        let lib = Library::in_memory().unwrap();
        lib.set_heard("shows", "ep:9", 5_000, true).unwrap();
        assert_eq!(lib.heard("shows", "ep:9").unwrap(), None);
    }
}
