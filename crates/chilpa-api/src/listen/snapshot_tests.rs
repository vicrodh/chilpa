use super::*;
use std::hash::{Hash, Hasher};

fn fixture() -> Chilpa {
    let mut app = Chilpa::new();
    app.listen = Workspace::default();
    app.shuffle = false; app.repeat = Repeat::Off;
    let duplicate = "duplicate\"\n\u{266b}".repeat(2048);
    app.queue = vec![duplicate.clone(), "middle".into(), duplicate];
    app.queue_origins = (0..3).map(|id| Some(crate::queue_origin::QueueOrigin {
        screen: "album".into(), id, key: "unicode \u{266b}".into(), ..Default::default()
    })).collect();
    app.removed = vec![false, true, false];
    app.queue_placement = vec![QueuePlacement::Original, QueuePlacement::Next, QueuePlacement::Later];
    app.queue_groups = vec![QueueGroup { key: 9, label: "Batch".into(), tracks: vec![1, 2], collapsed: true }];
    app.listen.buffers[0].order = vec![2, 0, 1];
    app.now = NowPlaying { path: app.queue[2].clone(), index: 2, seconds: 42.125,
        playing: true, queue_len: app.queue.len(), ..Default::default() };
    app.stop_after = Some(0);
    let mut other = Buffer::empty(2);
    other.paths = vec!["saved".into()]; other.order = vec![0]; other.removed = vec![false];
    other.placement = vec![QueuePlacement::Later];
    app.listen.buffers.push(other); app.listen.next = 3;
    app
}

fn old_workspace(app: &Chilpa) -> Workspace {
    let mut saved = app.listen.clone();
    if let Some(b) = app.buffer_snapshot(saved.active) {
        if let Some(at) = saved.buffers.iter_mut().find(|v| v.id == b.id) { *at = b; }
    }
    if let Some(media) = &app.media {
        saved.buffers.retain(|b| b.id != media.buffer);
        if saved.active == media.buffer { saved.active = 0; }
        if saved.buffers.is_empty() { saved.buffers.push(Buffer::empty(saved.next)); saved.next += 1; }
        if saved.selected == media.buffer { saved.selected = saved.buffers[0].id; }
    }
    saved
}

fn old_revision(app: &Chilpa) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for item in &app.listen.buffers {
        let mut b = app.buffer_snapshot(item.id).unwrap(); b.seconds = 0.0;
        serde_json::to_string(&b).unwrap().hash(&mut hash);
    }
    app.now.playing.hash(&mut hash); app.now.paused.hash(&mut hash);
    hash.finish().to_string()
}

fn metadata(app: &Chilpa) -> serde_json::Value { serde_json::from_str(&app.listen_json()).unwrap() }

#[test]
fn borrowed_sessions_and_hashes_match_owned_snapshots_with_live_overlays() {
    let mut app = fixture();
    for step in 0..5 {
        let fallback = Buffer::empty(app.listen.next);
        let bytes = serde_json::to_vec(&app.workspace_view(&fallback)).unwrap();
        assert!(bytes.len() > 8192, "the hash spans multiple buffered writes");
        assert_eq!(bytes, serde_json::to_vec(&old_workspace(&app)).unwrap());
        let restored: Workspace = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.buffers.len(), app.listen.buffers.len());
        assert_eq!(metadata(&app)["revision"], old_revision(&app));
        let view = app.buffer_view(app.listen.active).unwrap();
        assert!(std::ptr::eq(view.paths.as_ptr(), app.queue.as_ptr()));
        assert!(std::ptr::eq(view.origins.as_ptr(), app.queue_origins.as_ptr()));
        match step {
            0 => app.now.seconds = 123.75,
            1 => app.queue_origins[2].as_mut().unwrap().id = 88,
            2 => app.listen.buffers[0].order.swap(0, 1),
            _ => app.queue_groups[0].collapsed = false,
        }
    }
}

#[test]
fn queue_metadata_ignores_position_but_keeps_same_length_edits_and_status() {
    let mut app = fixture();
    let initial = metadata(&app);
    app.now.seconds += 1.0;
    assert_eq!(metadata(&app), initial);
    let mut previous = initial["revision"].clone();
    for step in 0..8 {
        match step {
            0 => app.queue_origins[0].as_mut().unwrap().id += 1,
            1 => app.listen.buffers[0].order.swap(0, 1),
            2 => app.queue_groups[0].collapsed = !app.queue_groups[0].collapsed,
            3 => app.removed[1] = false,
            4 => app.listen.buffers[1].name = "Renamed".into(),
            5 => app.now.index = 0,
            6 => app.stop_after = Some(2),
            _ => app.now.paused = true,
        }
        let value = metadata(&app);
        assert_ne!(value["revision"], previous, "edit {step} must refresh queue rows");
        assert_eq!(value["revision"], old_revision(&app));
        previous = value["revision"].clone();
    }
    app.listen.selected = 2; app.listen.pending = Some((2, 0, 0.0)); app.now.error = "failure".into();
    app.listen.undo = Some(Undo { originals: vec![], created: vec![], selected: 1 });
    let value = metadata(&app);
    assert_eq!(value["selected"], "2"); assert_eq!(value["name"], "Renamed");
    assert_eq!(value["pending"], true); assert_eq!(value["undo"], true);
    assert_eq!(value["error"], "failure");
}

#[test]
fn temporary_media_exclusion_preserves_saved_selection_and_fallback() {
    let mut app = fixture();
    let buffers = std::mem::take(&mut app.listen.buffers);
    let fallback = Buffer::empty(app.listen.next);
    assert_eq!(serde_json::to_vec(&app.workspace_view(&fallback)).unwrap(),
        serde_json::to_vec(&old_workspace(&app)).unwrap(), "no fallback without a temporary media session");
    app.listen.buffers = buffers;
    app.media = Some(crate::open_media::Session { buffer: 1,
        media: crate::open_media::OpenedMedia { title: String::new(), tracks: vec![], about: vec![] } });
    for only_media in [false, true] {
        if only_media { app.listen.buffers.truncate(1); }
        let fallback = Buffer::empty(app.listen.next);
        assert_eq!(serde_json::to_vec(&app.workspace_view(&fallback)).unwrap(),
            serde_json::to_vec(&old_workspace(&app)).unwrap());
    }
}
