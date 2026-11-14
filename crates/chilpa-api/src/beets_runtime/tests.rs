use super::*;
use std::{path::PathBuf, time::{Duration,Instant}};
struct Restore(chilpa_core::settings::Settings,chilpa_core::network::Policy);
impl Restore {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("CHILPA_TEST_PROFILE_ROOT").expect("use scripts/run-api-tests-isolated.py"));
        assert_eq!(std::fs::read(root.join(".chilpa-api-test-profile")).unwrap(),b"chilpa-api-unit-tests-v1\n");
        Self(chilpa_core::settings::load(),chilpa_core::network::current())
    }
}
impl Drop for Restore {
    fn drop(&mut self) { let _ = chilpa_core::settings::save(&self.0); chilpa_core::network::configure(self.1); }
}
#[test]
fn settings_select_the_engine_and_runtime_without_an_open_editor() {
    let _restore = Restore::new();
    let mut api = Chilpa::with_listed(vec![],vec![]);
    assert!(api.adjust(ENGINE,"beets"));
    assert_eq!(api.preference(ENGINE).as_deref(),Some("beets"));
    assert!(api.adjust(PYTHON,"/fixture/custom python"));
    assert_eq!(api.beets_python(),"/fixture/custom python");
    let (rows,_) = api.settings_rows("library",&crate::RowNotes::default());
    assert_eq!(rows.iter().find(|row| row.key==ENGINE).unwrap().value,"beets");
    let runtime = rows.iter().find(|row| row.key==PYTHON).unwrap();
    assert_eq!(runtime.control,"file");
    assert_eq!(runtime.value,"/fixture/custom python");
    assert!(runtime.options[0].ends_with("(*)"));
    assert!(!api.settings_working("library"));
}
#[test]
fn offline_installation_preserves_the_previous_runtime_and_starts_no_worker() {
    let restore = Restore::new();
    let mut api = Chilpa::with_listed(vec![],vec![]);
    assert!(api.metadata_set_beets_runtime("/fixture/previous python"));
    chilpa_core::network::configure(chilpa_core::network::Policy { offline:true,..restore.1 });
    assert!(api.maintain(INSTALL).is_some());
    assert_eq!(api.beets_python(),"/fixture/previous python");
    assert!(api.beets_runtime.is_none());
}
#[test]
#[ignore = "downloads an isolated runtime; run explicitly through the API profile runner"]
fn requested_managed_installation_is_usable_after_its_cache_is_released() {
    let _restore = Restore::new();
    assert_eq!(std::env::var("CHILPA_MANAGED_BEETS_SMOKE").as_deref(),Ok("1"));
    chilpa_core::network::configure(chilpa_core::network::Policy::default());
    let sentinel = chilpa_dirs::data().join("curated-music-sentinel");
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel,b"curated metadata stays intact").unwrap();
    let mut api = Chilpa::with_listed(vec![],vec![]);
    assert!(api.maintain(INSTALL).is_some());
    let started = Instant::now();
    let mut phase = String::new();
    while api.beets_runtime_busy() {
        api.advance_beets_runtime();
        let message = api.beets_manager().view().message;
        if message != phase { println!("setup phase: {message}"); phase = message; }
        assert!(started.elapsed()<Duration::from_secs(300),"managed installation timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
    let installed = packages::installed().unwrap_or_else(|| panic!("managed runtime was not activated: {}",api.beets_manager().view().message));
    assert_eq!(api.beets_python(),installed.python().to_string_lossy());
    let check = tempfile::tempdir().unwrap();
    let response = chilpa_meta::beets::lookup(&installed.python(),check.path(),&chilpa_meta::beets::Request::probe(),&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert_eq!(response.beets_version,packages::BEETS);
    assert_eq!(std::fs::read(sentinel).unwrap(),b"curated metadata stays intact");
}
