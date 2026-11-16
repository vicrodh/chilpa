//! Pinned bootstrap artifacts and app-owned runtime generations.
use super::managed::Failure;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::OnceLock, time::Duration};

pub(super) const BEETS: &str = chilpa_meta::beets::BEETS_VERSION;
pub(super) const PYTHON: &str = "3.13.18";
pub(super) const REQUIREMENTS: &str = include_str!("../../data/beets-requirements.txt");
const OWNER: &str = "chilpa-managed-beets-1";
const MAX_BOOTSTRAP: usize = 32 * 1024 * 1024;
const MAX_EXPANDED: u64 = 128 * 1024 * 1024;
const MAX_RUNTIME: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct Catalog { uv: String, python: String, platforms: Vec<Artifact> }
#[derive(Clone, Deserialize)]
pub(super) struct Artifact { os: String, arch: String, url: String, sha256: String, bytes: usize, program: String }
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(include_str!("../../data/beets-bootstrap.json")).expect("the bundled bootstrap manifest is valid"))
}
pub(super) fn artifact() -> Option<Artifact> {
    if catalog().python != PYTHON { return None; }
    catalog().platforms.iter().find(|item| item.os == std::env::consts::OS && item.arch == std::env::consts::ARCH).cloned()
}
pub(super) fn supported() -> bool { artifact().is_some() }
pub(super) fn root() -> PathBuf { chilpa_dirs::data().join("beets") }

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Installation { format: u32, beets: String, python: String, uv: String, os: String, arch: String, generation: String, previous: Option<String> }
impl Installation {
    pub(super) fn python(&self) -> PathBuf { chilpa_dirs::python_environment(&root().join(&self.generation).join("environment")) }
}
fn owned(root: &Path, name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.starts_with("managed-")
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte,b'-'|b'.'|b'_'))
        && fs::symlink_metadata(root.join(name)).is_ok_and(|info| info.is_dir() && !info.file_type().is_symlink())
        && fs::read(root.join(name).join("owner")).is_ok_and(|bytes| bytes == OWNER.as_bytes())
}
pub(super) fn installed() -> Option<Installation> {
    let root = root();
    let mut bytes = Vec::new();
    fs::File::open(root.join("current.json")).ok()?.take(8193).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 8192 { return None; }
    let mut result: Installation = serde_json::from_slice(&bytes).ok()?;
    result.previous = result.previous.filter(|name| owned(&root,name));
    (result.format == 1 && result.beets == BEETS && result.os == std::env::consts::OS && result.arch == std::env::consts::ARCH
        && owned(&root,&result.generation) && result.python().is_file()).then_some(result)
}
fn active_generation(path: &str) -> Option<String> {
    let root = root();
    let relative = Path::new(path).strip_prefix(&root).ok()?;
    let name = relative.components().next()?.as_os_str().to_str()?;
    owned(&root,name).then(|| name.to_string())
}
fn prune(keep: &[&str]) -> Result<(),Failure> {
    let root = root();
    let entries = fs::read_dir(&root).map_err(|_| Failure::Storage)?;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if !keep.contains(&name.as_str()) && owned(&root,&name) { fs::remove_dir_all(entry.path()).map_err(|_| Failure::Storage)?; }
    }
    Ok(())
}
pub(super) fn generation(previous_python: &str) -> Result<tempfile::TempDir, Failure> {
    let root = root();
    fs::create_dir_all(&root).map_err(|_| Failure::Storage)?;
    let previous = installed();
    let custom = active_generation(previous_python);
    let keep: Vec<&str> = previous.iter().flat_map(|item| std::iter::once(item.generation.as_str()).chain(item.previous.as_deref()))
        .chain(custom.as_deref()).collect();
    prune(&keep)?;
    let directory = tempfile::Builder::new().prefix("managed-").tempdir_in(root).map_err(|_| Failure::Storage)?;
    fs::write(directory.path().join("owner"),OWNER).map_err(|_| Failure::Storage)?;
    Ok(directory)
}
pub(super) fn activate(directory: tempfile::TempDir, previous_python: &str) -> Result<(), Failure> {
    let root = root();
    let previous = active_generation(previous_python).or_else(|| installed().map(|item| item.generation));
    let generation = directory.path().file_name().and_then(|name| name.to_str()).ok_or(Failure::Storage)?.to_string();
    let installation = Installation { format:1, beets:BEETS.into(), python:PYTHON.into(), uv:catalog().uv.clone(),
        os:std::env::consts::OS.into(),arch:std::env::consts::ARCH.into(),generation:generation.clone(),previous };
    let mut temporary = tempfile::NamedTempFile::new_in(&root).map_err(|_| Failure::Storage)?;
    serde_json::to_writer_pretty(&mut temporary,&installation).map_err(|_| Failure::Storage)?;
    temporary.flush().and_then(|_| temporary.as_file().sync_all()).map_err(|_| Failure::Storage)?;
    temporary.persist(root.join("current.json")).map_err(|_| Failure::Storage)?;
    let _ = directory.keep();
    let keep: Vec<&str> = std::iter::once(generation.as_str()).chain(installation.previous.as_deref()).collect();
    let _ = prune(&keep);
    Ok(())
}
pub(super) fn check_size(directory: &Path) -> Result<(), Failure> {
    let mut pending = vec![directory.to_path_buf()];
    let (mut total,mut count) = (0u64,0usize);
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).map_err(|_| Failure::Storage)? {
            let entry = entry.map_err(|_| Failure::Storage)?;
            let kind = entry.file_type().map_err(|_| Failure::Storage)?;
            count += 1;
            if count > 32768 { return Err(Failure::DiskLimit); }
            if kind.is_dir() { pending.push(entry.path()); }
            else { total = total.saturating_add(fs::symlink_metadata(entry.path()).map_err(|_| Failure::Storage)?.len()); }
            if total > MAX_RUNTIME { return Err(Failure::DiskLimit); }
        }
    }
    Ok(())
}

pub(super) struct Bootstrap { _directory: tempfile::TempDir, pub(super) executable: PathBuf }
pub(super) fn bootstrap(cache: &Path) -> Result<Bootstrap, Failure> {
    let item = artifact().ok_or(Failure::Unavailable)?;
    if item.bytes > MAX_BOOTSTRAP { return Err(Failure::Verify); }
    let response = chilpa_core::call_server(&chilpa_core::Call { method:"GET", url:&item.url, headers:&[], body:None,
        timeout:Duration::from_secs(60), max_bytes:MAX_BOOTSTRAP }).map_err(|error| {
            #[cfg(test)]
            if std::env::var_os("CHILPA_MANAGED_BEETS_SMOKE").is_some() { eprintln!("bootstrap transport: {error}"); }
            let _ = error;
            Failure::Download
        })?;
    if response.status != 200 || response.body.len() != item.bytes || format!("{:x}",Sha256::digest(&response.body)) != item.sha256 {
        #[cfg(test)]
        if std::env::var_os("CHILPA_MANAGED_BEETS_SMOKE").is_some() { eprintln!("bootstrap response: status={}, bytes={}",response.status,response.body.len()); }
        return Err(Failure::Verify);
    }
    unpack(cache,&item,&response.body)
}
fn unpack(cache: &Path, item: &Artifact, bytes: &[u8]) -> Result<Bootstrap, Failure> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| Failure::Archive)?;
    if archive.len() > 64 { return Err(Failure::Archive); }
    let mut size = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| Failure::Archive)?;
        crate::plugin_package::portable_path(file.name()).map_err(|_| Failure::Archive)?;
        if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) { return Err(Failure::Archive); }
        size = size.saturating_add(file.size());
        if size > MAX_EXPANDED { return Err(Failure::Archive); }
    }
    let directory = tempfile::Builder::new().prefix("bootstrap-").tempdir_in(cache).map_err(|_| Failure::Storage)?;
    archive.extract(directory.path()).map_err(|_| Failure::Archive)?;
    let executable = directory.path().join(&item.program);
    if !executable.is_file() { return Err(Failure::Archive); }
    Ok(Bootstrap { _directory:directory, executable })
}

#[cfg(test)]
mod tests;
