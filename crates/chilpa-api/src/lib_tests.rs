use super::*;

#[test]
fn listing_sources_returns_the_local_library() {
    let mut chilpa = Chilpa::new();
    assert_eq!(chilpa.dispatch(Command::ListSources), Event::Sources(vec![SourceId::local()]));
}

#[test]
fn the_mix_walks_tempo_to_tempo_without_jumps() {
    // It starts at the 120 one and should go on 118, 125, 90.
    let bpms = vec![Some(90.0), Some(120.0), Some(125.0), Some(118.0)];
    assert_eq!(order_by_tempo(&bpms, 1), vec![1, 3, 2, 0]);
}

#[test]
fn what_has_no_tempo_goes_last() {
    let bpms = vec![Some(120.0), None, Some(122.0), None, Some(160.0)];
    let order = order_by_tempo(&bpms, 0);
    assert_eq!(&order[..3], &[0, 2, 4], "the ones that do come first, nearest to nearest");
    let queue: Vec<usize> = order[3..].to_vec();
    assert_eq!(queue, vec![1, 3], "and the blind ones at the end, in their own order");
}

#[test]
fn a_mix_loses_no_track_and_repeats_none() {
    let bpms = vec![Some(100.0), Some(140.0), None, Some(101.0), Some(139.0), None];
    let order = order_by_tempo(&bpms, 3);
    let mut seen = order.clone();
    seen.sort_unstable();
    assert_eq!(seen, (0..6).collect::<Vec<_>>(), "it is a permutation: {order:?}");
    assert_eq!(order[0], 3, "it starts where it was told to");
}

#[test]
fn with_no_tempo_at_all_the_mix_leaves_the_order_alone() {
    let bpms = vec![None, None, None];
    assert_eq!(order_by_tempo(&bpms, 0), vec![0, 1, 2]);
}

#[test]
fn channels_are_named_the_way_they_are_said() {
    assert_eq!(channels(1), "mono");
    assert_eq!(channels(2), "stereo");
    assert_eq!(channels(6), "6 channels");
}

#[test]
fn the_state_starts_empty_and_without_error() {
    const CHILD: &str = "CHILPA_TEST_EMPTY_STATE";
    if std::env::var(CHILD).as_deref() == Ok("1") {
        let chilpa = Chilpa::new();
        assert_eq!(chilpa.now_playing(), &NowPlaying::default());
        assert!(chilpa.queue_paths().is_empty());
        return;
    }
    // I exercise the real constructor without restoring my saved queue or
    // changing the process-wide environment of concurrently running tests.
    let profile = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lib_tests::the_state_starts_empty_and_without_error", "--nocapture"])
        .env(CHILD, "1")
        .envs(chilpa_dirs::profile_environment(profile.path()))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success(), "isolated startup failed: {stdout}\n{}", String::from_utf8_lossy(&result.stderr));
    assert!(stdout.contains("1 passed; 0 failed"), "the isolated startup assertion did not run: {stdout}");
}

#[test]
fn every_listed_output_names_the_stack_it_belongs_to() {
    // A bare "hw:1,0" does not say which stack it came from, and that is
    // exactly what somebody has to be told when a remembered output is
    // no longer there.
    for output in Chilpa::new().outputs() {
        let (stack, bare) = output.id.split_once(':').unwrap_or_else(|| panic!("{} names no stack", output.id));
        // Asked of the core and not of the audio crate. The facade is
        // not allowed to reach into the engine (ADR-001), and the answer
        // to "which stacks are there" belongs to whoever assembles them.
        // Writing the list out here instead would be the same leak with
        // the wires hidden: a copy that goes stale the day one is added.
        assert!(chilpa_core::backends::StackKind::from_key(stack).is_some(), "{} names no stack this build has", output.id);
        assert!(!bare.is_empty(), "{} names no output", output.id);
        assert!(!output.label.is_empty(), "{} has no name to show", output.id);
    }
}

#[test]
fn what_does_not_exist_fails_with_a_message_for_the_ui() {
    let mut chilpa = Chilpa::new();
    let track = TrackRef { source: SourceId::local(), external_id: "1".into() };
    assert!(matches!(chilpa.dispatch(Command::Play(track)), Event::Failed { .. }));
}
