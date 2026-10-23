//! Package swaps retain the previous directory until startup succeeds.
use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, io::Write};
use chilpa_plugin::manifest::Asked;

/// What the host keeps for the listener inside a plugin's folder, apart
/// from the package: the WASM store and the sealed credential. A swap hands
/// them to whichever package ends up in place, and never leaves a copy.
pub(crate) const STORE: &str = "store";
pub(crate) const CREDENTIAL: &str = "credential.sealed";
const HOST_STATE: [&str; 2] = [STORE, CREDENTIAL];

#[derive(serde::Serialize, serde::Deserialize)]
struct Permissions { existed: bool, granted: Option<Asked> }

pub(crate) struct Replacement {
    root: PathBuf,
    id: String,
    committed: bool,
}

impl Replacement {
    pub(crate) fn begin(root: &Path, id: &str, incoming: &Path) -> Result<Self, String> {
        if !valid(id) { return Err("Invalid plugin identity.".into()); }
        let backup = root.join(format!(".rollback-{id}"));
        if backup.exists() { return Err("A previous plugin update needs recovery. Restart Chilpa.".into()); }
        std::fs::create_dir(&backup).map_err(|_| "The plugin backup could not be created.")?;
        let approvals = root.join("approvals.json");
        let granted: BTreeMap<String, Asked> = std::fs::read(&approvals).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        let snapshot = Permissions { existed:approvals.exists(), granted:granted.get(id).cloned() };
        if std::fs::write(backup.join("permissions.json"), serde_json::to_vec(&snapshot).map_err(|_| "The plugin permissions are invalid.")?).is_err() {
            let _ = std::fs::remove_dir_all(&backup);
            return Err("The plugin permissions could not be backed up.".into());
        }
        let target = root.join(id);
        if target.exists() {
            if std::fs::rename(&target, backup.join("previous")).is_err() {
                let _ = std::fs::remove_dir(&backup);
                return Err("The previous plugin could not be backed up.".into());
            }
        } else if std::fs::write(backup.join("new"), b"").is_err() {
            let _ = std::fs::remove_dir(&backup);
            return Err("The plugin transaction could not be saved.".into());
        }
        let transaction = Self { root: root.into(), id: id.into(), committed: false };
        std::fs::rename(incoming, &target).map_err(|_| "The plugin could not be put in place.")?;
        let previous = backup.join("previous");
        for name in HOST_STATE {
            let (from, to) = (previous.join(name), target.join(name));
            if from.symlink_metadata().is_err() { continue; }
            // The listener's state wins over anything a package ships under these names.
            if to.is_dir() { let _ = std::fs::remove_dir_all(&to); } else { let _ = std::fs::remove_file(&to); }
            std::fs::rename(from, to).map_err(|_| "The plugin's saved data could not be kept.")?;
        }
        Ok(transaction)
    }

    pub(crate) fn commit(mut self) -> Result<(), String> {
        // Keep one previous package for delayed activation of disabled plugins.
        let backup = self.root.join(format!(".rollback-{}", self.id));
        let garbage = self.root.join(format!(".previous-{}", self.id));
        if garbage.exists() { std::fs::remove_dir_all(&garbage).map_err(|_| "The previous plugin backup could not be cleared.")?; }
        std::fs::rename(backup, &garbage).map_err(|_| "The plugin update could not be committed.")?;
        self.committed = true;
        if !garbage.join("previous").exists() { let _ = std::fs::remove_dir_all(garbage); }
        Ok(())
    }
}

impl Drop for Replacement {
    fn drop(&mut self) {
        if !self.committed { let _ = recover(&self.root, &self.id); }
    }
}

fn valid(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}

fn recover(root: &Path, id: &str) -> Result<(), String> {
    let backup = root.join(format!(".rollback-{id}"));
    let previous = backup.join("previous");
    let target = root.join(id);
    if previous.exists() || backup.join("new").exists() {
        // State the swap already handed over goes back; what the previous
        // package still holds was never moved and stays.
        for name in HOST_STATE {
            let (from, to) = (target.join(name), previous.join(name));
            if previous.exists() && from.symlink_metadata().is_ok() && to.symlink_metadata().is_err() {
                std::fs::rename(from, to).map_err(|_| "The plugin's saved data could not be restored.")?;
            }
        }
        if target.exists() { std::fs::remove_dir_all(&target).map_err(|_| "The failed plugin could not be removed.")?; }
        if previous.exists() { std::fs::rename(previous, target).map_err(|_| "The previous plugin could not be restored.")?; }
    }
    if let Ok(bytes) = std::fs::read(backup.join("permissions.json")) {
        let before: Permissions = serde_json::from_slice(&bytes).map_err(|_| "The plugin permission backup is invalid.")?;
        let path = root.join("approvals.json");
        let mut approved: BTreeMap<String, Asked> = std::fs::read(&path).ok().and_then(|bytes|serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        if let Some(granted) = before.granted { approved.insert(id.into(), granted); } else { approved.remove(id); }
        if !before.existed && approved.is_empty() {
            if path.exists() { std::fs::remove_file(&path).map_err(|_| "The plugin permissions could not be restored.")?; }
        } else {
            let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| "The plugin permissions could not be restored.")?;
            file.write_all(&serde_json::to_vec_pretty(&approved).map_err(|_| "The plugin permissions are invalid.")?).map_err(|_| "The plugin permissions could not be restored.")?;
            file.persist(path).map_err(|_| "The plugin permissions could not be restored.")?;
        }
    }
    std::fs::remove_dir_all(backup).map_err(|_| "The plugin recovery journal could not be cleared.".into())
}

pub(crate) fn rollback_previous(root: &Path, id: &str) -> Result<bool, String> {
    if !valid(id) { return Err("Invalid plugin identity.".into()); }
    let previous = root.join(format!(".previous-{id}"));
    if !previous.join("previous").exists() { return Ok(false); }
    std::fs::rename(previous, root.join(format!(".rollback-{id}"))).map_err(|_| "The previous plugin could not be restored.")?;
    recover(root, id)?;
    Ok(true)
}

pub(crate) fn recover_all(root: &Path) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(root) else { return Ok(()) };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(id) = name.strip_prefix(".rollback-").filter(|id| valid(id)) { recover(root, id)?; }
        if name.starts_with(".retired-") { let _ = std::fs::remove_dir_all(entry.path()); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_startup_and_interrupted_updates_restore_the_previous_package() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target = root.join("app.test.one");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("plugin"), "old").unwrap();
        let incoming = root.join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        std::fs::write(incoming.join("plugin"), "new").unwrap();
        let swap = Replacement::begin(root, "app.test.one", &incoming).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("plugin")).unwrap(), "new");
        std::mem::forget(swap); // Model a process exiting before startup completed.
        recover_all(root).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("plugin")).unwrap(), "old");
        std::fs::create_dir(&incoming).unwrap();
        let swap = Replacement::begin(root, "app.test.one", &incoming).unwrap();
        drop(swap);
        assert_eq!(std::fs::read_to_string(target.join("plugin")).unwrap(), "old");
        std::fs::create_dir(&incoming).unwrap();
        std::fs::write(incoming.join("plugin"), "good").unwrap();
        Replacement::begin(root, "app.test.one", &incoming).unwrap().commit().unwrap();
        recover_all(root).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("plugin")).unwrap(), "good");
    }

    /// A plugin folder with an old package, a store key and a sealed credential.
    fn installed_with_state(root: &Path) -> PathBuf {
        let target = root.join("app.test.one");
        std::fs::create_dir_all(target.join("store")).unwrap();
        std::fs::write(target.join("plugin.wasm"), "old").unwrap();
        std::fs::write(target.join("store").join("cursor"), "kept").unwrap();
        std::fs::write(target.join("credential.sealed"), "sealed").unwrap();
        target
    }

    fn incoming(root: &Path, program: &str) -> PathBuf {
        let incoming = root.join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        std::fs::write(incoming.join("plugin.wasm"), program).unwrap();
        incoming
    }

    fn assert_state(target: &Path, program: &str) {
        assert_eq!(std::fs::read_to_string(target.join("plugin.wasm")).unwrap(), program);
        assert_eq!(std::fs::read_to_string(target.join("store").join("cursor")).unwrap(), "kept", "the plugin's store was lost");
        assert_eq!(std::fs::read_to_string(target.join("credential.sealed")).unwrap(), "sealed", "the sealed credential was lost");
    }

    #[test]
    fn an_update_keeps_the_store_and_the_sealed_credential_and_leaves_no_copy_behind() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target = installed_with_state(root);
        Replacement::begin(root, "app.test.one", &incoming(root, "new")).unwrap().commit().unwrap();
        assert_state(&target, "new");
        let kept = root.join(".previous-app.test.one").join("previous");
        assert!(kept.join("plugin.wasm").exists(), "the previous package stays for a rollback");
        assert!(!kept.join("credential.sealed").exists() && !kept.join("store").exists(),
            "a second copy would outlive disconnecting the account");
    }

    #[test]
    fn a_failed_update_and_a_rollback_hand_the_state_back_to_the_previous_package() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target = installed_with_state(root);
        drop(Replacement::begin(root, "app.test.one", &incoming(root, "broken")).unwrap());
        assert_state(&target, "old");
        Replacement::begin(root, "app.test.one", &incoming(root, "new")).unwrap().commit().unwrap();
        assert!(rollback_previous(root, "app.test.one").unwrap());
        assert_state(&target, "old");
    }
}
