use super::*;
use super::super::draft::testing::plain;

fn candidate() -> Candidate {
    let id = "3d3207f9-c4c2-425c-a364-f359b4320805";
    Candidate { score:93, release:engine::Release { provider:engine::Source::MusicBrainz,
        id:id.into(), title:"Found album".into(), credits:vec![], date:"2001".into(), genres:vec!["Jazz".into()],
        label:String::new(), catalog_number:String::new(), catalog_numbers:vec![], country:String::new(), barcode:String::new(),
        discs:1, media_titles:vec![], release_group_id:id.into(), url:String::new(), tracks:(1..=2).map(|number| engine::ReleaseTrack {
            disc:1, number, title:format!("Found {number}"), duration_ms:1000, credits:vec![], recording_id:id.into(), track_id:id.into(),
        }).collect() }, mappings:vec![engine::Mapping { track:1, index:1, composers:vec!["Writer".into()], work:"Suite".into() },
            engine::Mapping { track:2, index:0, composers:vec![], work:String::new() }] }
}

fn ready() -> Editor {
    let candidate = candidate();
    let mut editor = Editor { state:EditorState::Ready, draft:Some(plain()), release:Some(release(&candidate)),
        beets:BeetsState { enabled:true, candidates:vec![candidate], selected:Some(0), ..BeetsState::default() }, ..Editor::default() };
    editor.rebuild_preview(); editor.refresh(); editor
}

fn response() -> Response {
    Response { protocol:engine::PROTOCOL, beets_version:engine::BEETS_VERSION.into(),
        recommendation:"strong".into(), candidates:vec![candidate()], error:None }
}

#[test]
fn proposals_follow_beets_mapping_and_keep_the_real_metadata_source() {
    let editor = ready();
    assert_eq!(editor.view.lookup.engine, "beets");
    assert_eq!(editor.view.lookup.provider, "musicbrainz");
    assert_eq!(editor.view.lookup.matched, 2);
    let changes = &editor.view.lookup.changes;
    assert_eq!(changes.iter().find(|change| change.key == "track:1:title").unwrap().proposed, "Found 2");
    assert_eq!(changes.iter().find(|change| change.key == "track:2:title").unwrap().proposed, "Found 1");
    assert!(changes.iter().any(|change| change.field == "musicbrainz_recording_id"));
    assert!(!changes.iter().any(|change| change.field == "discogs_release_id"));
    assert!(!changes.iter().find(|change| change.field == "genres").unwrap().selected);
    assert!(!editor.draft.as_ref().unwrap().touched, "previewing must not edit the form");
}

#[test]
fn applying_selected_fields_preserves_unselected_tracks_and_curated_genres() {
    let mut api = Chilpa::with_listed(Vec::new(),Vec::new());
    api.metadata = ready();
    let keys: Vec<_> = api.metadata.changes.iter().map(|change| change.key.clone()).collect();
    for key in keys { assert!(api.metadata_choose_change(&key, key == "track:1:title")); }
    assert_eq!(api.metadata_apply(),2);
    let draft = api.metadata.draft.as_ref().unwrap();
    assert_eq!(draft.rows[0].cells[fields::track("title").unwrap()].now,"Found 2");
    assert_eq!(draft.rows[1].cells[fields::track("title").unwrap()].now,"Track 2");
    assert_eq!(draft.album_cell("genres").now,"Rock");
    assert_eq!(draft.album_cell("title").now,"A record");
    assert!(api.metadata_view().dirty);
    assert!(api.metadata_choose_change("album:genres",true));
    api.metadata_apply();
    assert_eq!(api.metadata.draft.as_ref().unwrap().album_cell("genres").now,"Jazz");
}

#[test]
fn cancellation_clears_completed_and_late_response_slots() {
    for before in [true,false] {
        let mut state = BeetsState::default();
        let task = state.task(Request::probe()).unwrap();
        let slot = task.slot.clone();
        assert!(state.task(Request::probe()).is_none(), "one queued or running job per editor");
        if before { slot.finish(Ok(response())); }
        state.cancel();
        if !before { slot.finish(Ok(response())); }
        assert!(slot.take().is_none(), "closed sessions cannot retain metadata responses");
        drop(task);
        assert!(!state.busy());
    }
}

#[test]
fn the_lookup_request_contains_current_metadata_and_duration_without_music_paths() {
    let mut draft = plain();
    draft.set_track(1,"title","Curated title");
    let request = request(&draft,&HashMap::from([(1,123456)]));
    let json = serde_json::to_string(&request).unwrap();
    assert!(!json.contains("/m/A record"));
    assert!(!json.contains("external_id"));
    assert_eq!(request.tracks[0].title,"Curated title");
    assert_eq!(request.tracks[0].duration_ms,123456);
    assert_eq!(request.tracks[1].duration_ms,0);
}

#[test]
fn closing_the_editor_discards_late_proposals_without_touching_the_draft() {
    let mut editor = ready();
    let task = editor.beets.task(Request::probe()).unwrap();
    let slot = task.slot.clone();
    assert!(!editor.draft.as_ref().unwrap().touched);
    editor.forget();
    slot.finish(Ok(response()));
    assert!(slot.take().is_none());
    assert!(editor.draft.is_none());
    assert!(editor.release.is_none());
    assert!(editor.changes.is_empty());
    assert!(editor.beets.candidates.is_empty());
    drop(task);
    assert!(!editor.beets.busy());
}

#[test]
fn saving_the_form_cancels_a_pending_proposal_without_applying_it() {
    let mut api = Chilpa::with_listed(Vec::new(),Vec::new());
    api.metadata = ready();
    api.metadata.mode = super::super::Mode::Sidecar;
    let task = api.metadata.beets.task(Request::probe()).unwrap();
    let slot = task.slot.clone();
    // No manual changes: saving is refused and must not apply the proposal.
    assert!(!api.metadata_save());
    assert!(slot.cancelled.load(Ordering::Acquire));
    assert_eq!(api.metadata.draft.as_ref().unwrap().album_cell("genres").now,"Rock");
    assert!(!api.metadata.draft.as_ref().unwrap().touched);
    assert!(api.metadata.changes.is_empty());
    drop(task);
    assert!(!api.metadata.beets.busy());
}

#[test]
fn disabling_network_cancels_an_active_lookup_but_keeps_form_values() {
    let mut api = Chilpa::with_listed(Vec::new(),Vec::new());
    api.metadata = ready();
    let old_policy = chilpa_core::network::current();
    let previous = api.preference("integrations.offline").unwrap_or_else(|| "false".into());
    api.remember_preference_checked("integrations.offline","false").unwrap();
    let task = api.metadata.beets.task(Request { tracks:vec![engine::Track { id:1, ..Default::default() }], ..Request::default() }).unwrap();
    let slot = task.slot.clone();
    api.remember_preference_checked("integrations.offline","true").unwrap();
    assert!(slot.cancelled.load(Ordering::Acquire));
    assert_eq!(api.metadata.draft.as_ref().unwrap().album_cell("genres").now,"Rock");
    assert!(!api.metadata.draft.as_ref().unwrap().touched);
    drop(task);
    api.remember_preference_checked("integrations.offline",&previous).unwrap();
    chilpa_core::network::configure(old_policy);
}
