//! The library analysis reports a complete lifecycle even when there is
//! nothing to analyse.  The Settings page relies on that final event to
//! replace its progress bar and make AutoMix availability current again.

#[test]
fn an_empty_library_analysis_finishes_and_reports_zero_work() {
    let dir = std::env::temp_dir().join(format!("chilpa-api-analysis-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.start_analysis();

    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = chilpa.analysis_status().clone();
        if !status.running {
            assert_eq!(status.done, 0);
            assert_eq!(status.total, 0);
            assert_eq!(status.analyzed, 0);
            assert_eq!(status.without_pulse, 0);
            assert_eq!(status.failed, 0);
            break;
        }
        assert!(std::time::Instant::now() < limit, "the analysis did not finish");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let _ = std::fs::remove_dir_all(dir);
}
