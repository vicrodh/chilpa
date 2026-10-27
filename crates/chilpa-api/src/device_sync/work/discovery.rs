//! I match known storage markers, never a USB port, name or first volume.
use super::{Destination, Root, UiError};
use crate::device_sync::{bytes, catalog::Catalog, failure};
use chilpa_sync::{Cancellation, RelativePath};

pub(super) fn folder(root: &Root, volume: Option<&chilpa_devices::volumes::Volume>) -> String {
    match root {
        Root::Mtp { folder } => folder.clone(),
        Root::Mounted { path } => volume.and_then(|v| path.strip_prefix(&v.path).ok())
            .filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new(path.file_name().unwrap_or_default()))
            .to_string_lossy().replace('\\', "/"),
    }
}
pub(crate) fn mounted() -> Destination {
    Destination { id:"mounted".into(), name:chilpa_i18n::t("Mounted storage"),
        storage:chilpa_i18n::t("Choose a folder on a memory card or USB drive"), available:String::new(), capacity:String::new(), writable:true, profiles:Vec::new() }
}
pub(crate) fn discover(catalog: &Catalog, cancel: &Cancellation) -> (Vec<Destination>, Option<UiError>) {
    let mut rows = Vec::new();
    let mut error = None;
    let isolated = std::env::var("CHILPA_DEVICE_TEST_MOUNTED_ONLY").as_deref() == Ok("1");
    if !isolated {
        match chilpa_devices::volumes::mounted() {
            Ok(volumes) => for volume in volumes {
                if cancel.check().is_err() { break; }
                let mut profiles = Vec::new();
                for profile in &catalog.profiles {
                    let relative = folder(&profile.root, Some(&volume));
                    let Ok(relative) = RelativePath::new(relative) else { continue };
                    match chilpa_devices::volumes::identity(&volume.path.join(relative.as_str())) {
                        Ok(Some(target)) if target.id == profile.target_id => profiles.push(profile.id.clone()),
                        Err(problem) if error.is_none() => error = Some(UiError::from(problem)),
                        _ => (),
                    }
                }
                rows.push(Destination { id:volume.id, name:volume.name, storage:chilpa_i18n::t("Memory card"), available:bytes(volume.available), capacity:bytes(volume.capacity), writable:volume.writable, profiles });
            },
            Err(problem) => error = Some(UiError::from(problem)),
        }
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        match chilpa_devices::transport::mtp::devices() {
            Err(problem) if error.is_none() => error = Some(UiError::from(problem)),
            Ok(devices) => {
                let roots: Vec<_> = catalog.profiles.iter().map(|p| folder(&p.root, None))
                    .collect::<std::collections::BTreeSet<_>>().into_iter().filter_map(|s| RelativePath::new(s).ok()).collect();
                if devices.len() > 8 { error = Some(failure(chilpa_i18n::mark("Connect at most eight USB players at a time."), "")); }
                for device in devices.into_iter().take(8) {
                    if let Err(problem) = cancel.check() { error = Some(UiError::from(problem)); break; }
                    match chilpa_devices::transport::mtp::describe_identities(device.location, &roots, cancel) {
                        Ok((description, identities)) => for volume in description.volumes {
                            if rows.len() >= 128 { error = Some(failure(chilpa_i18n::mark("Too many device storage locations were reported."), "")); break; }
                            let profiles = catalog.profiles.iter().filter(|profile| identities.iter().any(|(id, root, target)| *id == volume.id && *root == folder(&profile.root, None) && *target == profile.target_id)).map(|p| p.id.clone()).collect();
                            rows.push(Destination { id:format!("{}/{}", device.location, volume.id), name:if description.name.trim().is_empty() {device.name.clone()} else {description.name.clone()}, storage:volume.name, available:bytes(volume.available), capacity:bytes(volume.capacity), writable:volume.writable, profiles });
                        },
                        Err(problem) if error.is_none() => error = Some(UiError::from(problem)),
                        _ => (),
                    }
                }
            },
            _ => (),
        }
    }
    rows.push(mounted_for(catalog));
    (rows, error)
}

/// I validate fixed mounted roots without opening USB or acquiring a writable lease.
pub(crate) fn mounted_for(catalog: &Catalog) -> Destination {
    let mut manual = mounted();
    for profile in &catalog.profiles {
        if let Root::Mounted { path } = &profile.root {
            if chilpa_devices::volumes::identity(path).ok().flatten().is_some_and(|t| t.id == profile.target_id) {
                manual.profiles.push(profile.id.clone());
            }
        }
    }
    manual
}
