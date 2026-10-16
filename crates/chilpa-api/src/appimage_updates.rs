//! I run the bundled delta updater away from the UI and replace only verified images.
use crate::Chilpa;
use serde_json::{json, Value};
use semver::Version;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::{Path, PathBuf}, process::{Command, Stdio},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

#[derive(Clone)]
struct Installation { image: PathBuf, helper: PathBuf, restart: PathBuf, repository: String, version: Version,
    information: Option<String>, expected: Option<(String,u64)> }

pub(crate) struct Updates {
    installation: Option<Installation>,
    view: Arc<Mutex<Value>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Updates { fn drop(&mut self) { self.cancel.store(true, Ordering::Release); } }

impl Updates {
    fn new(automatic: bool) -> Self {
        let installation = chilpa_dirs::appimage().and_then(|image| {
            let path = chilpa_dirs::executable_helper("../share/chilpa/release.json")?;
            let bytes = fs::read(path).ok().filter(|bytes| bytes.len() < 4096)?;
            let release: Value = serde_json::from_slice(&bytes).ok()?;
            let repository = release["repository"].as_str()?.to_string();
            let parts = repository.split('/').collect::<Vec<_>>();
            if parts.len() != 2 || parts.iter().any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))) { return None; }
            let version = Version::parse(release["version"].as_str()?).ok()?;
            Some(Installation { image, helper: chilpa_dirs::executable_helper("chilpa-appimage-updater.AppImage")?,
                restart: chilpa_dirs::executable_helper("chilpa-restart")?, repository, version, information:None, expected:None })
        });
        let supported = installation.is_some();
        Self { installation, view: Arc::new(Mutex::new(json!({"supported":supported,
            "automatic":automatic,"phase":"idle","progress":0.0,"busy":false}))),
            cancel: Arc::new(AtomicBool::new(false)) }
    }
    fn start(&self, download: bool) -> bool {
        let Some(mut installation) = self.installation.clone() else { return false };
        {
            let mut state = self.view.lock().unwrap();
            if state["busy"] == true || state["phase"] == "ready" || download && state["phase"] != "available" { return false; }
            if download {
                let Some(tag) = state["tag"].as_str() else { return false; };
                installation.information = Some(information(&installation, tag));
                let (Some(hash),Some(size)) = (state["sha256"].as_str(),state["size"].as_u64()) else { return false; };
                installation.expected = Some((hash.into(),size));
            }
            state["busy"] = true.into();
            state["phase"] = if download { "downloading" } else { "checking" }.into();
            state["progress"] = 0.0.into();
            state["reason"] = "".into();
        }
        self.cancel.store(false, Ordering::Release);
        let (view, cancel) = (self.view.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let result = if download { download_image(&installation, &view, &cancel) }
                else { check_image(&installation, &view, &cancel) };
            let mut state = view.lock().unwrap();
            state["busy"] = false.into();
            state["reason"] = result.as_ref().err().filter(|reason| reason.as_str() == "read_only").cloned().unwrap_or_default().into();
            state["phase"] = match result { Ok(phase) => phase,
                Err(_) if cancel.load(Ordering::Acquire) => "cancelled", Err(_) => "error" }.into();
        });
        true
    }
}

fn command(installation: &Installation, image: &Path) -> Command {
    let mut process = Command::new(&installation.helper);
    process.arg("--appimage-extract-and-run").arg(image)
        .env_remove("LD_LIBRARY_PATH").env_remove("QT_PLUGIN_PATH")
        .env_remove("QML2_IMPORT_PATH").env_remove("QML_IMPORT_PATH")
        .stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(information) = &installation.information { process.arg("--update-info").arg(information); }
    #[cfg(target_os = "linux")]
    { use std::os::unix::process::CommandExt; process.process_group(0); }
    process
}

fn run(mut process: Command, cancel: &AtomicBool, limit: Duration, mut line: impl FnMut(&str)) -> Result<i32, String> {
    let mut child = process.spawn().map_err(|error| error.to_string())?;
    let output = child.stdout.take().ok_or("No updater output")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(32);
    let reader = std::thread::spawn(move || {
        let mut output = output;
        let mut bytes = [0; 4096];
        while let Ok(count) = output.read(&mut bytes) {
            if count == 0 || sender.send(bytes[..count].to_vec()).is_err() { break; }
        }
    });
    let started = Instant::now();
    let mut pending = String::new();
    let result = loop {
        if cancel.load(Ordering::Acquire) || started.elapsed() > limit {
            break Err("Update cancelled or timed out".into());
        }
        if let Ok(bytes) = receiver.recv_timeout(Duration::from_millis(50)) {
            pending.push_str(&String::from_utf8_lossy(&bytes));
            if pending.len() > 32768 { pending.clear(); }
            while let Some(end) = pending.find(['\r', '\n']) {
                line(&pending[..end]); pending.drain(..=end);
            }
        }
        let status = match child.try_wait() { Ok(status) => status, Err(error) => break Err(error.to_string()) };
        if let Some(status) = status {
            // I drain buffered output before interpreting a successful exit.
            for bytes in receiver.try_iter() { pending.push_str(&String::from_utf8_lossy(&bytes)); }
            for text in pending.split(['\r', '\n']) { line(text); }
            break Ok(status.code().unwrap_or(2));
        }
    };
    if result.is_err() {
        #[cfg(target_os = "linux")]
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL); }
        let _ = child.kill(); let _ = child.wait();
    }
    drop(receiver);
    let _ = reader.join();
    result
}

fn information(installation: &Installation, tag: &str) -> String {
    let (owner, repository) = installation.repository.split_once('/').unwrap();
    let version = tag.strip_prefix('v').unwrap_or(tag);
    format!("gh-releases-zsync|{owner}|{repository}|{tag}|Chilpa-{version}-x86_64.AppImage.zsync")
}

fn check_image(installation: &Installation, view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    let channel = if installation.version.pre.is_empty() { "releases/latest" } else { "releases?per_page=30" };
    let address = format!("https://api.github.com/repos/{}/{channel}", installation.repository);
    let response = chilpa_core::call_server(&chilpa_core::Call { method:"GET", url:&address,
        headers:&[("Accept".into(),"application/vnd.github+json".into())], body:None,
        timeout:Duration::from_secs(5), max_bytes:512 * 1024 }).map_err(|_| "The release service is unavailable")?;
    if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
    if response.status == 404 { return Ok("current"); }
    if response.status != 200 { return Err("The release service is unavailable".into()); }
    let document: Value = serde_json::from_slice(&response.body).map_err(|_| "Invalid release metadata")?;
    let Some(document) = select_release(&document, &installation.version) else { return Ok("current"); };
    let tag = document["tag_name"].as_str().ok_or("No release version")?;
    let (hash,size) = release_asset(&document, tag)?;
    let mut selected = installation.clone();
    selected.information = Some(information(installation, tag));
    let mut state = view.lock().unwrap();
    state["tag"] = tag.into();
    state["version"] = tag.strip_prefix('v').unwrap_or(tag).into();
    state["sha256"] = hash.into();
    state["size"] = size.into();
    drop(state);
    let mut process = command(&selected, &installation.image);
    process.arg("--check-for-update");
    match run(process, cancel, Duration::from_secs(30), |_| {})? { 0 => Ok("current"), 1 => Ok("available"),
        _ => Err("The update check failed".into()) }
}

fn candidate_version(document: &Value, current: &Version) -> Option<Version> {
    let tag = document["tag_name"].as_str()?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    let prerelease = !version.pre.is_empty();
    (document["draft"] == false && document["prerelease"] == prerelease
        && (!current.pre.is_empty() || !prerelease)
        && version.cmp_precedence(current).is_gt()).then_some(version)
}

fn select_release<'a>(document: &'a Value, current: &Version) -> Option<&'a Value> {
    let releases = document.as_array().map(Vec::as_slice).unwrap_or(std::slice::from_ref(document));
    releases.iter().filter_map(|release| candidate_version(release, current).map(|version| (release, version)))
        .max_by(|(_, first), (_, second)| first.cmp_precedence(second)).map(|(release, _)| release)
}

fn release_asset(document: &Value, tag: &str) -> Result<(String,u64), String> {
    let name = format!("Chilpa-{}-x86_64.AppImage", tag.strip_prefix('v').unwrap_or(tag));
    let assets = document["assets"].as_array().ok_or("No release assets")?;
    let mut images = assets.iter().filter(|asset| asset["name"] == name);
    let asset = images.next().ok_or("No AppImage for this platform")?;
    if images.next().is_some() || !assets.iter().any(|asset| asset["name"] == format!("{name}.zsync")) {
        return Err("Incomplete or ambiguous release assets".into());
    }
    let hash = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("No AppImage checksum")?;
    let size = asset["size"].as_u64().ok_or("No AppImage size")?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) || size == 0 || size > 2 * 1024 * 1024 * 1024 {
        return Err("Invalid AppImage checksum or size".into());
    }
    Ok((hash.to_ascii_lowercase(),size))
}

fn progress(line: &str) -> Option<f64> {
    let before = line.strip_suffix("% done").or_else(|| line.split_once("% done").map(|pair| pair.0))?;
    let value = before.trim().rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-')).next()?.parse::<f64>().ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value / 100.0)
}

fn download_image(installation: &Installation, view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    let parent = installation.image.parent().ok_or("No image folder")?;
    let directory = tempfile::Builder::new().prefix(".chilpa-update-").tempdir_in(parent).map_err(|e| {
        if matches!(e.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem) { "read_only".into() }
        else { e.to_string() }
    })?;
    let seed = directory.path().join("Chilpa-seed.AppImage");
    fs::hard_link(&installation.image, &seed).map_err(|e| e.to_string())?;
    let original = fs::metadata(&installation.image).map_err(|e| e.to_string())?;
    let mut unchanged = false;
    let result = run(command(installation, &seed), cancel, Duration::from_secs(1800), |line| {
        if let Some(value) = progress(line) { view.lock().unwrap()["progress"] = value.into(); }
        if line == "Update not required, exiting." { unchanged = true; }
    })?;
    if result != 0 { return Err("The update download failed".into()); }
    if unchanged { return Ok("current"); }
    let candidates = fs::read_dir(directory.path()).map_err(|e| e.to_string())?.filter_map(Result::ok)
        .map(|entry| entry.path()).filter(|path| path != &seed && path.extension().is_some_and(|ext| ext == "AppImage"))
        .collect::<Vec<_>>();
    if candidates.len() != 1 { return Err("The updater returned no unique image".into()); }
    let candidate = &candidates[0];
    let candidate = candidate.canonicalize().map_err(|e| e.to_string())?;
    if candidate.parent() != Some(directory.path()) || candidate.extension().is_none_or(|ext| ext != "AppImage") {
        return Err("The updated image is outside its staging folder".into());
    }
    let mut header = [0; 11];
    fs::File::open(&candidate).and_then(|mut file| file.read_exact(&mut header)).map_err(|e| e.to_string())?;
    if &header[..4] != b"\x7fELF" || &header[8..11] != b"AI\x02" { return Err("The update is not an AppImage".into()); }
    if let Some((expected,size)) = &installation.expected {
        let mut file = fs::File::open(&candidate).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != *size { return Err("The update size differs from its release".into()); }
        let mut digest = Sha256::new(); let mut buffer = [0; 65536];
        loop {
            if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 { break; }
            digest.update(&buffer[..count]);
        }
        if format!("{:x}",digest.finalize()) != *expected { return Err("The update checksum differs from its release".into()); }
    }
    let current = fs::metadata(&installation.image).map_err(|e| e.to_string())?;
    if current.len() != original.len() || current.modified().ok() != original.modified().ok() {
        return Err("The installed image changed during its update".into());
    }
    {
        let mut state = view.lock().unwrap();
        if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
        state["phase"] = "installing".into();
    }
    replace_image(&installation.image, &seed, &candidate)?;
    view.lock().unwrap()["progress"] = 1.0.into();
    Ok("ready")
}

fn replace_image(image: &Path, seed: &Path, candidate: &Path) -> Result<(), String> {
    let file = fs::OpenOptions::new().write(true).open(candidate).map_err(|e| e.to_string())?;
    file.set_permissions(fs::metadata(image).map_err(|e| e.to_string())?.permissions()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    let mut previous = image.as_os_str().to_os_string(); previous.push(".previous");
    fs::rename(seed, PathBuf::from(previous)).map_err(|e| e.to_string())?;
    fs::rename(candidate, image).map_err(|e| e.to_string())?;
    fs::File::open(image.parent().ok_or("No image folder")?).and_then(|file| file.sync_all()).map_err(|e| e.to_string())?;
    Ok(())
}

impl Chilpa {
    fn updates(&mut self) -> &Updates {
        if self.appimage_updates.is_none() {
            self.appimage_updates = Some(Updates::new(self.preference("updates.automatic").as_deref() != Some("false")));
        }
        self.appimage_updates.as_ref().unwrap()
    }
    pub fn appimage_updates_json(&mut self) -> String { self.updates().view.lock().unwrap().to_string() }
    pub fn appimage_updates_check(&mut self, automatic: bool) {
        if automatic {
            let previous = self.preference("updates.last_check").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            if now.saturating_sub(previous) < 86400 || self.updates().view.lock().unwrap()["automatic"] != true { return; }
            if self.updates().start(false) { self.remember_preference("updates.last_check", &now.to_string()); }
        } else { self.updates().start(false); }
    }
    pub fn appimage_updates_download(&mut self) { self.updates().start(true); }
    pub fn appimage_updates_cancel(&mut self) {
        let updates = self.updates();
        let mut state = updates.view.lock().unwrap();
        if state["busy"] == true && state["phase"] != "installing" {
            updates.cancel.store(true, Ordering::Release);
            state["phase"] = "cancelling".into();
        }
    }
    pub fn appimage_updates_automatic(&mut self, on: bool) {
        self.remember_preference("updates.automatic", if on { "true" } else { "false" });
        self.updates().view.lock().unwrap()["automatic"] = on.into();
    }
    pub fn appimage_updates_restart(&mut self) -> bool {
        let updates = self.updates();
        if updates.view.lock().unwrap()["phase"] != "ready" { return false; }
        let Some(installation) = &updates.installation else { return false };
        Command::new(&installation.restart).arg(std::process::id().to_string()).arg(&installation.image)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_accepts_cli_records_and_rejects_invalid_percentages() {
        assert_eq!(progress("\x1b[2K12.5% done (3 of 24 MiB)..."), Some(0.125));
        assert_eq!(progress("100% done"), Some(1.0));
        for text in ["101% done", "-1% done", "NaN% done", "a status message"] { assert_eq!(progress(text), None); }
    }
    #[test]
    fn release_checks_never_downgrade_or_offer_prereleases() {
        let current = Version::parse("1.9.0").unwrap();
        for tag in ["v1.8.0","v1.9.0","v1.9.0+other","v2.0.0-rc.1"] {
            let document = json!({"tag_name":tag,"draft":false,"prerelease":false});
            assert!(select_release(&document, &current).is_none());
        }
        assert!(select_release(&json!({"tag_name":"v1.10.0","draft":false,"prerelease":false}), &current).is_some());
        assert!(select_release(&json!({"tag_name":"v2.0.0","draft":true,"prerelease":false}), &current).is_none());
    }
    #[test]
    fn beta_checks_select_the_newest_published_version_without_affecting_stable_installations() {
        let current = Version::parse("0.1.0-beta").unwrap();
        let mut releases = json!([
            {"tag_name":"v0.1.0-beta.2","draft":false,"prerelease":true},
            {"tag_name":"v0.1.0-beta.10","draft":false,"prerelease":true},
            {"tag_name":"v0.2.0-beta","draft":true,"prerelease":true},
            {"tag_name":"not-a-version","draft":false,"prerelease":true},
            {"tag_name":"v0.2.0-beta","draft":false,"prerelease":false}
        ]);
        assert_eq!(select_release(&releases, &current).unwrap()["tag_name"], "v0.1.0-beta.10");
        assert!(select_release(&releases, &Version::new(0, 0, 0)).is_none());
        releases.as_array_mut().unwrap().push(json!({"tag_name":"v0.1.0","draft":false,"prerelease":false}));
        assert_eq!(select_release(&releases, &current).unwrap()["tag_name"], "v0.1.0");
        assert_eq!(select_release(&releases, &Version::new(0, 0, 0)).unwrap()["tag_name"], "v0.1.0");
        assert!(select_release(&releases, &Version::new(0, 1, 0)).is_none());
    }
    #[test]
    fn an_update_requires_the_selected_platform_delta_and_checksum() {
        let hash = "a".repeat(64);
        let mut document = json!({"assets":[{"name":"Chilpa-1.10.0-x86_64.AppImage","size":1024,"digest":format!("sha256:{hash}")},
            {"name":"Chilpa-1.10.0-x86_64.AppImage.zsync"}]});
        assert_eq!(release_asset(&document,"v1.10.0").unwrap(), (hash,1024));
        assert!(release_asset(&document,"v1.9.0").is_err());
        document["assets"][0]["digest"] = Value::Null;
        assert!(release_asset(&document,"v1.10.0").is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn replacement_is_atomic_and_keeps_one_previous_image() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Player.AppImage");
        let seed = directory.path().join("seed");
        let candidate = directory.path().join("candidate");
        fs::write(&image, b"original").unwrap(); fs::hard_link(&image, &seed).unwrap();
        fs::write(&candidate, b"new").unwrap();
        replace_image(&image, &seed, &candidate).unwrap();
        assert_eq!(fs::read(&image).unwrap(), b"new");
        assert_eq!(fs::read(directory.path().join("Player.AppImage.previous")).unwrap(), b"original");
        fs::hard_link(&image, &seed).unwrap(); fs::write(&candidate, b"next").unwrap();
        replace_image(&image, &seed, &candidate).unwrap();
        assert_eq!(fs::read(directory.path().join("Player.AppImage.previous")).unwrap(), b"new");
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn failed_replacement_preserves_the_running_image() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Player.AppImage");
        fs::write(&image, b"original").unwrap();
        assert!(replace_image(&image, &directory.path().join("seed"), &directory.path().join("missing")).is_err());
        assert_eq!(fs::read(image).unwrap(), b"original");
    }
    #[cfg(target_os = "linux")]
    fn helper(directory: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("updater");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn a_download_is_staged_before_replacement_and_reports_progress() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Player.AppImage");
        fs::write(&image, b"original").unwrap();
        let executable = helper(directory.path(), "printf '42%% done\\r'\nprintf '\\177ELF\\000\\000\\000\\000AI\\002new' > \"$(dirname \"$2\")/New.AppImage\"");
        let installation = Installation { image: image.clone(), helper: executable, restart: PathBuf::new(),
            repository:"example/fixture".into(), version:Version::new(0,0,0), information:None, expected:None };
        let view = Mutex::new(json!({}));
        assert_eq!(download_image(&installation, &view, &AtomicBool::new(false)).unwrap(), "ready");
        assert_eq!(fs::read(&image).unwrap(), b"\x7fELF\0\0\0\0AI\x02new");
        assert_eq!(fs::read(directory.path().join("Player.AppImage.previous")).unwrap(), b"original");
        assert_eq!(view.lock().unwrap()["progress"], 1.0);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn a_checksum_mismatch_never_replaces_the_running_image() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Player.AppImage");
        fs::write(&image,b"original").unwrap();
        let executable = helper(directory.path(),"printf '\\177ELF\\000\\000\\000\\000AI\\002new' > \"$(dirname \"$2\")/New.AppImage\"");
        let installation = Installation {image:image.clone(),helper:executable,restart:PathBuf::new(),
            repository:"example/fixture".into(),version:Version::new(0,0,0),information:None,expected:Some(("0".repeat(64),14))};
        assert!(download_image(&installation,&Mutex::new(json!({})),&AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(image).unwrap(),b"original");
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cancellation_terminates_the_owned_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let executable = helper(directory.path(), "sleep 60");
        let installation = Installation { image: PathBuf::new(), helper: executable, restart: PathBuf::new(),
            repository:"example/fixture".into(), version:Version::new(0,0,0), information:None, expected:None };
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let trigger = std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(100)); flag.store(true, Ordering::Release); });
        let started = Instant::now();
        assert!(run(command(&installation, Path::new("unused")), &cancel, Duration::from_secs(5), |_| {}).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        trigger.join().unwrap();
    }
}
