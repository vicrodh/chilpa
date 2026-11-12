use super::*;
use chilpa_library::store::offline::Track;
use std::sync::atomic::AtomicBool;

fn track(id: i64) -> (i64, queue::Work) {
    let track = Track { id, source: 1, entry: format!("entry {id}"), revision: None, codec: None, title: format!("Track {id}") };
    (id, queue::Work::Fetch { track, fresh: false })
}

fn job(seq: u64) -> queue::Job {
    queue::Job { seq, scopes: HashMap::new(), collection: String::new(), root: std::path::PathBuf::new(), limit: super::super::limit::Limit::default() }
}

/// Kept 1, failed copy 2, waiting 3 and 7, on its way 4, failed now 5.
fn fixture(total: u64) -> OfflineMarks {
    let mut tracker = queue::Tracker::default();
    assert!(tracker.push("album", vec![17], job, vec![track(4), track(3), track(7)]));
    tracker.refuse(&[5, 1], "The server answered 500");
    tracker.current = Some(queue::Current { id: 4, seq: tracker.jobs[0].seq, phase: "downloading", bytes: 50, total, cancel: Arc::new(AtomicBool::new(false)) });
    tracker.jobs[0].settled = 1;
    let (kept, stored) = split(vec![
        Marked { track: 1, ready: true, error: String::new() },
        Marked { track: 2, ready: false, error: "The download is incomplete. Try again.".into() },
    ]);
    let mut cache = Cache { kept: Arc::new(kept), stored: Arc::new(stored), read: true, ..Default::default() };
    let mut out = snapshot(&mut cache, &tracker);
    // What waits is read from the queue; the current item left it already.
    out.queued = Arc::new(HashSet::from([3, 7]));
    out
}

#[test]
fn each_track_says_its_own_state_and_the_session_wins_over_the_library() {
    let marks = fixture(200);
    assert_eq!(
        [1, 2, 3, 4, 5, 6, 0].map(|id| marks.state(id)),
        ["failed", "failed", "queued", "downloading", "failed", "", ""],
    );
    assert_eq!(marks.error(1), "The server answered 500", "this session's failure is newer than the kept copy");
    assert_eq!(marks.error(2), "The download is incomplete. Try again.");
    assert_eq!(marks.error(3), "");
    assert_eq!(marks.fraction(4), 0.25);
    assert_eq!(marks.fraction(3), -1.0);
    assert!(marks.active());
    assert_eq!(fixture(0).fraction(4), -1.0, "an unknown size turns, it does not fill");
    assert_eq!(marks.tip(2), "The download is incomplete. Try again.");
    assert_eq!(marks.tip(3), chilpa_i18n::t("Waiting in the offline queue"));
    assert_eq!(marks.tip(6), "");
}

#[test]
fn a_selection_line_counts_what_settled_and_the_file_on_its_way() {
    let marks = fixture(200);
    assert_eq!(marks.job("album", "[17]"), (1.0 + 0.25) / 3.0);
    assert_eq!(marks.job("album", ""), (1.0 + 0.25) / 3.0, "the last one of its kind");
    assert_eq!(marks.job("album", "[18]"), -1.0);
    assert_eq!(marks.job("playlist", "[17]"), -1.0);
    assert_eq!(fixture(0).job("album", "[17]"), 1.0 / 3.0);
}

#[test]
fn the_menu_offers_what_the_tracks_can_do_now() {
    let mut marks = fixture(200);
    marks.failures = Arc::new(HashMap::from([(5, "why".to_string())]));
    let acts = |ids: &[i64]| marks.entries(ids).iter().map(|entry| entry["act"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(acts(&[6]), ["offline:start"]);
    assert_eq!(marks.entries(&[6])[0]["text"], chilpa_i18n::t("Make available offline"));
    assert_eq!(acts(&[1]), ["offline:redownload", "offline:remove_now"]);
    assert_eq!(marks.entries(&[1])[0]["text"], chilpa_i18n::t("Re-download"));
    assert_eq!(marks.entries(&[1])[1]["text"], chilpa_i18n::tn("Remove offline copy", "Remove offline copies", 1));
    assert_eq!(acts(&[2]), ["offline:start", "offline:remove_now"]);
    assert_eq!(marks.entries(&[2])[0]["text"], chilpa_i18n::t("Retry download"));
    assert_eq!(acts(&[5]), ["offline:start"], "a failure with no copy left has nothing to remove");
    assert_eq!(acts(&[1, 6]), ["offline:start", "offline:remove_now"], "some kept: the rest is made available");
    assert_eq!(marks.entries(&[1, 2])[1]["text"], chilpa_i18n::tn("Remove offline copy", "Remove offline copies", 2));
    assert_eq!(acts(&[3]), ["offline:cancel_selection"]);
    assert_eq!(acts(&[1, 4]), ["offline:cancel_selection"], "anything on its way: only cancel");
    assert!(marks.entries(&[]).is_empty());
}
