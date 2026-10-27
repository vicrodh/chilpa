use super::*;
use engine::{Identity, Observation, SampleFormat};
fn case(id: &str, rate: u32) -> Case {
    Case { identity:Identity { backend:"fixture".into(),id:id.into(),stable_id:id.into(),strong:true,
        transport:"fixture".into(),generation:"1".into() },
        requested:StreamFormat::new(SampleFormat::S16,rate,2),exclusive_requested:true,opened:None,
        observation:Observation::default(),state:State::Available,method:"fixture".into(),timestamp:1,
        audible:State::Untested,physical_format:State::Unknown,error:String::new() }
}
#[test]
fn history_preserves_case_identity_and_never_promotes_opening_to_audibility() {
    let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("history.json");
    history::append(&path,case("uid-a",44_100)).unwrap();
    history::append(&path,case("uid-b",96_000)).unwrap();
    let rows = history::load(&path).unwrap();
    assert_eq!(rows.len(),2); assert_eq!(rows[0].audible,State::Untested);
    assert_ne!(rows[0].identity,rows[1].identity);
    let mut replaced = case("uid-a",44_100); replaced.state = State::Failed;
    let rows = history::append(&path,replaced).unwrap();
    assert_eq!(rows.len(),2); assert_eq!(rows.last().unwrap().state,State::Failed);
}
#[test]
fn history_is_bounded_and_damaged_history_is_preserved() {
    let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("history.json");
    for n in 0..140 { history::append(&path,case(&format!("uid-{n}"),44_100)).unwrap(); }
    let rows = history::load(&path).unwrap(); assert_eq!(rows.len(),128);
    std::fs::write(&path,b"damaged").unwrap();
    assert!(history::append(&path,case("new",48_000)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(),b"damaged");
}
#[test]
fn history_write_failure_does_not_claim_persistence() {
    let temp = tempfile::tempdir().unwrap(); let parent = temp.path().join("file");
    std::fs::write(&parent,b"unchanged").unwrap();
    assert!(history::append(&parent.join("history.json"),case("a",44_100)).is_err());
    assert_eq!(std::fs::read(parent).unwrap(),b"unchanged");
}
#[test]
fn late_results_do_not_repopulate_a_closed_view() {
    let mut api = crate::Chilpa::new();
    let (tx,rx) = mpsc::sync_channel(1);
    api.audio_output.pending = Some(rx);
    api.audio_output.shown = true; api.audio_output.selection = "old".into();
    api.audio_output_command("close","");
    tx.send(ResultDoc { selection:"old".into(),inspection:Inspection::unavailable("fixture","old","1".into()),
        outputs:Some(vec![json!({"id":"old"})]),history:vec![case("old",44_100)],last:None,error:String::new(),hardware_generation:String::new() }).unwrap();
    api.advance_audio_output();
    assert!(api.audio_output.pending.is_none()); assert!(api.audio_output.outputs.is_empty());
    assert!(api.audio_output.history.is_empty()); assert!(api.audio_output_doc(0).is_none());
}
#[test]
fn downstream_uncertainty_is_not_presented_as_processing() {
    let mut api = crate::Chilpa::new();
    api.now.playing = true;
    api.audio_output.shown = true;
    api.audio_output.route = Some(Route { revision:1,device:"fixture:a".into(),
        delivery:chilpa_core::output_evidence::Delivery { requested:case("a",44_100).requested,
            opened:case("a",44_100).requested,transforms:Vec::new() },planned_processing:Vec::new(),exclusive_requested:false,observation:Observation::default() });
    let (_,text) = api.audio_output_doc(0).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["now"]["samplesChanged"],false);
    api.audio_output.route.as_mut().unwrap().delivery.transforms.push(engine::Transform::Narrowed {from_bits:24,to_bits:16});
    api.audio_output.route.as_mut().unwrap().revision += 1;
    let (_,text) = api.audio_output_doc(0).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["now"]["samplesChanged"],true);
}
#[test]
fn a_response_for_another_selection_is_discarded() {
    let mut api = crate::Chilpa::new();
    let (tx,rx) = mpsc::sync_channel(1);
    api.audio_output.pending = Some(rx); api.audio_output.shown = true;
    api.audio_output.selection = "new".into();
    tx.send(ResultDoc { selection:"old".into(),inspection:Inspection::unavailable("fixture","old","1".into()),
        outputs:Some(vec![json!({"id":"old"})]),history:Vec::new(),last:None,error:String::new(),hardware_generation:String::new() }).unwrap();
    api.advance_audio_output();
    assert!(api.audio_output.inspection.is_none()); assert!(api.audio_output.outputs.is_empty());
    assert_eq!(api.audio_output.selection,"new");
}
#[test]
fn an_active_job_cannot_spawn_a_second_check_and_cancellation_keeps_the_job_owned() {
    let mut api = crate::Chilpa::new(); let (_tx,rx) = mpsc::sync_channel(1);
    api.audio_output.pending = Some(rx);
    api.audio_output_command("check",r#"{"format":{"sample_format":"s16","sample_rate_hz":44100,"channels":2},"exclusive":false}"#);
    assert!(api.audio_output.pending.is_some()); assert!(!api.audio_output.error.is_empty());
    api.audio_output_command("cancel","");
    assert!(api.audio_output.pending.is_some()); assert!(api.audio_output.cancel.load(Ordering::Acquire));
}
#[test]
fn a_settings_action_requests_the_shared_panel_without_inspecting_or_selecting() {
    let mut api = crate::Chilpa::new();
    assert_eq!(api.maintain("audio.outputDetails"),Some(String::new()));
    assert!(api.take_audio_output_request()); assert!(!api.take_audio_output_request());
    assert!(api.audio_output.pending.is_none()); assert!(api.audio_output.inspection.is_none());
}
#[test]
fn a_case_from_another_inspection_cannot_open_or_select_an_output() {
    let mut api = crate::Chilpa::new();
    api.audio_output.selection = "fixture:target".into();
    api.audio_output.inspection = Some(Inspection::unavailable("fixture","fixture:target","revision-2".into()));
    let request = json!({"output":"fixture:target","generation":"revision-1",
        "format":{"sample_format":"s16","sample_rate_hz":44100,"channels":2},"exclusive":false});
    assert!(!api.audio_output_command("check",&request.to_string()));
    assert!(api.audio_output.pending.is_none()); assert!(!api.audio_output.error.is_empty());
    assert!(!api.audio_output_command("use","fixture:other"));
    assert_eq!(api.audio_output.selection,"fixture:target");
}

#[test]
fn a_restored_paused_queue_can_stop_before_checking_without_opening_audio() {
    let mut api=crate::Chilpa::new();
    api.queue=vec!["fixture.wav".into()]; api.now.path="fixture.wav".into();
    api.now.playing=true;api.now.paused=true;
    api.stop();
    assert!(!api.now.playing);assert!(!api.now.paused);assert!(api.player.is_none());
    assert_eq!(api.queue,["fixture.wav"]);assert_eq!(api.now.path,"fixture.wav");
}
#[test]
fn a_completed_check_keeps_the_inspected_catalog_without_listing_again() {
    let mut api=crate::Chilpa::new();
    let (tx,rx)=mpsc::sync_channel(1);
    api.audio_output.pending=Some(rx);api.audio_output.shown=true;api.audio_output.selection="fixture:local".into();
    api.audio_output.outputs=vec![json!({"id":"fixture:local"}),json!({"id":"cast:remote"})];
    tx.send(ResultDoc {selection:"fixture:local".into(),inspection:Inspection::unavailable("fixture","fixture:local","1".into()),
        outputs:None,history:Vec::new(),last:Some(case("fixture:local",44100)),error:String::new(),hardware_generation:String::new()}).unwrap();
    api.advance_audio_output();
    assert_eq!(api.audio_output.outputs.len(),2);assert_eq!(api.audio_output.outputs[1]["id"],"cast:remote");
}

#[test]
fn queue_path_is_read_only_and_keeps_downstream_uncertainty() {
    let mut api = crate::Chilpa::new();
    let idle: Value = serde_json::from_str(&api.audio_signal_path()).unwrap();
    assert_eq!(idle["status"], "idle");
    api.now.playing = true;
    let waiting: Value = serde_json::from_str(&api.audio_signal_path()).unwrap();
    assert_eq!(waiting["status"], "waiting");
    api.audio_output.route = Some(Route { revision: 1, device: "fixture:a".into(),
        delivery: engine::Delivery { requested: case("a",44_100).requested,
            opened: case("a",44_100).requested, transforms: Vec::new() },
        planned_processing: vec![engine::Transform::DspApplied], exclusive_requested: true,
        observation: Observation::default() });
    let path: Value = serde_json::from_str(&api.audio_signal_path()).unwrap();
    assert_eq!(path["status"], "unverified");
    assert_eq!(path["stages"].as_array().unwrap().len(), 4);
    assert_eq!(path["stages"][3]["mode"], "Mode not reported");
    assert_eq!(path["stages"][1]["value"], "Unchanged", "planned DSP is not observed processing");
    assert!(path["stages"][2]["detail"].as_str().unwrap().contains("No physical readback"));
    api.now.bit_perfect = true;
    let unchanged: Value = serde_json::from_str(&api.audio_signal_path()).unwrap();
    assert_eq!(unchanged["status"], "unchanged");
    assert_eq!(unchanged["stages"][3]["mode"], "Mode not reported", "unchanged engine samples do not certify exclusive acquisition");
    api.audio_output.route.as_mut().unwrap().delivery.transforms.push(engine::Transform::Narrowed { from_bits:24, to_bits:16 });
    let converted: Value = serde_json::from_str(&api.audio_signal_path()).unwrap();
    assert_eq!(converted["status"], "processed", "observed conversion wins over a stale unchanged flag");
    for _ in 0..10 { let _ = api.audio_signal_path(); }
    assert!(!api.audio_output.shown);
    assert!(api.audio_output.pending.is_none());
    assert!(api.audio_output.inspection.is_none());
    assert!(api.audio_output.outputs.is_empty());
    assert!(api.audio_output.hardware_generation.is_empty());
    assert!(api.audio_output.history.is_empty());
}
