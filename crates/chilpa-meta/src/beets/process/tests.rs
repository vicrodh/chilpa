use super::*;
use super::super::Track;
use std::path::PathBuf;

// The protocol fixtures use only Python's standard library. A real beets
// smoke is separate; CI can select its interpreter without a shell command.
fn python() -> PathBuf { std::env::var_os("CHILPA_BEETS_TEST_PYTHON").map(PathBuf::from).unwrap_or_else(|| "python3".into()) }

fn with_lookup(body: &str) -> String {
    // Exercise the shipped framing and owner guard without a network service
    // or an installed beets dependency.
    format!("bridge = {{'__name__': 'chilpa_bridge_test'}}\nexec({}, bridge)\n{body}\nbridge['lookup'] = fixture_lookup\nbridge['main']()\n", serde_json::to_string(HELPER).unwrap())
}

#[test]
fn a_candidate_carries_the_genres_its_release_names_and_the_lookup_asks_for_them() {
    // The shipped helper, run on stand-ins for a match and for the configuration it sets.
    let script = format!(r#"
import json, sys, types
bridge = {{'__name__': 'chilpa_bridge_test'}}
exec({helper}, bridge)
def release(genres):
    track = types.SimpleNamespace(title='Part I', medium=1, medium_index=1, length=1, track_id='r', release_track_id='t')
    info = types.SimpleNamespace(data_source='MusicBrainz', tracks=[track], album_id='a', album='Album', genres=genres)
    return types.SimpleNamespace(info=info, mapping={{}}, distance=0.0)
fitted = (0.0, [1], 'CD', {{}})
named = bridge['candidate'](release(['rock', 'metal', 'folk', 'jazz', 'blues', 'pop', 'soul']), {{}}, fitted)['release']['genres']
whole = bridge['candidate'](release(None), {{}}, (0.25, [2], 'SACD', {{}}))
unnamed = whole['release']['genres']
class Setting(dict):
    def __missing__(self, key): self[key] = Setting(); return self[key]
    def set(self, value): self['value'] = value
    def read(self, user=True): pass
config = Setting()
sys.modules['beets'] = types.SimpleNamespace(__version__=bridge['BEETS_VERSION'], config=config)
bridge['lookup']({{'operation': 'probe'}})
plain = config['match']['preferred']['media'].get('value')
bridge['lookup']({{'operation': 'probe', 'media': 'SACD'}})
print(json.dumps([named, unnamed, config['musicbrainz']['genres'].get('value'), config['musicbrainz']['search_limit'].get('value'),
    [whole['score'], whole['whole'], whole['discs'], whole['format']], plain,
    config['match']['preferred']['media'].get('value'), config['match']['ignored_media'].get('value')]))
"#, helper = serde_json::to_string(HELPER).unwrap());
    let output = Command::new(python()).args(["-I", "-c", &script]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let seen: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(seen, serde_json::json!([["rock", "metal", "folk", "jazz", "blues"], [], true, 10,
        [75, 100, [2], "SACD"], null, ["(Hybrid )?SACD"], ["Hybrid SACD (CD layer)"]]));
}

#[test]
fn the_owner_keeps_the_pipe_open_until_the_result_frame_is_flushed() {
    let workspace = tempfile::tempdir().unwrap();
    let helper = with_lookup("def fixture_lookup(request):\n    return {'protocol': 1, 'beets_version': '2.14.1', 'candidates': [], 'recommendation': 'none'}");
    let result = exchange(&python(), workspace.path(), &Request::probe(), &AtomicBool::new(false), &helper, Duration::from_secs(5)).unwrap();
    assert_eq!(result.beets_version, super::super::BEETS_VERSION);
    assert!(result.candidates.is_empty());
}

#[test]
fn closing_the_owner_pipe_stops_the_shipped_helper_during_a_lookup() {
    let workspace = tempfile::tempdir().unwrap();
    let helper = with_lookup("import pathlib,time\ndef fixture_lookup(request):\n    pathlib.Path('started').write_text('ready')\n    time.sleep(30)");
    let mut child = OwnedChild(Command::new(python()).args(["-I", "-c", &helper]).current_dir(workspace.path())
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let mut input = child.0.stdin.take().unwrap();
    input.write_all(&Request::probe().encode().unwrap()).unwrap();
    input.write_all(b"\n").unwrap();
    let started = workspace.path().join("started");
    let deadline = Instant::now();
    while !started.exists() && deadline.elapsed() < Duration::from_secs(4) { std::thread::sleep(Duration::from_millis(10)); }
    assert!(started.exists(), "the helper must reach the lookup before its owner disappears");
    drop(input);
    let closed = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() { assert!(status.success()); break; }
        assert!(closed.elapsed() < Duration::from_secs(2), "owner EOF must stop the active helper");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cancelling_a_running_child_reaps_its_pipes_and_does_not_change_music() {
    let workspace = tempfile::tempdir().unwrap();
    let music = workspace.path().join("curated.flac");
    let sidecar = workspace.path().join("album.sidecar");
    std::fs::write(&music, b"original music sentinel").unwrap();
    std::fs::write(&sidecar, b"original sidecar sentinel").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let directory = workspace.path().to_path_buf();
    let started = workspace.path().join("started");
    let worker = std::thread::spawn(move || exchange(&python(), &directory, &Request::probe(), &stop,
        "import pathlib,sys,time; sys.stdin.buffer.readline(); pathlib.Path('started').write_text('ready'); time.sleep(30)", Duration::from_secs(5)));
    let deadline = Instant::now();
    while !started.exists() && deadline.elapsed() < Duration::from_secs(4) { std::thread::sleep(Duration::from_millis(10)); }
    let running = started.exists();
    let cancellation = Instant::now();
    cancel.store(true, Ordering::Release);
    assert_eq!(worker.join().unwrap().unwrap_err(), Error::Cancelled);
    assert!(running, "the cancellation must exercise a child that actually started");
    assert!(cancellation.elapsed() < Duration::from_secs(2), "both pipe threads must finish with the child");
    assert_eq!(std::fs::read(music).unwrap(), b"original music sentinel");
    assert_eq!(std::fs::read(sidecar).unwrap(), b"original sidecar sentinel");
}

#[test]
fn a_runtime_that_never_reads_its_input_still_times_out() {
    let workspace = tempfile::tempdir().unwrap();
    let request = Request { tracks:(0..200).map(|id| Track { id, title:"a".repeat(3000), ..Track::default() }).collect(), ..Request::default() };
    let started = Instant::now();
    let result = exchange(&python(), workspace.path(), &request, &AtomicBool::new(false), "import time; time.sleep(30)", Duration::from_millis(120));
    assert_eq!(result.unwrap_err(), Error::Timeout);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn oversized_output_is_bounded_and_stops_the_runtime() {
    let workspace = tempfile::tempdir().unwrap();
    let helper = format!("import sys,time; sys.stdin.buffer.readline(); sys.stdout.buffer.write(b'x'*{}); sys.stdout.buffer.flush(); time.sleep(30)", MAX_OUTPUT + 8192);
    let result = exchange(&python(), workspace.path(), &Request::probe(), &AtomicBool::new(false), &helper, Duration::from_secs(5));
    assert_eq!(result.unwrap_err(), Error::OutputLimit);
}

#[test]
fn malformed_output_and_missing_runtimes_are_explicit_failures() {
    let workspace = tempfile::tempdir().unwrap();
    let result = exchange(&python(), workspace.path(), &Request::probe(), &AtomicBool::new(false), "import sys; sys.stdin.buffer.readline(); print('not JSON')", Duration::from_secs(5));
    assert_eq!(result.unwrap_err(), Error::InvalidResponse);
    assert_eq!(lookup(&workspace.path().join("missing-interpreter"), workspace.path(), &Request::probe(), &AtomicBool::new(false)).unwrap_err(), Error::Unavailable);
}

#[test]
fn cancellation_and_request_validation_happen_before_process_creation() {
    let workspace = tempfile::tempdir().unwrap();
    let missing = workspace.path().join("missing-interpreter");
    assert_eq!(lookup(&missing, workspace.path(), &Request::probe(), &AtomicBool::new(true)).unwrap_err(), Error::Cancelled);
    assert_eq!(lookup(&missing, workspace.path(), &Request::default(), &AtomicBool::new(false)).unwrap_err(), Error::TrackLimit);
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
}
