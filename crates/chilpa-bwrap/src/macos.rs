//! Seatbelt confines each worker to its own package, state and cache.
use crate::process::{log_errors, ChildProcess, LaunchedNative, NativeLauncher, NativeProcess, Notice};
use crate::{NativePaths, Unavailable};
use chilpa_plugin::{bridge::BridgeError, native::NativeClient};
use std::{ffi::OsString, path::{Path, PathBuf}, process::{Command, Stdio}, sync::Arc};
use std::os::unix::process::CommandExt;

const POLICY: &str = include_str!("macos.sb");
/// The sandbox program, part of the system.
const SANDBOX: &str = "/usr/bin/sandbox-exec";

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
    let mut command = Command::new(SANDBOX);
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

fn footprint(pid: u32) -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v2>::uninit();
    let result = unsafe { libc::proc_pid_rusage(pid as _, libc::RUSAGE_INFO_V2, usage.as_mut_ptr().cast()) };
    if result != 0 { return None; }
    let usage = unsafe { usage.assume_init() };
    Some(usage.ri_resident_size.max(usage.ri_phys_footprint))
}

/// Seatbelt from the system, with the plugin's folders at their own paths.
/// I never fall back to launching outside the sandbox when confinement fails.
pub struct Seatbelt;

impl NativeLauncher for Seatbelt {
    fn availability(&self) -> Result<(), Unavailable> {
        if unsafe { libc::geteuid() } == 0 { return Err(Unavailable::Privileged); }
        if !Path::new(SANDBOX).is_file() { return Err(Unavailable::MissingProgram(SANDBOX.into())); }
        Ok(())
    }

    fn launch(&self, paths: &NativePaths, name: &str, memory_bytes: usize, network: bool, notice: Notice)
        -> Result<LaunchedNative, BridgeError> {
        if memory_bytes < 64 * 1024 * 1024 { return Err(BridgeError::Denied); }
        let mut sandboxed = command(paths, network)?;
        // The plugin is told the same canonical folders the policy grants.
        let guest_directories = (directory(&paths.data)?, directory(&paths.cache)?);
        let mut child = sandboxed.spawn().map_err(|_| BridgeError::Disconnected)?;
        let input = child.stdin.take().ok_or(BridgeError::Disconnected)?;
        let output = child.stdout.take().ok_or(BridgeError::Disconnected)?;
        if let Some(errors) = child.stderr.take() { log_errors(name, errors); }
        // The process leads its own group (`setpgid` above), and the sandbox
        // also denies it forks.
        let process = Arc::new(ChildProcess::new(child, true));
        let weak = Arc::downgrade(&process);
        let monitor = std::thread::Builder::new().name("chilpa-native-budget".into()).spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(125));
            let Some(process) = weak.upgrade() else { break };
            if !process.alive() { break; }
            if footprint(process.id()).is_none_or(|bytes| bytes > memory_bytes as u64) {
                process.terminate();
                process.wait();
                break;
            }
        });
        if monitor.is_err() { process.terminate(); process.wait(); return Err(BridgeError::Disconnected); }
        let ended = process.clone();
        let client = NativeClient::start(input, output, move || { ended.terminate(); ended.wait(); }, notice);
        Ok(LaunchedNative { client, guest_directories, process })
    }
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
