//! Inspection and one-file commits. All calls run on the metadata batch worker.
use super::{
    fields,
    model::{Row, Stamp},
};
use chilpa_library::metadata_manifest::MetadataManifest;
use chilpa_library::{Home, Library, Sidecar};
use chilpa_tags::edit::{self, Id3Version, Options};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub(super) const MAX_SIDECAR_BYTES: u64 = 1024 * 1024;

pub(super) fn overlay(path: &Path) -> Result<(Option<Sidecar>, String), String> {
    let name = path.to_string_lossy();
    let home = Home::of(&name, None);
    let Some(current) = chilpa_library::sidecar::in_force(&home, &name) else {
        return Ok((None, "absent".into()));
    };
    let mut bytes = Vec::new();
    std::fs::File::open(&current)
        .map_err(|error| error.to_string())?
        .take(MAX_SIDECAR_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_SIDECAR_BYTES {
        return Err(chilpa_i18n::t(
            "This sidecar exceeds the batch editor limit.",
        ));
    }
    let sidecar = if current == home.sidecar() {
        Sidecar::from_json(&bytes)
    } else {
        chilpa_library::sidecar::qbz::import(&bytes)
    }
    .map_err(|error| error.to_string())?;
    if sidecar.version > chilpa_library::sidecar::VERSION {
        return Err(chilpa_i18n::t("A newer Chilpa wrote this sidecar."));
    }
    Ok((
        Some(sidecar),
        format!("{}:{:x}", current.display(), Sha256::digest(&bytes)),
    ))
}

pub(super) fn inspect(
    path: &Path,
    logical: &Path,
    library: Option<&Library>,
    manifest: &MetadataManifest,
) -> Result<Row, String> {
    let stamp = Stamp::read(path)?;
    let said = edit::snapshot(path).map_err(|error| error.to_string())?;
    let original = fields::values(&said);
    let (sidecar, evidence) = overlay(path)?;
    let home = Home::of(&path.to_string_lossy(), None);
    let effective = fields::effective(
        &original,
        sidecar.as_ref(),
        &home.key(&path.to_string_lossy()),
    );
    manifest
        .home_evidence(&home.sidecar().to_string_lossy(), &evidence)
        .map_err(|error| error.to_string())?;
    let mut track_id = None;
    let mut library_path = path.to_string_lossy().into_owned();
    if let Some(library) = library {
        track_id = library
            .metadata_track_id(path)
            .map_err(|error| error.to_string())?;
        if track_id.is_none() {
            track_id = library
                .metadata_track_id(logical)
                .map_err(|error| error.to_string())?;
            if track_id.is_some() {
                library_path = logical.to_string_lossy().into_owned();
            }
        }
    }
    let layers = edit::inspect(&[path.to_path_buf()]);
    let refusal = layers.refusals.first().copied().flatten();
    let placed = if refusal.is_none() {
        edit::placed(path)
            .map_err(|error| format!("{error:?}"))?
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let mut header = [0; 4];
    let id3_v23 = std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header[..3] == b"ID3"
        && header[3] == 3;
    #[cfg(unix)]
    let hardlinked = {
        use std::os::unix::fs::MetadataExt;
        Some(
            std::fs::metadata(path)
                .map_err(|error| error.to_string())?
                .nlink()
                > 1,
        )
    };
    #[cfg(not(unix))]
    let hardlinked = None;
    let mut row = Row {
        path: path.to_string_lossy().into_owned(),
        library_path,
        track_id,
        original,
        effective,
        stamp,
        placed,
        selected: refusal.is_none(),
        status: if refusal.is_some() {
            "unsupported"
        } else {
            "unchanged"
        }
        .into(),
        phase: "tags".into(),
        error: refusal
            .map(|why| {
                format!(
                    "{} ({why:?})",
                    chilpa_i18n::t("This file cannot be tagged.")
                )
            })
            .unwrap_or_default(),
        hardlinked,
        id3_v23,
        duration_ms: chilpa_tags::read(path)
            .ok()
            .map(|info| info.duration.as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(0),
        ..Default::default()
    };
    row.group = group_identity(path, &row.effective, sidecar.as_ref(), &said)?;
    row.entry(0)?;
    if Stamp::read(path)? != row.stamp {
        return Err(chilpa_i18n::t(
            "The file changed while its metadata was being read.",
        ));
    }
    Ok(row)
}

pub(super) fn group_identity(
    path: &Path,
    values: &serde_json::Map<String, serde_json::Value>,
    sidecar: Option<&Sidecar>,
    snapshot: &edit::Snapshot,
) -> Result<String, String> {
    let home = Home::of(&path.to_string_lossy(), None);
    if let Some(presentation) = sidecar
        .and_then(|sidecar| sidecar.presentation(&home.key(&path.to_string_lossy()), snapshot))
    {
        return Ok(presentation.key);
    }
    let text = |field| {
        values
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let artist = if !text("album_artist").is_empty() {
        text("album_artist")
    } else if values
        .get("compilation")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        ""
    } else {
        text("artist_credit")
    };
    let identity = serde_json::to_vec(&(
        chilpa_library::album_folder::album_folder(path),
        text("musicbrainz_release_id"),
        text("album"),
        artist,
        text("catalog_number"),
        text("date"),
    ))
    .map_err(|error| error.to_string())?;
    Ok(format!("g:{:x}", Sha256::digest(identity)))
}

fn conflict() -> String {
    chilpa_i18n::t(
        "The file or its sidecar changed after preview. Reopen the selection before saving it.",
    )
}

pub(super) fn save(
    row: &mut Row,
    manifest: &MetadataManifest,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let path = std::path::PathBuf::from(&row.path);
    let home = Home::of(&row.path, None);
    let home_path = home.sidecar().to_string_lossy().into_owned();
    let changes = row.changes();
    if changes.is_empty() && row.membership.is_none() {
        return Ok(());
    }
    if std::fs::canonicalize(&path).map_err(|error| error.to_string())? != path
        || Stamp::read(&path)? != row.stamp
    {
        row.status = "conflict".into();
        return Err(conflict());
    }
    let (sidecar, evidence) = overlay(&path)?;
    let expected = manifest
        .home_evidence(&home_path, &evidence)
        .map_err(|error| error.to_string())?;
    if evidence != expected && row.overlay_evidence.as_deref() != Some(evidence.as_str()) {
        row.status = "conflict".into();
        return Err(conflict());
    }
    if row.phase == "tags" {
        let current = edit::snapshot(&path).map_err(|error| error.to_string())?;
        if fields::values(&current) != row.original {
            row.status = "conflict".into();
            return Err(conflict());
        }
        let effective = fields::effective(&row.original, sidecar.as_ref(), &home.key(&row.path));
        if effective != row.effective {
            row.status = "conflict".into();
            return Err(conflict());
        }
    }
    let mut next = sidecar.clone();
    if let Some(member) = &row.membership {
        let snapshot = edit::snapshot(&path).map_err(|error| error.to_string())?;
        let mut member = member.clone();
        member["evidence"] = serde_json::json!(
            chilpa_library::sidecar::presentation_evidence_after(&snapshot, &changes)
        );
        let presentation = chilpa_library::sidecar::Presentation::read(&member)
            .ok_or_else(|| chilpa_i18n::t("The local album proposal is invalid."))?;
        next.get_or_insert_with(|| Sidecar::new(crate::now_unix()))
            .put_presentation(&home.key(&row.path), &presentation);
    }
    if let Some(next) = &mut next {
        fields::release_overlay(next, &home.key(&row.path), &changes);
    }
    let staged_overlay = if next != sidecar {
        let next = next.unwrap();
        if next.to_json().len() > MAX_SIDECAR_BYTES as usize {
            return Err(chilpa_i18n::t(
                "This sidecar exceeds the batch editor limit.",
            ));
        }
        Some(
            chilpa_library::sidecar::stage_fields(&home, next, crate::now_unix())
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    if row.phase == "tags" && changes.is_empty() {
        row.phase = "overlay".into();
    }
    if row.phase == "tags" {
        let file = fields::write(&path, &changes)?;
        let options = Options {
            id3: if row.id3_v23 {
                Id3Version::V23
            } else {
                Id3Version::V24
            },
            secondary_layers: false,
        };
        let staged =
            edit::stage(&[file], options, &mut |_, _| {}).map_err(|error| format!("{error:?}"))?;
        if std::fs::canonicalize(&path).map_err(|error| error.to_string())? != path
            || Stamp::read(&path)? != row.stamp
            || overlay(&path)?.1 != expected
        {
            row.status = "conflict".into();
            return Err(conflict());
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        staged.commit().map_err(|error| format!("{error:?}"))?;
        row.file_written = true;
        row.phase = "overlay".into();
        row.stamp = Stamp::read(&path)?;
    }
    if row.phase == "overlay" {
        // Persist the completed file phase before a potentially failing overlay.
        // The worker saves this row again on either outcome.
        if let Some(staged) = staged_overlay {
            if cancelled.load(Ordering::Acquire) && !row.file_written {
                row.phase = "tags".into();
                return Ok(());
            }
            land_overlay(row, staged, &expected)?;
        }
        row.overlay_evidence = Some(overlay(&path)?.1);
        manifest
            .update_home(&home_path, row.overlay_evidence.as_deref().unwrap())
            .map_err(|error| error.to_string())?;
        row.phase = "reconcile".into();
    }
    Ok(())
}

/// The sidecar lease protects in-process writers. Revalidate the captured
/// destination immediately before landing, including membership-only saves.
pub(super) fn land_overlay(
    row: &mut Row,
    staged: chilpa_library::sidecar::StagedHome,
    expected: &str,
) -> Result<(), String> {
    let path = Path::new(&row.path);
    let evidence = overlay(path)?.1;
    if std::fs::canonicalize(path).map_err(|error| error.to_string())? != path
        || Stamp::read(path)? != row.stamp
        || evidence != expected && row.overlay_evidence.as_deref() != Some(evidence.as_str())
    {
        row.status = "conflict".into();
        return Err(conflict());
    }
    staged.land().map_err(|error| error.to_string())?;
    row.overlay_written = true;
    Ok(())
}

pub(super) fn unsupported(path: &Path, reason: String) -> Row {
    let name = path.to_string_lossy().into_owned();
    Row {
        path: name.clone(),
        group: format!("unreadable:{name}"),
        status: "unsupported".into(),
        phase: "read".into(),
        error: reason,
        ..Default::default()
    }
}
