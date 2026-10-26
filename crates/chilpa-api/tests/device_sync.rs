//! I exercise the facade on disposable libraries and mounted cards, without USB or a user profile.
use chilpa_api::device_sync::{Pick, Service, Store};
use chilpa_library::{Library, PlaylistEntry};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn the_device_sidebar_shortcut_defaults_on_and_remembers_the_opt_out() {
    isolated(
        "the_device_sidebar_shortcut_defaults_on_and_remembers_the_opt_out",
        || {
            let value = |api: &mut chilpa_api::Chilpa| {
                api.settings_rows("navigation", &chilpa_api::RowNotes::default())
                    .0
                    .into_iter()
                    .find(|row| row.key == "navigation.devices")
                    .unwrap()
                    .value
            };
            let mut api = chilpa_api::Chilpa::new();
            assert_eq!(value(&mut api), "true");
            assert!(api.adjust("navigation.devices", "false"));
            drop(api);
            let mut reopened = chilpa_api::Chilpa::new();
            assert_eq!(value(&mut reopened), "false");
            assert!(!reopened.adjust("navigation.devices", "invalid"));
            assert_eq!(value(&mut reopened), "false");
            assert!(reopened.adjust("navigation.devices", "true"));
            assert_eq!(value(&mut reopened), "true");
        },
    );
}

fn isolated(name: &str, run: impl FnOnce()) {
    if std::env::var("CHILPA_DEVICE_TEST").as_deref() == Ok(name) {
        run();
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("CHILPA_DEVICE_TEST", name)
        .envs(chilpa_dirs::profile_environment(profile.path()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Fixture {
    _directory: tempfile::TempDir,
    library: Library,
    library_path: PathBuf,
    card: PathBuf,
    profiles: PathBuf,
    playlist: i64,
    source: i64,
    first: PathBuf,
    second: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let library_path = directory.path().join("library.sqlite3");
        let library = Library::open(&library_path).unwrap();
        let source = library.source_id("local", "default").unwrap();
        let first = directory.path().join("first.flac");
        let second = directory.path().join("second.flac");
        for (id, path, title) in [(1, &first, "First"), (2, &second, "Second")] {
            std::fs::write(path, format!("fixture media {id}")).unwrap();
            library
                .connection()
                .execute(
                    "INSERT INTO tracks(id,source_id,external_id,title,artist,added_at)
                VALUES (?1,?2,?3,?4,'Artist',1)",
                    rusqlite::params![id, source, path.to_str().unwrap(), title],
                )
                .unwrap();
        }
        let playlist = library.create_playlist(source, "Daily", "").unwrap();
        let card = directory.path().join("card");
        std::fs::create_dir(&card).unwrap();
        std::fs::write(card.join("unmanaged.flac"), b"foreign file").unwrap();
        let profiles = directory.path().join("profiles");
        let fixture = Self {
            _directory: directory,
            library,
            library_path,
            card,
            profiles,
            playlist,
            source,
            first,
            second,
        };
        fixture.members(&[1, 2, 1]);
        fixture
    }
    fn members(&self, ids: &[i64]) {
        let entries: Vec<_> = ids
            .iter()
            .map(|id| PlaylistEntry {
                track_id: Some(*id),
                source_id: Some(self.source),
                external_id: if *id == 1 { &self.first } else { &self.second }
                    .to_str()
                    .unwrap()
                    .into(),
            })
            .collect();
        self.library
            .set_playlist_entries(self.playlist, &entries)
            .unwrap();
    }
    fn service(&self) -> Service {
        let mut service = Service::new(self.library_path.clone(), self.profiles.clone());
        service.load().unwrap();
        wait(&mut service);
        service
    }
    fn paired(&self) -> Service {
        let mut service = self.service();
        service.select_destination("mounted").unwrap();
        service
            .add_profile("Test card", self.card.to_str().unwrap())
            .unwrap();
        wait(&mut service);
        assert_eq!(service.profiles().len(), 1);
        let id = service
            .options()
            .iter()
            .find(|o| o.name == "Daily")
            .unwrap()
            .id
            .clone();
        service.toggle(&id).unwrap();
        service
    }
}
fn wait(service: &mut Service) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while service.status().busy {
        assert!(
            Instant::now() < deadline,
            "device task exceeded its deadline"
        );
        service.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        service.status().error.is_none(),
        "{:?}",
        service.status().error
    );
}
fn failed(service: &mut Service) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while service.status().busy {
        assert!(Instant::now() < deadline);
        service.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(service.status().error.is_some());
}
fn preview(service: &mut Service) -> String {
    service.preview(false).unwrap();
    wait(service);
    assert_eq!(service.status().phase, "review");
    service.status().plan.clone()
}
fn apply(service: &mut Service, token: &str, removals: bool) {
    service.apply(token, removals).unwrap();
    wait(service);
    assert_eq!(service.status().phase, "done");
}
fn find_files(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(find_files(&path, extension));
        } else if path.extension().is_some_and(|ext| ext == extension) {
            files.push(path);
        }
    }
    files
}

fn name_fixture_albums(fixture: &Fixture) {
    fixture
        .library
        .connection()
        .execute_batch(
            "INSERT INTO albums(id,title,album_artist) VALUES
        (10,'First record','Artist'),(20,'Second record','Artist');
        UPDATE tracks SET album_id=10,disc_number=1,track_number=1 WHERE id=1;
        UPDATE tracks SET album_id=20,disc_number=2,track_number=1 WHERE id=2;",
        )
        .unwrap();
}

#[test]
fn device_album_batches_keep_their_names_and_accumulate_without_duplicates() {
    isolated(
        "device_album_batches_keep_their_names_and_accumulate_without_duplicates",
        || {
            let fixture = Fixture::new();
            name_fixture_albums(&fixture);
            let mut service = fixture.paired();
            let list = service
                .options()
                .iter()
                .find(|r| r.checked)
                .unwrap()
                .id
                .clone();
            service.toggle(&list).unwrap();
            for ids in [&[1][..], &[1][..], &[2][..]] {
                service.add_library_tracks(ids).unwrap();
                wait(&mut service);
            }
            let batches: Vec<_> = service
                .options()
                .iter()
                .filter(|r| matches!(&r.pick, Pick::Tracks { .. }))
                .collect();
            assert_eq!(batches.len(), 2);
            assert_eq!(
                batches.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
                ["Second record", "First record"]
            );
            assert!(batches
                .iter()
                .all(|r| r.description.contains("Artist") && r.checked));
            assert_ne!(batches[0].id, batches[1].id);
            service.save_selection().unwrap();
            wait(&mut service);
            drop(service);
            let reopened = fixture.service();
            let saved: std::collections::BTreeSet<_> = reopened
                .options()
                .iter()
                .filter(|r| r.checked)
                .map(|r| r.name.as_str())
                .collect();
            assert_eq!(
                saved,
                ["First record", "Second record"].into_iter().collect()
            );
        },
    );
}

#[test]
fn device_legacy_selection_gets_readable_names_without_rewriting_its_profile() {
    isolated(
        "device_legacy_selection_gets_readable_names_without_rewriting_its_profile",
        || {
            let fixture = Fixture::new();
            name_fixture_albums(&fixture);
            let service = fixture.paired();
            let keys = fixture
                .library
                .export_tracks(&[1])
                .unwrap()
                .into_iter()
                .map(|t| t.key)
                .collect();
            Store {
                directory: fixture.profiles.clone(),
            }
            .update(|catalog| {
                catalog.profiles[0].picks = vec![Pick::Tracks {
                    id: "library-selection".into(),
                    name: "Library selection".into(),
                    keys,
                }];
                Ok(())
            })
            .unwrap();
            drop(service);
            let before = std::fs::read(fixture.profiles.join("profiles.json")).unwrap();
            let mut reopened = fixture.service();
            let saved = reopened
                .options()
                .iter()
                .find(|r| r.id == "library-selection")
                .unwrap();
            assert_eq!(saved.name, "First record");
            assert!(saved.description.contains("Artist"));
            reopened.browse("library-selection").unwrap();
            wait(&mut reopened);
            let document: serde_json::Value =
                serde_json::from_str(&reopened.browser_json()).unwrap();
            assert_eq!(document["rows"][0]["title"], "First record");
            assert!(!reopened
                .browser_json()
                .contains(fixture.first.to_str().unwrap()));
            assert_eq!(
                std::fs::read(fixture.profiles.join("profiles.json")).unwrap(),
                before
            );
            reopened.add_library_tracks(&[1]).unwrap();
            wait(&mut reopened);
            assert_eq!(
                reopened
                    .options()
                    .iter()
                    .filter(|r| matches!(&r.pick, Pick::Tracks { .. }))
                    .count(),
                1
            );
            assert!(reopened
                .options()
                .iter()
                .any(|r| r.id == "library-selection"));
        },
    );
}

#[test]
fn device_browsing_preserves_selection_and_an_approved_preview_token() {
    isolated(
        "device_browsing_preserves_selection_and_an_approved_preview_token",
        || {
            let fixture = Fixture::new();
            name_fixture_albums(&fixture);
            let mut service = fixture.paired();
            let token = preview(&mut service);
            let status = serde_json::to_value(service.status()).unwrap();
            let profile = std::fs::read(fixture.profiles.join("profiles.json")).unwrap();
            let selected = service
                .options()
                .iter()
                .find(|r| r.checked)
                .unwrap()
                .id
                .clone();
            service.browse(&selected).unwrap();
            wait(&mut service);
            assert_eq!(serde_json::to_value(service.status()).unwrap(), status);
            let document: serde_json::Value =
                serde_json::from_str(&service.browser_json()).unwrap();
            assert_eq!(document["rows"].as_array().unwrap().len(), 2);
            service
                .open_browser_album(document["rows"][0]["id"].as_str().unwrap())
                .unwrap();
            service.filter_browser("First");
            service.browser_page(usize::MAX);
            service.browser_back();
            service.browser_back();
            assert_eq!(service.browser_json(), "{}");
            assert_eq!(serde_json::to_value(service.status()).unwrap(), status);
            assert_eq!(
                std::fs::read(fixture.profiles.join("profiles.json")).unwrap(),
                profile
            );
            apply(&mut service, &token, false);
        },
    );
}

#[test]
fn device_recorded_music_is_scoped_to_the_profile_and_excludes_unmanaged_files() {
    isolated(
        "device_recorded_music_is_scoped_to_the_profile_and_excludes_unmanaged_files",
        || {
            let fixture = Fixture::new();
            name_fixture_albums(&fixture);
            let mut service = fixture.paired();
            let token = preview(&mut service);
            apply(&mut service, &token, false);
            let profile = service.status().profile.clone();
            let store = Store {
                directory: fixture.profiles.clone(),
            };
            let state = store.state_path(&profile).unwrap();
            let before = std::fs::read(&state).unwrap();
            service.browse("recorded").unwrap();
            wait(&mut service);
            let root: serde_json::Value = serde_json::from_str(&service.browser_json()).unwrap();
            assert_eq!(root["recorded"], true);
            assert!(root["notice"]
                .as_str()
                .unwrap()
                .contains("Files changed elsewhere may differ"));
            assert!(!service.browser_json().contains("unmanaged"));
            for album in root["rows"].as_array().unwrap() {
                service
                    .open_browser_album(album["id"].as_str().unwrap())
                    .unwrap();
                let tracks: serde_json::Value =
                    serde_json::from_str(&service.browser_json()).unwrap();
                assert!(tracks["rows"].as_array().unwrap().iter().all(|r| r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("Music/")
                    || r["path"].as_str().unwrap().starts_with("Playlists/")));
                service.browser_back();
            }
            assert_eq!(std::fs::read(&state).unwrap(), before);
            let other = fixture.card.parent().unwrap().join("other-card");
            std::fs::create_dir(&other).unwrap();
            service
                .add_profile("Other card", other.to_str().unwrap())
                .unwrap();
            wait(&mut service);
            assert_eq!(service.browser_json(), "{}");
            service.browse("recorded").unwrap();
            wait(&mut service);
            let empty: serde_json::Value = serde_json::from_str(&service.browser_json()).unwrap();
            assert_eq!(empty["rows"].as_array().unwrap().len(), 0);
            service.select_profile(&profile).unwrap();
            assert_eq!(service.browser_json(), "{}");
        },
    );
}

#[test]
fn closing_a_device_view_releases_a_pending_browser_result() {
    isolated(
        "closing_a_device_view_releases_a_pending_browser_result",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let selected = service
                .options()
                .iter()
                .find(|r| r.checked)
                .unwrap()
                .id
                .clone();
            service.browse(&selected).unwrap();
            service.release_view();
            assert!(!service.status().busy);
            assert_eq!(service.browser_json(), "{}");
            service.poll();
            service.filter_browser("late");
            service.browser_page(50);
            assert_eq!(service.browser_json(), "{}");
            service.open_view().unwrap();
            wait(&mut service);
            assert!(!service.options().is_empty());
        },
    );
}

#[test]
fn device_preview_preserves_occurrences_and_confirms_only_managed_removals() {
    isolated(
        "device_preview_preserves_occurrences_and_confirms_only_managed_removals",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let changes = fixture.library.connection().total_changes();
            let main_before = std::fs::read(&fixture.library_path).unwrap();
            let wal = PathBuf::from(format!("{}-wal", fixture.library_path.display()));
            let wal_before = std::fs::read(&wal).ok();
            let token = preview(&mut service);
            assert_eq!((service.status().copies, service.status().tracks), (3, 2));
            assert_eq!(service.change_count(), 3);
            assert!(service
                .change_page(0, 200)
                .iter()
                .any(|row| row.title == "First" && row.artist == "Artist"));
            assert!(service.apply("a stale review", false).is_err());
            assert!(!fixture.card.join("Music").exists());
            apply(&mut service, &token, false);
            let playlist = find_files(&fixture.card.join("Playlists"), "m3u8")
                .pop()
                .unwrap();
            let text = std::fs::read_to_string(&playlist).unwrap();
            let occurrences: Vec<_> = text.lines().filter(|line| !line.starts_with('#')).collect();
            assert_eq!(occurrences.len(), 3);
            assert_eq!(occurrences[0], occurrences[2]);
            assert_ne!(occurrences[0], occurrences[1]);
            assert!(occurrences
                .iter()
                .all(|path| playlist.parent().unwrap().join(path).is_file()));
            let revision = service.profile().unwrap().revision;
            let token = preview(&mut service);
            assert_eq!(
                service.profile().unwrap().revision,
                revision,
                "an unchanged selection must not advance its revision"
            );
            assert_eq!(service.change_count(), 0);
            assert_eq!(service.status().unchanged, 3);
            apply(&mut service, &token, false);
            assert_eq!(
                fixture.library.connection().total_changes(),
                changes,
                "device reads must not write to the library"
            );
            assert_eq!(std::fs::read(&fixture.library_path).unwrap(), main_before);
            assert_eq!(
                std::fs::read(&wal).ok(),
                wal_before,
                "readers must not change the library WAL"
            );
            fixture.members(&[1, 1]);
            service.set_removals(true).unwrap();
            let token = preview(&mut service);
            assert_eq!(
                (service.status().removals, service.status().updates),
                (1, 1)
            );
            assert!(service.apply(&token, false).is_err());
            assert_eq!(find_files(&fixture.card.join("Music"), "flac").len(), 2);
            apply(&mut service, &token, true);
            assert_eq!(find_files(&fixture.card.join("Music"), "flac").len(), 1);
            assert_eq!(
                std::fs::read(fixture.card.join("unmanaged.flac")).unwrap(),
                b"foreign file"
            );
            let id = service.profile().unwrap().id.clone();
            drop(service);
            let mut reopened = fixture.service();
            assert_eq!(reopened.profile().unwrap().id, id);
            preview(&mut reopened);
            assert_eq!(reopened.change_count(), 0);
            assert_eq!(reopened.status().unchanged, 2);
        },
    );
}

#[test]
fn device_missing_music_never_becomes_a_prune_or_a_silent_empty_selection() {
    isolated(
        "device_missing_music_never_becomes_a_prune_or_a_silent_empty_selection",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let token = preview(&mut service);
            apply(&mut service, &token, false);
            service.set_removals(true).unwrap();
            std::fs::remove_file(&fixture.second).unwrap();
            service.preview(false).unwrap();
            failed(&mut service);
            assert!(service.status().plan.is_empty());
            assert_eq!(find_files(&fixture.card.join("Music"), "flac").len(), 2);
            fixture.library.connection().execute("UPDATE playlist_tracks SET track_id=NULL,source_id=NULL WHERE playlist_id=?1 AND position=1", [fixture.playlist]).unwrap();
            service.preview(false).unwrap();
            failed(&mut service);
            assert_eq!(find_files(&fixture.card.join("Music"), "flac").len(), 2);
        },
    );
}

#[test]
fn device_identity_and_profile_revisions_reject_a_stale_review() {
    isolated(
        "device_identity_and_profile_revisions_reject_a_stale_review",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let token = preview(&mut service);
            let store = Store {
                directory: fixture.profiles.clone(),
            };
            store
                .update(|catalog| {
                    catalog.profiles[0].revision += 1;
                    Ok(())
                })
                .unwrap();
            service.apply(&token, false).unwrap();
            failed(&mut service);
            assert!(!fixture.card.join("Music").exists());
            service.load().unwrap();
            wait(&mut service);
            let token = preview(&mut service);
            let marker = fixture.card.join(".chilpa-sync/identity.json");
            let original = std::fs::read(&marker).unwrap();
            let mut replacement: serde_json::Value = serde_json::from_slice(&original).unwrap();
            replacement["id"] = serde_json::json!("11111111111111111111111111111111");
            std::fs::write(&marker, replacement.to_string()).unwrap();
            service.apply(&token, false).unwrap();
            failed(&mut service);
            assert!(!fixture.card.join("Music").exists());
            std::fs::write(marker, original).unwrap();
            let token = preview(&mut service);
            let selected = service
                .options()
                .iter()
                .find(|row| row.checked)
                .unwrap()
                .id
                .clone();
            service.toggle(&selected).unwrap();
            assert!(service.apply(&token, false).is_err());
        },
    );
}

#[test]
fn device_catalog_rejects_damage_without_overwriting_the_original() {
    isolated(
        "device_catalog_rejects_damage_without_overwriting_the_original",
        || {
            let fixture = Fixture::new();
            let service = fixture.paired();
            drop(service);
            let store = Store {
                directory: fixture.profiles.clone(),
            };
            let file = fixture.profiles.join("profiles.json");
            let valid = std::fs::read(&file).unwrap();
            assert!(store
                .update(|catalog| {
                    catalog.profiles.push(catalog.profiles[0].clone());
                    Ok(())
                })
                .is_err());
            assert_eq!(std::fs::read(&file).unwrap(), valid);
            for damaged in [b"{broken".to_vec(), vec![b' '; 1024 * 1024 + 1]] {
                std::fs::write(&file, &damaged).unwrap();
                let mut changed = false;
                assert!(store
                    .update(|_| {
                        changed = true;
                        Ok(())
                    })
                    .is_err());
                assert!(!changed);
                assert_eq!(std::fs::read(&file).unwrap(), damaged);
            }
        },
    );
}

#[test]
fn device_query_profiles_keep_their_own_expression_when_switching_cards() {
    isolated(
        "device_query_profiles_keep_their_own_expression_when_switching_cards",
        || {
            let fixture = Fixture::new();
            chilpa_api::saved_queries::save("A query", "title:First").unwrap();
            let mut service = fixture.paired();
            let first = service.profile().unwrap().id.clone();
            let list = service
                .options()
                .iter()
                .find(|row| row.checked)
                .unwrap()
                .id
                .clone();
            service.toggle(&list).unwrap();
            let query = service
                .options()
                .iter()
                .find(|row| row.description == "title:First")
                .unwrap()
                .id
                .clone();
            service.toggle(&query).unwrap();
            let token = preview(&mut service);
            apply(&mut service, &token, false);
            let second_card = fixture._directory.path().join("other-card");
            std::fs::create_dir(&second_card).unwrap();
            service
                .add_profile("Other card", second_card.to_str().unwrap())
                .unwrap();
            wait(&mut service);
            let second = service.profile().unwrap().id.clone();
            let store = Store {
                directory: fixture.profiles.clone(),
            };
            store
                .update(|catalog| {
                    let profile = catalog
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == second)
                        .unwrap();
                    profile.picks = vec![Pick::Query {
                        id: query.clone(),
                        name: "Other expression".into(),
                        expression: "title:Second".into(),
                    }];
                    profile.revision += 1;
                    Ok(())
                })
                .unwrap();
            service.load().unwrap();
            wait(&mut service);
            service.select_profile(&second).unwrap();
            assert_eq!(
                service
                    .options()
                    .iter()
                    .find(|o| o.id == query)
                    .unwrap()
                    .description,
                "title:Second"
            );
            service.select_profile(&first).unwrap();
            assert_eq!(
                service
                    .options()
                    .iter()
                    .find(|o| o.id == query)
                    .unwrap()
                    .description,
                "title:First"
            );
            preview(&mut service);
            assert_eq!(service.change_count(), 0);
        },
    );
}

#[test]
fn device_library_selection_resolves_off_thread_and_survives_reopening() {
    isolated(
        "device_library_selection_resolves_off_thread_and_survives_reopening",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let list = service
                .options()
                .iter()
                .find(|row| row.checked)
                .unwrap()
                .id
                .clone();
            service.toggle(&list).unwrap();
            service.add_library_tracks(&[2, 1, 2]).unwrap();
            assert!(service.status().busy);
            wait(&mut service);
            service.save_selection().unwrap();
            wait(&mut service);
            drop(service);
            let mut reopened = fixture.service();
            assert!(reopened
                .options()
                .iter()
                .any(|row| matches!(&row.pick, Pick::Tracks { .. }) && row.checked));
            let token = preview(&mut reopened);
            assert_eq!((reopened.status().tracks, reopened.status().copies), (2, 3));
            apply(&mut reopened, &token, false);
            let playlist = find_files(&fixture.card.join("Playlists"), "m3u8")
                .pop()
                .unwrap();
            let text = std::fs::read_to_string(playlist).unwrap();
            let members: Vec<_> = text.lines().filter(|line| !line.starts_with('#')).collect();
            assert_eq!(members.len(), 3);
            assert_eq!(members[0], members[2]);
        },
    );
}

#[test]
fn device_surface_reopening_keeps_completion_and_invalidates_a_closed_review() {
    isolated(
        "device_surface_reopening_keeps_completion_and_invalidates_a_closed_review",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.paired();
            let token = preview(&mut service);
            let revision = service.revision();
            let profile = service.profile().unwrap().id.clone();
            service.select_profile(&profile).unwrap();
            service.select_destination("mounted").unwrap();
            service.set_removals(false).unwrap();
            assert_eq!(service.status().plan, token);
            assert_eq!(
                service.revision(),
                revision,
                "unchanged controls must preserve the review"
            );
            service.release_view();
            assert!(service.options().is_empty());
            service.open_view().unwrap();
            wait(&mut service);
            assert!(service.apply(&token, false).is_err());
            let token = preview(&mut service);
            service.apply(&token, false).unwrap();
            service.release_view();
            wait(&mut service);
            assert_eq!(service.status().phase, "done");
            assert_eq!(service.status().copies, 3);
            service.open_view().unwrap();
            wait(&mut service);
            assert_eq!(service.status().phase, "done");
            assert_eq!(service.status().copies, 3);
            assert!(service.options().iter().any(|row| row.checked));
            assert!(service.status().plan.is_empty());
        },
    );
}

#[test]
fn device_staged_library_tracks_survive_choosing_a_new_storage_profile() {
    isolated(
        "device_staged_library_tracks_survive_choosing_a_new_storage_profile",
        || {
            let fixture = Fixture::new();
            let mut service = fixture.service();
            service.stage_library_tracks("[2,1,2]").unwrap();
            assert!(
                !service.status().busy,
                "a staged request must wait for its surface to open"
            );
            service.load().unwrap();
            wait(&mut service);
            assert!(service
                .options()
                .iter()
                .any(|row| matches!(&row.pick, Pick::Tracks { .. }) && row.checked));
            service.select_destination("mounted").unwrap();
            service
                .add_profile("New card", fixture.card.to_str().unwrap())
                .unwrap();
            wait(&mut service);
            assert!(service
                .options()
                .iter()
                .any(|row| matches!(&row.pick, Pick::Tracks { .. }) && row.checked));
            let document = service.catalog_json("");
            assert!(
                !document.contains(fixture.first.to_str().unwrap()),
                "saved source keys must not be copied into a view catalog"
            );
            let token = preview(&mut service);
            assert_eq!(service.status().tracks, 2);
            apply(&mut service, &token, false);
        },
    );
}

#[test]
fn device_missing_mount_is_not_recreated_on_the_host() {
    isolated("device_missing_mount_is_not_recreated_on_the_host", || {
        let fixture = Fixture::new();
        let mut service = fixture.paired();
        let displaced = fixture.card.with_file_name("disconnected-card");
        std::fs::rename(&fixture.card, &displaced).unwrap();
        service.preview(false).unwrap();
        failed(&mut service);
        assert!(
            !fixture.card.exists(),
            "an absent mount must not become a local copy destination"
        );
        assert!(displaced.join(".chilpa-sync/identity.json").is_file());
        assert!(!displaced.join("Music").exists());
    });
}
