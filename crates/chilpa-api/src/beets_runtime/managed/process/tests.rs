use super::*;
use crate::beets_runtime::managed::{Phase,Status};
use std::{sync::Mutex,io::Write};
fn shared() -> Arc<Shared> {
    Arc::new(Shared { status:Mutex::new(Status { phase:Phase::Waiting,progress:0.0,installing:true }),
        cancel:AtomicBool::new(false),child:Mutex::new(None),result:Mutex::new(None) })
}
fn arguments(name: &str) -> Vec<OsString> {
    vec!["--exact".into(),format!("beets_runtime::managed::process::tests::{name}").into(),"--ignored".into(),"--nocapture".into()]
}
#[test]
fn cancellation_kills_and_reaps_a_started_installer_child() {
    let directory = tempfile::tempdir().unwrap();
    let shared = shared();
    let signal = directory.path().join("started");
    let flag = shared.clone();
    let cancel = std::thread::spawn(move || {
        let started = Instant::now();
        while !signal.exists() { assert!(started.elapsed()<Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(5)); }
        flag.cancel.store(true,Ordering::Release);
    });
    let started = Instant::now();
    let result = run(&shared,&std::env::current_exe().unwrap(),&arguments("fixture_waiting_child"),directory.path(),directory.path(),started+Duration::from_secs(6));
    cancel.join().unwrap();
    assert_eq!(result,Err(Failure::Cancelled));
    assert!(started.elapsed()<Duration::from_secs(3),"an inherited pipe must not hold the cancelled job open");
    assert!(shared.child.lock().unwrap().is_none());
    assert!(!directory.path().join("finished").exists());
}
#[test]
fn timeouts_and_output_overflow_release_the_installer_slot() {
    for (name,duration,expected) in [("fixture_waiting_child",Duration::from_millis(400),Failure::Timeout),
        ("fixture_large_output",Duration::from_secs(3),Failure::OutputLimit)] {
        let directory = tempfile::tempdir().unwrap();
        let shared = shared();
        let started = Instant::now();
        assert_eq!(run(&shared,&std::env::current_exe().unwrap(),&arguments(name),directory.path(),directory.path(),started+duration),Err(expected));
        assert!(shared.child.lock().unwrap().is_none());
        assert!(started.elapsed()<Duration::from_secs(3));
    }
}
#[test]
#[ignore = "subprocess fixture, selected by the process tests"]
fn fixture_waiting_child() {
    std::fs::write("started",b"started").unwrap();
    std::thread::sleep(Duration::from_secs(5));
    std::fs::write("finished",b"finished").unwrap();
}
#[test]
#[ignore = "subprocess fixture, selected by the process tests"]
fn fixture_large_output() {
    let _ = std::io::stdout().write_all(&vec![b'x';OUTPUT_LIMIT*4]);
    std::thread::sleep(Duration::from_secs(5));
}
