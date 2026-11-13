use super::{Error, Request, Response, MAX_OUTPUT};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

pub const TIMEOUT: Duration = Duration::from_secs(90);
const HELPER: &str = include_str!("bridge.py");

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

/// `workspace` is a caller-owned, disposable directory under the app's cache.
/// The executable is a Python environment containing the supported beets version.
/// Arguments are fixed; no shell, personal plugins or file paths are accepted.
pub fn lookup(program: &Path, workspace: &Path, request: &Request, cancelled: &AtomicBool) -> Result<Response, Error> {
    exchange(program, workspace, request, cancelled, HELPER, TIMEOUT)
}

fn exchange(program: &Path, workspace: &Path, request: &Request, cancelled: &AtomicBool, helper: &str, timeout: Duration) -> Result<Response, Error> {
    let input = request.encode()?;
    if cancelled.load(Ordering::Acquire) { return Err(Error::Cancelled); }
    let child = Command::new(program).args(["-I", "-c", helper]).current_dir(workspace)
        .env("BEETSDIR", workspace).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|error| if error.kind() == std::io::ErrorKind::NotFound { Error::Unavailable } else { Error::LookupFailed })?;
    let mut child = OwnedChild(child);
    let mut stdout = child.0.stdout.take().ok_or(Error::LookupFailed)?;
    let mut stdin = child.0.stdin.take().ok_or(Error::LookupFailed)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let too_large = overflow.clone();
    let reader = std::thread::Builder::new().name("chilpa-beets-output".into()).spawn(move || {
        let mut output = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stdout.read(&mut chunk)?;
            if read == 0 { break; }
            let keep = read.min(MAX_OUTPUT.saturating_sub(output.len()));
            output.extend_from_slice(&chunk[..keep]);
            if keep < read { too_large.store(true, Ordering::Release); }
        }
        Ok::<_, std::io::Error>(output)
    }).map_err(|_| Error::LookupFailed)?;
    let writer = match std::thread::Builder::new().name("chilpa-beets-input".into()).spawn(move || {
        stdin.write_all(&input)?;
        stdin.write_all(b"\n")?;
        // Retain the owner end of the pipe in the thread result until the
        // child exits. The helper observes EOF if the application dies.
        Ok::<_,std::io::Error>(stdin)
    }) {
        Ok(writer) => writer,
        Err(_) => {
            let _ = child.0.kill(); let _ = child.0.wait(); let _ = reader.join();
            return Err(Error::LookupFailed);
        }
    };
    let start = Instant::now();
    let result = loop {
        if cancelled.load(Ordering::Acquire) { break Err(Error::Cancelled); }
        if overflow.load(Ordering::Acquire) { break Err(Error::OutputLimit); }
        if start.elapsed() >= timeout { break Err(Error::Timeout); }
        match child.0.try_wait() {
            Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(Error::LookupFailed) },
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break Err(Error::LookupFailed),
        }
    };
    if result.is_err() { let _ = child.0.kill(); }
    let _ = child.0.wait();
    let written = writer.join();
    let output = reader.join();
    result?;
    if overflow.load(Ordering::Acquire) { return Err(Error::OutputLimit); }
    if !matches!(written, Ok(Ok(_))) { return Err(Error::LookupFailed); }
    let output = output.map_err(|_| Error::LookupFailed)?.map_err(|_| Error::LookupFailed)?;
    if cancelled.load(Ordering::Acquire) { return Err(Error::Cancelled); }
    let response = Response::decode(&output, request)?;
    if cancelled.load(Ordering::Acquire) { return Err(Error::Cancelled); }
    Ok(response)
}

#[cfg(test)]
mod tests;
