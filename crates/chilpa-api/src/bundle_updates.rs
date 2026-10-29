//! Self-update for installations that are not an AppImage: an extracted tarball and a macOS app bundle.
//!
//! The release file is checked against the size and SHA-256 digest GitHub lists for it, unpacked
//! beside the installation, validated, and swapped in by rename. The old copy stays next to the
//! new one as `<name>.previous`, one at a time.
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::{self, Read, Write}, path::{Component, Path, PathBuf}, process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

pub(crate) const REPOSITORY: &str = "vicrodh/chilpa";
const MAX_UNPACKED: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const RECEIPT: &str = ".chilpa-installer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind { Tarball, MacApp }

impl Kind {
    pub(crate) fn label(self) -> &'static str { match self { Kind::Tarball => "Tarball", Kind::MacApp => "macOS" } }
    pub(crate) fn key(self) -> &'static str { match self { Kind::Tarball => "tarball", Kind::MacApp => "mac" } }
    /// The folder or bundle a release file unpacks into.
    fn top(self, version: &str) -> String { match self { Kind::Tarball => format!("chilpa-{version}"), Kind::MacApp => "Chilpa.app".into() } }
    fn executable(self) -> &'static str { match self { Kind::Tarball => "chilpa-qt", Kind::MacApp => "Contents/MacOS/chilpa-qt" } }
}

#[derive(Clone, Debug)]
pub(crate) struct Native { pub kind: Kind, pub target: PathBuf, pub version: Version, pub repository: String }

/// The release file for this installation: `chilpa-X-linux-ARCH.tar.gz` or `Chilpa-X-macos-ARCH.zip`.
pub(crate) fn asset_name(kind: Kind, version: &str) -> String {
    let architecture = std::env::consts::ARCH;
    match kind {
        Kind::Tarball => format!("chilpa-{version}-linux-{architecture}.tar.gz"),
        Kind::MacApp => format!("Chilpa-{version}-macos-{architecture}.zip"),
    }
}

/// `owner/name` made of the characters GitHub allows, nothing else.
pub(crate) fn repository_name(text: &str) -> Option<String> {
    let parts = text.split('/').collect::<Vec<_>>();
    let clean = |part: &&str| !part.is_empty() && !part.bytes().all(|b| b == b'.') && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    (parts.len() == 2 && parts.iter().all(clean)).then(|| text.to_string())
}

pub(crate) fn detect() -> Option<Native> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    #[cfg(target_os = "linux")]
    if let Some(found) = tarball(&executable) { return Some(found); }
    #[cfg(target_os = "macos")]
    if let Some(found) = mac_app(&executable) { return Some(found); }
    let _ = executable;
    None
}

/// A tarball carries a `release.json` of kind `tarball` beside `chilpa-qt`; packages and AppImages do not.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn tarball(executable: &Path) -> Option<Native> {
    if executable.file_name()? != "chilpa-qt" { return None; }
    let directory = executable.parent()?;
    let bytes = fs::read(directory.join("release.json")).ok().filter(|bytes| bytes.len() < 4096)?;
    let release: Value = serde_json::from_slice(&bytes).ok()?;
    if release["kind"] != "tarball" { return None; }
    Some(Native { kind: Kind::Tarball, target: directory.to_path_buf(), version: Version::parse(release["version"].as_str()?).ok()?,
        repository: repository_name(release["repository"].as_str()?)? })
}

#[cfg(target_os = "macos")]
fn mac_app(executable: &Path) -> Option<Native> {
    let bundle = executable.ancestors().find(|path| path.extension().is_some_and(|extension| extension == "app"))?;
    // Homebrew and mounted images are managed elsewhere; a bundle with a Developer ID is another trust channel.
    if bundle.starts_with("/Volumes") || bundle.to_string_lossy().contains("/Caskroom/") { return None; }
    let output = Command::new("/usr/bin/codesign").args(["--display", "--verbose=2"]).arg(bundle).output().ok()?;
    if !output.status.success() || !ad_hoc(&String::from_utf8_lossy(&output.stderr)) { return None; }
    Some(Native { kind: Kind::MacApp, target: bundle.to_path_buf(), version: Version::parse(env!("CARGO_PKG_VERSION")).ok()?, repository: REPOSITORY.into() })
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn ad_hoc(description: &str) -> bool {
    description.lines().any(|line| line.trim() == "Signature=adhoc")
        && !description.lines().any(|line| line.starts_with("TeamIdentifier=") && line != "TeamIdentifier=not set")
}

fn no_escape(path: &Path, top: &str) -> Result<(), String> {
    if !path.starts_with(top) || path.components().any(|part| matches!(part, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err("The update archive contains an invalid path.".into());
    }
    Ok(())
}

/// A link may point anywhere inside the unpacked top folder, and nowhere outside it.
fn link_inside(entry: &Path, target: &Path, top: &str, hard: bool) -> Result<(), String> {
    let base = if hard { Path::new("") } else { entry.parent().unwrap_or(Path::new("")) };
    let mut resolved = PathBuf::new();
    for part in base.join(target).components() {
        match part {
            Component::Normal(name) => resolved.push(name),
            Component::CurDir => {}
            Component::ParentDir => if !resolved.pop() { return Err("The update archive has a link outside its folder.".into()); },
            _ => return Err("The update archive has an absolute link.".into()),
        }
    }
    if resolved.starts_with(top) { Ok(()) } else { Err("The update archive has a link outside its folder.".into()) }
}

fn unpack_tarball(payload: &Path, destination: &Path, top: &str) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(payload).map_err(|e| e.to_string())?));
    archive.set_unpack_xattrs(false);
    let (mut total, mut count) = (0u64, 0usize);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        no_escape(&path, top)?;
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
            return Err("The update archive has an unsupported entry.".into());
        }
        if let Some(target) = entry.link_name().map_err(|e| e.to_string())? { link_inside(&path, &target, top, kind.is_hard_link())?; }
        total = total.checked_add(entry.size()).filter(|total| *total <= MAX_UNPACKED).ok_or("The update archive is too large.")?;
        count += 1;
        if count > MAX_ENTRIES { return Err("The update archive has too many entries.".into()); }
        if !entry.unpack_in(destination).map_err(|e| e.to_string())? { return Err("The update archive escaped its staging folder.".into()); }
    }
    Ok(())
}

/// Checks every name and link of a zip before `ditto` writes any of it.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn check_zip(payload: &Path, top: &str) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES { return Err("The update archive has too many entries.".into()); }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| e.to_string())?;
        let path = file.enclosed_name().ok_or("The update archive contains an invalid path.")?;
        no_escape(&path, top)?;
        total = total.checked_add(file.size()).filter(|total| *total <= MAX_UNPACKED).ok_or("The update archive is too large.")?;
        match file.unix_mode().unwrap_or(0) & 0o170000 {
            0o120000 => {
                let mut target = String::new();
                file.by_ref().take(4097).read_to_string(&mut target).map_err(|e| e.to_string())?;
                if target.len() > 4096 { return Err("The update archive has an oversized link.".into()); }
                link_inside(&path, Path::new(&target), top, false)?;
            }
            0 | 0o100000 | 0o040000 => {}
            _ => return Err("The update archive has an unsupported entry.".into()),
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn unpack_zip(payload: &Path, destination: &Path, top: &str) -> Result<(), String> {
    check_zip(payload, top)?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let status = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(payload).arg(destination).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("The update archive could not be unpacked.".into()) }
}

#[cfg(not(target_os = "macos"))]
fn unpack_zip(_: &Path, _: &Path, _: &str) -> Result<(), String> { Err("macOS bundles are only updated on macOS.".into()) }

/// What the unpacked copy must be before it replaces the installed one.
fn check_fresh(kind: Kind, fresh: &Path, version: &str) -> Result<(), String> {
    let executable = fresh.join(kind.executable());
    let metadata = fs::metadata(&executable).map_err(|_| "The update does not contain Chilpa.")?;
    if !metadata.is_file() { return Err("The update does not contain Chilpa.".into()); }
    match kind {
        Kind::Tarball => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 { return Err("The updated program is not executable.".into()); }
            }
            let release: Value = serde_json::from_slice(&fs::read(fresh.join("release.json")).map_err(|_| "The update has no release record.")?)
                .map_err(|_| "The update has an invalid release record.")?;
            if release["kind"] != "tarball" || release["version"] != version { return Err("The update is not the listed version.".into()); }
        }
        Kind::MacApp => {
            if !fresh.join("Contents/Info.plist").is_file() { return Err("The update does not contain Chilpa.".into()); }
            #[cfg(target_os = "macos")]
            check_mac(fresh)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn check_mac(app: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(app).status().map_err(|e| e.to_string())?;
    if !status.success() { return Err("The updated application failed code-signature validation.".into()); }
    let text = |program: &str, arguments: &[&str]| -> Result<String, String> {
        let output = Command::new(program).args(arguments).output().map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let minimum = text("/usr/libexec/PlistBuddy", &["-c", "Print LSMinimumSystemVersion", &app.join("Contents/Info.plist").to_string_lossy()])?;
    let current = text("/usr/bin/sw_vers", &["-productVersion"])?;
    let parse = |value: &str| -> Option<(u64, u64)> { let mut parts = value.split('.'); Some((parts.next()?.parse().ok()?, parts.next().unwrap_or("0").parse().ok()?)) };
    match (parse(&minimum), parse(&current)) {
        (Some(minimum_version), Some(current_version)) if current_version >= minimum_version => Ok(()),
        (Some(_), Some(_)) => Err(format!("This update requires macOS {minimum}.")),
        _ => Err("The update lists no usable macOS version.".into()),
    }
}

struct Hashing<W> { inner: W, hasher: Sha256 }
impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> { self.inner.flush() }
}

/// Moves `fresh` into `target`'s place and `target` to `previous`, undoing what it did if a step fails.
/// An older `previous` waits at `aside` (inside the staging folder, which the caller deletes) until the
/// new one is in place, so a failed update never costs the copy already kept.
fn swap(kind: Kind, target: &Path, fresh: &Path, previous: &Path, aside: &Path) -> Result<(), String> {
    let older = match fs::symlink_metadata(previous) {
        Ok(found) if found.is_dir() && previous.join(if kind == Kind::Tarball { "release.json" } else { "Contents/Info.plist" }).is_file() => {
            fs::rename(previous, aside).map_err(|e| e.to_string())?;
            true
        }
        Ok(_) => return Err(format!("{} is in the way. Move it, then try again.", previous.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    let put_back = || if older { let _ = fs::rename(aside, previous); };
    if let Err(error) = fs::rename(target, previous) {
        put_back();
        return Err(error.to_string());
    }
    if let Err(error) = fs::rename(fresh, target) {
        return match fs::rename(previous, target) {
            Ok(()) => { put_back(); Err(error.to_string()) }
            Err(restore) => Err(format!("Install failed: {error}; restore failed: {restore}. The original is at {}", previous.display())),
        };
    }
    Ok(())
}

/// Keeps the version `install.sh` recorded in step with an installation it made and the app has since updated.
pub(crate) fn refresh_receipt(target: &Path, version: &str) {
    let (Some(directory), Some(name)) = (target.parent(), target.file_name()) else { return };
    if name != "Chilpa.AppImage" && name != "Chilpa.app" { return; }
    let path = directory.join(RECEIPT);
    if fs::read_to_string(&path).ok().and_then(|text| text.lines().next().map(str::to_owned)).as_deref() != Some("chilpa-installer-v1") { return; }
    let partial = directory.join(format!("{RECEIPT}.partial"));
    if fs::write(&partial, format!("chilpa-installer-v1\n{version}\n")).is_ok() && fs::rename(&partial, &path).is_err() { let _ = fs::remove_file(&partial); }
}

pub(crate) struct Plan<'a> { pub native: &'a Native, pub tag: &'a str, pub hash: &'a str, pub size: u64 }

pub(crate) fn install(plan: &Plan, cancel: &AtomicBool, progress: &dyn Fn(f64)) -> Result<(), String> {
    let version = plan.tag.strip_prefix('v').unwrap_or(plan.tag);
    if !plan.tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"-.+".contains(&b)) { return Err("Invalid release tag.".into()); }
    let url = format!("https://github.com/{}/releases/download/{}/{}", plan.native.repository, plan.tag, asset_name(plan.native.kind, version));
    install_from(&url, plan, cancel, progress)
}

fn install_from(url: &str, plan: &Plan, cancel: &AtomicBool, progress: &dyn Fn(f64)) -> Result<(), String> {
    let Native { kind, target, .. } = plan.native;
    let version = plan.tag.strip_prefix('v').unwrap_or(plan.tag);
    let parent = target.parent().ok_or("The installation has no folder.")?;
    // The directory itself is the lock: it leaves nothing behind and dies with the process.
    let lock = fs::File::open(target).map_err(|e| e.to_string())?;
    lock.try_lock().map_err(|_| "Another update is already running.")?;
    if fs::symlink_metadata(target).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("This installation is a link; update the folder it points to.".into());
    }
    let fingerprint = |path: &Path| fs::metadata(path).map(|m| (m.len(), m.modified().ok()));
    let original = fingerprint(&target.join(kind.executable())).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new().prefix(".chilpa-update-").tempdir_in(parent).map_err(|e| {
        if matches!(e.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem) { "read_only".into() } else { e.to_string() }
    })?;
    let payload = stage.path().join("download");
    let mut output = Hashing { inner: fs::File::create(&payload).map_err(|e| e.to_string())?, hasher: Sha256::new() };
    let mut last = Instant::now();
    chilpa_core::release::download(url, &mut output, plan.size, || cancel.load(Ordering::Acquire) || chilpa_core::network::current().offline,
        |bytes| if last.elapsed() >= Duration::from_millis(200) { progress(bytes as f64 / plan.size.max(1) as f64); last = Instant::now(); })
        .map_err(str::to_string)?;
    output.inner.sync_all().map_err(|e| e.to_string())?;
    if format!("{:x}", output.hasher.finalize()) != plan.hash { return Err("The update checksum differs from its release.".into()); }
    if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
    let top = kind.top(version);
    let unpacked = stage.path().join("unpacked");
    match kind { Kind::Tarball => unpack_tarball(&payload, &unpacked, &top)?, Kind::MacApp => unpack_zip(&payload, &unpacked, &top)? }
    let fresh = unpacked.join(&top);
    check_fresh(*kind, &fresh, version)?;
    if fingerprint(&target.join(kind.executable())).map_err(|e| e.to_string())? != original {
        return Err("The installed copy changed during its update.".into());
    }
    if cancel.load(Ordering::Acquire) { return Err("Update cancelled".into()); }
    let mut previous = target.as_os_str().to_os_string();
    previous.push(".previous");
    swap(*kind, target, &fresh, &PathBuf::from(previous), &stage.path().join("older"))?;
    if *kind == Kind::MacApp { refresh_receipt(target, version); }
    fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(|e| e.to_string())?;
    progress(1.0);
    Ok(())
}

/// Starts the new copy once this process has exited.
pub(crate) fn restart(native: &Native) -> bool {
    let waiter = r#"n=0; while kill -0 "$1" 2>/dev/null; do n=$((n+1)); [ "$n" -lt 300 ] || exit 1; sleep 0.2; done; shift; exec "$@""#;
    let mut process = Command::new("/bin/sh");
    process.args(["-c", waiter, "sh"]).arg(std::process::id().to_string());
    match native.kind {
        Kind::Tarball => process.arg(native.target.join("chilpa-qt")),
        Kind::MacApp => process.args(["/usr/bin/open", "-n"]).arg(&native.target),
    };
    #[cfg(unix)]
    { use std::os::unix::process::CommandExt; process.process_group(0); }
    process.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn gzip(entries: &[(&str, &[u8], u32)], links: &[(&str, &str, bool)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        // The builder refuses `..` in names; the raw header is how a hostile archive carries one.
        let raw = |header: &mut tar::Header, path: &str, target: Option<&str>| {
            let old = header.as_old_mut();
            old.name[..path.len()].copy_from_slice(path.as_bytes());
            if let Some(target) = target { old.linkname[..target.len()].copy_from_slice(target.as_bytes()); }
        };
        for (path, bytes, mode) in entries {
            let mut header = tar::Header::new_gnu();
            raw(&mut header, path, None);
            header.set_size(bytes.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder.append(&header, *bytes).unwrap();
        }
        for (path, target, hard) in links {
            let mut header = tar::Header::new_gnu();
            raw(&mut header, path, Some(target));
            header.set_entry_type(if *hard { tar::EntryType::Link } else { tar::EntryType::Symlink });
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            builder.append(&header, io::empty()).unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&builder.into_inner().unwrap()).unwrap();
        encoder.finish().unwrap()
    }

    fn release(version: &str) -> String { format!(r#"{{"repository":"example/fixture","version":"{version}","kind":"tarball"}}"#) }

    fn good(version: &str) -> Vec<u8> {
        let top = format!("chilpa-{version}");
        let record = release(version);
        gzip(&[(&format!("{top}/chilpa-qt"), b"new program", 0o755), (&format!("{top}/release.json"), record.as_bytes(), 0o644)], &[])
    }

    fn installed(root: &Path, version: &str) -> PathBuf {
        let directory = root.join("apps/chilpa");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("chilpa-qt"), b"old program").unwrap();
        fs::write(directory.join("release.json"), release(version)).unwrap();
        directory
    }

    fn serve(body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/chilpa.tar.gz", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = peer.read(&mut request);
            let _ = peer.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
            let _ = peer.write_all(&body);
        });
        (address, task)
    }

    fn native(directory: &Path) -> Native {
        Native { kind: Kind::Tarball, target: directory.to_path_buf(), version: Version::new(0, 1, 0), repository: "example/fixture".into() }
    }

    #[test]
    fn names_follow_the_published_files() {
        let architecture = std::env::consts::ARCH;
        assert_eq!(asset_name(Kind::Tarball, "0.1.1"), format!("chilpa-0.1.1-linux-{architecture}.tar.gz"));
        assert_eq!(asset_name(Kind::MacApp, "0.1.1"), format!("Chilpa-0.1.1-macos-{architecture}.zip"));
        assert_eq!(Kind::Tarball.top("0.1.1"), "chilpa-0.1.1");
        assert_eq!(repository_name("vicrodh/chilpa").as_deref(), Some("vicrodh/chilpa"));
        for bad in ["vicrodh", "a/b/c", "a/", "a/b c", "../x"] { assert!(repository_name(bad).is_none(), "{bad}"); }
    }

    #[test]
    fn only_a_marked_directory_is_a_tarball_installation() {
        let root = tempfile::tempdir().unwrap();
        let directory = installed(root.path(), "0.1.0");
        let found = tarball(&directory.join("chilpa-qt")).unwrap();
        assert_eq!((found.kind, found.version.to_string(), found.target.as_path()), (Kind::Tarball, "0.1.0".into(), directory.as_path()));
        assert!(tarball(&directory.join("other")).is_none());
        fs::write(directory.join("release.json"), r#"{"repository":"example/fixture","version":"0.1.0"}"#).unwrap();
        assert!(tarball(&directory.join("chilpa-qt")).is_none(), "a record without the tarball kind belongs to another installation");
        fs::remove_file(directory.join("release.json")).unwrap();
        assert!(tarball(&directory.join("chilpa-qt")).is_none());
    }

    #[test]
    fn an_archive_unpacks_only_inside_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let payload = root.path().join("payload");
        fs::write(&payload, good("0.1.1")).unwrap();
        unpack_tarball(&payload, &root.path().join("ok"), "chilpa-0.1.1").unwrap();
        check_fresh(Kind::Tarball, &root.path().join("ok/chilpa-0.1.1"), "0.1.1").unwrap();
        assert!(check_fresh(Kind::Tarball, &root.path().join("ok/chilpa-0.1.1"), "0.1.2").is_err(), "the listed version must match");
        for (index, archive) in [
            gzip(&[("elsewhere/chilpa-qt", b"x", 0o755)], &[]),
            gzip(&[("chilpa-0.1.1/../escape", b"x", 0o644)], &[]),
            gzip(&[("chilpa-0.1.1/chilpa-qt", b"x", 0o755)], &[("chilpa-0.1.1/out", "../../outside", false)]),
            gzip(&[("chilpa-0.1.1/chilpa-qt", b"x", 0o755)], &[("chilpa-0.1.1/abs", "/outside/target", false)]),
            gzip(&[("chilpa-0.1.1/chilpa-qt", b"x", 0o755)], &[("chilpa-0.1.1/hard", "other/file", true)]),
        ].into_iter().enumerate() {
            fs::write(&payload, archive).unwrap();
            assert!(unpack_tarball(&payload, &root.path().join(format!("bad{index}")), "chilpa-0.1.1").is_err(), "case {index}");
            assert!(!root.path().join("escape").exists() && !root.path().join(format!("bad{index}/escape")).exists());
        }
        fs::write(&payload, gzip(&[("chilpa-0.1.1/chilpa-qt", b"x", 0o755)], &[("chilpa-0.1.1/alias", "chilpa-qt", false)])).unwrap();
        unpack_tarball(&payload, &root.path().join("inside"), "chilpa-0.1.1").unwrap();
    }

    #[test]
    fn a_zip_is_checked_before_anything_is_written() {
        let build = |entries: &[(&str, &[u8], u32)]| {
            let mut bytes = Vec::new();
            let mut archive = zip::ZipWriter::new(io::Cursor::new(&mut bytes));
            for (name, content, mode) in entries {
                let options = zip::write::SimpleFileOptions::default().unix_permissions(*mode & 0o777);
                if mode & 0o170000 == 0o120000 {
                    archive.add_symlink(*name, std::str::from_utf8(content).unwrap(), options).unwrap();
                } else {
                    archive.start_file(*name, options).unwrap();
                    archive.write_all(content).unwrap();
                }
            }
            archive.finish().unwrap();
            bytes
        };
        let root = tempfile::tempdir().unwrap();
        let payload = root.path().join("payload.zip");
        fs::write(&payload, build(&[("Chilpa.app/Contents/MacOS/chilpa-qt", b"x", 0o100755), ("Chilpa.app/Contents/Frameworks/Qt", b"Versions/A/Qt", 0o120755)])).unwrap();
        check_zip(&payload, "Chilpa.app").unwrap();
        for (index, entries) in [
            vec![("Other.app/x", &b"x"[..], 0o100644)],
            vec![("Chilpa.app/../x", &b"x"[..], 0o100644)],
            vec![("Chilpa.app/link", &b"../../outside"[..], 0o120755)],
            vec![("Chilpa.app/link", &b"/outside/target"[..], 0o120755)],
        ].into_iter().enumerate() {
            fs::write(&payload, build(&entries)).unwrap();
            assert!(check_zip(&payload, "Chilpa.app").is_err(), "case {index}");
        }
    }

    #[test]
    fn the_previous_copy_is_kept_once_and_a_failed_swap_restores_the_original() {
        let root = tempfile::tempdir().unwrap();
        let directory = installed(root.path(), "0.1.0");
        let previous = root.path().join("apps/chilpa.previous");
        let make = |name: &str, program: &str| {
            let fresh = root.path().join(name);
            fs::create_dir_all(&fresh).unwrap();
            fs::write(fresh.join("chilpa-qt"), program).unwrap();
            fs::write(fresh.join("release.json"), release("x")).unwrap();
            fresh
        };
        swap(Kind::Tarball, &directory, &make("one", "program one"), &previous, &root.path().join("aside-one")).unwrap();
        assert_eq!(fs::read(directory.join("chilpa-qt")).unwrap(), b"program one");
        assert_eq!(fs::read(previous.join("chilpa-qt")).unwrap(), b"old program");
        swap(Kind::Tarball, &directory, &make("two", "program two"), &previous, &root.path().join("aside-two")).unwrap();
        assert_eq!(fs::read(previous.join("chilpa-qt")).unwrap(), b"program one", "only one previous copy is kept");
        assert!(swap(Kind::Tarball, &directory, &root.path().join("missing"), &previous, &root.path().join("aside-three")).is_err());
        assert_eq!(fs::read(directory.join("chilpa-qt")).unwrap(), b"program two", "a failed swap leaves the installed copy");
        assert_eq!(fs::read(previous.join("chilpa-qt")).unwrap(), b"program one", "and the copy already kept");
        fs::remove_dir_all(&previous).unwrap();
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("mine.txt"), "keep").unwrap();
        assert!(swap(Kind::Tarball, &directory, &make("four", "program four"), &previous, &root.path().join("aside-four")).is_err(), "a folder that is not ours is never touched");
        assert_eq!(fs::read(previous.join("mine.txt")).unwrap(), b"keep");
    }

    #[test]
    fn the_installer_receipt_follows_only_an_installation_it_made() {
        let root = tempfile::tempdir().unwrap();
        let image = root.path().join("Chilpa.AppImage");
        refresh_receipt(&image, "0.1.1");
        assert!(!root.path().join(RECEIPT).exists(), "no receipt is created");
        fs::write(root.path().join(RECEIPT), "chilpa-installer-v1\n0.1.0\n").unwrap();
        refresh_receipt(&image, "0.1.1");
        assert_eq!(fs::read_to_string(root.path().join(RECEIPT)).unwrap(), "chilpa-installer-v1\n0.1.1\n");
        refresh_receipt(&root.path().join("Renamed.AppImage"), "0.1.2");
        assert_eq!(fs::read_to_string(root.path().join(RECEIPT)).unwrap(), "chilpa-installer-v1\n0.1.1\n");
        fs::write(root.path().join(RECEIPT), "something else\n0.1.0\n").unwrap();
        refresh_receipt(&image, "0.1.2");
        assert_eq!(fs::read_to_string(root.path().join(RECEIPT)).unwrap(), "something else\n0.1.0\n");
    }

    #[test]
    fn only_an_ad_hoc_signature_without_a_team_is_ours() {
        assert!(ad_hoc("Identifier=app.chilpa.Chilpa\nSignature=adhoc\nTeamIdentifier=not set\n"));
        assert!(!ad_hoc("Signature=adhoc\nTeamIdentifier=ABCDE12345\n"));
        assert!(!ad_hoc("Authority=Developer ID Application: Someone\nTeamIdentifier=ABCDE12345\n"));
    }

    #[test]
    fn a_listed_release_replaces_the_installation_and_a_wrong_one_does_not() {
        let root = tempfile::tempdir().unwrap();
        let directory = installed(root.path(), "0.1.0");
        let body = good("0.1.1");
        let hash = format!("{:x}", Sha256::digest(&body));
        let (address, task) = serve(body.clone());
        let plan = Plan { native: &native(&directory), tag: "v0.1.1", hash: &hash, size: body.len() as u64 };
        install_from(&address, &plan, &AtomicBool::new(false), &|_| ()).unwrap();
        task.join().unwrap();
        assert_eq!(fs::read(directory.join("chilpa-qt")).unwrap(), b"new program");
        assert_eq!(fs::read(root.path().join("apps/chilpa.previous/chilpa-qt")).unwrap(), b"old program");
        assert!(fs::read_dir(root.path().join("apps")).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".chilpa-update-")), "staging is cleaned");

        let other = tempfile::tempdir().unwrap();
        let directory = installed(other.path(), "0.1.0");
        let (address, task) = serve(body.clone());
        let plan = Plan { native: &native(&directory), tag: "v0.1.1", hash: &"0".repeat(64), size: body.len() as u64 };
        assert!(install_from(&address, &plan, &AtomicBool::new(false), &|_| ()).is_err());
        task.join().unwrap();
        assert_eq!(fs::read(directory.join("chilpa-qt")).unwrap(), b"old program", "a checksum mismatch never replaces the installation");
        assert!(!other.path().join("apps/chilpa.previous").exists());
    }

    #[test]
    fn a_release_that_is_not_the_listed_version_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let directory = installed(root.path(), "0.1.0");
        let body = good("0.1.5");
        let hash = format!("{:x}", Sha256::digest(&body));
        let (address, task) = serve(body.clone());
        let plan = Plan { native: &native(&directory), tag: "v0.1.1", hash: &hash, size: body.len() as u64 };
        assert!(install_from(&address, &plan, &AtomicBool::new(false), &|_| ()).is_err());
        task.join().unwrap();
        assert_eq!(fs::read(directory.join("chilpa-qt")).unwrap(), b"old program");
    }

    #[test]
    fn a_tag_with_path_characters_never_builds_an_address() {
        let root = tempfile::tempdir().unwrap();
        let directory = installed(root.path(), "0.1.0");
        let plan = Plan { native: &native(&directory), tag: "v1/../x", hash: "", size: 1 };
        assert!(install(&plan, &AtomicBool::new(false), &|_| ()).is_err());
    }
}
