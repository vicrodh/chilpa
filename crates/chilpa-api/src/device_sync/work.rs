//! I open device sessions only inside one requested background job.
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
}
pub(crate) fn mounted() -> Destination {
    Destination {
        id: "mounted".into(),
        name: chilpa_i18n::t("Mounted storage"),
        storage: chilpa_i18n::t("Choose a folder on a memory card or USB drive"),
        available: String::new(),
        capacity: String::new(),
        writable: true,
    }
}
pub(crate) fn discover(cancel: &Cancellation) -> (Vec<Destination>, Option<UiError>) {
    let mut rows = Vec::new();
    let mut error = None;
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    match chilpa_devices::transport::mtp::devices() {
        Err(problem) => error = Some(UiError::from(problem)),
        Ok(devices) => {
            if devices.len() > 8 {
                error = Some(failure(
                    chilpa_i18n::mark("Connect at most eight USB players at a time."),
                    "",
                ));
            }
            for device in devices.into_iter().take(8) {
                if let Err(problem) = cancel.check() {
                    error = Some(UiError::from(problem));
                    break;
                }
                let description = match chilpa_devices::transport::mtp::describe(device.location) {
                    Ok(description) => description,
                    Err(problem) => {
                        if error.is_none() {
                            error = Some(UiError::from(problem));
                        }
                        continue;
                    }
                };
                for volume in description.volumes {
                    if rows.len() >= 128 {
                        error = Some(failure(
                            chilpa_i18n::mark("Too many device storage locations were reported."),
                            "",
                        ));
                        break;
                    }
                    rows.push(Destination {
                        id: format!("{}/{}", device.location, volume.id),
                        name: if description.name.trim().is_empty() {
                            device.name.clone()
                        } else {
                            description.name.clone()
                        },
                        storage: volume.name,
                        available: super::bytes(volume.available),
                        capacity: super::bytes(volume.capacity),
                        writable: volume.writable,
                    });
                }
            }
        }
    }
    #[cfg(target_os = "android")]
    let _ = cancel;
    rows.push(mounted());
    (rows, error)
}

fn transport(root: &Root, destination: &str, create: bool) -> Result<Box<dyn Transport>, UiError> {
    match root {
        Root::Mounted { path } => {
            if destination != "mounted" {
                return Err(failure(
                    chilpa_i18n::mark(
                        "Choose mounted storage for this profile, or select a player profile.",
                    ),
                    "",
                ));
            }
            if !create && (!path.is_dir() || !path.join(".chilpa-sync/identity.json").is_file()) {
                return Err(failure(
                    chilpa_i18n::mark(
                        "The saved storage folder is unavailable. Reconnect its original memory.",
                    ),
                    "",
                ));
            }
            Ok(Box::new(
                chilpa_devices::transport::Mounted::open(path).map_err(UiError::from)?,
            ))
        }
        Root::Mtp { folder } => {
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
            {
                let (location, volume) = destination.split_once('/').ok_or_else(|| {
                    failure(
                        chilpa_i18n::mark("Choose the connected player and its storage first."),
                        "",
                    )
                })?;
                let location = location.parse().map_err(|_| {
                    failure(
                        chilpa_i18n::mark("The USB connection changed. Refresh the device list."),
                        "",
                    )
                })?;
                let volume = volume.parse().map_err(|_| {
                    failure(
                        chilpa_i18n::mark("The USB connection changed. Refresh the device list."),
                        "",
                    )
                })?;
                let folder =
                    chilpa_sync::RelativePath::new(folder.clone()).map_err(UiError::from)?;
                let device = if create {
                    chilpa_devices::transport::mtp::Mtp::open(location, volume, folder)
                } else {
                    chilpa_devices::transport::mtp::Mtp::open_existing(location, volume, folder)
                };
                Ok(Box::new(device.map_err(UiError::from)?))
            }
            #[cfg(target_os = "android")]
            {
                let _ = (folder, destination, create);
                Err(failure(
                    chilpa_i18n::mark("USB host sync is available in the desktop application."),
                    "",
                ))
            }
        }
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
    let selection = if recover {
        None
    } else {
        Some(selection::resolve(library, &profile.picks, cancel)?)
    };
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
    let (inputs, playlists) = match selection {
        Some(value) => value,
        None => selection::resolve(library, &profile.picks, cancel)?,
    };
    let plan = chilpa_sync::prepare(
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
    })
}
pub(crate) fn apply(
    store: &Store,
    ready: Ready,
    approve_removals: bool,
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
    let result = chilpa_sync::synchronize(
        &mut state,
        device.as_mut(),
        &ready.plan,
        ready.plan.id(),
        approve_removals,
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
