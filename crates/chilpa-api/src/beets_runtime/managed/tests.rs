use super::*;
#[test]
fn a_setup_has_one_worker_and_cancellation_releases_its_slot() {
    let mut managed = Managed::new();
    let entered = Arc::new(AtomicBool::new(false));
    let observed = entered.clone();
    managed.start(true,move |shared| {
        observed.store(true,Ordering::Release);
        while !shared.cancel.load(Ordering::Acquire) { std::thread::sleep(Duration::from_millis(5)); }
        Err(Failure::Cancelled)
    });
    let extra = Arc::new(AtomicBool::new(false));
    let observed = extra.clone();
    managed.start(false,move |_| { observed.store(true,Ordering::Release); Err(Failure::Install) });
    let started = Instant::now();
    while !entered.load(Ordering::Acquire) { assert!(started.elapsed()<Duration::from_secs(2)); std::thread::yield_now(); }
    assert!(managed.busy());
    managed.cancel();
    while managed.poll().is_none() { assert!(started.elapsed()<Duration::from_secs(2)); std::thread::sleep(Duration::from_millis(5)); }
    assert!(!managed.busy());
    assert!(!extra.load(Ordering::Acquire));
    assert!(matches!(managed.shared.status.lock().unwrap().phase,Phase::Failed(Failure::Cancelled)));
}
#[test]
fn a_worker_panic_does_not_leave_the_settings_page_busy() {
    let mut managed = Managed::new();
    managed.start(false,|_| panic!("fixture worker failure"));
    let started = Instant::now();
    while managed.poll().is_none() { assert!(started.elapsed()<Duration::from_secs(2)); std::thread::sleep(Duration::from_millis(5)); }
    assert!(!managed.busy());
    assert!(matches!(managed.shared.status.lock().unwrap().phase,Phase::Failed(Failure::Install)));
}
