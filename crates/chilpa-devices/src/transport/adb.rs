//! I use an explicitly selected ADB transport and a device-side flock for one active job.
use super::{Entry, Transport};
use crate::{conflict, invalid, Cancellation, RelativePath, Result, BUFFER_BYTES};
use std::io::{BufRead, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum AdbSelector { Serial(String), Transport(u64) }
impl AdbSelector {
    fn command(&self) -> Command {
        let mut command = Command::new("adb");
        match self { Self::Serial(s) => { command.arg("-s").arg(s); }, Self::Transport(id) => { command.arg("-t").arg(id.to_string()); } }
        command
    }
}

struct Lease { child: Child, input: Option<ChildStdin> }
impl Drop for Lease {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Adb { selector: AdbSelector, root: String, _lease: Lease }

fn quoted(value: &str) -> String { format!("'{}'", value.replace('\'', "'\\''")) }

/// I bound metadata responses while draining stderr fully so it cannot block a child.
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 64 * 1024 { return Err(std::io::Error::other("ADB response exceeds 64 KiB")); }
        self.0.extend_from_slice(bytes); Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn pipe(selector: &AdbSelector, script: &str, input: Option<&mut dyn Read>, output: &mut dyn Write,
    cancel: &Cancellation, timeout: Duration, progress: &mut dyn FnMut(u64)) -> Result<()> {
    cancel.check()?;
    let mut child = selector.command().arg("shell").arg("-T").arg(script)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let mut child_input = child.stdin.take().unwrap();
    let mut child_output = child.stdout.take().unwrap();
    let mut child_error = child.stderr.take().unwrap();
    let halt = Arc::new(AtomicBool::new(false));
    let monitor_halt = halt.clone();
    let monitor_cancel = cancel.clone();
    let monitor = std::thread::spawn(move || -> std::io::Result<(std::process::ExitStatus, bool)> {
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? { return Ok((status, false)); }
            if monitor_halt.load(Ordering::Relaxed) || monitor_cancel.check().is_err() || started.elapsed() >= timeout {
                let _ = child.kill();
                return Ok((child.wait()?, started.elapsed() >= timeout));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let errors = std::thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            match child_error.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => { let keep = n.min(32 * 1024 - retained.len()); retained.extend_from_slice(&buffer[..keep]); }
            }
        }
        retained
    });
    let io_result = (|| -> Result<()> {
        let mut buffer = vec![0; BUFFER_BYTES];
        if let Some(input) = input {
            let mut bytes = 0;
            loop {
                cancel.check()?;
                let n = input.read(&mut buffer)?;
                if n == 0 { break; }
                child_input.write_all(&buffer[..n])?;
                bytes += n as u64; progress(bytes);
            }
        }
        drop(child_input);
        loop {
            cancel.check()?;
            let n = child_output.read(&mut buffer)?;
            if n == 0 { break; }
            output.write_all(&buffer[..n])?;
        }
        Ok(())
    })();
    if io_result.is_err() { halt.store(true, Ordering::Relaxed); }
    let monitored = monitor.join().map_err(|_| invalid("ADB monitor stopped"))??;
    let errors = errors.join().map_err(|_| invalid("ADB error reader stopped"))?;
    cancel.check()?;
    io_result?;
    if monitored.1 { return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "ADB operation timed out").into()); }
    if !monitored.0.success() { return Err(conflict(format!("ADB command failed: {}", String::from_utf8_lossy(&errors).trim()))); }
    Ok(())
}

fn text(selector: &AdbSelector, script: &str) -> Result<String> {
    let mut output = Limited(Vec::new());
    pipe(selector, script, None, &mut output, &Cancellation::default(), Duration::from_secs(20), &mut |_| ())?;
    String::from_utf8(output.0).map_err(|_| invalid("ADB returned invalid UTF-8 metadata"))
}

impl Adb {
    pub fn open(selector: AdbSelector, root: &str) -> Result<Self> {
        if !root.starts_with("/storage/") && !root.starts_with("/sdcard/") { return Err(invalid("ADB sync roots must be in shared storage")); }
        if root.len() > 2048 || root.chars().any(char::is_control) { return Err(invalid("invalid ADB sync root")); }
        let root = text(&selector, &format!("test -d {0} && readlink -f {0}", quoted(root)))?.trim().to_owned();
        if !root.starts_with("/storage/") { return Err(invalid("ADB sync root resolves outside shared storage")); }
        let metadata = format!("{root}/.chilpa-sync");
        // Android's shared-storage FUSE returns ENOSYS for flock. I keep the kernel lease
        // on shell-private storage, keyed by the canonical root, rather than claiming a lock there.
        let lock = format!("/data/local/tmp/chilpa-sync-lock-{}", crate::hash_text(&root));
        // mksh marks its extra descriptors close-on-exec; the explicit duplication admits fd 9
        // to flock while the parent shell retains the same open file description.
        let setup = format!("test ! -L {0} && mkdir -p {0} && test ! -L {1} && exec 9>>{1} && flock -n 9 9>&9 && printf 'CHILPA_READY\\n' && cat >/dev/null 9>&9", quoted(&metadata), quoted(&lock));
        let mut child = selector.command().arg("shell").arg("-T").arg(setup)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let handshake = std::thread::spawn(move || {
            let mut line = String::new();
            let result = std::io::BufReader::new(stdout).take(64).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let lease = Lease { child, input };
        let ready = receiver.recv_timeout(Duration::from_secs(20));
        if !matches!(ready, Ok(Ok(ref s)) if s.trim() == "CHILPA_READY") {
            drop(lease); let _ = handshake.join();
            return Err(conflict("ADB storage is busy or its sync lock could not be acquired"));
        }
        let _ = handshake.join();
        Ok(Self { selector, root, _lease:lease })
    }
    fn path(&self, path: &RelativePath) -> String { format!("{}/{}", self.root, path.as_str()) }
    fn guarded(&self, path: &RelativePath) -> String {
        let mut prefix = self.root.clone();
        let mut guard = format!("test ! -L {} && ", quoted(&prefix));
        for part in path.as_str().split('/') {
            prefix.push('/'); prefix.push_str(part);
            guard.push_str(&format!("test ! -L {} && ", quoted(&prefix)));
        }
        guard
    }
    fn transfer_timeout(size: u64) -> Duration { Duration::from_secs((30 + size / (256 * 1024)).min(7200)) }
    fn alive(&mut self) -> Result<()> {
        if self._lease.child.try_wait()?.is_some() { return Err(conflict("ADB sync lease disconnected; reconnect before continuing")); }
        Ok(())
    }
}

impl Transport for Adb {
    fn available_bytes(&mut self) -> Result<u64> {
        self.alive()?;
        let output = text(&self.selector, &format!("df -k {}", quoted(&self.root)))?;
        let line = output.lines().last().ok_or_else(|| invalid("ADB returned no capacity"))?;
        let fields: Vec<_> = line.split_whitespace().collect();
        let available = fields.get(3).and_then(|s| s.parse::<u64>().ok()).ok_or_else(|| invalid("ADB returned invalid capacity"))?;
        available.checked_mul(1024).ok_or_else(|| invalid("ADB capacity overflow"))
    }
    fn stat(&mut self, path: &RelativePath) -> Result<Option<Entry>> {
        self.alive()?;
        let script = format!("{guard}if test -e {path}; then stat -c '%F|%s' {path}; else printf 'MISSING\\n'; fi", path=quoted(&self.path(path)), guard=self.guarded(path));
        let output = text(&self.selector, &script)?;
        let output = output.trim();
        if output == "MISSING" { return Ok(None); }
        let (kind, size) = output.split_once('|').ok_or_else(|| invalid("ADB returned invalid file metadata"))?;
        if kind != "regular file" && kind != "directory" && kind != "regular empty file" { return Err(conflict("ADB path is not a regular file or directory")); }
        Ok(Some(Entry { directory:kind == "directory", size:size.parse().map_err(|_| invalid("ADB returned invalid file size"))? }))
    }
    fn create_dir_all(&mut self, path: &RelativePath) -> Result<()> {
        self.alive()?;
        text(&self.selector, &format!("{}mkdir -p {}", self.guarded(path), quoted(&self.path(path))))?;
        Ok(())
    }
    fn read(&mut self, path: &RelativePath, out: &mut dyn Write, cancel: &Cancellation) -> Result<()> {
        let size = self.stat(path)?.ok_or_else(|| conflict("ADB file disappeared"))?.size;
        let script = format!("{}cat {}", self.guarded(path), quoted(&self.path(path)));
        pipe(&self.selector, &script, None, out, cancel, Self::transfer_timeout(size), &mut |_| ())
    }
    fn write_new(&mut self, path: &RelativePath, input: &mut (dyn Read + Send), size: u64,
        cancel: &Cancellation, progress: &mut (dyn FnMut(u64) + Send)) -> Result<()> {
        self.alive()?;
        let script = format!("{guard}set -C && cat > {path} && test \"$(stat -c %s {path})\" = {size} && sync", path=quoted(&self.path(path)), guard=self.guarded(path));
        pipe(&self.selector, &script, Some(input), &mut Limited(Vec::new()), cancel, Self::transfer_timeout(size), progress)
    }
    fn rename_new(&mut self, from: &RelativePath, to: &RelativePath) -> Result<()> {
        self.alive()?;
        let script = format!("{guard_from}{guard_to}test ! -e {to} && mv -n {from} {to} && test ! -e {from} && sync", from=quoted(&self.path(from)), to=quoted(&self.path(to)), guard_from=self.guarded(from), guard_to=self.guarded(to));
        text(&self.selector, &script)?; Ok(())
    }
    fn remove_file(&mut self, path: &RelativePath) -> Result<()> {
        self.alive()?;
        text(&self.selector, &format!("{}rm {} && sync", self.guarded(path), quoted(&self.path(path))))?;
        Ok(())
    }
    fn published(&mut self, path: &RelativePath, cancel: &Cancellation) -> Result<()> {
        self.alive()?;
        let mut uri = String::from("file://");
        for byte in self.path(path).bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) { uri.push(byte as char); }
            else { uri.push_str(&format!("%{byte:02X}")); }
        }
        let script = format!("am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d {}", quoted(&uri));
        pipe(&self.selector, &script, None, &mut Limited(Vec::new()), cancel, Duration::from_secs(20), &mut |_| ())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shell_arguments_keep_quotes_and_substitutions_literal() {
        assert_eq!(super::quoted("a'b $(c)"), "'a'\\''b $(c)'");
    }
}
