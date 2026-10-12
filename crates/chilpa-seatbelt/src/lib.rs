//! macOS process isolation for private plugin smoke builds.
//!
//! Apple deprecated `sandbox-exec` and does not document its profile language
//! for third-party products. This launcher needs a supported sandbox before
//! it can be used in a public release.
#![cfg(target_os = "macos")]

use chilpa_plugin::native::NativeClient;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// Host-selected paths. The manifest cannot replace any of them.
pub struct NativePaths {
    pub folder: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub export: Option<PathBuf>,
}

/// Starts one process with read access to its package and system libraries,
/// write access to its private storage, and no access to the rest of home.
pub fn launch_native(
    paths: &NativePaths,
    name: &str,
    notice: impl Fn(&str, &Value) + Send + 'static,
) -> Result<Arc<NativeClient>, std::io::Error> {
    let folder = directory(&paths.folder)?;
    std::fs::create_dir_all(&paths.data)?;
    std::fs::create_dir_all(&paths.cache)?;
    let data = directory(&paths.data)?;
    let cache = directory(&paths.cache)?;
    let export = paths.export.as_deref().map(directory).transpose()?;
    let executable = folder.join("plugin");
    if !executable.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "plugin executable is missing",
        ));
    }
    let profile = profile(&executable, &folder, &data, &cache, export.as_deref())?;
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command
        .arg("-p")
        .arg(profile)
        .arg(&executable)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &data)
        .env("TMPDIR", format!("{}/", cache.display()))
        .env("XDG_CACHE_HOME", &cache)
        .env("CHILPA_PLUGIN_DATA_DIR", &data)
        .env("CHILPA_PLUGIN_CACHE_DIR", &cache)
        .current_dir(&data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(export) = &export {
        command.env("CHILPA_PLUGIN_EXPORT_DIR", export);
    }
    let mut child = command.spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("plugin stdin is missing"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("plugin stdout is missing"))?;
    if let Some(errors) = child.stderr.take() {
        let name = name.to_string();
        let _ = std::thread::Builder::new()
            .name("chilpa-native-err".into())
            .spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(errors)
                    .lines()
                    .map_while(Result::ok)
                {
                    eprintln!("plugin[{name}]: {line}");
                }
            });
    }
    Ok(NativeClient::start(
        input,
        output,
        move || {
            let _ = child.kill();
            let _ = child.wait();
        },
        notice,
    ))
}

fn directory(path: &Path) -> Result<PathBuf, std::io::Error> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(std::io::Error::other(
            "a plugin directory is not a directory",
        ));
    }
    Ok(path)
}

fn quoted(path: &Path) -> Result<String, std::io::Error> {
    let path = path
        .to_str()
        .ok_or_else(|| std::io::Error::other("plugin path is not UTF-8"))?;
    if path
        .chars()
        .any(|ch| ch == '"' || ch == '\\' || ch.is_control())
    {
        return Err(std::io::Error::other(
            "plugin path contains an unsupported character",
        ));
    }
    serde_json::to_string(path).map_err(std::io::Error::other)
}

fn profile(
    executable: &Path,
    folder: &Path,
    data: &Path,
    cache: &Path,
    export: Option<&Path>,
) -> Result<String, std::io::Error> {
    let mut readable = String::from("(literal \"/\") (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") \
        (subpath \"/Library\") (subpath \"/dev\") (literal \"/private\") (literal \"/private/var\") \
        (subpath \"/private/etc\") (subpath \"/private/var/db\") (subpath \"/private/var/run\")");
    for path in [folder, data, cache].into_iter().chain(export) {
        readable.push_str(&format!(" (subpath {})", quoted(path)?));
        for parent in path.ancestors().skip(1) {
            readable.push_str(&format!(" (literal {})", quoted(parent)?));
        }
    }
    let mut writable = format!("(subpath {}) (subpath {})", quoted(data)?, quoted(cache)?);
    if let Some(export) = export {
        writable.push_str(&format!(" (subpath {})", quoted(export)?));
    }
    Ok(format!("(version 1)\n(deny default)\n(allow process-exec (literal {}))\n(allow file-read* {readable})\n(allow file-write* {writable})\n(allow mach-lookup)\n(allow sysctl-read)\n(allow network*)\n", quoted(executable)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_can_write_its_data_but_not_read_home_or_write_its_package() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let folder = root.join("package");
        let data = root.join("data");
        let cache = root.join("cache");
        for path in [&folder, &data, &cache] {
            std::fs::create_dir(path).unwrap();
        }
        let outside = root.join("outside.txt");
        std::fs::write(&outside, "private").unwrap();
        let profile = profile(Path::new("/bin/bash"), &folder, &data, &cache, None).unwrap();
        let output = Command::new("/usr/bin/sandbox-exec")
            .arg("-p").arg(profile).arg("/bin/bash").arg("-c")
            .arg("read value < \"$OUTSIDE\"; printf '%s' \"$value\"; printf inside > \"$DATA/inside.txt\"; printf altered > \"$PACKAGE/blocked.txt\"")
            .env_clear().env("OUTSIDE", &outside).env("DATA", &data).env("PACKAGE", &folder)
            .output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty(), "an outside file was read");
        assert!(
            data.join("inside.txt").is_file(),
            "private storage was not writable"
        );
        assert!(
            !folder.join("blocked.txt").exists(),
            "the package was writable"
        );
    }
}
