use super::*;
use std::io::Write;

fn silence() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&436u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&176_400u32.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&400u32.to_le_bytes());
    bytes.resize(444, 0);
    bytes
}

#[test]
fn an_early_retry_failure_keeps_the_partial_file_in_the_disk_quota() {
    let (root, lib, keys, mut track) = fixture("http://127.0.0.1:1/original");
    lib.connection()
        .execute(
            "UPDATE sources SET provider='subsonic' WHERE id=?1",
            [track.source],
        )
        .unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    let old = track.entry.clone();
    track.entry = super::super::super::net::entry(&stored, "song");
    lib.connection()
        .execute(
            "UPDATE tracks SET external_id=?1 WHERE source_id=?2 AND external_id=?3",
            rusqlite::params![track.entry, track.source, old],
        )
        .unwrap();
    let scope = super::super::super::session::Scope::of(&stored);
    let folder = root.path().join("OfflineFolder");
    let copy = draft(&stored, &track, &folder).unwrap();
    lib.queue_offline_copy(&copy, "track/1").unwrap();
    let part = Path::new(&copy.path).with_extension("part");
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, [0u8; 123]).unwrap();
    lib.fail_offline_copy(&copy.key, "", 123).unwrap();
    assert!(download(
        &lib,
        &folder,
        &keys,
        &track,
        &scope,
        "track/1",
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 123);
    assert_eq!(lib.offline_bytes().unwrap(), 123);
}

#[test]
fn checking_a_completed_copy_is_cancellable_and_corruption_invalidates_availability() {
    let bytes = silence();
    let (address, origin) = origin(&bytes, "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let stored = lib.server(track.source).unwrap().unwrap();
    let scope = super::super::super::session::Scope::of(&stored);
    download(
        &lib,
        &root.path().join("OfflineFolder"),
        &keys,
        &track,
        &scope,
        "album/1",
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    origin.join().unwrap();
    let copy = lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry,
        )
        .unwrap()
        .unwrap();
    let checks = std::cell::Cell::new(0);
    assert!(matches!(
        super::super::checked(&copy, || {
            let count = checks.get();
            checks.set(count + 1);
            count >= 2
        }),
        Err(super::super::CheckError::Cancelled)
    ));
    assert!(
        checks.get() >= 3,
        "cancellation was checked between hash reads"
    );
    let queue: super::super::InvalidCopies = Arc::new(super::super::Invalidations {
        pending: Mutex::new(vec![copy.clone()]),
        ..Default::default()
    });
    assert!(
        !super::super::super::watch::invalidate_copies(&lib, &queue),
        "a stale failure cannot invalidate bytes that verify again"
    );
    let mut changed = bytes.clone();
    changed[100] = 1;
    std::fs::write(&copy.path, &changed).unwrap();
    queue.pending.lock().unwrap().push(copy.clone());
    lib.set_source_reachable(track.source, false).unwrap();
    assert!(super::super::super::watch::invalidate_copies(&lib, &queue));
    assert!(lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry
        )
        .unwrap()
        .is_none());
    assert!(lib
        .unplayable_entries(&[track.entry.clone()])
        .unwrap()
        .contains(&track.entry));
    assert_eq!(lib.offline_bytes().unwrap(), copy.bytes);
    std::fs::write(&copy.path, &bytes).unwrap();
    assert!(lib
        .complete_offline_copy(&copy, stored.session_revision)
        .unwrap());
    std::fs::write(&copy.path, changed).unwrap();
    queue.scan.store(true, Ordering::Release);
    assert!(
        super::super::super::watch::invalidate_copies(&lib, &queue),
        "overflow triggers a bounded current-copy audit"
    );
    assert!(lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry
        )
        .unwrap()
        .is_none());
}

#[test]
fn recovery_preserves_reservations_when_a_registered_path_is_not_a_regular_file() {
    let (root, lib, _keys, track) = fixture("http://127.0.0.1:1/original");
    let stored = lib.server(track.source).unwrap().unwrap();
    let copy = draft(&stored, &track, &root.path().join("OfflineFolder")).unwrap();
    lib.queue_offline_copy(&copy, "track/1").unwrap();
    lib.reserve_offline_copy(&copy.key, MAX_FILE_BYTES).unwrap();
    let copy = lib.offline_copy_by_key(&copy.key).unwrap().unwrap();
    assert!(lib.begin_offline_removal(&copy).unwrap());
    let copy = lib.offline_copy_by_key(&copy.key).unwrap().unwrap();
    std::fs::create_dir_all(&copy.path).unwrap();
    assert!(super::recovery::reconcile(&lib, &copy, &AtomicBool::new(false)).is_err());
    assert_eq!(lib.offline_bytes().unwrap(), MAX_FILE_BYTES);
    assert!(lib.offline_copy_by_key(&copy.key).unwrap().is_some());
    assert_eq!(super::super::disk_bytes(&copy), MAX_FILE_BYTES);
    std::fs::remove_dir(&copy.path).unwrap();
    std::fs::write(&copy.path, []).unwrap();
    assert!(super::recovery::reconcile(&lib, &copy, &AtomicBool::new(false)).unwrap());
    assert!(Path::new(&copy.path).is_file());
    assert!(lib.offline_copy_by_key(&copy.key).unwrap().is_some());
    let copy = lib.offline_copy_by_key(&copy.key).unwrap().unwrap();
    assert_eq!(copy.state, "failed");
    assert_eq!(copy.bytes, 0);
}

fn origin(body: &[u8], mime: &str) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/original", listener.local_addr().unwrap());
    let reply=format!("HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nETag: fixture-v1\r\nConnection: close\r\n\r\n",body.len());
    let body = body.to_vec();
    let task = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = peer.read(&mut request);
        peer.write_all(reply.as_bytes()).unwrap();
        peer.write_all(&body).unwrap();
    });
    (url, task)
}

fn fixture(
    address: &str,
) -> (
    tempfile::TempDir,
    Library,
    super::super::super::keys::Keys,
    Track,
) {
    let root = tempfile::tempdir().unwrap();
    let lib = Library::open(&root.path().join("library.sqlite3")).unwrap();
    let keys = super::super::super::keys::Keys::derived(root.path().join("keys"));
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "dlna".into(),
            instance: "0123456789abcdef".into(),
            identity: "fixture-machine".into(),
            account: "fixture-account".into(),
            address: address.into(),
            libraries: "[]".into(),
            details: "{}".into(),
            ..Default::default()
        })
        .unwrap();
    let stored = lib.server(source).unwrap().unwrap();
    let entry = super::super::super::net::entry(&stored, "song");
    let generation = lib.begin_server_sync(source).unwrap();
    lib.save_server_tracks(
        source,
        generation,
        &[chilpa_library::ServerTrack {
            external_id: entry.clone(),
            title: "Silent fixture".into(),
            album: "Album".into(),
            album_key: "album".into(),
            codec: Some("wav".into()),
            revision: Some(format!("v1 {address}")),
            ..Default::default()
        }],
    )
    .unwrap();
    lib.finish_server_sync(source, generation).unwrap();
    let track = Track {
        source,
        entry,
        revision: Some(format!("v1 {address}")),
        codec: Some("wav".into()),
        title: "Silent fixture".into(),
    };
    (root, lib, keys, track)
}

#[test]
fn originals_are_verified_persisted_and_reused_after_the_origin_is_gone() {
    let bytes = silence();
    let (address, origin) = origin(&bytes, "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let folder = root.path().join("OfflineFolder");
    let scope =
        super::super::super::session::Scope::of(&lib.server(track.source).unwrap().unwrap());
    let cancelled = AtomicBool::new(false);
    download(
        &lib,
        &folder,
        &keys,
        &track,
        &scope,
        "album/1",
        &cancelled,
        |_| {},
    )
    .unwrap();
    origin.join().unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    let item = lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry,
        )
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read(&item.path).unwrap(), bytes);
    assert_eq!(item.validator, "fixture-v1");
    assert!(Path::new(&item.path).starts_with(folder.join("DLNA")));
    assert!(!Path::new(&item.path).with_extension("part").exists());
    assert!(super::super::checked(&item, || false).is_ok());
    download(
        &lib,
        &folder,
        &keys,
        &track,
        &scope,
        "playlist/1",
        &cancelled,
        |_| {},
    )
    .unwrap();
    assert_eq!(lib.offline_bytes().unwrap(), bytes.len() as u64);
    let mut changed = bytes.clone();
    changed[100] = 1;
    std::fs::write(&item.path, changed).unwrap();
    assert!(super::super::checked(&item, || false).is_err());
    assert!(
        inspect(Path::new(&item.path), Some("wav"), &cancelled).is_ok(),
        "these changed bytes still decode as complete audio"
    );
    assert!(download(
        &lib,
        &folder,
        &keys,
        &track,
        &scope,
        "album/1",
        &cancelled,
        |_| {}
    )
    .is_err());
    assert!(
        lib.offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry
        )
        .unwrap()
        .is_none(),
        "retry must not certify a known checksum mismatch"
    );
}

#[test]
fn queued_identities_keep_the_connection_scope_captured_at_selection_time() {
    let (root, lib, keys, track) = fixture("http://127.0.0.1:1/original");
    let stored = lib.server(track.source).unwrap().unwrap();
    let scope = super::super::super::session::Scope::of(&stored);
    lib.edit_server(
        track.source,
        &chilpa_library::NewServer {
            provider: stored.provider,
            instance: stored.instance,
            account: "different-account".into(),
            identity: stored.identity,
            address: stored.address,
            libraries: stored.libraries,
            details: stored.details,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(download(
        &lib,
        &root.path().join("OfflineFolder"),
        &keys,
        &track,
        &scope,
        "playlist/1",
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
    assert_eq!(lib.offline_bytes().unwrap(), 0);
    assert!(!root.path().join("OfflineFolder").exists());
}

#[test]
fn a_success_status_and_audio_mime_do_not_promote_a_json_error() {
    let (address, origin) = origin(br#"{"error":"refused"}"#, "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let folder = root.path().join("OfflineFolder");
    let scope =
        super::super::super::session::Scope::of(&lib.server(track.source).unwrap().unwrap());
    assert!(download(
        &lib,
        &folder,
        &keys,
        &track,
        &scope,
        "track/1",
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
    origin.join().unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    assert!(lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry
        )
        .unwrap()
        .is_none());
    assert_eq!(lib.offline_bytes().unwrap(), 0);
    let item = draft(&stored, &track, &folder).unwrap();
    assert!(!Path::new(&item.path).exists());
    assert!(!Path::new(&item.path).with_extension("part").exists());
}

#[test]
fn verified_containers_keep_their_mime_identity_apart_from_the_decoded_codec() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../chilpa-decode/tests/fixtures/offline");
    for (name, hint, expected) in [
        ("silence.wav", "wav", "wav"),
        ("silence.aiff", "aiff", "aiff"),
        ("silence.aac.m4a", "aac", "m4a"),
        ("silence.alac.m4a", "alac", "m4a"),
        ("silence.ogg", "vorbis", "ogg"),
    ] {
        let (_, _, _, container) =
            inspect(&root.join(name), Some(hint), &AtomicBool::new(false)).unwrap();
        assert_eq!(container, expected, "{name}");
    }
}

#[test]
fn removing_a_copy_waits_for_readers_and_then_releases_its_registered_space() {
    let (address, origin) = origin(&silence(), "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let scope =
        super::super::super::session::Scope::of(&lib.server(track.source).unwrap().unwrap());
    download(
        &lib,
        &root.path().join("OfflineFolder"),
        &keys,
        &track,
        &scope,
        "album/1",
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    origin.join().unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    let copy = lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry,
        )
        .unwrap()
        .unwrap();
    let pinned = super::super::checked(&copy, || false).unwrap();
    assert!(super::removal::remove(&lib, &copy, Some(&scope)).is_err());
    assert!(Path::new(&copy.path).is_file());
    assert!(lib.offline_copy_by_key(&copy.key).unwrap().is_some());
    drop(pinned);
    super::removal::remove(&lib, &copy, Some(&scope)).unwrap();
    assert!(!Path::new(&copy.path).exists());
    assert!(lib.offline_copy_by_key(&copy.key).unwrap().is_none());
    assert_eq!(lib.offline_bytes().unwrap(), 0);
}

#[test]
fn recovery_promotes_only_a_verified_final_file_and_invalidates_missing_copies() {
    let (root, lib, _keys, track) = fixture("http://127.0.0.1:1/original");
    let stored = lib.server(track.source).unwrap().unwrap();
    let item = draft(&stored, &track, &root.path().join("OfflineFolder")).unwrap();
    lib.queue_offline_copy(&item, "album/1").unwrap();
    lib.reserve_offline_copy(&item.key, MAX_FILE_BYTES).unwrap();
    let item = lib.offline_copy_by_key(&item.key).unwrap().unwrap();
    std::fs::create_dir_all(Path::new(&item.path).parent().unwrap()).unwrap();
    std::fs::write(&item.path, silence()).unwrap();
    assert!(super::recovery::reconcile(&lib, &item, &AtomicBool::new(false)).unwrap());
    let completed = lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry,
        )
        .unwrap()
        .unwrap();
    assert!(super::super::checked(&completed, || false).is_ok());
    lib.set_source_reachable(track.source, false).unwrap();
    assert!(lib
        .unplayable_entries(&[track.entry.clone()])
        .unwrap()
        .is_empty());
    std::fs::remove_file(&completed.path).unwrap();
    assert!(super::recovery::reconcile(&lib, &completed, &AtomicBool::new(false)).unwrap());
    assert!(lib
        .offline_copy(
            track.source,
            &stored.account,
            &stored.identity,
            &track.entry
        )
        .unwrap()
        .is_none());
    assert!(lib
        .unplayable_entries(&[track.entry.clone()])
        .unwrap()
        .contains(&track.entry));
    assert_eq!(lib.offline_bytes().unwrap(), 0);
}
