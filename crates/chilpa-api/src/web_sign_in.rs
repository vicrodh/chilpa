//! A plugin's sign-in page in a window of its own, stopped at the return
//! address the plugin declares, for providers whose browser sign-in ends on
//! an address only their own application receives.
//!
//! The window is the `chilpa-web-sign-in` executable next to this one. It
//! exits when the address is reached or the window is closed, so nothing of
//! the web engine stays in memory. Without it, the link opens the browser and
//! the plugin asks for the address by hand, as before.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EXECUTABLE: &str = if cfg!(windows) { "chilpa-web-sign-in.exe" } else { "chilpa-web-sign-in" };
/// Longer than any sign-in a person finishes; the window is closed after it.
const LONGEST: Duration = Duration::from_secs(30 * 60);

/// What a plugin's link asks for: the page, the address to stop at, how to
/// hand that address back, and the user agent the page expects.
#[derive(Debug, PartialEq)]
pub(crate) struct Capture {
    pub url: String,
    pub until: String,
    pub action: String,
    pub field: String,
    pub user_agent: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Reached(String),
    Closed,
    Failed(String),
}

fn secure(value: &str) -> bool {
    value.len() <= 8192 && url::Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

fn key(value: &Value) -> Option<String> {
    value.as_str().filter(|key| !key.is_empty() && key.len() <= 64 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.'))
        .map(str::to_string)
}

impl Capture {
    /// The capture a link declares, or nothing when it is malformed.
    pub(crate) fn from_link(url: &Value, capture: &Value) -> Option<Capture> {
        let url = url.as_str().filter(|url| secure(url))?.to_string();
        let until = capture["until"].as_str().filter(|until| secure(until))?.to_string();
        let user_agent = capture["user_agent"].as_str().filter(|agent| !agent.is_empty() && agent.len() <= 512 && !agent.contains(['\r', '\n']))
            .map(str::to_string);
        Some(Capture { url, until, action: key(&capture["action"])?, field: key(&capture["field"])?, user_agent })
    }
}

/// The window's executable, when this installation has one.
pub(crate) fn executable() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.with_file_name(EXECUTABLE);
    path.is_file().then_some(path)
}

/// Opens the window and waits until the address is reached or it is closed.
pub(crate) fn run(executable: &Path, capture: &Capture, title: &str) -> Outcome {
    run_controlled(executable,capture,title,LONGEST,||false)
}

pub(crate) fn run_controlled(executable: &Path, capture: &Capture, title: &str, limit:Duration,cancel:impl Fn()->bool) -> Outcome {
    if cancel() {return Outcome::Closed;}
    let mut command = Command::new(executable);
    // The window keeps its website data between sign-ins, as a browser does, so a
    // bot check the person passed once is not asked again.
    let (data, cache) = (chilpa_dirs::data().join("web-sign-in"), chilpa_dirs::cache().join("web-sign-in"));
    command.args(["--url", &capture.url, "--until", &capture.until, "--title", title])
        .arg("--data-dir").arg(&data).arg("--cache-dir").arg(&cache)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(agent) = &capture.user_agent {
        command.args(["--user-agent", agent]);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if !cancel() && started.elapsed() < limit => std::thread::sleep(Duration::from_millis(200)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Closed;
            }
            Err(error) => return Outcome::Failed(error.to_string()),
        }
    };
    let mut printed = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.by_ref().take(16 * 1024).read_to_string(&mut printed);
    }
    match status.code() {
        Some(0) => {
            let address = printed.trim();
            if address.starts_with(&capture.until) { Outcome::Reached(address.to_string()) } else { Outcome::Failed("unexpected address".into()) }
        }
        Some(1) => Outcome::Closed,
        Some(3) => Outcome::Failed("no web engine in this build".into()),
        _ => Outcome::Failed(format!("the sign-in window ended with {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn capture() -> Value {
        json!({"until": "https://example.test/return", "action": "finish", "field": "redirect_url", "user_agent": "Agent/1"})
    }

    #[test]
    fn a_link_declares_secure_addresses_and_plain_keys() {
        let found = Capture::from_link(&json!("https://login.example.test/a?x=1"), &capture()).unwrap();
        assert_eq!(found.until, "https://example.test/return");
        assert_eq!((found.action.as_str(), found.field.as_str()), ("finish", "redirect_url"));
        assert_eq!(found.user_agent.as_deref(), Some("Agent/1"));
        assert!(Capture::from_link(&json!("http://login.example.test/a"), &capture()).is_none());
        let mut other = capture();
        other["action"] = json!("finish now");
        assert!(Capture::from_link(&json!("https://login.example.test/a"), &other).is_none());
        other = capture();
        other["until"] = json!("javascript:alert(1)");
        assert!(Capture::from_link(&json!("https://login.example.test/a"), &other).is_none());
    }

    #[cfg(unix)]
    fn script(folder: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = folder.join("window");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn the_reached_address_comes_back_and_a_closed_window_says_so() {
        let folder = tempfile::tempdir().unwrap();
        let found = Capture::from_link(&json!("https://login.example.test/a"), &capture()).unwrap();
        let reached = script(folder.path(), "echo 'https://example.test/return?code=7'");
        assert_eq!(run(&reached, &found, "Source"), Outcome::Reached("https://example.test/return?code=7".into()));
        let closed = script(folder.path(), "exit 1");
        assert_eq!(run(&closed, &found, "Source"), Outcome::Closed);
        let elsewhere = script(folder.path(), "echo 'https://elsewhere.test/'");
        assert!(matches!(run(&elsewhere, &found, "Source"), Outcome::Failed(_)));
        let unavailable = script(folder.path(), "exit 3");
        assert!(matches!(run(&unavailable, &found, "Source"), Outcome::Failed(_)));
    }
    #[cfg(unix)]
    #[test]
    fn a_cancelled_sign_in_does_not_start_and_an_open_child_is_reaped() {
        let folder=tempfile::tempdir().unwrap();
        let capture=Capture::from_link(&json!("https://login.example.test/a"),&capture()).unwrap();
        assert_eq!(run_controlled(&folder.path().join("missing"),&capture,"Source",Duration::from_secs(1),||true),Outcome::Closed);
        let window=script(folder.path(),"exec sleep 30");
        let cancel=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal=cancel.clone();
        let trigger=std::thread::spawn(move||{std::thread::sleep(Duration::from_millis(30));signal.store(true,std::sync::atomic::Ordering::Release);});
        let began=Instant::now();
        assert_eq!(run_controlled(&window,&capture,"Source",Duration::from_secs(1),||cancel.load(std::sync::atomic::Ordering::Acquire)),Outcome::Closed);
        trigger.join().unwrap();assert!(began.elapsed()<Duration::from_secs(2));
    }
}
