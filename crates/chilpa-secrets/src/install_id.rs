//! This installation's identifier, which serves as salt for the fallback.
//!
//! **It is not a secret.** Keeping it in the clear is fine. It is there for
//! two things: so two installations on the same machine derive different
//! keys, and so moving the directory to another machine breaks the
//! derivation, because the `machine-id` there is another one.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

const FILE: &str = "install-id";

/// The id this install already has, if any. Nothing is written.
pub(crate) fn load(storage_dir: &Path) -> Option<Uuid> {
    let contents = fs::read_to_string(storage_dir.join(FILE)).ok()?;
    Uuid::parse_str(contents.trim()).ok()
}

pub(crate) fn load_or_create(storage_dir: &Path) -> std::io::Result<Uuid> {
    if let Some(uuid) = load(storage_dir) {
        return Ok(uuid);
    }
    // A missing or unreadable one is made anew. Whatever was encrypted with
    // a previous one is lost, and that is better than failing to start.

    let path = storage_dir.join(FILE);
    fs::create_dir_all(storage_dir)?;
    let uuid = Uuid::new_v4();
    // It is written aside and renamed: a power cut mid write would leave a
    // truncated id, and a truncated id is a different key.
    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(uuid.to_string().as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(uuid)
}

/// The machine's identifier, if the system gives one.
///
/// Its absence is acceptable: the derivation falls back to the install
/// UUID, which is still tied to the machine because the directory is.
pub(crate) fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        for candidate in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(contents) = fs::read_to_string(candidate) {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output =
            Command::new("ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let Some(idx) = line.find("IOPlatformUUID") else { continue };
            let tail = &line[idx..];
            let Some(start) = tail.find('"').map(|i| i + 1) else { continue };
            let rest = &tail[start..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        // Without this, Windows opens a console for `reg` and flashes a black
        // box on every startup.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = Command::new("reg")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("MachineGuid") {
                return line.split_whitespace().last().map(str::to_string).filter(|s| !s.is_empty());
            }
        }
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}
