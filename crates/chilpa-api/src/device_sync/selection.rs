//! I deduplicate media bytes and preserve each playlist occurrence on a single SQLite snapshot.
use super::{catalog::Pick, failure, UiError};
use chilpa_library::{Filter, Library, TrackOrder};
use chilpa_sync::{Cancellation, Input, Playlist, MAX_ASSETS};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OptionRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub checked: bool,
    #[serde(skip)]
    pub pick: Pick,
}
pub fn options(library: &Path) -> Result<Vec<OptionRow>, UiError> {
    let library = Library::open_read_only(library).map_err(|e| {
        failure(
            chilpa_i18n::mark("Open the Library before choosing music for a device."),
            e,
        )
    })?;
    let mut rows = Vec::new();
    for list in library.export_playlists().map_err(|e| {
        failure(
            chilpa_i18n::mark("I could not read the local playlists."),
            e,
        )
    })? {
        if rows.len() >= 2048 {
            return Err(failure(
                chilpa_i18n::mark("The device playlist picker can show at most 2,048 playlists."),
                "",
            ));
        }
        let id = format!("playlist:{}", list.key);
        rows.push(OptionRow {
            id: id.clone(),
            name: list.name.clone(),
            description: chilpa_i18n::tf(
                "{} track",
                "{} tracks",
                list.tracks as i64,
                &[&list.tracks.to_string()],
            ),
            checked: false,
            pick: Pick::Playlist {
                id,
                name: list.name,
                key: list.key,
            },
        });
    }
    for query in crate::saved_queries::list()
        .map_err(|e| failure(chilpa_i18n::mark("I could not read the saved queries."), e))?
        .entries
    {
        let id = format!("query:{}", query.id);
        rows.push(OptionRow {
            id: id.clone(),
            name: query.name.clone(),
            description: query.expression.clone(),
            checked: false,
            pick: Pick::Query {
                id,
                name: query.name,
                expression: query.expression,
            },
        });
    }
    Ok(rows)
}
pub fn resolve_with_progress(path: &Path, picks: &[Pick], cancel: &Cancellation,
    progress: &mut (dyn FnMut(chilpa_sync::ReviewProgress) + Send)) -> Result<(Vec<Input>, Vec<Playlist>), UiError> {
    if picks.iter().all(|pick| matches!(pick, Pick::Episode { .. })) {
        return Ok((Vec::new(), Vec::new()));
    }
    let library = Library::open_read_only(path).map_err(|e| {
        failure(
            chilpa_i18n::mark("I could not read the Library for device sync."),
            e,
        )
    })?;
    library.read_snapshot(|| {
        let mut selected=BTreeSet::new(); let mut lists=Vec::new();
        for (index, pick) in picks.iter().enumerate() {
            progress(chilpa_sync::ReviewProgress { stage:"selection", completed:index, total:picks.len(), current_path:pick.name().into(), selected_bytes:0 });
            cancel.check().map_err(UiError::from)?;
            let (id,name,ids)=match pick {
                Pick::Episode { .. } => continue,
                Pick::Playlist{id,key,..}=>{let(name,ids)=library.export_playlist(key).map_err(|e|failure(chilpa_i18n::mark("A selected playlist or one of its tracks is missing. I kept the device unchanged."),e))?;(id.clone(),name,ids)}
                Pick::Query{id,name,expression}=>{
                    let filter=Filter{query:crate::library_query::read(expression).map_err(|e|failure(chilpa_i18n::mark("The device query cannot be read."),e))?,..Default::default()};
                    let tracks=library.browse_tracks(&filter,TrackOrder::Album,MAX_ASSETS+1).map_err(|e|failure(chilpa_i18n::mark("I could not resolve the device query."),e))?;
                    (id.clone(),name.clone(),tracks.into_iter().map(|t|t.id).collect())
                }
                Pick::Tracks{id,name,keys}=>(id.clone(),name.clone(),library.export_track_ids(keys).map_err(|e|failure(chilpa_i18n::mark("Some selected Library tracks are missing. I kept the device unchanged."),e))?),
            };
            if ids.len()>MAX_ASSETS {return Err(failure(chilpa_i18n::mark("A device selection can contain at most 50,000 files and playlists."),""));}
            selected.extend(ids.iter().copied());
            if matches!(pick, Pick::Playlist { .. }) { lists.push((id,name,ids)); }
            if selected.len()+lists.len()>MAX_ASSETS {return Err(failure(chilpa_i18n::mark("A device selection can contain at most 50,000 files and playlists."),""));}
        }
        let ids:Vec<_>=selected.into_iter().collect();
        let tracks=library.export_tracks(&ids).map_err(|e|failure(chilpa_i18n::mark("Some selected Library tracks are missing. I kept the device unchanged."),e))?;
        let mut inputs=Vec::with_capacity(tracks.len());let mut keys=BTreeMap::new();
        let total = tracks.len();
        for (index, track) in tracks.into_iter().enumerate() {
            progress(chilpa_sync::ReviewProgress { stage:"selection_files", completed:index, total, current_path:track.title.clone(), selected_bytes:0 });
            cancel.check().map_err(UiError::from)?;
            let path=track.path.filter(|_|track.available).ok_or_else(||failure(chilpa_i18n::mark("This selection includes music without an available local file. I kept the device unchanged."),&track.title))?;
            if !Path::new(&path).is_file() {return Err(failure(chilpa_i18n::mark("A selected local file is unavailable. I kept the device unchanged."),&track.title));}
            keys.insert(track.id,track.key.clone());
            inputs.push(Input{id:track.key,source:PathBuf::from(path),artist:track.artist,
                album:if track.disc>1 {format!("{} - Disc {}",track.album,track.disc)}else{track.album},title:track.title,track_number:track.number});
        }
        let mut playlists=Vec::with_capacity(lists.len());
        for(id,name,ids) in lists {
            let tracks=ids.iter().map(|id|keys.get(id).cloned().ok_or_else(||failure(chilpa_i18n::mark("A playlist member could not be resolved."),""))).collect::<Result<_,_>>()?;
            playlists.push(Playlist{id,name,tracks});
        }
        Ok((inputs,playlists))
    }).map_err(|e|failure(chilpa_i18n::mark("I could not read this Library selection. Try again."),e))?
}
