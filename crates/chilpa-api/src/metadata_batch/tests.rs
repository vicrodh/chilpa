use super::{
    Selection,
    fields::{self, Patch},
    model::Row,
    worker::Session,
};
use chilpa_library::{Home, Sidecar, sidecar::Override};
use chilpa_tags::edit::{self, Change, FileChanges, Options};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn mp3(directory: &Path, name: &str, artist: &str) -> PathBuf {
    let path = directory.join(format!("{name}.mp3"));
    let mut bytes = Vec::new();
    for _ in 0..40 {
        let mut frame = vec![0; 417];
        frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0]);
        bytes.extend(frame);
    }
    std::fs::write(&path, bytes).unwrap();
    let mut seed = FileChanges::none(path.clone());
    seed.title = Change::Set(name.into());
    seed.album = Change::Set("A record".into());
    seed.artist_credit = Change::Set(artist.into());
    seed.genres = Change::Set(vec!["Heavy Metal".into(), "Personal-Shelf".into()]);
    seed.track_number = Change::Set(2);
    edit::stage(&[seed], Options::default(), &mut |_, _| {})
        .unwrap()
        .commit()
        .unwrap();
    path
}

fn session(directory: &Path) -> Session {
    Session::new(
        7,
        directory.join("absent-library.sqlite"),
        &directory.join("cache"),
    )
    .unwrap()
}
fn patch(field: &str, operation: &str, value: Value) -> Patch {
    Patch {
        field: field.into(),
        operation: operation.into(),
        value,
        group_key: None,
    }
}
fn all() -> Selection {
    Selection {
        mode: "all".into(),
        keys: vec![],
        artists: vec![],
        selected: true,
        replace: false,
        group_key: None,
        groups: vec![],
    }
}

#[test]
fn genre_append_preserves_original_spellings_and_uncertain_names() {
    let before = json!(["Heavy Metal", "Personal-Shelf", "Progresive Metal"]);
    let add = patch(
        "genres",
        "append_unique",
        json!([
            "heavy-metal",
            "personal shelf",
            "Progressive Metal",
            "Wardcore"
        ]),
    );
    add.validate().unwrap();
    assert_eq!(
        add.result(&before),
        json!([
            "Heavy Metal",
            "Personal-Shelf",
            "Progresive Metal",
            "Progressive Metal",
            "Wardcore"
        ])
    );
    assert_eq!(
        patch("genres", "remove_values", json!(["heavy metal"])).result(&before),
        json!(["Personal-Shelf", "Progresive Metal"])
    );
}

#[test]
fn fields_catalog_has_unique_writable_keys_and_rejects_unimplemented_fields() {
    let definitions = fields::definitions();
    let keys: std::collections::BTreeSet<_> = definitions
        .iter()
        .map(|field| field["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys.len(), 26);
    assert_eq!(keys.len(), definitions.len());
    assert!(keys.contains("album") && keys.contains("title"));
    for field in ["file_path", "discogs_release_id"] {
        assert!(patch(field, "set", json!(2)).validate().is_err());
    }
    assert!(
        patch("title", "append_unique", json!(["x"]))
            .validate()
            .is_err()
    );
    assert!(
        patch("track_number", "set", json!(10000))
            .validate()
            .is_err()
    );
    assert!(patch("genres", "set", json!("Rock")).validate().is_err());
}

#[test]
fn external_selection_preview_is_read_only_and_save_only_changes_named_fields() {
    let directory = tempfile::tempdir().unwrap();
    let first = mp3(directory.path(), "first", "Artist A");
    let second = mp3(directory.path(), "second", "Artist B");
    let bytes = std::fs::read(&first).unwrap();
    let other = std::fs::read(&second).unwrap();
    let original = edit::snapshot(&first).unwrap();
    let session = session(directory.path());
    let cancelled = AtomicBool::new(false);
    session
        .open(&[directory.path().to_path_buf(), first.clone()], &cancelled)
        .unwrap();
    assert_eq!(
        session.page("items", 0, 100, 1).unwrap()["counts"]["files"],
        2
    );
    session
        .select(&Selection {
            mode: "artists".into(),
            artists: vec!["Artist A".into()],
            replace: true,
            ..all()
        })
        .unwrap();
    session
        .patch(&patch(
            "genres",
            "append_unique",
            json!(["heavy-metal", "Symphonic Metal"]),
        ))
        .unwrap();
    let view = session.page("items", 0, 100, 2).unwrap();
    assert_eq!(view["counts"]["selected"], 1);
    assert_eq!(view["summary"]["changed"], 1);
    assert_eq!(std::fs::read(&first).unwrap(), bytes);
    assert_eq!(std::fs::read(&second).unwrap(), other);
    session.save(&cancelled).unwrap();
    let mut after = edit::snapshot(&first).unwrap();
    assert_eq!(
        after.genres,
        vec!["Heavy Metal", "Personal-Shelf", "Symphonic Metal"]
    );
    after.genres = original.genres.clone();
    assert_eq!(after, original);
    assert_eq!(std::fs::read(&second).unwrap(), other);
    assert!(!directory.path().join("absent-library.sqlite").exists());
    assert_eq!(
        session.page("items", 0, 100, 3).unwrap()["summary"]["saved"],
        1
    );
}

#[test]
fn file_field_mask_preserves_folder_defaults_neighbors_unknown_keys_and_artwork() {
    let directory = tempfile::tempdir().unwrap();
    let first = mp3(directory.path(), "first", "Artist A");
    let second = mp3(directory.path(), "second", "Artist B");
    let home = Home::of(first.to_str().unwrap(), None);
    let mut sidecar = Sidecar::new(1);
    sidecar.album.genres = Override::Value(vec!["Overlay Genre".into()]);
    sidecar.album.title = Override::Value("Overlay Album".into());
    sidecar.album.cover = Override::Value(".chilpa.cover.png".into());
    sidecar.unknown.insert("future".into(), json!({"keep":1}));
    sidecar
        .track_mut("first.mp3")
        .unknown
        .insert("future_track".into(), json!([1, 2]));
    std::fs::write(home.sidecar(), sidecar.to_json()).unwrap();
    std::fs::write(home.cover("png"), b"cover bytes").unwrap();
    let session = session(directory.path());
    let cancelled = AtomicBool::new(false);
    session.open(&[first.clone()], &cancelled).unwrap();
    session
        .patch(&patch(
            "genres",
            "append_unique",
            json!(["Symphonic Metal"]),
        ))
        .unwrap();
    session.save(&cancelled).unwrap();
    let actual = edit::snapshot(&first).unwrap();
    assert_eq!(actual.genres, vec!["Overlay Genre", "Symphonic Metal"]);
    assert_eq!(actual.album.as_deref(), Some("A record"));
    let saved = Sidecar::from_json(&std::fs::read(home.sidecar()).unwrap()).unwrap();
    assert_eq!(saved.version, chilpa_library::sidecar::VERSION);
    assert_eq!(saved.album, sidecar.album);
    assert_eq!(saved.unknown, sidecar.unknown);
    assert_eq!(
        saved.tracks["first.mp3"].unknown,
        sidecar.tracks["first.mp3"].unknown
    );
    assert!(
        saved.tracks["first.mp3"]
            .file_fields
            .contains("album.genres")
    );
    assert!(saved.effective_album("first.mp3").genres.is_absent());
    assert_eq!(
        saved.effective_album("second.mp3").genres,
        sidecar.album.genres
    );
    assert_eq!(
        edit::snapshot(&second).unwrap().genres,
        vec!["Heavy Metal", "Personal-Shelf"]
    );
    assert_eq!(std::fs::read(home.cover("png")).unwrap(), b"cover bytes");
    let mut scan = chilpa_tags::read(&first).unwrap().tags;
    saved.overlay(&mut scan, "first.mp3");
    assert_eq!(scan.genres, actual.genres);
    assert_eq!(scan.album.as_deref(), Some("Overlay Album"));
}

#[test]
fn changed_file_conflicts_and_cancelled_save_preserve_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "first", "Artist A");
    let session = session(directory.path());
    session
        .open(&[path.clone()], &AtomicBool::new(false))
        .unwrap();
    session
        .patch(&patch("title", "set", json!("A new title")))
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    session.save(&AtomicBool::new(true)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut changed = before.clone();
    changed.push(0);
    std::fs::write(&path, &changed).unwrap();
    session.save(&AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), changed);
    let view = session.page("items", 0, 100, 1).unwrap();
    assert_eq!(view["items"][0]["status"], "conflict");
    assert_eq!(view["items"][0]["fileWritten"], false);
}

#[test]
fn unsupported_field_rejects_entire_proposal_and_page_revision_is_stable() {
    let directory = tempfile::tempdir().unwrap();
    let session = session(directory.path());
    let row = Row {
        path: "/fixture/one".into(),
        selected: true,
        status: "unchanged".into(),
        placed: vec!["title".into()],
        ..Default::default()
    };
    session.manifest.insert(&row.entry(0).unwrap()).unwrap();
    assert!(
        session
            .patch(&patch("genres", "set", json!(["Rock"])))
            .is_err()
    );
    let first = session.page("items", 0, 100, 1).unwrap();
    let second = session.page("groups", 0, 100, 2).unwrap();
    assert_eq!(first["dataRevision"], second["dataRevision"]);
    assert_eq!(first["dirty"], false);
    assert_eq!(second["requestId"], 2);
}

#[cfg(unix)]
#[test]
fn nested_links_stay_in_scope_and_hardlink_paths_are_independent() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("scope");
    std::fs::create_dir(&root).unwrap();
    let path = mp3(&root, "inside", "Artist A");
    let other = directory.path().join("outside.mp3");
    std::fs::hard_link(&path, &other).unwrap();
    symlink(directory.path(), root.join("cycle")).unwrap();
    symlink(&other, root.join("outside-link.mp3")).unwrap();
    symlink(&path, root.join("inside-link.mp3")).unwrap();
    let outside = std::fs::read(&other).unwrap();
    let session = session(directory.path());
    session.open(&[root], &AtomicBool::new(false)).unwrap();
    let view = session.page("items", 0, 100, 1).unwrap();
    assert_eq!(view["counts"]["files"], 3);
    assert_eq!(view["summary"]["unsupported"], 2);
    session
        .patch(&patch("title", "set", json!("Changed")))
        .unwrap();
    session.save(&AtomicBool::new(false)).unwrap();
    assert_eq!(
        edit::snapshot(&path).unwrap().title.as_deref(),
        Some("Changed")
    );
    assert_eq!(std::fs::read(&other).unwrap(), outside);
}

#[test]
fn retry_after_file_commit_finishes_overlay_without_rewriting_audio() {
    use super::{files, model::Stamp};
    let directory = tempfile::tempdir().unwrap();
    // The worker hands rows over by canonical path and a save refuses any
    // other; a temporary folder may sit behind a link (macOS /var).
    let path = std::fs::canonicalize(mp3(directory.path(), "first", "Artist A")).unwrap();
    let home = Home::of(path.to_str().unwrap(), None);
    let mut sidecar = Sidecar::new(1);
    sidecar.album.genres = Override::Value(vec!["Overlay Genre".into()]);
    std::fs::write(home.sidecar(), sidecar.to_json()).unwrap();
    let session = session(directory.path());
    let mut row = files::inspect(&path, &path, None, &session.manifest).unwrap();
    row.patch(&patch(
        "genres",
        "append_unique",
        json!(["Symphonic Metal"]),
    ))
    .unwrap();
    let write = fields::write(&path, &row.changes()).unwrap();
    edit::stage(&[write], Options::default(), &mut |_, _| {})
        .unwrap()
        .commit()
        .unwrap();
    row.file_written = true;
    row.phase = "overlay".into();
    row.stamp = Stamp::read(&path).unwrap();
    let committed = row.stamp.clone();
    let bytes = std::fs::read(&path).unwrap();
    files::save(&mut row, &session.manifest, &AtomicBool::new(false)).unwrap();
    assert!(row.file_written);
    assert_eq!(row.phase, "reconcile");
    assert!(Stamp::read(&path).unwrap() == committed);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let saved = Sidecar::from_json(&std::fs::read(home.sidecar()).unwrap()).unwrap();
    assert!(
        saved.tracks["first.mp3"]
            .file_fields
            .contains("album.genres")
    );
    row.complete().unwrap();
    assert_eq!(row.status, "saved");
    assert!(row.changes().is_empty());
}

#[test]
fn indexed_reconcile_preserves_track_identity_and_album_marks() {
    use chilpa_library::{Library, scan};
    use rusqlite::params;
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let path = mp3(&music, "first", "Artist A");
    let database = directory.path().join("library.sqlite");
    let library = Library::open(&database).unwrap();
    scan::scan_root(&library, &music).unwrap();
    let id = library.metadata_track_id(&path).unwrap().unwrap();
    let album: i64 = library
        .connection()
        .query_row("SELECT album_id FROM tracks WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    library
        .connection()
        .execute(
            "INSERT INTO album_favorites(album_id,added_at) VALUES (?1,42)",
            [album],
        )
        .unwrap();
    library
        .connection()
        .execute(
            "UPDATE albums SET artwork_path='fixture-cover' WHERE id=?1",
            [album],
        )
        .unwrap();
    library
        .connection()
        .execute(
            "UPDATE tracks SET release_artwork='fixture-cover' WHERE id=?1",
            [id],
        )
        .unwrap();
    let session = Session::new(9, database, &directory.path().join("cache")).unwrap();
    session
        .open(&[path.clone()], &AtomicBool::new(false))
        .unwrap();
    session
        .patch(&patch("album", "set", json!("Renamed album")))
        .unwrap();
    session.save(&AtomicBool::new(false)).unwrap();
    let view = session.page("items", 0, 100, 1).unwrap();
    assert_eq!(view["items"][0]["status"], "saved", "{view}");
    let (current_album,title,art): (i64,String,Option<String>) = library.connection().query_row(
        "SELECT a.id,a.title,a.artwork_path FROM tracks t JOIN albums a ON a.id=t.album_id WHERE t.id=?1",[id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(title, "Renamed album");
    assert_eq!(art.as_deref(), Some("fixture-cover"));
    assert!(
        library
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM album_favorites WHERE album_id=?1 AND added_at=42)",
                params![current_album],
                |row| row.get::<_, bool>(0)
            )
            .unwrap()
    );
    assert_eq!(library.metadata_track_id(&path).unwrap(), Some(id));
    assert!(library.check().unwrap().is_empty());
    session.save(&AtomicBool::new(false)).unwrap();
    assert_eq!(
        session.page("items", 0, 100, 2).unwrap()["summary"]["changed"],
        0
    );
}

#[test]
fn unknown_typed_sidecar_values_are_preserved_without_overriding_valid_tags() {
    let bytes = br#"{"format":"chilpa-sidecar","version":2,"album":{"genres":7},"tracks":{"one.mp3":{"title":{"future":"kind"}}}}"#;
    let sidecar = Sidecar::from_json(bytes).unwrap();
    let original = json!({"title":"Embedded","genres":["Rock"]})
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        fields::effective(&original, Some(&sidecar), "one.mp3"),
        original
    );
    assert_eq!(Sidecar::from_json(&sidecar.to_json()).unwrap(), sidecar);
}

fn two_discs(directory: &Path) -> Vec<PathBuf> {
    ["Original", "Bonus"]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let folder = directory.join(name);
            std::fs::create_dir_all(&folder).unwrap();
            let path = mp3(&folder, "one", "Artist A");
            let mut change = FileChanges::none(path.clone());
            change.album_artist = Change::Set("Artist A".into());
            change.disc_number = Change::Set(index as u32 + 1);
            change.disc_total = Change::Set(2);
            change.track_number = Change::Set(1);
            change.track_total = Change::Set(1);
            edit::stage(&[change], Options::default(), &mut |_, _| {})
                .unwrap()
                .commit()
                .unwrap();
            path
        })
        .collect()
}

fn joining(session: &Session) -> super::grouping::Request {
    let page = session.page("groups", 0, 100, 1).unwrap();
    let groups = page["items"].as_array().unwrap();
    let discs: Vec<_> = groups
        .iter()
        .flat_map(|group| {
            group["discs"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |disc| json!({"groupKey":group["key"],"sourceDisc":disc["number"],"number":disc["number"],"trackNumberMode":"keep"}))
        })
        .collect();
    serde_json::from_value(json!({"operation":"join","dataRevision":session.revision,
        "groups":groups.iter().map(|group|group["key"].clone()).collect::<Vec<_>>(),"targets":{"mode":"selected"},
        "album":"A record","albumArtist":"Artist A","discs":discs})).unwrap()
}

#[test]
fn membership_only_join_preserves_media_and_survives_first_scan() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    let paths = two_discs(&music);
    let bytes: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    let modified: Vec<_> = paths
        .iter()
        .map(|path| std::fs::metadata(path).unwrap().modified().unwrap())
        .collect();
    let session = session(directory.path());
    let cancel = AtomicBool::new(false);
    session.open(&[music.clone()], &cancel).unwrap();
    assert_eq!(session.page("groups", 0, 100, 0).unwrap()["total"], 2);
    session.group_plan(&joining(&session), &cancel).unwrap();
    let plan = session.document("plan").unwrap();
    assert_eq!(plan["canApply"], true, "{plan}");
    assert_eq!(session.page("items", 0, 100, 0).unwrap()["dirty"], false);
    for path in &paths {
        assert!(!Home::of(&path.to_string_lossy(), None).sidecar().exists());
    }
    session.apply_plan(plan["id"].as_str().unwrap()).unwrap();
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["summary"]["changed"], 2, "{page}");
    let first = session.manifest.next(0).unwrap().unwrap();
    assert!(Row::read(&first).unwrap().changes().is_empty());
    session.save(&cancel).unwrap();
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["summary"]["saved"], 2, "{page}");
    assert_eq!(page["dirty"], false, "{page}");
    for item in page["items"].as_array().unwrap() {
        assert_eq!(item["fileWritten"], false, "{item}");
        assert_eq!(item["sidecarWritten"], true, "{item}");
    }
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(std::fs::read(path).unwrap(), bytes[index]);
        assert_eq!(
            std::fs::metadata(path).unwrap().modified().unwrap(),
            modified[index]
        );
    }
    let library =
        chilpa_library::Library::open(&directory.path().join("first-scan.sqlite")).unwrap();
    chilpa_library::scan::scan_root(&library, &music).unwrap();
    let albums: i64 = library
        .connection()
        .query_row("SELECT count(DISTINCT album_id) FROM tracks", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(albums, 1);
    let members: i64 = library
        .connection()
        .query_row("SELECT count(*) FROM local_release_members", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(members, 2);
    assert!(library.check().unwrap().is_empty());
    // An external edit invalidates only the affected file's membership.
    let mut change = FileChanges::none(paths[0].clone());
    change.album = Change::Set("Another album".into());
    edit::stage(&[change], Options::default(), &mut |_, _| {})
        .unwrap()
        .commit()
        .unwrap();
    chilpa_library::scan::scan_root(&library, &music).unwrap();
    let members: i64 = library
        .connection()
        .query_row("SELECT count(*) FROM local_release_members", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(members, 1);
}

#[test]
fn indexed_membership_join_carries_marks_artwork_and_track_identity() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    let paths = two_discs(&music);
    let database = directory.path().join("library.sqlite");
    let library = chilpa_library::Library::open(&database).unwrap();
    chilpa_library::scan::scan_root(&library, &music).unwrap();
    let ids: Vec<_> = paths
        .iter()
        .map(|path| library.metadata_track_id(path).unwrap().unwrap())
        .collect();
    library.connection().execute_batch("INSERT INTO album_favorites SELECT id,42 FROM albums; UPDATE albums SET artwork_path='fixture-cover'; UPDATE tracks SET release_artwork='fixture-cover';").unwrap();
    let session = Session::new(7, database, &directory.path().join("cache")).unwrap();
    let cancel = AtomicBool::new(false);
    session.open(&[music.clone()], &cancel).unwrap();
    session.group_plan(&joining(&session), &cancel).unwrap();
    let plan = session.document("plan").unwrap();
    session.apply_plan(plan["id"].as_str().unwrap()).unwrap();
    session.save(&cancel).unwrap();
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["summary"]["saved"], 2, "{page}");
    assert_eq!(
        paths
            .iter()
            .map(|path| library.metadata_track_id(path).unwrap().unwrap())
            .collect::<Vec<_>>(),
        ids
    );
    let (albums,marks,art):(i64,i64,i64)=library.connection().query_row("SELECT (SELECT count(DISTINCT album_id) FROM tracks),(SELECT count(*) FROM album_favorites WHERE added_at=42),(SELECT count(*) FROM albums WHERE artwork_path='fixture-cover')",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!((albums, marks, art), (1, 1, 1));
    assert!(library.check().unwrap().is_empty());
}

#[test]
fn disc_plan_refuses_conflicting_real_release_ids_and_stale_apply() {
    let directory = tempfile::tempdir().unwrap();
    let paths = two_discs(directory.path());
    for (index, path) in paths.iter().enumerate() {
        let mut changes = FileChanges::none(path.clone());
        changes.musicbrainz_release_id =
            Change::Set(format!("00000000-0000-0000-0000-00000000000{}", index + 1));
        edit::stage(&[changes], Options::default(), &mut |_, _| {})
            .unwrap()
            .commit()
            .unwrap();
    }
    let mut session = session(directory.path());
    let cancel = AtomicBool::new(false);
    session.open(&paths, &cancel).unwrap();
    session.group_plan(&joining(&session), &cancel).unwrap();
    let plan = session.document("plan").unwrap();
    assert_eq!(plan["canApply"], false, "{plan}");
    assert!(session.apply_plan(plan["id"].as_str().unwrap()).is_err());
    session.revision += 1;
    assert!(session.apply_plan(plan["id"].as_str().unwrap()).is_err());
    assert_eq!(session.page("items", 0, 100, 0).unwrap()["dirty"], false);
}

#[test]
fn group_filtered_patch_and_replace_selection_preserve_other_groups() {
    let directory = tempfile::tempdir().unwrap();
    let paths = two_discs(directory.path());
    let session = session(directory.path());
    session.open(&paths, &AtomicBool::new(false)).unwrap();
    let groups = session.page("groups", 0, 100, 0).unwrap();
    let group = groups["items"][0]["key"].as_str().unwrap();
    let mut operation = patch("genres", "append_unique", json!(["Symphonic Metal"]));
    operation.group_key = Some(group.into());
    session.patch(&operation).unwrap();
    assert_eq!(
        session.page("items", 0, 100, 0).unwrap()["summary"]["changed"],
        1
    );
    let selection: Selection = serde_json::from_value(json!({"mode":"artists","artists":["Nobody"],"replace":true,"selected":true,"groupKey":group})).unwrap();
    session.select(&selection).unwrap();
    assert_eq!(
        session.page("items", 0, 100, 0).unwrap()["counts"]["selected"],
        1
    );
    let group = session.group(group).unwrap();
    assert_eq!(
        session
            .filtered_page("items", Some(&group), 0, 100, 0)
            .unwrap()["scope"]["selected"],
        0
    );
}

#[test]
fn candidate_genre_plan_keeps_originals_and_requires_explicit_apply() {
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "one", "Artist A");
    let session = session(directory.path());
    session
        .open(&[path.clone()], &AtomicBool::new(false))
        .unwrap();
    let entry = session.manifest.next(0).unwrap().unwrap();
    let row = Row::read(&entry).unwrap();
    let candidate = json!({"score":95,"release":{"provider":"musicbrainz","id":"00000000-0000-0000-0000-000000000001","title":"A record","credits":[],"date":"","genres":["heavy-metal","Symphonic Metal"],"label":"","catalog_number":"","catalog_numbers":[],"country":"","barcode":"","discs":1,"media_titles":[],"release_group_id":"00000000-0000-0000-0000-000000000002","url":"","tracks":[{"disc":1,"number":1,"title":"Matched title","duration_ms":1000,"credits":[],"recording_id":"00000000-0000-0000-0000-000000000003","track_id":"00000000-0000-0000-0000-000000000004"}]},"mappings":[{"track":entry.id,"index":0,"composers":[],"work":""}]});
    let body = json!({"group":entry.group,"evidence":{entry.id.to_string():super::lookup::evidence(&row).unwrap()},"candidate":candidate});
    session
        .manifest
        .put_document("candidate", "fixture", &body.to_string())
        .unwrap();
    let request: super::candidate::Request = serde_json::from_value(json!({"candidateKey":"fixture","groupKey":format!("7:{}",entry.group),"dataRevision":session.revision,"targets":{"mode":"selected"},"fields":["genres"],"genresOperation":"append_unique"})).unwrap();
    session.candidate(&request).unwrap();
    let plan = session.document("plan").unwrap();
    assert_eq!(plan["canApply"], true, "{plan}");
    assert_eq!(session.page("items", 0, 100, 0).unwrap()["dirty"], false);
    session.apply_plan(plan["id"].as_str().unwrap()).unwrap();
    let row = Row::read(&session.manifest.entry(entry.id).unwrap().unwrap()).unwrap();
    assert_eq!(
        row.changes()["genres"],
        json!(["Heavy Metal", "Personal-Shelf", "Symphonic Metal"])
    );
    assert!(!row.changes().contains_key("title"));
    session.save(&AtomicBool::new(false)).unwrap();
    assert_eq!(edit::snapshot(&path).unwrap().title.as_deref(), Some("one"));
}

#[test]
fn split_keeps_real_ids_and_disc_positions_until_an_explicit_retag() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("Combined");
    std::fs::create_dir(&music).unwrap();
    let mut paths = Vec::new();
    for disc in 1..=2 {
        let path = mp3(&music, &format!("disc-{disc}"), "Artist A");
        let mut changes = FileChanges::none(path.clone());
        changes.disc_number = Change::Set(disc);
        changes.disc_total = Change::Set(2);
        changes.track_number = Change::Set(1);
        changes.track_total = Change::Set(1);
        changes.musicbrainz_release_id = Change::Set("00000000-0000-0000-0000-000000000001".into());
        edit::stage(&[changes], Options::default(), &mut |_, _| {})
            .unwrap()
            .commit()
            .unwrap();
        paths.push(path);
    }
    let session = session(directory.path());
    let cancel = AtomicBool::new(false);
    session.open(&paths, &cancel).unwrap();
    let page = session.page("groups", 0, 100, 0).unwrap();
    assert_eq!(page["total"], 1);
    let group = page["items"][0]["key"].as_str().unwrap();
    let request: super::grouping::Request =
        serde_json::from_value(json!({"operation":"split","dataRevision":session.revision,
        "groups":[group],"targets":{"mode":"selected"},"discs":[
        {"groupKey":group,"sourceDisc":1,"number":1,"album":"A record"},
        {"groupKey":group,"sourceDisc":2,"number":2,"album":"A record"}]}))
        .unwrap();
    session.group_plan(&request, &cancel).unwrap();
    let plan = session.document("plan").unwrap();
    assert_eq!(plan["canApply"], true, "{plan}");
    session.apply_plan(plan["id"].as_str().unwrap()).unwrap();
    session.save(&cancel).unwrap();
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["summary"]["saved"], 2, "{page}");
    for (index, path) in paths.iter().enumerate() {
        let said = edit::snapshot(path).unwrap();
        assert_eq!(
            said.musicbrainz_release_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
        assert_eq!(said.disc_number, Some(index as u32 + 1));
        assert_eq!(said.disc_total, Some(2));
    }
    assert_eq!(session.page("groups", 0, 100, 0).unwrap()["total"], 2);
    let database = directory.path().join("visible.sqlite");
    let library = chilpa_library::Library::open(&database).unwrap();
    chilpa_library::scan::scan_root(&library, &music).unwrap();
    library.set_group_album_versions(true).unwrap();
    let reader = crate::Reader::open(&database).unwrap();
    assert_eq!(
        reader
            .albums_view("title", "", "", &crate::Filter::default())
            .ids
            .len(),
        2
    );
    chilpa_library::scan::scan_root(&library, &music).unwrap();
    assert_eq!(
        reader
            .albums_view("title", "", "", &crate::Filter::default())
            .ids
            .len(),
        2
    );
    let preferences: i64 = library
        .connection()
        .query_row(
            "SELECT count(*) FROM album_preferences WHERE group_key LIKE 'apart:%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        preferences, 0,
        "local membership must not materialize user edition preferences"
    );
}

#[cfg(unix)]
#[test]
fn replacing_captured_path_with_symlink_conflicts_without_touching_target() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "first", "Artist A");
    let other = mp3(directory.path(), "other", "Artist A");
    let bytes = std::fs::read(&other).unwrap();
    let session = session(directory.path());
    session
        .open(&[path.clone()], &AtomicBool::new(false))
        .unwrap();
    session
        .patch(&patch("title", "set", json!("Changed")))
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    session.save(&AtomicBool::new(false)).unwrap();
    assert_eq!(
        session.page("items", 0, 100, 0).unwrap()["items"][0]["status"],
        "conflict"
    );
    assert_eq!(std::fs::read(&other).unwrap(), bytes);
}

#[test]
fn pages_stop_at_byte_budget_and_continue_at_absolute_offset() {
    let directory = tempfile::tempdir().unwrap();
    let session = session(directory.path());
    for index in 0..20 {
        let text = "x".repeat(100_000);
        let row = Row {
            path: format!("/fixture/{index}.mp3"),
            group: "fixture".into(),
            status: "unchanged".into(),
            effective: json!({"title":text}).as_object().unwrap().clone(),
            selected: true,
            ..Default::default()
        };
        session.manifest.insert(&row.entry(0).unwrap()).unwrap();
    }
    let first = session.page("items", 0, 100, 9).unwrap();
    assert!(serde_json::to_vec(&first).unwrap().len() <= super::model::MAX_PAGE_BYTES);
    let next = first["nextOffset"].as_u64().unwrap() as usize;
    assert!(next > 0 && next < 20, "{next}");
    let second = session.page("items", next, 100, 10).unwrap();
    assert_ne!(first["items"][0]["key"], second["items"][0]["key"]);
    assert_eq!(first["dataRevision"], second["dataRevision"]);
    assert_eq!(second["pageIdentity"]["offset"], next);
    assert_eq!(second["total"], 20);
}

#[test]
fn failed_atomic_operation_keeps_revision_and_error_through_pages() {
    use super::worker::{Job, Worker};
    fn response(worker: &Worker) -> Value {
        let start = std::time::Instant::now();
        loop {
            if let Some(value) = worker.take() {
                return value;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "one", "Artist A");
    let worker = Worker::start(7, vec![path], directory.path().join("absent.sqlite")).unwrap();
    let opened = response(&worker);
    assert!(worker.ask(Job::Patch(patch("track_number", "set", json!(10000)))));
    let failed = response(&worker);
    assert!(!failed["error"].as_str().unwrap().is_empty());
    assert_eq!(failed["dataRevision"], opened["dataRevision"]);
    assert!(worker.ask(Job::Page {
        kind: "items".into(),
        offset: 0,
        limit: 100,
        request: 2
    }));
    let page = response(&worker);
    assert_eq!(page["error"], failed["error"]);
    assert_eq!(page["dataRevision"], opened["dataRevision"]);
    worker.close();
}

#[test]
fn membership_land_rechecks_sidecar_after_staging_without_rewriting_media() {
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "one", "Artist A");
    let session = session(directory.path());
    session
        .open(&[path.clone()], &AtomicBool::new(false))
        .unwrap();
    let entry = session.manifest.next(0).unwrap().unwrap();
    let mut row = Row::read(&entry).unwrap();
    let before = std::fs::read(&path).unwrap();
    let home = Home::of(&path.to_string_lossy(), None);
    let (held, expected) = super::files::overlay(&path).unwrap();
    let mut proposed = held.unwrap_or_else(|| Sidecar::new(1));
    proposed.unknown.insert("proposal".into(), json!(true));
    let staged = chilpa_library::sidecar::stage_fields(&home, proposed, 2).unwrap();
    let mut concurrent = Sidecar::new(3);
    concurrent
        .unknown
        .insert("concurrent".into(), json!("preserve"));
    let landed = concurrent.to_json();
    // An external process need not participate in the in-process write lease.
    std::fs::write(home.sidecar(), &landed).unwrap();
    assert!(super::files::land_overlay(&mut row, staged, &expected).is_err());
    assert_eq!(row.status, "conflict");
    assert!(!row.file_written && !row.overlay_written);
    assert_eq!(std::fs::read(home.sidecar()).unwrap(), landed);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn version_key_only_change_invalidates_cached_api_album_grouping() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("catalog.sqlite");
    let library = chilpa_library::Library::open(&database).unwrap();
    let source = library.source_id("local", "default").unwrap();
    let first = library
        .source_edition(source, "first", "Same album", "Artist", None)
        .unwrap();
    let second = library
        .source_edition(source, "second", "Same album", "Artist", None)
        .unwrap();
    for id in [first, second] {
        library.connection().execute("INSERT INTO tracks(source_id,external_id,title,artist,album_id,added_at) VALUES(?1,?2,'Track','Artist',?3,1)",rusqlite::params![source,format!("/fixture/{id}.flac"),id]).unwrap();
    }
    library.set_group_album_versions(true).unwrap();
    let reader = crate::Reader::open(&database).unwrap();
    assert_eq!(
        reader
            .albums_view("title", "", "", &crate::Filter::default())
            .ids
            .len(),
        1
    );
    library
        .connection()
        .execute(
            "UPDATE albums SET version_key=?2 WHERE id=?1",
            rusqlite::params![second, "local:explicit-disc"],
        )
        .unwrap();
    assert_eq!(
        reader
            .albums_view("title", "", "", &crate::Filter::default())
            .ids
            .len(),
        2
    );
    library
        .connection()
        .execute(
            "UPDATE albums SET version_key=?2 WHERE id=?1",
            rusqlite::params![second, "source:restored"],
        )
        .unwrap();
    assert_eq!(
        reader
            .albums_view("title", "", "", &crate::Filter::default())
            .ids
            .len(),
        1
    );
}

#[test]
fn explicit_track_positions_raise_totals_and_partial_scope_keeps_known_totals() {
    let directory = tempfile::tempdir().unwrap();
    let paths = two_discs(directory.path());
    let session = session(directory.path());
    let cancel = AtomicBool::new(false);
    session.open(&paths, &cancel).unwrap();
    let first = session.manifest.next(0).unwrap().unwrap();
    let key = format!("7:{}", first.id);
    let request:super::grouping::Request=serde_json::from_value(json!({"operation":"join","dataRevision":session.revision,
        "groups":[format!("7:{}",first.group)],"targets":{"mode":"keys","keys":[key]},"album":"A record","albumArtist":"Artist A",
        "discs":[{"keys":[key],"number":1,"trackNumbers":{(key.clone()):99}}]})).unwrap();
    session.group_plan(&request, &cancel).unwrap();
    let plan = session.document("plan").unwrap();
    assert_eq!(plan["canApply"], true, "{plan}");
    let preview = session.page("plan", 0, 100, 0).unwrap();
    let changes = preview["items"][0]["changes"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|change| change["field"] == "track_total" && change["after"] == 99)
    );
    assert!(
        changes
            .iter()
            .any(|change| change["field"] == "track_number" && change["after"] == 99)
    );
    // An unchanged position carries an explicitly known larger disc total.
    let mut row = Row::read(&first).unwrap();
    row.effective.insert("track_total".into(), json!(8));
    let mut neighbor = Row::read(&first).unwrap();
    neighbor.path.push_str(".other");
    neighbor.effective.insert("track_number".into(), json!(2));
    neighbor.effective.insert("track_total".into(), json!(8));
    session
        .manifest
        .insert(&neighbor.entry(0).unwrap())
        .unwrap();
    session
        .manifest
        .update(&row.entry(first.id).unwrap())
        .unwrap();
    let request:super::grouping::Request=serde_json::from_value(json!({"operation":"join","dataRevision":session.revision,
        "groups":[format!("7:{}",first.group)],"targets":{"mode":"keys","keys":[key]},"album":"A record","albumArtist":"Artist A",
        "discs":[{"keys":[key],"number":1}]})).unwrap();
    session.group_plan(&request, &cancel).unwrap();
    let preview = session.page("plan", 0, 100, 0).unwrap();
    assert!(
        !preview["items"][0]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["field"] == "track_total")
    );
}

#[test]
fn closing_lookup_clears_ownership_before_opening_another_selection() {
    use super::worker::Job;
    fn settle(api: &mut crate::Chilpa, closed: bool) {
        let start = std::time::Instant::now();
        loop {
            api.metadata_batch_poll();
            if closed && api.metadata_batch.worker.is_none()
                || !closed && api.metadata_batch.view["state"] == "ready"
            {
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "one", "Artist A");
    let mut api = crate::Chilpa::new();
    api.library =
        Some(chilpa_library::Library::open(&directory.path().join("library.sqlite")).unwrap());
    let paths = json!([path]).to_string();
    assert!(api.metadata_batch_open(&paths));
    settle(&mut api, false);
    let first_session = api.metadata_batch_session();
    let group = api.metadata_batch.view["items"][0]["group"]
        .as_str()
        .unwrap()
        .to_string();
    let permit = super::lookup::Permit::acquire(api.metadata_beets_lease()).unwrap();
    assert!(api.metadata_batch.ask(Job::Lookup {
        group,
        runtime: directory.path().join("unavailable-python"),
        permit
    }));
    api.metadata_batch.looking_up = true;
    api.metadata_batch_close();
    settle(&mut api, true);
    assert!(!api.metadata_batch.looking_up);
    assert!(api.metadata_batch_open(&paths));
    assert!(api.metadata_batch_session() > first_session);
    api.metadata_batch_network_changed();
    assert!(
        !api.metadata_batch
            .worker
            .as_ref()
            .unwrap()
            .cancelled
            .load(std::sync::atomic::Ordering::Acquire)
    );
    settle(&mut api, false);
    assert_eq!(api.metadata_batch.view["counts"]["files"], 1);
    api.metadata_batch_close();
    settle(&mut api, true);
}

#[test]
fn oversized_combined_proposal_headers_roll_back_without_losing_selection() {
    let directory = tempfile::tempdir().unwrap();
    let path = mp3(directory.path(), "one", "Artist A");
    let session = session(directory.path());
    session.open(&[path], &AtomicBool::new(false)).unwrap();
    let lookup = json!({"label":"x".repeat(150*1024)});
    session.put_document("lookup", &lookup).unwrap();
    let first = session.manifest.next(0).unwrap().unwrap();
    let result = session.manifest.transaction(|| {
        let mut row = Row::read(&first)?;
        row.patch(&patch("title", "set", json!("Pending")))?;
        session
            .manifest
            .update(&row.entry(first.id)?)
            .map_err(|error| error.to_string())?;
        session.put_document("plan", &json!({"label":"y".repeat(150*1024)}))
    });
    assert!(result.is_err());
    assert_eq!(
        session.manifest.entry(first.id).unwrap().unwrap().body,
        first.body
    );
    assert_eq!(session.document("lookup").unwrap(), lookup);
    assert!(session.document("plan").unwrap().is_null());
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["state"], "ready");
    assert_eq!(page["counts"]["files"], 1);
    assert!(serde_json::to_vec(&page).unwrap().len() < super::model::MAX_PAGE_BYTES);
}

#[test]
fn group_credit_and_artist_selection_preserve_album_artist_with_guest_credits() {
    let directory = tempfile::tempdir().unwrap();
    let first = mp3(directory.path(), "one", "Avantasia");
    let guest = mp3(directory.path(), "two", "Guest Singer");
    for path in [&first, &guest] {
        let mut change = FileChanges::none(path.clone());
        change.album_artist = Change::Set("Avantasia".into());
        if path == &guest {
            change.album_artists = Change::Set(vec!["Avantasia".into(), "Side Project".into()]);
        }
        edit::stage(&[change], Options::default(), &mut |_, _| {})
            .unwrap()
            .commit()
            .unwrap();
    }
    let session = session(directory.path());
    session
        .open(&[first.clone(), guest.clone()], &AtomicBool::new(false))
        .unwrap();
    let page = session.page("groups", 0, 100, 0).unwrap();
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["artist"], "Avantasia");
    let selection: Selection = serde_json::from_value(
        json!({"mode":"artists","artists":[" avantasia "],"replace":true,"selected":true}),
    )
    .unwrap();
    session.select(&selection).unwrap();
    assert_eq!(
        session.page("items", 0, 100, 0).unwrap()["counts"]["selected"],
        2
    );
    let selection: Selection = serde_json::from_value(
        json!({"mode":"artists","artists":["side project"],"replace":true,"selected":true}),
    )
    .unwrap();
    session.select(&selection).unwrap();
    let page = session.page("items", 0, 100, 0).unwrap();
    assert_eq!(page["counts"]["selected"], 1);
    let selected = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["selected"] == true)
        .unwrap();
    assert_eq!(selected["artist"], "Guest Singer");
    assert_eq!(selected["values"]["album_artist"], "Avantasia");
    assert_eq!(
        selected["values"]["album_artists"],
        json!(["Avantasia", "Side Project"])
    );
    let mut entry = session.manifest.next(0).unwrap().unwrap();
    let mut row = Row::read(&entry).unwrap();
    row.effective
        .insert("album_artist".into(), json!("Other Curated Credit"));
    entry = row.entry(entry.id).unwrap();
    session.manifest.update(&entry).unwrap();
    assert_eq!(
        session.page("groups", 0, 100, 0).unwrap()["items"][0]["artist"],
        ""
    );
}

#[test]
fn save_progress_freezes_targets_and_retry_counts_only_pending_files() {
    use super::MetadataBatchPhase as Phase;
    use std::sync::atomic::Ordering;
    let directory = tempfile::tempdir().unwrap();
    let first = mp3(directory.path(), "first", "Artist A");
    mp3(directory.path(), "second", "Artist A");
    let session = session(directory.path());
    let cancelled = AtomicBool::new(false);
    session.progress.begin(7, Phase::Opening);
    session.open(&[directory.path().into(), first.clone()], &cancelled).unwrap();
    assert_eq!(session.progress.read().completed, 2);
    session.select(&all()).unwrap();
    session.patch(&patch("title", "set", json!("New title"))).unwrap();
    let before = std::fs::read(&first).unwrap();
    let revision = session.revision;
    cancelled.store(true, Ordering::Release);
    session.progress.begin(7, Phase::Saving);
    session.save(&cancelled).unwrap();
    let stopped = session.progress.read();
    assert_eq!((stopped.completed, stopped.total, stopped.saved), (0, 2, 0));
    assert_eq!(std::fs::read(&first).unwrap(), before);
    cancelled.store(false, Ordering::Release);
    std::fs::remove_file(&first).unwrap();
    session.progress.begin(7, Phase::Saving);
    session.save(&cancelled).unwrap();
    let partial = session.progress.read();
    assert_eq!((partial.completed, partial.total, partial.saved, partial.failed), (2, 2, 1, 1));
    assert_eq!(session.revision, revision, "counter updates do not publish data revisions");
    session.progress.begin(7, Phase::Saving);
    session.save(&cancelled).unwrap();
    let retry = session.progress.read();
    assert_eq!((retry.completed, retry.total, retry.saved, retry.failed), (1, 1, 0, 1));
}
