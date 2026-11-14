//! Persistence and assembly of explicit local members whose evidence was read.
use super::{
    super::Library,
    parts::{self, Part, Track},
};
use crate::sidecar::Presentation;
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

impl Library {
    pub(crate) fn set_file_presentation(
        &self,
        id: i64,
        presentation: Option<&Presentation>,
    ) -> rusqlite::Result<()> {
        match presentation {
            Some(presentation) => {
                self.db.execute("INSERT INTO local_release_members(track_id,group_key,title,artist,evidence)
                VALUES (?1,?2,?3,?4,?5) ON CONFLICT(track_id) DO UPDATE SET group_key=excluded.group_key,title=excluded.title,
                artist=excluded.artist,evidence=excluded.evidence WHERE group_key<>excluded.group_key OR title<>excluded.title
                OR artist<>excluded.artist OR evidence<>excluded.evidence",params![id,presentation.key,presentation.title,presentation.artist,presentation.evidence])?;
            }
            None => {
                self.db
                    .execute("DELETE FROM local_release_members WHERE track_id=?1", [id])?;
            }
        }
        Ok(())
    }
}

pub(super) fn release_parts(
    tracks: Vec<Track>,
) -> (Vec<Part>, Vec<Vec<usize>>, HashMap<i64, String>) {
    let mut normal = Vec::new();
    let mut grouped = BTreeMap::<String, (String, String, Vec<Track>)>::new();
    let mut keys = HashMap::new();
    for mut track in tracks {
        let presentation = track.local.take();
        if let Some((key, title, artist)) = presentation {
            keys.insert(track.id, key.clone());
            grouped
                .entry(key)
                .or_insert_with(|| (title, artist, Vec::new()))
                .2
                .push(track);
        } else {
            normal.push(track);
        }
    }
    let mut parts = parts::collect(normal);
    let mut releases = parts::assemble(&parts);
    for (_, (title, artist, tracks)) in grouped {
        let start = parts.len();
        let mut members = parts::collect(tracks);
        for part in &mut members {
            part.title = title.clone();
            part.artist = artist.clone();
        }
        parts.extend(members);
        releases.push((start..parts.len()).collect());
    }
    (parts, releases, keys)
}
