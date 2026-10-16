//! What the listener keeps from a source that is browsed: a station, a show.
//!
//! The row carries enough to show the entry again without asking the source,
//! so the saved list opens with the network down. It is the listener's, not
//! the plugin's: reinstalling the plugin does not lose it.

use super::{now_unix, Library};
use rusqlite::params;

/// An entry of a source, as it was when it was saved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedEntry {
    pub external_id: String,
    pub title: String,
    pub subtitle: String,
    /// Empty when it had no picture.
    pub image_url: String,
    /// What the source said it is: "folder" or "item".
    pub kind: String,
    pub live: bool,
}

impl Library {
    /// What is saved from that source, the latest first.
    pub fn saved_entries(&self, source: &str) -> rusqlite::Result<Vec<SavedEntry>> {
        self.db
            .prepare(
                "SELECT external_id, title, subtitle, image_url, kind, live FROM source_saved
                 WHERE source = ?1 ORDER BY saved_at DESC, rowid DESC",
            )?
            .query_map([source], |r| {
                Ok(SavedEntry {
                    external_id: r.get(0)?,
                    title: r.get(1)?,
                    subtitle: r.get(2)?,
                    image_url: r.get(3)?,
                    kind: r.get(4)?,
                    live: r.get(5)?,
                })
            })?
            .collect()
    }

    /// Saves an entry. Saving it again refreshes how it reads and keeps its
    /// place in the list.
    pub fn save_entry(&self, source: &str, entry: &SavedEntry) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO source_saved (source, external_id, title, subtitle, image_url, kind, live, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(source, external_id) DO UPDATE SET
                 title = excluded.title, subtitle = excluded.subtitle,
                 image_url = excluded.image_url, kind = excluded.kind, live = excluded.live",
            params![
                source,
                entry.external_id,
                entry.title,
                entry.subtitle,
                entry.image_url,
                entry.kind,
                entry.live,
                now_unix()
            ],
        )?;
        Ok(())
    }

    /// Takes an entry out of the saved ones. One that was not there is not
    /// an error.
    pub fn forget_entry(&self, source: &str, external_id: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "DELETE FROM source_saved WHERE source = ?1 AND external_id = ?2",
            params![source, external_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(id: &str, title: &str) -> SavedEntry {
        SavedEntry {
            external_id: id.into(),
            title: title.into(),
            subtitle: "Somewhere".into(),
            kind: "item".into(),
            live: true,
            ..SavedEntry::default()
        }
    }

    #[test]
    fn what_is_saved_comes_back_as_it_was_and_per_source() {
        let lib = Library::in_memory().unwrap();
        lib.save_entry("radio", &station("station:1", "One")).unwrap();
        lib.save_entry("radio", &station("station:2", "Two")).unwrap();
        lib.save_entry("shows", &station("show:1", "A Show")).unwrap();

        let saved = lib.saved_entries("radio").unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].title, "Two", "the latest first");
        assert!(saved[1].live);
        assert_eq!(lib.saved_entries("shows").unwrap().len(), 1);
        assert!(lib.saved_entries("nobody").unwrap().is_empty());
    }

    #[test]
    fn saving_twice_is_saving_once_and_refreshes_how_it_reads() {
        let lib = Library::in_memory().unwrap();
        lib.save_entry("radio", &station("station:1", "One")).unwrap();
        lib.save_entry("radio", &station("station:2", "Two")).unwrap();
        lib.save_entry("radio", &station("station:1", "One FM")).unwrap();
        let saved = lib.saved_entries("radio").unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[1].title, "One FM", "renamed, and still in its place");
    }

    #[test]
    fn forgetting_takes_only_that_one() {
        let lib = Library::in_memory().unwrap();
        lib.save_entry("radio", &station("station:1", "One")).unwrap();
        lib.save_entry("shows", &station("station:1", "Same id, other source")).unwrap();
        lib.forget_entry("radio", "station:1").unwrap();
        lib.forget_entry("radio", "never-there").unwrap();
        assert!(lib.saved_entries("radio").unwrap().is_empty());
        assert_eq!(lib.saved_entries("shows").unwrap().len(), 1);
    }
}
