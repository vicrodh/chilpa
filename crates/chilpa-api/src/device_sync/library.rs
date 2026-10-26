//! I name Library batches and resolve browser metadata on read-only worker snapshots.
use super::{
    catalog::{Catalog, Pick},
    failure,
    selection::OptionRow,
    UiError,
};
use chilpa_library::{ExportTrack, Filter, Library, TrackOrder};
use chilpa_sync::{Cancellation, MAX_ASSETS};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

pub(super) type Labels = HashMap<(String, String), (String, String)>;

pub(super) fn ids(library: &Library, pick: &Pick) -> Result<Vec<i64>, UiError> {
    let result = match pick {
        Pick::Playlist { key, .. } => library.export_playlist(key).map(|(_, ids)| ids),
        Pick::Tracks { keys, .. } => library.export_track_ids(keys),
        Pick::Query { expression, .. } => {
            let filter = Filter {
                query: crate::library_query::read(expression).map_err(|e| {
                    failure(chilpa_i18n::mark("The device query cannot be read."), e)
                })?,
                ..Default::default()
            };
            library
                .browse_tracks(&filter, TrackOrder::Album, MAX_ASSETS + 1)
                .map(|rows| rows.into_iter().map(|r| r.id).collect())
        }
    };
    let ids = result.map_err(|e| {
        failure(
            chilpa_i18n::mark(
                "Some selected Library tracks are missing. I kept the device unchanged.",
            ),
            e,
        )
    })?;
    if ids.len() > MAX_ASSETS {
        return Err(failure(
            chilpa_i18n::mark("A device selection can contain at most 50,000 files and playlists."),
            "",
        ));
    }
    Ok(ids)
}

pub(super) fn tracks(
    path: &Path,
    pick: &Pick,
    cancel: &Cancellation,
) -> Result<Vec<ExportTrack>, UiError> {
    cancel.check().map_err(UiError::from)?;
    let library = Library::open_read_only(path).map_err(|e| {
        failure(
            chilpa_i18n::mark("I could not read this Library selection. Try again."),
            e,
        )
    })?;
    library.read_snapshot(|| {
        let ids = ids(&library, pick)?;
        cancel.check().map_err(UiError::from)?;
        let mut tracks = Vec::new();
        let mut bytes = 0usize;
        for chunk in ids.chunks(200) {
            cancel.check().map_err(UiError::from)?;
            let rows = library.export_tracks(chunk).map_err(|e| failure(chilpa_i18n::mark("Some selected Library tracks are missing. I kept the device unchanged."), e))?;
            for track in rows {
                bytes = bytes.saturating_add(track.key.len() + track.path.as_ref().map_or(0, String::len) + track.title.len() + track.artist.len() + track.album.len());
                if bytes > chilpa_sync::MAX_METADATA_BYTES { return Err(failure(chilpa_i18n::mark("This music view exceeds 50,000 files or 16 MiB of metadata."), "")); }
                tracks.push(track);
            }
        }
        Ok(tracks)
    }).map_err(|e| failure(chilpa_i18n::mark("I could not read this Library selection. Try again."), e))?
}

pub(super) fn describe(tracks: &[ExportTrack]) -> (String, String) {
    let albums: BTreeSet<_> = tracks
        .iter()
        .map(|t| (t.artist.as_str(), t.album.as_str()))
        .collect();
    let count = chilpa_i18n::tf(
        "{} track",
        "{} tracks",
        tracks.len() as i64,
        &[&tracks.len().to_string()],
    );
    let short = |value: &str| value.chars().take(160).collect::<String>();
    if let Some((artist, album)) = albums.first().filter(|_| albums.len() == 1) {
        let title = if !album.is_empty() {
            short(album)
        } else if tracks.len() == 1 {
            short(&tracks[0].title)
        } else {
            chilpa_i18n::t("Selected music")
        };
        return (
            title,
            [short(artist), count]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    let title = chilpa_i18n::tf(
        "{} album",
        "{} albums",
        albums.len() as i64,
        &[&albums.len().to_string()],
    );
    let names = albums
        .iter()
        .take(2)
        .map(|(_, name)| short(name))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (
        title,
        [names, count]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
    )
}

pub(super) fn batch(tracks: Vec<ExportTrack>) -> OptionRow {
    let (name, description) = describe(&tracks);
    let keys: Vec<_> = tracks.into_iter().map(|t| t.key).collect();
    let digest = Sha256::digest(serde_json::to_vec(&keys).unwrap_or_default());
    let id = format!("tracks:{digest:x}");
    let pick = Pick::Tracks {
        id: id.clone(),
        name: name.clone(),
        keys,
    };
    OptionRow {
        id,
        name,
        description,
        checked: true,
        pick,
    }
}

/// I enrich legacy generic selections once per catalog load, without rewriting profiles.
pub(super) fn labels(
    path: &Path,
    catalog: &Catalog,
    cancel: &Cancellation,
) -> Result<Labels, UiError> {
    let mut labels = Labels::new();
    if !catalog
        .profiles
        .iter()
        .any(|p| p.picks.iter().any(|s| matches!(s, Pick::Tracks { .. })))
    {
        return Ok(labels);
    }
    let library = Library::open_read_only(path).map_err(|e| {
        failure(
            chilpa_i18n::mark("I could not read this Library selection. Try again."),
            e,
        )
    })?;
    library
        .read_snapshot(|| {
            for profile in &catalog.profiles {
                for pick in &profile.picks {
                    cancel.check().map_err(UiError::from)?;
                    if let Pick::Tracks { keys, .. } = pick {
                        // I keep a missing saved selection visible; opening it reports the specific failure.
                        if let Ok(tracks) = library
                            .export_track_ids(keys)
                            .and_then(|ids| library.export_tracks(&ids))
                        {
                            labels
                                .insert((profile.id.clone(), pick.id().into()), describe(&tracks));
                        }
                    }
                }
            }
            Ok(labels)
        })
        .map_err(|e| {
            failure(
                chilpa_i18n::mark("I could not read this Library selection. Try again."),
                e,
            )
        })?
}
