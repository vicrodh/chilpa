//! El identificador de esta instalación, que sirve de sal para el respaldo.
//!
//! **No es un secreto.** Guardarlo en claro está bien. Está para dos cosas:
//! que dos instalaciones en la misma máquina deriven llaves distintas, y que
//! mover el directorio a otra máquina rompa la derivación, porque allá el
//! `machine-id` es otro.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

const FILE: &str = "install-id";

pub(crate) fn load_or_create(storage_dir: &Path) -> std::io::Result<Uuid> {
    let path = storage_dir.join(FILE);

    if let Ok(contents) = fs::read_to_string(&path) {
        if let Ok(uuid) = Uuid::parse_str(contents.trim()) {
            return Ok(uuid);
        }
        // Uno ilegible se rehace. Lo que estaba cifrado con el anterior se
        // pierde, y eso es mejor que quedarse sin arrancar.
    }

    fs::create_dir_all(storage_dir)?;
    let uuid = Uuid::new_v4();
    // Se escribe aparte y se renombra: un corte de luz a media escritura
    // dejaría un id truncado, y un id truncado es una llave distinta.
    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(uuid.to_string().as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(uuid)
}

/// El identificador de la máquina, si el sistema da uno.
///
/// Que falte es aceptable: la derivación se queda con el UUID de
/// instalación, que sigue atado al equipo porque el directorio lo está.
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
        // Sin esto, Windows le abre una consola a `reg` y parpadea una caja
        // negra en cada arranque.
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
