//! A plugin's sign-in page in a window of its own, stopped at the return
//! address the plugin declares, for providers whose browser sign-in ends on
//! an address only their own application receives.
//!
//! The window is the `chilpa-web-sign-in` executable next to this one. It
//! exits when the address is reached or the window is closed, so nothing of
//! the web engine stays in memory. Without it, or when it says it has no web
//! engine, the link opens the browser and the plugin asks for the address by
//! hand, as before.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EXECUTABLE: &str = if cfg!(windows) { "chilpa-web-sign-in.exe" } else { "chilpa-web-sign-in" };
/// Longer than any sign-in a person finishes; the window is closed after it.
const LONGEST: Duration = Duration::from_secs(30 * 60);
/// The window's website data, under Chilpa's data and cache folders.
const STORE: &str = "sign-in-window";
/// Where the earlier engine kept it, in a format the current one does not read.
const PREVIOUS_STORE: &str = "web-sign-in";

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
    let (data, cache) = (chilpa_dirs::data(), chilpa_dirs::cache());
    forget_previous_store(&[&data, &cache]);
    // The window keeps its website data between sign-ins, as a browser does, so a
    // bot check the person passed once is not asked again.
    open(executable, capture, title, (&data.join(STORE), &cache.join(STORE)), limit, cancel)
}

/// Removes the website data the earlier engine left; nothing else reads it.
fn forget_previous_store(roots: &[&Path]) {
    for root in roots {
        let _ = std::fs::remove_dir_all(root.join(PREVIOUS_STORE));
    }
}

fn open(executable: &Path, capture: &Capture, title: &str, (data, cache): (&Path, &Path), limit: Duration, cancel: impl Fn() -> bool) -> Outcome {
    let mut command = Command::new(executable);
    command.args(["--url", &capture.url, "--until", &capture.until, "--title", title])
        .arg("--data-dir").arg(data).arg("--cache-dir").arg(cache)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(agent) = &capture.user_agent {
        command.args(["--user-agent", agent]);
    }
    // The window leads a process group of its own, and the engine's processes
    // stay in it, so ending the group ends all of them.
    #[cfg(target_os = "linux")]
    { use std::os::unix::process::CommandExt; command.process_group(0); }
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
                end_group(&child);
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Closed;
            }
            Err(error) => {
                end_group(&child);
                return Outcome::Failed(error.to_string());
            }
        }
    };
    // A window that crashed or was killed leaves its engine's processes behind
    // until they notice; after a clean exit the group is already empty.
    end_group(&child);
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
        Some(3) => Outcome::Failed("no web engine".into()),
        _ => Outcome::Failed(format!("the sign-in window ended with {status}")),
    }
}

/// Kills what is left of the window's process group.
fn end_group(child: &std::process::Child) {
    #[cfg(target_os = "linux")]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // SAFETY: a signal to the group the window leads (`process_group(0)` in `open`).
        unsafe { libc::kill(-group, libc::SIGKILL); }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = child;
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
    fn open_in(folder: &Path, window: &Path, capture: &Capture, limit: Duration, cancel: impl Fn() -> bool) -> Outcome {
        open(window, capture, "Source", (&folder.join("data"), &folder.join("cache")), limit, cancel)
    }

    #[cfg(unix)]
    #[test]
    fn the_reached_address_comes_back_and_a_closed_window_says_so() {
        let folder = tempfile::tempdir().unwrap();
        let found = Capture::from_link(&json!("https://login.example.test/a"), &capture()).unwrap();
        let run = |window: &Path| open_in(folder.path(), window, &found, LONGEST, || false);
        let reached = script(folder.path(), "echo 'https://example.test/return?code=7'");
        assert_eq!(run(&reached), Outcome::Reached("https://example.test/return?code=7".into()));
        let closed = script(folder.path(), "exit 1");
        assert_eq!(run(&closed), Outcome::Closed);
        let elsewhere = script(folder.path(), "echo 'https://elsewhere.test/'");
        assert!(matches!(run(&elsewhere), Outcome::Failed(_)));
        let unavailable = script(folder.path(), "exit 3");
        assert!(matches!(run(&unavailable), Outcome::Failed(_)));
    }

    #[cfg(unix)]
    #[test]
    fn the_window_is_told_its_store_and_the_earlier_store_is_removed() {
        let folder = tempfile::tempdir().unwrap();
        let found = Capture::from_link(&json!("https://login.example.test/a"), &capture()).unwrap();
        let told = folder.path().join("arguments");
        let window = script(folder.path(), &format!("printf '%s\\n' \"$@\" > '{}'; exit 1", told.display()));
        assert_eq!(open_in(folder.path(), &window, &found, LONGEST, || false), Outcome::Closed);
        let arguments = std::fs::read_to_string(&told).unwrap();
        let arguments: Vec<&str> = arguments.lines().collect();
        let after = |name: &str| arguments.iter().position(|argument| *argument == name).map(|at| arguments[at + 1]);
        assert_eq!(after("--data-dir"), Some(folder.path().join("data").to_str().unwrap()));
        assert_eq!(after("--cache-dir"), Some(folder.path().join("cache").to_str().unwrap()));
        assert_eq!(after("--user-agent"), Some("Agent/1"));
        assert_eq!(after("--until"), Some("https://example.test/return"));

        let root = folder.path().join("root");
        for store in [PREVIOUS_STORE, STORE] {
            std::fs::create_dir_all(root.join(store).join("Cookies")).unwrap();
        }
        forget_previous_store(&[&root, &folder.path().join("absent")]);
        assert!(!root.join(PREVIOUS_STORE).exists());
        assert!(root.join(STORE).join("Cookies").is_dir());
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
        assert_eq!(open_in(folder.path(),&window,&capture,Duration::from_secs(1),||cancel.load(std::sync::atomic::Ordering::Acquire)),Outcome::Closed);
        trigger.join().unwrap();assert!(began.elapsed()<Duration::from_secs(2));
    }

    /// Whether a process ends within a few seconds of its group being killed.
    #[cfg(target_os = "linux")]
    fn ends_soon(pid: libc::pid_t) -> bool {
        let began = Instant::now();
        // SAFETY: signal 0 only asks whether the process exists.
        while unsafe { libc::kill(pid, 0) } == 0 {
            if began.elapsed() > Duration::from_secs(3) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    /// The window starts a helper of its own, as the engine does, and records it.
    #[cfg(target_os = "linux")]
    fn with_helper(folder: &Path, then: &str) -> (PathBuf, PathBuf) {
        let recorded = folder.join("helper");
        (script(folder, &format!("sleep 30 & echo $! > '{}'; {then}", recorded.display())), recorded)
    }

    #[cfg(target_os = "linux")]
    fn helper(recorded: &Path) -> libc::pid_t {
        let began = Instant::now();
        loop {
            if let Some(pid) = std::fs::read_to_string(recorded).ok().and_then(|text| text.trim().parse().ok()) {
                return pid;
            }
            assert!(began.elapsed() < Duration::from_secs(3), "the window never started its helper");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_engine_processes_end_with_a_cancelled_or_crashed_window() {
        let folder = tempfile::tempdir().unwrap();
        let found = Capture::from_link(&json!("https://login.example.test/a"), &capture()).unwrap();
        let (window, recorded) = with_helper(folder.path(), "exec sleep 30");
        let began = Instant::now();
        let cancel = || recorded.exists() && began.elapsed() > Duration::from_millis(100);
        assert_eq!(open_in(folder.path(), &window, &found, Duration::from_secs(10), cancel), Outcome::Closed);
        assert!(ends_soon(helper(&recorded)), "a cancel leaves the helper running");

        let crashed = tempfile::tempdir().unwrap();
        let (window, recorded) = with_helper(crashed.path(), "kill -KILL $$");
        assert!(matches!(open_in(crashed.path(), &window, &found, Duration::from_secs(10), || false), Outcome::Failed(_)));
        assert!(ends_soon(helper(&recorded)), "a crashed window leaves the helper running");
    }
}
