//! The installer is a fixed command, with bounded output and an owned child.
use super::{Failure, Shared};
use std::{ffi::OsString, io::Read, path::Path, process::{Command, Stdio}, sync::{Arc, atomic::{AtomicBool,Ordering}}, time::{Duration,Instant}};
const OUTPUT_LIMIT: usize = 64 * 1024;
struct Guard(Arc<Shared>);
impl Drop for Guard { fn drop(&mut self) { self.0.kill(); } }
fn drain(mut pipe: impl Read, overflow: &AtomicBool) {
    let (mut bytes,mut total) = ([0u8;8192],0usize);
    while let Ok(count) = pipe.read(&mut bytes) {
        if count == 0 { break; }
        total += count;
        if total > OUTPUT_LIMIT { overflow.store(true,Ordering::Release); break; }
    }
}
pub(super) fn run(shared: &Arc<Shared>, program: &Path, arguments: &[OsString], generation: &Path, cache: &Path, deadline: Instant) -> Result<(),Failure> {
    shared.check(true)?;
    if Instant::now() >= deadline { return Err(Failure::Timeout); }
    let mut command = Command::new(program);
    command.args(arguments).current_dir(cache).env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Retain OS identity and transport variables, without inheriting Python,
    // pip or uv configuration. No shell or user build hooks are involved.
    command.envs(chilpa_dirs::installer_environment(cache));
    command.env("UV_CACHE_DIR",cache.join("uv"))
        .env("UV_PYTHON_INSTALL_DIR",generation.join("python"))
        .env("UV_PYTHON_BIN_DIR",generation.join("python-bin"))
        .env("UV_PYTHON_INSTALL_REGISTRY","false")
        .env("UV_CONCURRENT_DOWNLOADS","2").env("UV_CONCURRENT_INSTALLS","1").env("UV_CONCURRENT_BUILDS","1")
        .env("UV_HTTP_TIMEOUT","20").env("UV_HTTP_RETRIES","1");
    let child = command.spawn().map_err(|_| Failure::Install)?;
    *shared.child.lock().unwrap_or_else(|error| error.into_inner()) = Some(child);
    let _guard = Guard(shared.clone());
    let (output,errors) = {
        let mut slot = shared.child.lock().unwrap_or_else(|error| error.into_inner());
        let child = slot.as_mut().ok_or(Failure::Cancelled)?;
        (child.stdout.take().ok_or(Failure::Install)?, child.stderr.take().ok_or(Failure::Install)?)
    };
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = overflow.clone();
    let stdout = std::thread::Builder::new().name("beets-install-output".into()).spawn(move || drain(output,&flag)).map_err(|_| Failure::Install)?;
    let flag = overflow.clone();
    let stderr = match std::thread::Builder::new().name("beets-install-errors".into()).spawn(move || drain(errors,&flag)) {
        Ok(reader) => reader,
        Err(_) => { shared.kill(); let _ = stdout.join(); return Err(Failure::Install); }
    };
    let result = loop {
        if let Err(error) = shared.check(true) { break Err(error); }
        if Instant::now() >= deadline { break Err(Failure::Timeout); }
        if overflow.load(Ordering::Acquire) { break Err(Failure::OutputLimit); }
        let status = shared.child.lock().unwrap_or_else(|error| error.into_inner()).as_mut().ok_or(Failure::Cancelled)
            .and_then(|child| child.try_wait().map_err(|_| Failure::Install));
        match status {
            Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(Failure::Install) },
            Ok(None) => (),
            Err(error) => break Err(error),
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    shared.kill();
    let _ = stdout.join(); let _ = stderr.join();
    if overflow.load(Ordering::Acquire) { Err(Failure::OutputLimit) } else { result }
}

#[cfg(test)]
mod tests;
