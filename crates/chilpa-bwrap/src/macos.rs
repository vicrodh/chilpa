//! Seatbelt confines each worker to its own package, state and cache.
use crate::NativePaths;
use chilpa_plugin::{bridge::BridgeError, native::NativeClient};
use std::{ffi::OsString, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::{Arc, Mutex}};
use std::os::unix::process::CommandExt;

const POLICY: &str = include_str!("macos.sb");

fn directory(path: &Path) -> Result<PathBuf, BridgeError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| BridgeError::Disconnected)?;
    if !metadata.file_type().is_dir() { return Err(BridgeError::Denied); }
    let path = path.canonicalize().map_err(|_| BridgeError::Disconnected)?;
    path.is_dir().then_some(path).ok_or(BridgeError::Denied)
}

fn parameter(command: &mut Command, name: &str, path: &Path) {
    let mut argument = OsString::from(name);
    argument.push("=");
    argument.push(path);
    command.arg("-D").arg(argument);
}

fn command(paths: &NativePaths, network: bool) -> Result<Command, BridgeError> {
    if unsafe { libc::geteuid() } == 0 { return Err(BridgeError::Denied); }
    let folder = directory(&paths.folder)?;
    let program = folder.join("plugin").canonicalize().map_err(|_| BridgeError::Disconnected)?;
    if !program.is_file() || !program.starts_with(&folder) { return Err(BridgeError::Denied); }
    for path in [&paths.data, &paths.cache] { std::fs::create_dir_all(path).map_err(|_| BridgeError::Disconnected)?; }
    let data = directory(&paths.data)?;
    let cache = directory(&paths.cache)?;
    let temporary = cache.join("tmp");
    std::fs::create_dir_all(&temporary).map_err(|_| BridgeError::Disconnected)?;
    let mut policy = POLICY.to_string();
    let mut command = Command::new("/usr/bin/sandbox-exec");
    for (name, path) in [("PLUGIN", &folder), ("PROGRAM", &program), ("DATA", &data), ("CACHE", &cache)] {
        parameter(&mut command, name, path);
    }
    if let Some(export) = &paths.export {
        parameter(&mut command, "EXPORT", &directory(export)?);
        policy.push_str("\n(allow file-read* file-write* (subpath (param \"EXPORT\")))\n");
    }
    if network {
        policy.push_str("\n(allow network-outbound)\n(allow mach-lookup (global-name \"com.apple.mDNSResponder\") (global-name \"com.apple.system.opendirectoryd.libinfo\"))\n");
    }
    command.env_clear().env("PATH", "/usr/bin:/bin").env("HOME", &data)
        .env("CFFIXED_USER_HOME", &data).env("CFPREFERENCES_AVOID_DAEMON", "1")
        .env("TMPDIR", &temporary).env("XDG_CACHE_HOME", &cache)
        .env("CHILPA_PLUGIN_DATA_DIR", &data).env("CHILPA_PLUGIN_CACHE_DIR", &cache)
        .current_dir(&data).arg("-p").arg(policy).arg(program)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // The shared dyld cache reserves far more address space than resident memory.
    // I bound descriptors here and monitor the worker's physical footprint below.
    unsafe {
        command.pre_exec(|| {
            for (resource, limit) in [(libc::RLIMIT_NOFILE, 256), (libc::RLIMIT_CORE, 0)] {
                let limits = libc::rlimit { rlim_cur: limit, rlim_max: limit };
                if libc::setrlimit(resource, &limits) != 0 { return Err(std::io::Error::last_os_error()); }
            }
            if libc::setpgid(0, 0) != 0 { return Err(std::io::Error::last_os_error()); }
            Ok(())
        });
    }
    Ok(command)
}

fn end(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        // I own this process group; the sandbox also denies worker forks.
        unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL); }
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn footprint(pid: u32) -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v2>::uninit();
    let result = unsafe { libc::proc_pid_rusage(pid as _, libc::RUSAGE_INFO_V2, usage.as_mut_ptr().cast()) };
    if result != 0 { return None; }
    let usage = unsafe { usage.assume_init() };
    Some(usage.ri_resident_size.max(usage.ri_phys_footprint))
}

/// I never fall back to launching outside the sandbox when confinement fails.
pub fn launch_native(
    paths: &NativePaths,
    name: &str,
    memory_bytes: usize,
    network: bool,
    notice: impl Fn(&str, &serde_json::Value) + Send + 'static,
) -> Result<Arc<NativeClient>, BridgeError> {
    if memory_bytes < 64 * 1024 * 1024 { return Err(BridgeError::Denied); }
    let mut child = command(paths, network)?.spawn().map_err(|_| BridgeError::Disconnected)?;
    let input = child.stdin.take().ok_or(BridgeError::Disconnected)?;
    let output = child.stdout.take().ok_or(BridgeError::Disconnected)?;
    if let Some(errors) = child.stderr.take() {
        let name = name.to_string();
        let _ = std::thread::Builder::new().name("chilpa-native-err".into()).spawn(move || {
            use std::io::{BufRead, BufReader, Read};
            let mut errors = BufReader::new(errors);
            loop {
                let mut line = Vec::new();
                if errors.by_ref().take(8192).read_until(b'\n', &mut line).unwrap_or(0) == 0 { break; }
                eprintln!("plugin[{name}]: {}", String::from_utf8_lossy(&line).trim_end());
            }
        });
    }
    let child = Arc::new(Mutex::new(child));
    let weak = Arc::downgrade(&child);
    let monitor = std::thread::Builder::new().name("chilpa-native-budget".into()).spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(125));
        let Some(child) = weak.upgrade() else { break };
        let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
        if child.try_wait().ok().flatten().is_some() { break; }
        if footprint(child.id()).is_none_or(|bytes| bytes > memory_bytes as u64) { end(&mut child); break; }
    });
    if monitor.is_err() { end(&mut child.lock().unwrap_or_else(|e| e.into_inner())); return Err(BridgeError::Disconnected); }
    Ok(NativeClient::start(input, output, move || end(&mut child.lock().unwrap_or_else(|e| e.into_inner())), notice))
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
