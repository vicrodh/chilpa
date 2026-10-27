//! The one process stderr reader; every retained value belongs to the facade.
unsafe extern "C" {
    fn chilpa_start_log_capture() -> bool;
    fn chilpa_stop_log_capture();
}
pub struct LogCapture(bool);
/// I keep the reader alive until normal application teardown drains stderr.
pub fn start_logging() -> LogCapture {
    chilpa_api::developer::logs::initialize();
    // SAFETY: the native implementation serializes installation and owns its descriptors.
    let active = unsafe { chilpa_start_log_capture() };
    if !active { chilpa_api::developer::logs::append("warning: stderr capture could not start"); }
    LogCapture(active)
}
impl Drop for LogCapture {
    fn drop(&mut self) {
        // SAFETY: I own the successfully installed reader; stop restores stderr and joins it.
        if self.0 { unsafe { chilpa_stop_log_capture(); } }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn chilpa_developer_log_line(data: *const std::ffi::c_char, length: usize) {
    if data.is_null() || length > 4096 { return; }
    // SAFETY: the native reader lends a live bounded buffer for this call.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) };
    // I never unwind into the native reader or log a recursive failure.
    let _ = std::panic::catch_unwind(|| chilpa_api::developer::logs::append(&String::from_utf8_lossy(bytes)));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_preserves_stderr_and_bounds_shutdown_with_an_inheriting_child() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "log_capture::tests::capture_fixture_child", "--nocapture"])
            .env("CHILPA_LOG_CAPTURE_FIXTURE", "capture").output().unwrap();
        assert!(output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("CHILPA_SMOKE Main ready"));
        assert!(stderr.contains("password=fixture-value"));
    }
    #[test]
    fn capture_fixture_child() {
        match std::env::var("CHILPA_LOG_CAPTURE_FIXTURE").as_deref() {
            Ok("hold") => { std::thread::sleep(std::time::Duration::from_secs(2)); return; }
            Ok("capture") => {}
            _ => return,
        }
        // SAFETY: this test runs alone in a disposable subprocess with synthetic stderr.
        assert!(unsafe { chilpa_start_log_capture() });
        let guard = LogCapture(true);
        eprintln!("CHILPA_SMOKE Main ready");
        eprintln!("password=fixture-value");
        eprintln!("{}", "x".repeat(4500));
        { use std::io::Write; std::io::stderr().write_all(b"info: invalid \xff utf8\n").unwrap(); }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "log_capture::tests::capture_fixture_child", "--nocapture"])
            .env("CHILPA_LOG_CAPTURE_FIXTURE", "hold")
            .stdout(std::process::Stdio::null()).spawn().unwrap();
        let start = std::time::Instant::now();
        drop(guard);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        let text = chilpa_api::developer::logs::export();
        assert!(text.contains("CHILPA_SMOKE Main ready"));
        assert!(!text.contains("fixture-value"));
        assert!(text.contains("[log entry exceeded the line limit]"));
        assert!(text.contains('\u{fffd}'));
        let _ = child.kill(); let _ = child.wait();
    }
}
