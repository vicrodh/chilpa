//! Lifecycle of an explicitly approved, unsandboxed desktop companion.
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Read}, path::Path, process::{Child, ChildStdin, Command, Stdio}, sync::mpsc, time::{Duration, Instant}};

/// Closing the private input channel asks the helper to release its children.
pub struct Companion {
    child: Option<Child>,
    input: Option<ChildStdin>,
}

impl Drop for Companion {
    fn drop(&mut self) {
        self.input.take();
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                let until = Instant::now() + Duration::from_secs(12);
                while Instant::now() < until {
                    if child.try_wait().ok().flatten().is_some() { return; }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

fn program(folder: &Path, entry: &str) -> Option<std::path::PathBuf> {
    if entry.is_empty() || !Path::new(entry).components().all(|part| matches!(part, std::path::Component::Normal(_))) { return None; }
    let folder = folder.canonicalize().ok()?;
    let executable = folder.join(entry).canonicalize().ok()?;
    (executable.starts_with(folder) && executable.is_file()).then_some(executable)
}

/// One bounded handshake on stdout. No provider credentials are logged or shown.
/// The remaining stdout is closed; helpers use stderr for non-sensitive diagnostics.
pub fn start(folder: &Path, entry: &str, data: &Path, cache: &Path) -> Result<(Companion, Value), Value> {
    let failure = || json!({"error":"desktop_start"});
    if chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak && !host_access() {
        return Err(json!({"error":"desktop_permission",
            "command":"flatpak override --user --talk-name=org.freedesktop.Flatpak app.chilpa.Chilpa"}));
    }
    if chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Snap { return Err(json!({"error":"desktop_unsupported_sandbox"})); }
    let executable = program(folder, entry).ok_or_else(failure)?;
    let mut command = if chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak {
        let mut command = Command::new("flatpak-spawn");
        command.args(["--host", "--watch-bus"]).arg(&executable);
        command
    } else { Command::new(&executable) };
    command.arg("--managed").arg("--data").arg(data).arg("--cache").arg(cache).args(["--port", "0"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| failure())?;
    let stdout = child.stdout.take().ok_or_else(failure)?;
    let input = child.stdin.take();
    let mut owned = Companion { child: Some(child), input };
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).take(8193).read_line(&mut line)
            .ok().filter(|size| *size <= 8192 && line.ends_with('\n'))
            .and_then(|_| serde_json::from_str::<Value>(&line).ok());
        let _ = sender.send(result);
    });
    let reply = receiver.recv_timeout(Duration::from_secs(8)).ok().flatten();
    if let Some(reply) = reply.filter(valid_reply) { return Ok((owned, reply)); }
    // Startup never ran user work: reap immediately rather than retaining a reader.
    if let Some(mut child) = owned.child.take() { let _ = child.kill(); let _ = child.wait(); }
    Err(failure())
}

fn host_access() -> bool {
    let Ok(mut child) = Command::new("flatpak-spawn").args(["--host", "--watch-bus", "true"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false; };
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        if let Ok(Some(status)) = child.try_wait() { return status.success(); }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

fn valid_reply(reply: &Value) -> bool {
    reply["protocol"] == 1 && reply["port"].as_u64().is_some_and(|p| (1025..=65535).contains(&p))
        && reply["key"].as_str().is_some_and(|key| (32..=128).contains(&key.len())
            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_helper_cannot_escape_its_package() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("helper"), b"x").unwrap();
        assert!(program(root.path(), "helper").is_some());
        assert!(program(root.path(), "../helper").is_none());
        assert!(program(root.path(), "/bin/sh").is_none());
        std::os::unix::fs::symlink("/bin/sh", root.path().join("escape")).unwrap();
        assert!(program(root.path(), "escape").is_none());
    }
    #[test]
    fn handshake_accepts_only_a_bounded_loopback_connection() {
        let mut reply = json!({"protocol":1,"port":12345,"key":"x".repeat(43)});
        assert!(valid_reply(&reply));
        reply["port"] = json!(80);
        assert!(!valid_reply(&reply));
        reply["port"] = json!(12345);
        reply["key"] = json!("not a key");
        assert!(!valid_reply(&reply));
    }
    #[test]
    fn closing_the_owner_releases_the_helper_input() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("helper");
        std::fs::write(&helper, "#!/bin/sh\nprintf '%s\\n' '{\"protocol\":1,\"port\":12345,\"key\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}'\ncat >/dev/null\ntouch \"$3/closed\"\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (owner, _) = start(root.path(), "helper", root.path(), root.path()).unwrap();
        assert!(!root.path().join("closed").exists());
        drop(owner);
        let until = Instant::now() + Duration::from_secs(3);
        while !root.path().join("closed").exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(root.path().join("closed").exists());
    }
}
