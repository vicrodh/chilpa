use super::super::tests::{fixture, origin, silence};
use super::*;

fn job(scopes: HashMap<i64, Option<Scope>>, root: &Path) -> impl FnOnce(u64) -> Job {
    let root = root.to_path_buf();
    move |seq| Job { seq, scopes, collection: "fixture".into(), root, limit: Limit::default() }
}

fn fetch(track: &Track, id: i64) -> (i64, Work) {
    (id, Work::Fetch { track: track.clone(), fresh: false })
}

#[test]
fn a_second_selection_waits_behind_the_first_and_each_track_keeps_how_it_went() {
    let (address, origin) = origin(&silence(), "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let scope = Scope::of(&lib.server(track.source).unwrap().unwrap());
    let folder = root.path().join("OfflineFolder");
    let tracker = Mutex::new(Tracker::default());
    {
        let mut queue = lock(&tracker);
        assert!(queue.push("album", vec![17], job(HashMap::from([(track.source, Some(scope))]), &folder), vec![fetch(&track, track.id)]));
        // A second selection while the first waits: queued, not refused. Its
        // scope was not captured, so it fails on its own.
        assert!(queue.push("tracks", vec![999], job(HashMap::new(), &folder), vec![fetch(&track, 999)]));
        assert_eq!(queue.queued(), HashSet::from([track.id, 999]));
        assert!(queue.busy());
        assert_eq!(queue.progress.total, 2);
        queue.alive = true;
    }
    let changed = AtomicBool::new(false);
    run(&root.path().join("library.sqlite3"), &keys, &tracker, &changed);
    origin.join().unwrap();
    let queue = lock(&tracker);
    assert!(!queue.busy() && queue.current.is_none(), "the worker ends with the queue");
    assert_eq!((queue.progress.done, queue.progress.failed), (1, 1));
    assert!(!queue.failures.contains_key(&track.id));
    assert_eq!(queue.failures.get(&999), Some(&crate::servers::session::changed()));
    assert!(queue.jobs.iter().all(|count| count.settled == count.total));
    assert!(changed.load(Ordering::Acquire), "the window hears a copy was kept");
    assert_eq!(lib.offline_marks().unwrap().iter().map(|mark| (mark.track, mark.ready)).collect::<Vec<_>>(), [(track.id, true)]);
}

#[test]
fn cancelling_drops_the_waiting_tracks_of_a_selection_and_stops_the_current_one() {
    let (root, _lib, _keys, track) = fixture("http://127.0.0.1:1/original");
    let mut queue = Tracker::default();
    assert!(queue.push("album", vec![1], job(HashMap::new(), root.path()), vec![fetch(&track, 1), fetch(&track, 2), fetch(&track, 3)]));
    queue.cancel(|id| id == 2);
    assert_eq!(queue.queued(), HashSet::from([1, 3]));
    assert_eq!((queue.progress.total, queue.jobs[0].settled), (2, 1));
    assert!(queue.failures.is_empty(), "a cancelled track is not a failed one");
    let (item, cancel) = queue.next().unwrap();
    assert_eq!((item.id, queue.current.as_ref().unwrap().phase), (1, "downloading"));
    queue.cancel(|id| id == 3);
    assert!(!cancel.load(Ordering::Acquire), "another selection's file goes on");
    queue.cancel_all();
    assert!(cancel.load(Ordering::Acquire) && queue.queued().is_empty() && queue.progress.cancelled);
    queue.finish(&item, Err(chilpa_i18n::t("Download cancelled.")), true);
    assert!(queue.failures.is_empty());
    assert!(queue.next().is_none() && !queue.busy());
}

#[test]
fn a_refused_selection_marks_its_tracks_and_queueing_them_again_clears_it() {
    let (root, _lib, _keys, track) = fixture("http://127.0.0.1:1/original");
    let mut queue = Tracker::default();
    queue.refuse(&[track.id], "servers are not started");
    assert_eq!(queue.failures.get(&track.id).map(String::as_str), Some("servers are not started"));
    assert!(queue.push("tracks", vec![track.id], job(HashMap::new(), root.path()), vec![fetch(&track, track.id)]));
    assert!(!queue.failures.contains_key(&track.id));
    let many: Vec<(i64, Work)> = (0..MAX_TRACKS as i64).map(|id| fetch(&track, id + 10)).collect();
    assert!(!queue.push("tracks", Vec::new(), job(HashMap::new(), root.path()), many), "past the bound nothing is queued");
    assert_eq!(queue.queued().len(), 1);
}

#[test]
fn a_cancelled_first_download_leaves_no_copy_and_no_failure_behind() {
    let (root, lib, keys, track) = fixture("http://127.0.0.1:1/original");
    let scope = Scope::of(&lib.server(track.source).unwrap().unwrap());
    let folder = root.path().join("OfflineFolder");
    let result = obtain(&lib, &folder, &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(true), false, |_| {});
    assert_eq!(result, Err(chilpa_i18n::t("Download cancelled.")));
    let stored = lib.server(track.source).unwrap().unwrap();
    assert!(lib.offline_copies_for(track.source, &stored.account, &stored.identity, &track.entry).unwrap().is_empty());
    assert!(lib.offline_marks().unwrap().is_empty());
    assert_eq!(lib.offline_bytes().unwrap(), 0);
}

#[test]
fn a_failed_fresh_download_keeps_the_copy_it_was_replacing() {
    let bytes = silence();
    let (address, origin) = origin(&bytes, "audio/wav");
    let (root, lib, keys, track) = fixture(&address);
    let scope = Scope::of(&lib.server(track.source).unwrap().unwrap());
    let folder = root.path().join("OfflineFolder");
    download(&lib, &folder, &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(false), |_| {}).unwrap();
    origin.join().unwrap();
    let stored = lib.server(track.source).unwrap().unwrap();
    let kept = lib.offline_copy(track.source, &stored.account, &stored.identity, &track.entry).unwrap().unwrap();
    // The origin is gone: the new file never arrives.
    let sizes = Mutex::new(Vec::new());
    let result = obtain(&lib, &folder, &keys, &track, &scope, "track/1", &Limit::default(), &AtomicBool::new(false), true, |update| {
        if let Update::Size(total) = update {
            sizes.lock().unwrap().push(total);
        }
    });
    assert!(result.is_err());
    assert_eq!(*sizes.lock().unwrap(), [bytes.len() as u64], "a re-download knows the size it expects");
    let after = lib.offline_copy(track.source, &stored.account, &stored.identity, &track.entry).unwrap().unwrap();
    assert_eq!((after.key, after.checksum, after.bytes), (kept.key, kept.checksum, kept.bytes));
    assert_eq!(std::fs::read(&kept.path).unwrap(), bytes);
    assert_eq!(lib.offline_marks().unwrap().iter().map(|mark| mark.ready).collect::<Vec<_>>(), [true]);
}
