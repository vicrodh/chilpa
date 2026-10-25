use super::*;

fn simulate(old: &[&str], new: &[&str]) -> Changes {
    let plan = between(old, new, |i, j| old[i] == new[j]).unwrap();
    let mut rows = old.to_vec();
    let mut positions: Vec<_> = (0..old.len()).map(Some).collect();
    for &step in &plan.steps {
        if let Step::Move { from, to } = step {
            assert_ne!(from, to);
            assert!(step.destination() <= rows.len());
        }
        step.apply(&mut rows, new);
        for position in &mut positions { *position = step.remap(*position); }
    }
    assert_eq!(rows, new);
    assert_eq!(rows.len(), new.len());
    for (i, position) in positions.into_iter().enumerate() {
        assert_eq!(position, new.iter().position(|key| *key == old[i]));
    }
    plan
}

#[test]
fn queue_headers_slide_on_track_change() {
    let plan = simulate(&["History", "a", "Now playing", "b", "Up next", "c", "d"],
        &["History", "a", "b", "Now playing", "c", "Up next", "d"]);
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.steps.iter().all(|step| matches!(step, Step::Move { .. })));
}

#[test]
fn sidebar_drops_the_finished_occurrence() {
    assert_eq!(simulate(&["a", "b", "c"], &["b", "c"]).steps, [Step::Remove(0)]);
}

#[test]
fn insert_at_front() {
    assert_eq!(simulate(&["b", "c"], &["a", "b", "c"]).steps, [Step::Insert(0)]);
}

#[test]
fn remove_in_middle() {
    assert_eq!(simulate(&["a", "b", "c"], &["a", "c"]).steps, [Step::Remove(1)]);
}

#[test]
fn reorder_and_mixed_edits_use_current_indices() {
    simulate(&["a", "b", "c", "d"], &["d", "c", "b", "a"]);
    let plan = simulate(&["a", "b", "c", "d"], &["x", "d", "a", "y"]);
    assert!(matches!(plan.steps[0], Step::Remove(_)));
    assert!(matches!(plan.steps[2], Step::Move { .. }));
    assert!(matches!(plan.steps[3], Step::Insert(_)));
}

#[test]
fn unchanged_has_no_work() {
    assert_eq!(simulate(&["a", "b"], &["a", "b"]), Changes::default());
}

#[test]
fn content_changes_follow_structural_edits() {
    let plan = between(&["a", "b"], &["b", "a"], |i, j| i == 1 && j == 0).unwrap();
    assert_eq!(plan.changed, [1]);
}

#[test]
fn bounds_and_ambiguous_keys_reset() {
    assert!(between(&vec![0; MAX_ROWS + 1], &[], |_, _| true).is_none());
    assert!(between(&[], &vec![0; MAX_ROWS + 1], |_, _| true).is_none());
    assert!(between(&[], &(0..MAX_STEPS + 1).collect::<Vec<_>>(), |_, _| true).is_none());
    assert!(between(&(0..MAX_ROWS).collect::<Vec<_>>(), &(0..MAX_ROWS).collect::<Vec<_>>(), |_, _| true).is_some());
    assert!(between(&[1, 1], &[1], |_, _| true).is_none());
}

#[test]
fn all_small_unique_orders_preserve_rows_and_selection() {
    fn orders(prefix: Vec<&'static str>, rest: Vec<&'static str>, out: &mut Vec<Vec<&'static str>>) {
        out.push(prefix.clone());
        for i in 0..rest.len() {
            let mut next = prefix.clone(); next.push(rest[i]);
            let mut left = rest.clone(); left.remove(i);
            orders(next, left, out);
        }
    }
    let mut cases = Vec::new();
    orders(vec![], vec!["a", "b", "c", "d"], &mut cases);
    for old in &cases { for new in &cases { simulate(old, new); } }
    assert_eq!(Step::Move { from: 0, to: 3 }.destination(), 4);
    assert_eq!(Step::Move { from: 0, to: 3 }.remap(Some(2)), Some(1));
}

fn track() -> TrackEntry {
    TrackEntry::Track(crate::browse::row_of(chilpa_library::BrowseTrack {
        id: 7, external_id: "file.flac".into(), title: "Track".into(), artist: "Artist".into(),
        album: "Album".into(), album_id: Some(1), disc_number: None, track_number: None,
        duration_ms: 1000, sample_rate_hz: None, bit_depth: None, plays: 0, artwork_path: None,
        available: true, codec: None, composers: String::new(), disc_subtitle: None, work: None,
    }))
}

#[test]
fn duplicate_tracks_use_queue_occurrences() {
    let old = [track(), track(), track()];
    let a = ["queue:0", "queue:1", "queue:2"].map(String::from);
    let b = ["queue:2", "queue:1"].map(String::from);
    let plan = tracks(&old, &old[..2], &a, &b).unwrap();
    let mut rows = a.to_vec();
    for step in plan.steps { step.apply(&mut rows, &b); }
    assert_eq!(rows, b);
    assert!(tracks(&old, &old[..2], &[], &[]).is_none());
    assert_eq!(track_anchor(&old, &old[..2], &a, &b, Some(1)), Some(1));
    assert_eq!(track_anchor(&old, &old[..2], &a, &b, Some(0)), None);
}

#[test]
fn track_headers_keep_identity_when_their_metadata_changes() {
    let old = [TrackEntry::header("Now playing".into(), String::new()), track()];
    let mut new = [track(), TrackEntry::header("Now playing".into(), "Details".into())];
    let plan = tracks(&old, &new, &[String::new(), "queue:1".into()], &["queue:1".into(), String::new()]).unwrap();
    assert_eq!(plan.steps, [Step::Move { from: 1, to: 0 }]);
    assert_eq!(plan.changed, [0, 1]);
    if let TrackEntry::Header(h) = &mut new[1] { h.work = true; }
    assert!(tracks(&old, &new, &[], &[]).unwrap().steps.contains(&Step::Remove(0)));
}

fn album(id: i64) -> Album {
    Album { id, title: id.to_string(), artist: String::new(), year: None, track_count: 1,
        artwork_path: String::new(), play_count: 0 }
}

#[test]
fn shelf_cards_move_and_covers_update_in_place() {
    let old = vec![album(1), album(2)];
    let mut new = vec![album(3), album(1), album(2)];
    new[2].artwork_path = "cover.jpg".into();
    let before = album_rows(old.len(), 6, 1, &[]);
    let after = album_rows(new.len(), 6, 1, &[]);
    assert_eq!(after.len(), 3);
    let plan = albums(&old, &new, &before, &after).unwrap();
    assert_eq!(plan.steps, [Step::Insert(0)]);
    let mut rows = before;
    for step in plan.steps { step.apply(&mut rows, &after); }
    for at in plan.changed { rows[at] = after[at].clone(); }
    assert_eq!(rows, after);
    let mut cover = new.clone(); cover[0].artwork_path = "new.jpg".into();
    let plan = albums(&new, &cover, &after, &after).unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.changed, [0]);
}

#[test]
fn grid_notifications_always_resolve_their_cards_and_end_in_new_order() {
    let old = vec![album(1), album(2), album(3), album(4)];
    let new = vec![album(5), album(6), album(3), album(4), album(1), album(2)];
    for columns in [1, 2, 3] {
        let mut rows = album_rows(old.len(), columns, 0, &[]);
        let after = album_rows(new.len(), columns, 0, &[]);
        let plan = albums(&old, &new, &rows, &after).unwrap();
        let storage: Vec<_> = old.iter().chain(&new).collect();
        let staged: Vec<_> = after.iter().cloned().map(|mut row| { row.first += old.len(); row }).collect();
        for &step in &plan.steps {
            step.apply(&mut rows, &staged);
            for row in &rows { assert_eq!(storage[row.first..row.first + row.span].len(), row.span); }
        }
        let ids: Vec<_> = rows.iter().flat_map(|r| &storage[r.first..r.first + r.span]).map(|a| a.id).collect();
        assert_eq!(ids, new.iter().map(|a| a.id).collect::<Vec<_>>());
        for at in plan.published() { rows[at] = after[at].clone(); }
        assert_eq!(rows, after);
    }
    assert_eq!(album_rows(30, 4, 1, &[]).len(), 4);
    assert!(album_rows(30, 0, 1, &[]).is_empty());
}
