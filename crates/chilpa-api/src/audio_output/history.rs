//! I retain at most 128 cases in a 512 KiB atomic document, off the UI thread.
use chilpa_core::output_evidence::Case;
use std::{io::{Read, Write}, path::Path};
const LIMIT: u64 = 512 * 1024;
pub(super) fn load(path: &Path) -> Result<Vec<Case>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(chilpa_i18n::t("Audio check history could not be read.")),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(|_| chilpa_i18n::t("Audio check history could not be read."))?;
    if bytes.len() as u64 > LIMIT { return Err(chilpa_i18n::t("Audio check history is too large.")); }
    let rows: Vec<Case> = serde_json::from_slice(&bytes).map_err(|_| chilpa_i18n::t("Audio check history is damaged. The original file was kept."))?;
    if rows.len() > 128 { return Err(chilpa_i18n::t("Audio check history is too large.")); }
    Ok(rows)
}
pub(super) fn append(path: &Path, case: Case) -> Result<Vec<Case>,String> {
    let mut rows = load(path)?;
    rows.retain(|old| !(old.identity == case.identity && old.requested == case.requested
        && old.exclusive_requested == case.exclusive_requested));
    rows.push(case);
    if rows.len() > 128 { rows.drain(..rows.len()-128); }
    let bytes = serde_json::to_vec(&rows).map_err(|_| chilpa_i18n::t("Audio check history could not be saved."))?;
    if bytes.len() as u64 > LIMIT { return Err(chilpa_i18n::t("Audio check history is too large.")); }
    let directory = path.parent().ok_or_else(|| chilpa_i18n::t("Audio check history could not be saved."))?;
    std::fs::create_dir_all(directory).map_err(|_| chilpa_i18n::t("Audio check history could not be saved."))?;
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|_| chilpa_i18n::t("Audio check history could not be saved."))?;
    temp.write_all(&bytes).and_then(|()| temp.as_file().sync_all()).map_err(|_| chilpa_i18n::t("Audio check history could not be saved."))?;
    temp.persist(path).map_err(|_| chilpa_i18n::t("Audio check history could not be saved."))?;
    Ok(rows)
}
