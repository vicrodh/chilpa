//! I run the bundled delta updater away from the UI and replace only verified images.
use crate::{bundle_updates::{self as bundle, Native}, Chilpa};
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
    native: Option<Native>,
    view: Arc<Mutex<Value>>,
    cancel: Arc<AtomicBool>,
    launch_checked: AtomicBool,
}

impl Drop for Updates { fn drop(&mut self) { self.cancel.store(true, Ordering::Release); } }

impl Updates {
    fn new(automatic: bool) -> Self {
        let installation = chilpa_dirs::appimage().and_then(|image| {
            let path = chilpa_dirs::executable_helper("../share/chilpa/release.json")?;
            let bytes = fs::read(path).ok().filter(|bytes| bytes.len() < 4096)?;
            let release: Value = serde_json::from_slice(&bytes).ok()?;
            let repository = bundle::repository_name(release["repository"].as_str()?)?;
            let version = Version::parse(release["version"].as_str()?).ok()?;
            Some(Installation { image, helper: chilpa_dirs::executable_helper("chilpa-appimage-updater.AppImage")?,
                restart: chilpa_dirs::executable_helper("chilpa-restart")?, repository, version, information:None, expected:None })
        });
        let native = if installation.is_none() { bundle::detect() } else { None };
        let can_install = installation.is_some() || native.is_some();
        let current_version = installation.as_ref().map(|image| image.version.clone()).or_else(|| native.as_ref().map(|found| found.version.clone()))
            .map_or_else(|| env!("CARGO_PKG_VERSION").to_string(), |version| version.to_string());
        let (label, kind) = match (&installation, &native) {
            (Some(_), _) => ("AppImage", "appimage"),
            (None, Some(found)) => (found.kind.label(), found.kind.key()),
            (None, None) => ("System / manual", "manual"),
        };
        Self { installation, native, view: Arc::new(Mutex::new(json!({"supported":true,"can_install":can_install,
            "current_version":current_version,"installation":label,"kind":kind,
            "automatic":automatic,"phase":"idle","progress":0.0,"busy":false}))),
            cancel: Arc::new(AtomicBool::new(false)), launch_checked: AtomicBool::new(false) }
    }
    fn claim_launch_check(&self) -> bool {
        !self.launch_checked.swap(true, Ordering::AcqRel) && self.view.lock().unwrap()["automatic"] == true
    }
    fn start(&self, download: bool) -> bool {
        if chilpa_core::network::current().offline { return false; }
        let mut installation = self.installation.clone();
        let native = self.native.clone();
        if download && installation.is_none() && native.is_none() { return false; }
        let mut release = None;
        {
            let mut state = self.view.lock().unwrap();
            if state["busy"] == true || state["phase"] == "ready" || download && state["phase"] != "available" { return false; }
            if download {
                let Some(tag) = state["tag"].as_str() else { return false; };
                let (Some(hash),Some(size)) = (state["sha256"].as_str(),state["size"].as_u64()) else { return false; };
                if let Some(image) = installation.as_mut() {
                    image.information = Some(information(image, tag));
                    image.expected = Some((hash.into(),size));
                }
                release = Some((tag.to_string(), hash.to_string(), size));
            }
            if !download {
                for key in ["tag", "version", "release_url", "sha256", "size"] { state.as_object_mut().unwrap().remove(key); }
            }
            state["busy"] = true.into();
            state["phase"] = if download { "downloading" } else { "checking" }.into();
            state["progress"] = 0.0.into();
            state["reason"] = "".into();
        }
        self.cancel.store(false, Ordering::Release);
        let (view, cancel) = (self.view.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let result = match (download, &installation, &native, &release) {
                (true, Some(image), _, _) => download_image(image, &view, &cancel),
                (true, None, Some(found), Some((tag, hash, size))) => download_native(found, tag, hash, *size, &view, &cancel),
                (true, ..) => Err("No installable release".into()),
                (false, Some(image), ..) => check_image(image, &view, &cancel),
                (false, None, Some(found), _) => check_native(found, &view, &cancel),
                (false, None, None, _) => check_manual(&view, &cancel),
            };
            finish(&mut view.lock().unwrap(), result, cancel.load(Ordering::Acquire));
        });
        true
    }
}

fn finish(state: &mut Value, result: Result<&str, String>, cancelled: bool) {
    state["busy"] = false.into();
    state["reason"] = result.as_ref().err().filter(|reason| matches!(reason.as_str(), "read_only" | "rate_limited")).cloned().unwrap_or_default().into();
    state["phase"] = match result {
        Ok("ready") => "ready",
        _ if cancelled => "cancelled",
        Ok(phase) => phase,
        Err(_) => "error",
    }.into();
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
    if chilpa_core::network::current().offline { return Err(chilpa_core::network::blocked()); }
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
        if cancel.load(Ordering::Acquire) || chilpa_core::network::current().offline || started.elapsed() > limit {
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
    let architecture = std::env::consts::ARCH;
    format!("gh-releases-zsync|{owner}|{repository}|{tag}|Chilpa-{version}-{architecture}.AppImage.zsync")
}

fn latest_release(repository: &str, version: &Version, cancel: &AtomicBool) -> Result<Option<Value>, String> {
    if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
    let channel = if version.pre.is_empty() { "releases/latest" } else { "releases?per_page=30" };
    let address = format!("https://api.github.com/repos/{repository}/{channel}");
    let response = chilpa_core::call_server(&chilpa_core::Call { method:"GET", url:&address,
        headers:&[("Accept".into(),"application/vnd.github+json".into())], body:None,
        timeout:Duration::from_secs(5), max_bytes:512 * 1024 }).map_err(|_| "The release service is unavailable")?;
    if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
    release_response(response.status, &response.body, version)
}

fn release_response(status: u16, body: &[u8], version: &Version) -> Result<Option<Value>, String> {
    if matches!(status, 403 | 429) { return Err("rate_limited".into()); }
    if status != 200 { return Err("The release service is unavailable".into()); }
    let document: Value = serde_json::from_slice(body).map_err(|_| "Invalid release metadata")?;
    let is_release = |value: &Value| value["tag_name"].is_string() && value["draft"].is_boolean() && value["prerelease"].is_boolean();
    if !document.as_array().map_or_else(|| is_release(&document), |items| items.iter().all(is_release)) {
        return Err("Invalid release metadata".into());
    }
    let releases = document.as_array().map(Vec::as_slice).unwrap_or(std::slice::from_ref(&document));
    if !releases.is_empty() && !releases.iter().any(|item| item["tag_name"].as_str().is_some_and(|tag| Version::parse(tag.strip_prefix('v').unwrap_or(tag)).is_ok())) {
        return Err("Invalid release metadata".into());
    }
    Ok(select_release(&document, version).cloned())
}

fn note_release(view: &Mutex<Value>, repository: &str, document: &Value) -> Result<(), String> {
    let tag = document["tag_name"].as_str().ok_or("No release version")?;
    let mut state = view.lock().unwrap();
    state["tag"] = tag.into();
    state["version"] = tag.strip_prefix('v').unwrap_or(tag).into();
    state["release_url"] = format!("https://github.com/{repository}/releases/tag/{tag}").into();
    Ok(())
}

fn check_manual(view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    let version = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| "Invalid application version")?;
    let repository = bundle::REPOSITORY;
    let Some(document) = latest_release(repository, &version, cancel)? else { return Ok("current"); };
    note_release(view, repository, &document)?;
    Ok("available")
}

fn check_native(native: &Native, view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    let Some(document) = latest_release(&native.repository, &native.version, cancel)? else { return Ok("current"); };
    let tag = document["tag_name"].as_str().ok_or("No release version")?;
    let (hash, size) = asset_digest(&document, &bundle::asset_name(native.kind, tag.strip_prefix('v').unwrap_or(tag)), false)?;
    note_release(view, &native.repository, &document)?;
    let mut state = view.lock().unwrap();
    state["sha256"] = hash.into();
    state["size"] = size.into();
    Ok("available")
}

fn download_native(native: &Native, tag: &str, hash: &str, size: u64, view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    bundle::install(&bundle::Plan { native, tag, hash, size }, cancel, &|done| view.lock().unwrap()["progress"] = done.into())?;
    Ok("ready")
}

fn check_image(installation: &Installation, view: &Mutex<Value>, cancel: &AtomicBool) -> Result<&'static str, String> {
    let Some(document) = latest_release(&installation.repository, &installation.version, cancel)? else { return Ok("current"); };
    let tag = document["tag_name"].as_str().ok_or("No release version")?;
    let (hash,size) = release_asset(&document, tag)?;
    note_release(view, &installation.repository, &document)?;
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
    let name = format!("Chilpa-{}-{}.AppImage", tag.strip_prefix('v').unwrap_or(tag), std::env::consts::ARCH);
    asset_digest(document, &name, true)
}

/// The listed size and SHA-256 of one release file; an AppImage also needs its delta file beside it.
fn asset_digest(document: &Value, name: &str, delta: bool) -> Result<(String,u64), String> {
    let assets = document["assets"].as_array().ok_or("No release assets")?;
    let mut files = assets.iter().filter(|asset| asset["name"] == name);
    let asset = files.next().ok_or("No release file for this platform")?;
    if files.next().is_some() || delta && !assets.iter().any(|asset| asset["name"] == format!("{name}.zsync")) {
        return Err("Incomplete or ambiguous release assets".into());
    }
    let hash = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("No release checksum")?;
    let size = asset["size"].as_u64().ok_or("No release size")?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) || size == 0 || size > 2 * 1024 * 1024 * 1024 {
        return Err("Invalid release checksum or size".into());
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
    stage_seed(&installation.image, &seed, |from, to| fs::hard_link(from, to))?;
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
    let version = view.lock().unwrap()["version"].as_str().map(str::to_owned);
    if let Some(version) = version { bundle::refresh_receipt(&installation.image, &version); }
    view.lock().unwrap()["progress"] = 1.0.into();
    Ok("ready")
}

/// A hard link costs nothing; filesystems that cannot make one (exFAT, some network mounts) get a copy.
fn stage_seed(image: &Path, seed: &Path, link: impl Fn(&Path, &Path) -> std::io::Result<()>) -> Result<(), String> {
    if link(image, seed).is_ok() { return Ok(()); }
    fs::copy(image, seed).map(|_| ()).map_err(|e| e.to_string())
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
    pub(crate) fn updates_setting(&mut self, key: &str, value: &str) -> bool {
        if key != "updates.automatic" || !matches!(value, "true" | "false") { return false; }
        if self.remember_preference_checked(key, value).is_err() { return false; }
        self.updates().view.lock().unwrap()["automatic"] = (value == "true").into();
        true
    }

    pub(crate) fn updates_maintenance(&mut self, key: &str) -> Option<String> {
        match key {
            "updates.check" => { self.appimage_updates_check(false); Some(String::new()) }
            "updates.changelog" => { self.onboarding_command("whatsNew"); Some(String::new()) }
            _ => None,
        }
    }

    pub(crate) fn updates_settings_row(&mut self, row: &mut crate::settings_rows::SettingsRow) -> bool {
        if !row.key.starts_with("updates.") { return false; }
        let offline = self.offline();
        let state = self.updates().view.lock().unwrap();
        match row.key.as_str() {
            "updates.automatic" => row.value = (state["automatic"] == true).to_string(),
            "updates.current" | "updates.installation" => {
                row.kind = 2;
                row.control = "none".into();
                let value = state[if row.key == "updates.current" { "current_version" } else { "installation" }].as_str().unwrap_or_default();
                row.value = if value == "System / manual" { chilpa_i18n::t("System / manual") } else { value.into() };
            }
            "updates.check" => {
                if offline || state["busy"] == true || state["phase"] == "ready" { row.control = "buttonBusy".into(); }
                row.action = match state["phase"].as_str().unwrap_or_default() {
                    "checking" => chilpa_i18n::t("Checking…"),
                    "downloading" => chilpa_i18n::t("Downloading update…"),
                    "installing" => chilpa_i18n::t("Preparing the updated AppImage…"),
                    "cancelling" => chilpa_i18n::t("Cancelling update…"),
                    "ready" => chilpa_i18n::t("Update ready"),
                    _ => chilpa_i18n::t("Check"),
                };
                row.description = if offline { chilpa_i18n::t("Offline") }
                    else { match state["phase"].as_str().unwrap_or_default() {
                        "current" => chilpa_i18n::t("Chilpa is up to date."),
                        "available" => chilpa_i18n::t_args("Version {} is available.", &[state["version"].as_str().unwrap_or_default()]),
                        "error" => chilpa_i18n::t("The update check failed. Try again."),
                        _ => String::new(),
                    } };
            }
            _ => (),
        }
        true
    }

    fn updates(&mut self) -> &Updates {
        if self.appimage_updates.is_none() {
            self.appimage_updates = Some(Updates::new(self.preference("updates.automatic").as_deref() != Some("false")));
        }
        self.appimage_updates.as_ref().unwrap()
    }
    pub fn appimage_updates_json(&mut self) -> String {
        let offline = self.offline();
        let mut state = self.updates().view.lock().unwrap();
        state["offline"] = offline.into();
        state.to_string()
    }
    pub fn appimage_updates_check(&mut self, automatic: bool) {
        if automatic && !self.updates().claim_launch_check() { return; }
        self.updates().start(false);
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
        self.updates_setting("updates.automatic", if on { "true" } else { "false" });
    }
    pub fn appimage_updates_restart(&mut self) -> bool {
        let updates = self.updates();
        if updates.view.lock().unwrap()["phase"] != "ready" { return false; }
        if let Some(native) = &updates.native { return bundle::restart(native); }
        let Some(installation) = &updates.installation else { return false };
        Command::new(&installation.restart).arg(std::process::id().to_string()).arg(&installation.image)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_release_checks_need_valid_service_metadata_but_no_appimage_asset() {
        let current = Version::new(1, 0, 0);
        let release = json!({"tag_name":"v1.1.0", "draft":false, "prerelease":false});
        let document = release_response(200, &serde_json::to_vec(&release).unwrap(), &current).unwrap().unwrap();
        let view = Mutex::new(json!({"can_install":false}));
        note_release(&view, "example/fixture", &document).unwrap();
        assert_eq!(view.lock().unwrap()["release_url"], "https://github.com/example/fixture/releases/tag/v1.1.0");
        assert!(release_asset(&document, "v1.1.0").is_err(), "manual releases do not claim an installable AppImage");
        assert!(release_response(200, b"[]", &current).unwrap().is_none());
        for status in [404, 403, 429, 500] { assert!(release_response(status, b"{}", &current).is_err()); }
        for body in [b"not json".as_slice(), b"{}", b"null", b"[{}]", br#"{"tag_name":"invalid","draft":false,"prerelease":false}"#] { assert!(release_response(200, body, &current).is_err()); }
    }
    #[test]
    fn cancelled_checks_do_not_report_success_but_committed_images_stay_ready() {
        let mut state = json!({"busy":true});
        finish(&mut state, Ok("available"), true);
        assert_eq!(state["phase"], "cancelled");
        assert_eq!(state["busy"], false);
        finish(&mut state, Ok("ready"), true);
        assert_eq!(state["phase"], "ready");
        finish(&mut state, Err("service unavailable".into()), false);
        assert_eq!(state["phase"], "error");
    }
    #[test]
    fn launch_checks_are_claimed_once_and_manual_installations_cannot_download() {
        let updates = Updates { installation:None, native:None, view:Arc::new(Mutex::new(json!({"automatic":true}))),
            cancel:Arc::new(AtomicBool::new(false)), launch_checked:AtomicBool::new(false) };
        assert!(updates.claim_launch_check());
        assert!(!updates.claim_launch_check());
        assert!(!updates.start(true));
        updates.launch_checked.store(false, Ordering::Release);
        updates.view.lock().unwrap()["automatic"] = false.into();
        assert!(!updates.claim_launch_check());
        updates.view.lock().unwrap()["automatic"] = true.into();
        assert!(!updates.claim_launch_check(), "enabling the preference does not repeat this launch");
    }
    #[test]
    fn settings_show_the_actual_operation_and_disable_checks_after_publication() {
        let mut app = Chilpa::new();
        app.appimage_updates = Some(Updates { installation:None, native:None, view:Arc::new(Mutex::new(json!({"phase":"downloading","busy":true}))),
            cancel:Arc::new(AtomicBool::new(false)), launch_checked:AtomicBool::new(false) });
        let mut row = crate::settings_rows::SettingsRow { key:"updates.check".into(), ..Default::default() };
        app.updates_settings_row(&mut row);
        assert_eq!(row.control, "buttonBusy");
        assert_eq!(row.action, chilpa_i18n::t("Downloading update…"));
        *app.updates().view.lock().unwrap() = json!({"phase":"ready","busy":false});
        app.updates_settings_row(&mut row);
        assert_eq!(row.control, "buttonBusy");
        assert_eq!(row.action, chilpa_i18n::t("Update ready"));
    }
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
        let name = format!("Chilpa-1.10.0-{}.AppImage", std::env::consts::ARCH);
        let mut document = json!({"assets":[{"name":name,"size":1024,"digest":format!("sha256:{hash}")},
            {"name":format!("{name}.zsync")}]});
        assert_eq!(release_asset(&document,"v1.10.0").unwrap(), (hash,1024));
        assert!(release_asset(&document,"v1.9.0").is_err());
        document["assets"][0]["digest"] = Value::Null;
        assert!(release_asset(&document,"v1.10.0").is_err());
    }
    #[test]
    fn updates_never_select_another_architecture() {
        let other = if std::env::consts::ARCH == "aarch64" { "x86_64" } else { "aarch64" };
        let name = format!("Chilpa-1.10.0-{other}.AppImage");
        let document = json!({"assets":[{"name":name,"size":1024,"digest":format!("sha256:{}", "a".repeat(64))},
            {"name":format!("{name}.zsync")}]});
        assert!(release_asset(&document,"v1.10.0").is_err());
        let installation = Installation { image:PathBuf::new(), helper:PathBuf::new(), restart:PathBuf::new(),
            repository:"example/fixture".into(), version:Version::new(1,0,0), information:None, expected:None };
        assert!(information(&installation,"v1.10.0").ends_with(&format!("-{}.AppImage.zsync", std::env::consts::ARCH)));
    }
    #[test]
    fn a_rate_limited_service_is_told_apart_from_an_unavailable_one() {
        let current = Version::new(1, 0, 0);
        for status in [403, 429] { assert_eq!(release_response(status, b"{}", &current).unwrap_err(), "rate_limited"); }
        for status in [404, 500] { assert_eq!(release_response(status, b"{}", &current).unwrap_err(), "The release service is unavailable"); }
        let mut state = json!({"busy":true});
        finish(&mut state, Err("rate_limited".into()), false);
        assert_eq!((state["phase"].as_str(), state["reason"].as_str()), (Some("error"), Some("rate_limited")));
        finish(&mut state, Err("some other failure".into()), false);
        assert_eq!(state["reason"], "");
    }
    #[test]
    fn the_tarball_and_mac_files_are_found_by_name_with_their_listed_digest() {
        let hash = "b".repeat(64);
        let tarball = bundle::asset_name(bundle::Kind::Tarball, "1.10.0");
        let document = json!({"assets":[{"name":tarball,"size":2048,"digest":format!("sha256:{hash}")}]});
        assert_eq!(asset_digest(&document, &tarball, false).unwrap(), (hash, 2048));
        assert!(asset_digest(&document, &tarball, true).is_err(), "only an AppImage needs a delta file");
        assert!(asset_digest(&document, &bundle::asset_name(bundle::Kind::MacApp, "1.10.0"), false).is_err());
    }
    #[test]
    fn a_seed_is_copied_when_the_filesystem_cannot_link() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Player.AppImage");
        fs::write(&image, b"original").unwrap();
        let seed = directory.path().join("seed");
        stage_seed(&image, &seed, |_, _| Err(std::io::ErrorKind::PermissionDenied.into())).unwrap();
        assert_eq!(fs::read(&seed).unwrap(), b"original");
        let linked = directory.path().join("linked");
        stage_seed(&image, &linked, |from, to| fs::hard_link(from, to)).unwrap();
        assert_eq!(fs::read(&linked).unwrap(), b"original");
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
