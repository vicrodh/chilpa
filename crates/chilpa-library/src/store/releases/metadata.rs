//! Bounded release reconciliation for a file edit. Components include every
//! old album member and every physical assembly neighbor before reusing the
//! normal assembler. The temporary indexes live on this worker's connection.
use super::super::Library;
use rusqlite::{OptionalExtension, functions::FunctionFlags};
use std::path::Path;

const MAX_COMPONENT_TRACKS: i64 = 32768;
const MAX_COMPONENT_TEXT: i64 = 16 * 1024 * 1024;

impl Library {
    pub(crate) fn prepare_metadata_reconcile(&self) -> rusqlite::Result<()> {
        let initialized: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_temp_master WHERE name='metadata_release_pending')",
            [],
            |row| row.get(0),
        )?;
        if !initialized {
            self.db.execute_batch(
                "PRAGMA temp_store=FILE; PRAGMA temp.max_page_count=262144;
                CREATE TEMP TABLE metadata_release_pending(id INTEGER PRIMARY KEY);",
            )?;
        }
        Ok(())
    }

    pub(crate) fn metadata_release_changed(&self, id: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO metadata_release_pending VALUES (?1)",
            [id],
        )?;
        Ok(())
    }

    pub fn assemble_metadata_releases(&self) -> rusqlite::Result<()> {
        self.db.create_scalar_function(
            "metadata_container",
            2,
            FunctionFlags::SQLITE_DETERMINISTIC,
            |context| {
                let path = context.get::<String>(0)?;
                let location = super::parts::location(&path);
                let folder = Path::new(&location);
                let mut boxed = crate::album_folder::disc_folder(folder)
                    .is_some_and(|disc| disc.style == "scene");
                let mut parent = folder.parent().unwrap_or(folder);
                while crate::album_folder::disc_folder(parent)
                    .is_some_and(|disc| !disc.needs_confirmation)
                {
                    boxed |= crate::album_folder::disc_folder(parent)
                        .is_some_and(|disc| disc.style == "scene");
                    let Some(above) = parent.parent() else { break };
                    parent = above;
                }
                let title = if boxed {
                    String::new()
                } else {
                    super::parts::folded(&context.get::<String>(1)?)
                };
                let parent = parent.to_string_lossy();
                Ok(format!("{}:{}{}", parent.len(), parent, title))
            },
        )?;
        self.db.execute_batch("DROP TABLE IF EXISTS metadata_release_components;
            CREATE TEMP TABLE metadata_release_components(id INTEGER PRIMARY KEY,album INTEGER,source INTEGER,container TEXT,local_key TEXT);
            INSERT INTO metadata_release_components
                SELECT t.id,t.album_id,t.source_id,metadata_container(t.external_id,
                    CASE WHEN t.album_title='' THEN coalesce(a.title,'') ELSE t.album_title END),local.group_key
                FROM tracks t JOIN sources s ON s.id=t.source_id LEFT JOIN albums a ON a.id=t.album_id
                LEFT JOIN local_release_members local ON local.track_id=t.id
                WHERE s.provider='local';
            CREATE INDEX metadata_component_album ON metadata_release_components(album);
            CREATE INDEX metadata_component_location ON metadata_release_components(source,container);
            CREATE INDEX metadata_component_local ON metadata_release_components(local_key);
            CREATE TEMP TABLE IF NOT EXISTS metadata_release_members(id INTEGER PRIMARY KEY);")?;
        loop {
            let seed = self
                .db
                .query_row(
                    "SELECT id FROM metadata_release_pending ORDER BY id LIMIT 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(seed) = seed else { break };
            self.db
                .execute("DELETE FROM metadata_release_members", [])?;
            self.db.execute(
                "INSERT INTO metadata_release_members
                WITH RECURSIVE nodes(kind,key,source) AS (
                    SELECT 0,cast(album AS TEXT),0 FROM metadata_release_components WHERE id=?1 AND album IS NOT NULL
                    UNION SELECT 1,container,source FROM metadata_release_components WHERE id=?1
                    UNION SELECT 2,local_key,0 FROM metadata_release_components WHERE id=?1 AND local_key IS NOT NULL
                    UNION SELECT 1,c.container,c.source FROM nodes n JOIN metadata_release_components c
                        ON n.kind=0 AND c.album=cast(n.key AS INTEGER)
                    UNION SELECT 0,cast(c.album AS TEXT),0 FROM nodes n JOIN metadata_release_components c
                        ON n.kind=1 AND c.source=n.source AND c.container=n.key WHERE c.album IS NOT NULL
                    UNION SELECT 2,c.local_key,0 FROM nodes n JOIN metadata_release_components c
                        ON n.kind=0 AND c.album=cast(n.key AS INTEGER) WHERE c.local_key IS NOT NULL
                    UNION SELECT 0,cast(c.album AS TEXT),0 FROM nodes n JOIN metadata_release_components c
                        ON n.kind=2 AND c.local_key=n.key WHERE c.album IS NOT NULL
                )
                SELECT c.id FROM nodes n JOIN metadata_release_components c ON n.kind=0 AND c.album=cast(n.key AS INTEGER)
                UNION SELECT c.id FROM nodes n JOIN metadata_release_components c ON n.kind=1 AND c.source=n.source AND c.container=n.key
                UNION SELECT c.id FROM nodes n JOIN metadata_release_components c ON n.kind=2 AND c.local_key=n.key
                UNION SELECT ?1",
                [seed],
            )?;
            let (count, bytes): (i64, i64) = self.db.query_row(
                "SELECT count(*),coalesce(sum(
                length(t.external_id)+length(t.album_title)+length(t.artist)+length(t.album_credit)
                +length(coalesce(t.release_artwork,''))+512),0)
                FROM tracks t JOIN metadata_release_members m ON m.id=t.id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if count > MAX_COMPONENT_TRACKS || bytes > MAX_COMPONENT_TEXT {
                return Err(rusqlite::Error::InvalidParameterName(
                    "The affected release component exceeds the metadata reconciliation limit."
                        .into(),
                ));
            }
            self.assemble_release_scope(true)?;
            self.db.execute("DELETE FROM metadata_release_pending WHERE id IN (SELECT id FROM metadata_release_members)", [])?;
        }
        self.db.execute_batch("DROP TABLE metadata_release_components; DROP TABLE metadata_release_members; DROP TABLE metadata_release_pending;")?;
        Ok(())
    }
}
