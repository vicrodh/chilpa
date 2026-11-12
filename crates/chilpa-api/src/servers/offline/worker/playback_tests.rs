//! A server track with a verified offline copy plays from that copy, and
//! only from the server when the copy cannot be trusted.

use super::tests::{fixture, origin, silence};
use super::*;
use chilpa_core::remote::{Media, RemoteOpener};

fn opener(root: &Path, keys: super::super::super::keys::Keys) -> (super::super::super::opener::Opener, super::super::InvalidCopies) {
    let invalid = super::super::InvalidCopies::default();
    let library = Library::open_read_only(&root.join("library.sqlite3")).unwrap();
    (super::super::super::opener::Opener::on(Arc::new(keys), library, invalid.clone()), invalid)
}

fn played(media: &Media) -> Option<Vec<u8>> {
    match media {
        Media::File { file, .. } => {
            let mut bytes = Vec::new();
            file.try_clone().unwrap().read_to_end(&mut bytes).unwrap();
            Some(bytes)
        }
        Media::Remote(_) => None,
    }
}

fn keys_of(root: &Path) -> super::super::super::keys::Keys {
    super::super::super::keys::Keys::derived(root.join("keys"))
}

/// Origins that answer one request each, in order, with these bodies.
fn origins(bodies: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<usize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/original", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut served = 0;
        for body in bodies {
            let (mut peer, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = peer.read(&mut request);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            peer.write_all(head.as_bytes()).unwrap();
            peer.write_all(&body).unwrap();
            served += 1;
        }
        served
    });
    (url, task)
}

#[test]
fn a_kept_copy_plays_locally_even_with_the_server_gone_and_follows_a_fresh_copy_and_removal() {
    let first = silence();
    let mut second = silence();
    second[200] = 7;
    let (address, origin) = origins(vec![first.clone(), second.clone()]);
    let (root, lib, keys, track) = fixture(&address);
    let folder = root.path().join("OfflineFolder");
    let scope = super::super::super::session::Scope::of(&lib.server(track.source).unwrap().unwrap());
    let entry = chilpa_models::RemoteEntry::parse(&track.entry).unwrap();
    let (opener, _) = opener(root.path(), keys_of(root.path()));
    assert!(played(&opener.locate(&entry).unwrap().media).is_none(), "nothing kept: the server's stream");
    download(&lib, &folder, &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(played(&opener.locate(&entry).unwrap().media), Some(first.clone()), "kept: the local file");
    obtain(&lib, &folder, &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(false), true, |_| {}).unwrap();
    assert_eq!(origin.join().unwrap(), 2, "fetched again");
    // The origin is gone now: the next play still opens, from the new copy.
    let located = opener.locate(&entry).unwrap();
    assert_eq!(played(&located.media), Some(second.clone()), "the re-downloaded bytes, with no server");
    let stored = lib.server(track.source).unwrap().unwrap();
    let copy = lib.offline_copy(track.source, &stored.account, &stored.identity, &track.entry).unwrap().unwrap();
    assert_eq!(
        removal::remove(&lib, &copy, Some(&scope)),
        Err(chilpa_i18n::t("This offline file is in use. Stop playback before removing it.")),
        "what plays is not taken away"
    );
    assert!(Path::new(&copy.path).is_file());
    drop(located);
    removal::remove(&lib, &copy, Some(&scope)).unwrap();
    assert!(played(&opener.locate(&entry).unwrap().media).is_none(), "removed: back to the server's stream");
}

#[test]
fn a_copy_that_is_missing_or_changed_size_falls_back_to_the_server() {
    let bytes = silence();
    let (address, origin) = origin(&bytes, "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let scope = super::super::super::session::Scope::of(&lib.server(track.source).unwrap().unwrap());
    download(&lib, &root.path().join("OfflineFolder"), &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(false), |_| {}).unwrap();
    origin.join().unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    let copy = lib.offline_copy(track.source, &stored.account, &stored.identity, &track.entry).unwrap().unwrap();
    let entry = chilpa_models::RemoteEntry::parse(&track.entry).unwrap();
    let (opener, invalid) = opener(root.path(), keys_of(root.path()));
    std::fs::write(&copy.path, &bytes[..bytes.len() - 4]).unwrap();
    assert!(played(&opener.locate(&entry).unwrap().media).is_none(), "a shorter file is not played");
    assert!(invalid.pending.lock().unwrap().iter().any(|queued| queued.key == copy.key), "and is queued to be marked invalid");
    std::fs::remove_file(&copy.path).unwrap();
    assert!(played(&opener.locate(&entry).unwrap().media).is_none(), "a missing file is not played");
    std::fs::write(&copy.path, &bytes).unwrap();
    assert_eq!(played(&opener.locate(&entry).unwrap().media), Some(bytes), "the same bytes back: local again");
}
