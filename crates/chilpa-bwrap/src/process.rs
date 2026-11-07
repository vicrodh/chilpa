//! What every process backend shares: the launcher contract, the handle on
//! a started process and the reader of its standard error.

use crate::{NativePaths, Unavailable};
use chilpa_plugin::{bridge::BridgeError, native::NativeClient};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

/// Longest piece of a plugin's standard error logged as one line. A longer
/// line is logged in pieces, so one that never ends cannot grow its buffer.
const ERROR_PIECE: u64 = 8192;

/// What the plugin sends on its own, such as a log line or a host request.
pub type Notice = Box<dyn Fn(&str, &serde_json::Value) + Send + 'static>;

/// One way of starting process plugins on this system.
pub trait NativeLauncher: Send + Sync {
    /// Whether this backend can start a plugin here, checked without
    /// starting one.
    fn availability(&self) -> Result<(), Unavailable>;

    /// Starts the plugin confined to its folders, with the network or
    /// without it. It never starts one outside the sandbox.
    fn launch(&self, paths: &NativePaths, name: &str, memory_bytes: usize, network: bool, notice: Notice)
        -> Result<LaunchedNative, BridgeError>;
}

/// A plugin a backend started.
pub struct LaunchedNative {
    pub client: Arc<NativeClient>,
    /// Its data and cache folders as the plugin sees them, which is what
    /// `initialize` names.
    pub guest_directories: (PathBuf, PathBuf),
    /// The process behind the channel. It can end while its pipes stay open.
    pub process: Arc<dyn NativeProcess>,
}

/// The process behind a plugin's channel, apart from its pipes.
pub trait NativeProcess: Send + Sync {
    /// True until the process exits, whatever its pipes are doing.
    fn alive(&self) -> bool;
    /// Ends the process. Calling it again, or after it exited, does nothing.
    fn terminate(&self);
    /// Waits for the process to exit and reaps it. Call it after
    /// `terminate`; a second call returns at once.
    fn wait(&self);
}

/// The backend this system starts process plugins with.
pub fn launcher() -> Arc<dyn NativeLauncher> {
    #[cfg(target_os = "linux")]
    {
        if crate::linux::in_flatpak() {
            return Arc::new(crate::linux::NoBackend(Unavailable::Flatpak));
        }
        Arc::new(crate::Bwrap)
    }
    #[cfg(target_os = "macos")]
    {
        Arc::new(crate::Seatbelt)
    }
}

/// A process this host spawned itself.
pub(crate) struct ChildProcess {
    child: Mutex<Child>,
    /// The process leads its own group, and ending it ends the group.
    group: bool,
}

impl ChildProcess {
    pub(crate) fn new(child: Child, group: bool) -> Self {
        Self { child: Mutex::new(child), group }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn id(&self) -> u32 {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).id()
    }
}

impl NativeProcess for ChildProcess {
    fn alive(&self) -> bool {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).try_wait().is_ok_and(|status| status.is_none())
    }

    fn terminate(&self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        // A reaped process's id can belong to another one: only a running
        // child, still unreaped under this lock, is signalled.
        if child.try_wait().ok().flatten().is_none() {
            if self.group {
                unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL); }
            }
            let _ = child.kill();
        }
    }

    fn wait(&self) {
        let _ = self.child.lock().unwrap_or_else(|e| e.into_inner()).wait();
    }
}

/// Logs what a plugin prints on its standard error under its name.
pub(crate) fn log_errors(name: &str, errors: impl Read + Send + 'static) {
    let name = name.to_string();
    let _ = std::thread::Builder::new().name("chilpa-native-err".into()).spawn(move || {
        error_lines(errors, |line| eprintln!("plugin[{name}]: {line}"));
    });
}

fn error_lines(errors: impl Read, mut each: impl FnMut(&str)) {
    let mut errors = BufReader::new(errors);
    let mut line = Vec::new();
    loop {
        line.clear();
        if errors.by_ref().take(ERROR_PIECE).read_until(b'\n', &mut line).unwrap_or(0) == 0 { break; }
        each(String::from_utf8_lossy(&line).trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn shell(script: &str) -> Child {
        Command::new("/bin/sh").args(["-c", script])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap()
    }

    #[test]
    fn a_process_that_exits_is_seen_before_its_pipes_close() {
        // The background command keeps the pipe after the shell exits.
        let mut child = shell("sleep 3 & exit 0");
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let process = Arc::new(ChildProcess::new(child, false));
        let ended = process.clone();
        let client = NativeClient::start(input, output, move || { ended.terminate(); ended.wait(); }, |_, _| {});
        let start = Instant::now();
        while process.alive() && start.elapsed() < Duration::from_secs(2) { std::thread::sleep(Duration::from_millis(10)); }
        assert!(!process.alive(), "the shell exited");
        assert!(client.alive(), "its pipe is still open, held by what it left behind");
        client.stop();
        assert!(!client.alive());
    }

    #[test]
    fn ending_and_waiting_twice_is_harmless() {
        let process = ChildProcess::new(shell("exec sleep 30"), false);
        assert!(process.alive());
        process.terminate();
        process.terminate();
        process.wait();
        process.wait();
        assert!(!process.alive());
        process.terminate();
    }

    #[test]
    fn a_long_error_line_is_logged_in_bounded_pieces() {
        let long = "x".repeat(20_000);
        let mut pieces = Vec::new();
        error_lines(std::io::Cursor::new(format!("{long}\nnext\n")), |piece| pieces.push(piece.to_string()));
        assert!(pieces.iter().all(|piece| piece.len() as u64 <= ERROR_PIECE), "{:?}", pieces.iter().map(String::len).collect::<Vec<_>>());
        assert_eq!(pieces.last().map(String::as_str), Some("next"));
        assert_eq!(pieces[..pieces.len() - 1].concat(), long, "nothing of the long line is lost");
    }
}
