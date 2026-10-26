//! I persist at most 64 storage profiles and atomically preserve damaged catalogs.
use super::{failure, UiError};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_PROFILES: usize = 64;
const MAX_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Root {
    Mtp { folder: String },
    Mounted { path: PathBuf },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Pick {
    Playlist {
        id: String,
        name: String,
        key: String,
    },
    Query {
        id: String,
        name: String,
        expression: String,
    },
    Tracks {
        id: String,
        name: String,
        keys: Vec<String>,
    },
}
impl Pick {
    pub fn id(&self) -> &str {
        match self {
            Self::Playlist { id, .. } | Self::Query { id, .. } | Self::Tracks { id, .. } => id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Playlist { name, .. } | Self::Query { name, .. } | Self::Tracks { name, .. } => {
                name
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub target_id: String,
    pub name: String,
    pub root: Root,
    pub picks: Vec<Pick>,
    pub remove_unselected: bool,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub profiles: Vec<Profile>,
}
impl Default for Catalog {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: Vec::new(),
        }
    }
}

impl Catalog {
    fn validate(&self) -> Result<(), UiError> {
        if self.version != 1 || self.profiles.len() > MAX_PROFILES {
            return Err(damaged());
        }
        let mut ids = std::collections::HashSet::new();
        let mut targets = std::collections::HashSet::new();
        for profile in &self.profiles {
            if !identifier(&profile.id)
                || !identifier(&profile.target_id)
                || !ids.insert(&profile.id)
                || !targets.insert(&profile.target_id)
                || label(&profile.name)? != profile.name
                || profile.picks.len() > 512
            {
                return Err(damaged());
            }
            match &profile.root {
                Root::Mtp { folder } => {
                    chilpa_sync::RelativePath::new(folder.clone()).map_err(UiError::from)?;
                }
                Root::Mounted { path } if !path.is_absolute() => return Err(damaged()),
                _ => (),
            }
            let mut selected = std::collections::HashSet::new();
            for pick in &profile.picks {
                if pick.id().is_empty() || !selected.insert(pick.id()) || pick.name().len() > 1024 {
                    return Err(damaged());
                }
                match pick {
                    Pick::Query { expression, .. } => {
                        if expression.len() > 16 * 1024 {
                            return Err(damaged());
                        }
                        crate::library_query::read(expression).map_err(|e| {
                            failure(chilpa_i18n::mark("The device query cannot be read."), e)
                        })?;
                    }
                    Pick::Tracks { keys, .. } if keys.len() > chilpa_sync::MAX_ASSETS => {
                        return Err(damaged())
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Store {
    pub directory: PathBuf,
}
fn damaged() -> UiError {
    failure(
        chilpa_i18n::mark("The device profile catalog is damaged. I kept its original data."),
        "",
    )
}
fn identifier(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn label(value: &str) -> Result<String, UiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control) {
        return Err(failure(
            chilpa_i18n::mark("Use a name of 1 to 80 characters, without line breaks."),
            "",
        ));
    }
    Ok(value.into())
}
impl Store {
    pub fn read(&self) -> Result<Catalog, UiError> {
        let file = match std::fs::File::open(self.directory.join("profiles.json")) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Catalog::default()),
            Err(e) => {
                return Err(failure(
                    chilpa_i18n::mark("I could not read the device profiles."),
                    e,
                ))
            }
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not read the device profiles."),
                    e,
                )
            })?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(damaged());
        }
        let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|_| damaged())?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut Catalog) -> Result<T, UiError>,
    ) -> Result<T, UiError> {
        std::fs::create_dir_all(&self.directory).map_err(|e| {
            failure(
                chilpa_i18n::mark("I could not save the device profiles."),
                e,
            )
        })?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("catalog.lock"))
            .map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not save the device profiles."),
                    e,
                )
            })?;
        lease.try_lock().map_err(|e| {
            failure(
                chilpa_i18n::mark("Another device profile update is still running."),
                e,
            )
        })?;
        let mut catalog = self.read()?;
        let before = catalog.clone();
        let value = change(&mut catalog)?;
        catalog.validate()?;
        if catalog == before {
            return Ok(value);
        }
        let bytes = serde_json::to_vec(&catalog).map_err(|e| {
            failure(
                chilpa_i18n::mark("I could not save the device profiles."),
                e,
            )
        })?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(failure(
                chilpa_i18n::mark("Device profiles exceed 1 MiB."),
                "",
            ));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory).map_err(|e| {
            failure(
                chilpa_i18n::mark("I could not save the device profiles."),
                e,
            )
        })?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not save the device profiles."),
                    e,
                )
            })?;
        temporary
            .persist(self.directory.join("profiles.json"))
            .map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not save the device profiles."),
                    e,
                )
            })?;
        Ok(value)
    }
    pub(crate) fn active_profile(&self) -> Option<String> {
        let file = std::fs::File::open(self.directory.join("active-profile")).ok()?;
        let mut bytes = Vec::new();
        file.take(33).read_to_end(&mut bytes).ok()?;
        let id = String::from_utf8(bytes).ok()?;
        identifier(&id).then_some(id)
    }
    pub(crate) fn remember_profile(&self, id: &str) -> Result<(), UiError> {
        if !identifier(id) { return Err(damaged()); }
        if self.active_profile().as_deref() == Some(id) { return Ok(()); }
        std::fs::create_dir_all(&self.directory).map_err(|e| failure(chilpa_i18n::mark("I could not save the device profiles."), e))?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory).map_err(|e| failure(chilpa_i18n::mark("I could not save the device profiles."), e))?;
        temporary.write_all(id.as_bytes()).and_then(|_| temporary.as_file().sync_all()).map_err(|e| failure(chilpa_i18n::mark("I could not save the device profiles."), e))?;
        temporary.persist(self.directory.join("active-profile")).map_err(|e| failure(chilpa_i18n::mark("I could not save the device profiles."), e))?;
        Ok(())
    }
    pub fn state_path(&self, id: &str) -> Result<PathBuf, UiError> {
        if !identifier(id) {
            return Err(damaged());
        }
        Ok(self.directory.join(format!("{id}.sqlite3")))
    }
}
pub fn local_path(value: &str) -> Result<PathBuf, UiError> {
    let path = if value.starts_with("file:") {
        url::Url::parse(value)
            .ok()
            .and_then(|u| u.to_file_path().ok())
    } else {
        Some(Path::new(value).to_path_buf())
    };
    path.filter(|p| p.is_absolute())
        .ok_or_else(|| failure(chilpa_i18n::mark("Choose a local storage folder."), ""))
}
