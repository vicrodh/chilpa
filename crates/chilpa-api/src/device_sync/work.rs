//! I open device sessions only inside one requested background job.
mod playlist;
mod discovery;
pub(crate) use discovery::{discover, mounted_for};
pub(crate) use playlist::read_playlist;

use super::{
    catalog::{Pick, Profile, Root, Store},
    failure, selection, UiError,
};
use chilpa_sync::{transport::Transport, Action, Cancellation, Plan, State};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub storage: String,
    pub available: String,
    pub capacity: String,
    pub writable: bool,
    #[serde(skip)]
    pub(crate) profiles: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ChangeRow {
    pub action: String,
    pub playlist: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub size: String,
    pub path: String,
}
pub(crate) struct Ready {
    pub plan: Plan,
    pub profile: Profile,
    pub destination: String,
    pub tracks: usize,
    pub _episode_files: Option<tempfile::TempDir>,
}

fn transport(root: &Root, destination: &str, create: bool) -> Result<Box<dyn Transport>, UiError> {
    if destination.starts_with("volume:") {
        let volume = chilpa_devices::volumes::Volume::resolve(destination).map_err(UiError::from)?;
        let relative = discovery::folder(root, Some(&volume));
        let path = volume.folder(&relative, create).map_err(UiError::from)?;
        return Ok(Box::new(chilpa_devices::transport::Mounted::open(&path).map_err(UiError::from)?));
    }
    if destination == "mounted" {
        let Root::Mounted { path } = root else {
            return Err(failure(chilpa_i18n::mark("Choose the connected player and its storage first."), ""));
        };
        if !create && !path.join(".chilpa-sync/identity.json").is_file() {
            return Err(failure(chilpa_i18n::mark("The saved storage folder is unavailable. Reconnect its original memory."), ""));
        }
        return Ok(Box::new(chilpa_devices::transport::Mounted::open(path).map_err(UiError::from)?));
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let invalid_connection = || failure(chilpa_i18n::mark("The USB connection changed. Refresh the device list."), "");
        let (location, volume) = destination.split_once('/').ok_or_else(invalid_connection)?;
        let location = location.parse().map_err(|_| invalid_connection())?;
        let volume = volume.parse().map_err(|_| invalid_connection())?;
        let folder = chilpa_sync::RelativePath::new(discovery::folder(root, None)).map_err(UiError::from)?;
        let device = if create { chilpa_devices::transport::mtp::Mtp::open(location, volume, folder) }
            else { chilpa_devices::transport::mtp::Mtp::open_existing(location, volume, folder) };
        Ok(Box::new(device.map_err(UiError::from)?))
    }
    #[cfg(target_os = "android")]
    {
        let _ = (root, destination, create);
        Err(failure(chilpa_i18n::mark("USB host sync is available in the desktop application."), ""))
    }
}

fn checked_state(
    store: &Store,
    profile: &Profile,
    transport: &mut dyn Transport,
    cancel: &Cancellation,
) -> Result<State, UiError> {
    let target = chilpa_sync::inspect_target(transport, cancel).map_err(UiError::from)?;
    if target.as_ref().map(|t| t.id.as_str()) != Some(profile.target_id.as_str()) {
        return Err(failure(chilpa_i18n::mark("This storage does not match the saved profile. Choose its original memory or create a new profile."),""));
    }
    let mut state = State::open(&store.state_path(&profile.id)?).map_err(UiError::from)?;
    chilpa_sync::pair(&mut state, transport, &profile.name, cancel).map_err(UiError::from)?;
    Ok(state)
}
pub(crate) fn add(
    store: &Store,
    destination: &str,
    name: &str,
    folder: &str,
    cancel: &Cancellation,
) -> Result<Profile, UiError> {
    let name = super::catalog::label(name)?;
    let root = if destination == "mounted" {
        Root::Mounted {
            path: super::catalog::local_path(folder)?,
        }
    } else {
        chilpa_sync::RelativePath::new(folder.to_owned()).map_err(UiError::from)?;
        Root::Mtp {
            folder: folder.into(),
        }
    };
    let mut device = transport(&root, destination, true)?;
    let id = format!("{:032x}", rand::random::<u128>());
    let mut state = State::open(&store.state_path(&id)?).map_err(UiError::from)?;
    let target =
        chilpa_sync::pair(&mut state, device.as_mut(), &name, cancel).map_err(UiError::from)?;
    let profile = Profile {
        id,
        target_id: target.id,
        name,
        root,
        picks: Vec::new(),
        remove_unselected: false,
        revision: 0,
    };
    store.update(|catalog| {
        if catalog.profiles.len() >= super::catalog::MAX_PROFILES {
            return Err(failure(
                chilpa_i18n::mark("You can save at most 64 device storage profiles."),
                "",
            ));
        }
        if catalog
            .profiles
            .iter()
            .any(|p| p.target_id == profile.target_id)
        {
            return Err(failure(
                chilpa_i18n::mark("A profile already manages this device folder."),
                "",
            ));
        }
        catalog.profiles.push(profile.clone());
        Ok(())
    })?;
    store.remember_profile(&profile.id)?;
    Ok(profile)
}
pub(crate) fn save_selection(
    store: &Store,
    id: &str,
    picks: Vec<Pick>,
    remove_unselected: bool,
) -> Result<Profile, UiError> {
    if picks.len() > 512 {
        return Err(failure(
            chilpa_i18n::mark("Choose at most 512 playlists or queries for one storage."),
            "",
        ));
    }
    store.update(|catalog| {
        let profile = catalog
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| failure(chilpa_i18n::mark("Choose a saved device profile."), ""))?;
        if profile.picks != picks || profile.remove_unselected != remove_unselected {
            profile.picks = picks;
            profile.remove_unselected = remove_unselected;
            profile.revision = profile.revision.checked_add(1).ok_or_else(|| {
                failure(
                    chilpa_i18n::mark("The device profile revision is invalid."),
                    "",
                )
            })?;
        }
        Ok(profile.clone())
    })
}
pub(crate) fn prepare(
    store: &Store,
    library: &Path,
    profile: Profile,
    destination: String,
    recover: bool,
    cancel: &Cancellation,
) -> Result<Ready, UiError> {
    let mut selection = if recover {
        None
    } else {
        Some(selection::resolve(library, &profile.picks, cancel)?)
    };
    let mut episode_files = if let Some((inputs, _)) = &mut selection {
        super::episodes::resolve(&profile.picks, inputs, cancel)?
    } else { None };
    let mut device = transport(&profile.root, &destination, false)?;
    let mut state = checked_state(store, &profile, device.as_mut(), cancel)?;
    if state.interrupted().map_err(UiError::from)? {
        if !recover {
            let mut error = failure(
                chilpa_i18n::mark(
                    "An interrupted sync needs recovery before reviewing new changes.",
                ),
                "",
            );
            error.recovery = true;
            return Err(error);
        }
        chilpa_sync::recover(&mut state, device.as_mut(), cancel).map_err(UiError::from)?;
    }
    let (mut inputs, playlists) = match selection {
        Some(value) => value,
        None => selection::resolve(library, &profile.picks, cancel)?,
    };
    if recover { episode_files = super::episodes::resolve(&profile.picks, &mut inputs, cancel)?; }
    let plan = chilpa_sync::prepare_review(
        &state,
        device.as_mut(),
        &inputs,
        &playlists,
        profile.remove_unselected,
        cancel,
    )
    .map_err(UiError::from)?;
    Ok(Ready {
        plan,
        profile,
        destination,
        tracks: inputs.len(),
        _episode_files: episode_files,
    })
}
pub(crate) fn apply(
    store: &Store,
    ready: Ready,
    approve_removals: bool,
    verify_copies: bool,
    cancel: &Cancellation,
    progress: &Arc<Mutex<Option<chilpa_sync::Progress>>>,
) -> Result<(), UiError> {
    let current = store
        .read()?
        .profiles
        .into_iter()
        .find(|p| p.id == ready.profile.id)
        .ok_or_else(|| {
            failure(
                chilpa_i18n::mark("The device profile changed. Review it again."),
                "",
            )
        })?;
    if current.revision != ready.profile.revision {
        return Err(failure(
            chilpa_i18n::mark("The device profile changed. Review it again."),
            "",
        ));
    }
    let mut device = transport(&current.root, &ready.destination, false)?;
    let mut state = checked_state(store, &current, device.as_mut(), cancel)?;
    let result = chilpa_sync::synchronize_with_verification(
        &mut state,
        device.as_mut(),
        &ready.plan,
        ready.plan.id(),
        approve_removals,
        verify_copies,
        cancel,
        &mut |value| {
            if let Ok(mut slot) = progress.lock() {
                *slot = Some(value);
            }
        },
    );
    result.map_err(|error| {
        let mut error = UiError::from(error);
        error.recovery = state.interrupted().unwrap_or(false);
        error
    })
}
pub(crate) fn rows(plan: &Plan) -> Vec<ChangeRow> {
    plan.actions()
        .iter()
        .map(|action| {
            let (path, size, info, label) = match action {
                Action::Copy {
                    path,
                    bytes,
                    replaces,
                    info,
                } => (
                    path,
                    *bytes,
                    info,
                    if *replaces {
                        chilpa_i18n::mark("Update")
                    } else {
                        chilpa_i18n::mark("Copy")
                    },
                ),
                Action::Remove { path, bytes, info } => {
                    (path, *bytes, info, chilpa_i18n::mark("Remove"))
                }
            };
            ChangeRow {
                action: chilpa_i18n::t(label),
                playlist: matches!(info.kind, chilpa_sync::AssetKind::Playlist),
                title: info.title.clone(),
                artist: info.artist.clone(),
                album: info.album.clone(),
                size: super::bytes(size),
                path: path.to_string(),
            }
        })
        .collect()
}
pub(crate) fn default_library() -> PathBuf {
    chilpa_library::Library::default_path()
}
