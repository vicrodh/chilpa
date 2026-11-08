//! Bubblewrap from the host system: private namespaces for each process
//! plugin and for the compatibility worker.

use crate::flatpak::Exposed;
use crate::linux::{canonical_directory, spawn};
use crate::process::{log_errors, ChildProcess, LaunchedNative, NativeLauncher, NativeProcess, Notice, CACHE_DIR_VARIABLE, DATA_DIR_VARIABLE};
use crate::{NativePaths, Unavailable};

use chilpa_plugin::{bridge::BridgeError, native::NativeClient, pipe_transport::PipeTransport};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::{path::{Path, PathBuf}, process::{Command, Stdio}, sync::Arc};

/// The sandbox program, from the host's `/usr`.
const SANDBOX: &str = "/usr/bin/bwrap";
/// Applies the resource limits inside the sandbox, from the same `/usr`.
const LIMITS: &str = "/usr/bin/prlimit";
/// Where a native plugin finds its writable folders inside its namespace.
const GUEST_DATA: &str = "/data";
const GUEST_CACHE: &str = "/cache";
/// Where it finds an approved export folder.
const GUEST_EXPORT: &str = "/export";

/// These are host-selected runtime components, never repository arguments.
pub struct RuntimePaths {
    pub sandbox: PathBuf,
    pub system: PathBuf,
    pub libraries: PathBuf,
    pub adapter: PathBuf,
}

impl RuntimePaths {
    /// Runtime resources are installed by the application, outside package
    /// directories. I refuse an incomplete installation before launching it.
    pub fn installed(root: &Path) -> Option<Self> {
        let root = crate::linux::compat_runtime(root);
        let paths = Self { sandbox: SANDBOX.into(), system: "/usr".into(),
            libraries: root.join("libraries"), adapter: root.join("adapter") };
        (paths.sandbox.is_file() && paths.adapter.join("worker.pl").is_file()
            && paths.libraries.join("lib/perl5").is_dir()).then_some(paths)
    }
}

impl chilpa_plugin::bridge::WorkerLauncher for RuntimePaths {
    fn guest_root(&self, _: &Path) -> String { "/guest".into() }

    fn inspection_revision(&self) -> Option<String> {
        crate::linux::runtime_revision(&self.adapter, &self.libraries)
    }

    fn launch(&self, guest: &Path) -> Result<Box<dyn chilpa_plugin::bridge::WorkerTransport>, BridgeError> {
        launch(self, guest, 256 * 1024 * 1024).map(|transport| Box::new(transport) as Box<dyn chilpa_plugin::bridge::WorkerTransport>)
    }
}

pub fn launch(paths: &RuntimePaths, guest: &Path, memory_bytes: usize) -> Result<PipeTransport, BridgeError> {
    // A privileged caller could evade the process-count resource limit. I
    // refuse that configuration instead of claiming to contain its forks.
    if unsafe { libc::geteuid() } == 0 || memory_bytes < 64 * 1024 * 1024 {
        return Err(BridgeError::Denied);
    }
    let system = canonical_directory(&paths.system)?;
    let libraries = canonical_directory(&paths.libraries)?;
    let adapter = canonical_directory(&paths.adapter)?;
    let guest = canonical_directory(guest)?;
    let mut command = Command::new(&paths.sandbox);
    command.env_clear().args(["--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL"])
        .arg("--ro-bind").arg(system).arg("/usr")
        .args(["--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64",
            "--proc", "/proc", "--dev", "/dev", "--size", "16777216", "--tmpfs", "/tmp"])
        .arg("--ro-bind").arg(libraries).arg("/runtime")
        .arg("--ro-bind").arg(adapter).arg("/bridge")
        .arg("--ro-bind").arg(guest).arg("/guest")
        .args(["--remount-ro", "/dev", "--remount-ro", "/", "--chdir", "/guest", "--clearenv", "--setenv", "PATH", "/usr/bin",
            "--setenv", "PERL5LIB", "/runtime/lib/perl5", "--",
            LIMITS])
        .arg(format!("--as={memory_bytes}"))
        .args(["--nproc=0", "--nofile=64", "--fsize=1048576", "--cpu=120:120", "--core=0", "--",
            "/usr/bin/perl", "/bridge/worker.pl"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = spawn(command).map_err(|_| BridgeError::Disconnected)?;
    let input = child.stdin.take().ok_or(BridgeError::Disconnected)?;
    let output = child.stdout.take().ok_or(BridgeError::Disconnected)?;
    Ok(PipeTransport::isolated(input, output, move || {
        // The PID namespace dies with its init, and nproc prevents the guest
        // from creating siblings that retain its pipes or memory budget.
        let _ = child.kill();
        let _ = child.wait();
    }))
}

/// Starts each native plugin in its own namespaces. It sees `/usr`, the
/// certificates and the resolver read-only, its folder read-only, and only
/// its data and cache folders writable. Not the home, not the library.
/// Given the network, it reaches any address on its own: the manifest's
/// domains do not limit a process.
pub struct Bwrap;

impl NativeLauncher for Bwrap {
    fn availability(&self) -> Result<(), Unavailable> {
        if unsafe { libc::geteuid() } == 0 { return Err(Unavailable::Privileged); }
        match [SANDBOX, LIMITS].into_iter().find(|program| !Path::new(program).is_file()) {
            Some(missing) => Err(Unavailable::MissingProgram(missing.into())),
            None => Ok(()),
        }
    }

    fn launch(&self, paths: &NativePaths, name: &str, memory_bytes: usize, network: bool, notice: Notice)
        -> Result<LaunchedNative, BridgeError> {
        let export = checked_export(paths)?;
        launch_with(paths, export, name, memory_bytes, network, notice)
    }
}

/// The export folder, opened, and checked on what was opened. Bubblewrap is
/// handed this descriptor and binds the directory it holds, so a folder
/// above it that becomes a link after the check changes nothing.
fn checked_export(paths: &NativePaths) -> Result<Option<Exposed>, BridgeError> {
    let export = paths.export.as_deref().map(Exposed::open).transpose()?;
    if export.as_ref().is_some_and(|export| paths.is_private(&export.path)) { return Err(BridgeError::Denied); }
    Ok(export)
}

fn launch_with(paths: &NativePaths, export: Option<Exposed>, name: &str, memory_bytes: usize, network: bool, notice: Notice)
    -> Result<LaunchedNative, BridgeError> {
    if unsafe { libc::geteuid() } == 0 || memory_bytes < 64 * 1024 * 1024 {
        return Err(BridgeError::Denied);
    }
    let folder = canonical_directory(&paths.folder)?;
    for dir in [&paths.data, &paths.cache] {
        std::fs::create_dir_all(dir).map_err(|_| BridgeError::Disconnected)?;
    }
    let data = canonical_directory(&paths.data)?;
    let cache = canonical_directory(&paths.cache)?;
    if !folder.join("plugin").is_file() {
        return Err(BridgeError::Disconnected);
    }
    let mut command = Command::new(SANDBOX);
    command.env_clear().arg("--unshare-all");
    if network { command.arg("--share-net"); }
    command.args(["--die-with-parent", "--new-session", "--cap-drop", "ALL"])
        .args(["--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64",
            "--symlink", "usr/bin", "/bin", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
    // The loader's cache and configuration too: a native binary links
    // libraries that live outside /usr/lib on some systems (libgcc_s under
    // the compiler's folder), found only through them.
    for shared in ["/etc/ssl", "/etc/ca-certificates", "/etc/pki", "/etc/resolv.conf", "/etc/hosts", "/etc/nsswitch.conf",
        "/etc/ld.so.cache", "/etc/ld.so.conf", "/etc/ld.so.conf.d"] {
        command.args(["--ro-bind-try", shared, shared]);
    }
    command.arg("--ro-bind").arg(&folder).arg("/plugin")
        .arg("--bind").arg(&data).arg(GUEST_DATA)
        .arg("--bind").arg(&cache).arg(GUEST_CACHE);
    if let Some(export) = &export {
        let fd = export.fd.as_raw_fd();
        command.arg("--bind-fd").arg(fd.to_string()).arg(GUEST_EXPORT);
        // Inherited by bubblewrap alone: it closes the descriptor once bound.
        unsafe {
            command.pre_exec(move || if libc::fcntl(fd, libc::F_SETFD, 0) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) });
        }
    }
    command.args(["--chdir", GUEST_DATA, "--clearenv", "--setenv", "PATH", "/usr/bin", "--setenv", "HOME", GUEST_DATA,
            "--setenv", "XDG_CACHE_HOME", GUEST_CACHE, "--setenv", DATA_DIR_VARIABLE, GUEST_DATA,
            "--setenv", CACHE_DIR_VARIABLE, GUEST_CACHE, "--setenv", "SSL_CERT_DIR", "/etc/ssl/certs", "--",
            LIMITS])
        .arg(format!("--as={memory_bytes}"))
        .args(["--nofile=256", "--core=0", "--", "/plugin/plugin"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command).map_err(|_| BridgeError::Disconnected)?;
    let input = child.stdin.take().ok_or(BridgeError::Disconnected)?;
    let output = child.stdout.take().ok_or(BridgeError::Disconnected)?;
    // What the plugin prints goes to the log under its name.
    if let Some(errors) = child.stderr.take() { log_errors(name, errors); }
    let process = Arc::new(ChildProcess::new(child, false));
    let ended = process.clone();
    let client = NativeClient::start(input, output, move || { ended.terminate(); ended.wait(); }, notice);
    let export = export.map(|_| PathBuf::from(GUEST_EXPORT));
    Ok(LaunchedNative { client, guest_directories: (GUEST_DATA.into(), GUEST_CACHE.into()), export, process })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::Duration;

    #[test]
    fn isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it() {
        let root = tempfile::tempdir().unwrap();
        crate::worker_conformance::an_isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it(root.path(), true,
            |runtime| Box::new(RuntimePaths { sandbox: "/usr/bin/bwrap".into(), system: "/usr".into(),
                libraries: runtime.join("libraries"), adapter: runtime.join("adapter") }));
    }

    #[test]
    fn a_native_plugin_sees_only_its_folders() {
        let root = tempfile::tempdir().unwrap();
        crate::conformance::a_native_plugin_sees_only_its_folders(&Bwrap, root.path(), &|_| PathBuf::from("/export"));
    }

    #[test]
    fn the_export_bubblewrap_binds_is_the_folder_that_was_checked() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let folder = base.join("plugin");
        std::fs::create_dir(&folder).unwrap();
        let script = r#"#!/usr/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  touch /export/written
  printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
done
"#;
        std::fs::write(folder.join("plugin"), script).unwrap();
        std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
        for dir in ["music/export", "private/export"] { std::fs::create_dir_all(base.join(dir)).unwrap(); }
        let paths = NativePaths { folder, data: base.join("data"), cache: base.join("cache"),
            export: Some(base.join("music/export")), private: vec![base.join("private")] };
        let checked = checked_export(&paths).unwrap();
        // Between the check and bubblewrap, the folder above it becomes a
        // link into Chilpa's own.
        std::fs::rename(base.join("music"), base.join("moved")).unwrap();
        std::os::unix::fs::symlink(base.join("private"), base.join("music")).unwrap();
        let launched = launch_with(&paths, checked, "test", 512 * 1024 * 1024, false, Box::new(|_: &str, _: &Value| {})).unwrap();
        assert_eq!(launched.export, Some(PathBuf::from("/export")), "initialize names where it is bound");
        launched.client.call("write", json!({}), Duration::from_secs(5)).unwrap();
        launched.client.stop();
        assert!(base.join("moved/export/written").is_file(), "the folder that was checked got the write");
        assert!(!base.join("private/export/written").exists(), "Chilpa's own folder did not");
    }

    #[test]
    fn bwrap_names_its_private_roots_before_and_in_initialize_and_stops_once() {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(Bwrap.availability(), Ok(()), "this test starts bwrap");
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("plugin");
        std::fs::create_dir(&folder).unwrap();
        let script = r#"#!/usr/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  printf '{"jsonrpc":"2.0","id":%s,"result":{"data":"%s","cache":"%s"}}\n' "$id" "$CHILPA_PLUGIN_DATA_DIR" "$CHILPA_PLUGIN_CACHE_DIR"
done
"#;
        std::fs::write(folder.join("plugin"), script).unwrap();
        std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let paths = NativePaths { folder, data: root.path().join("data"), cache: root.path().join("cache"), export: None, private: Vec::new() };
        let launched = Bwrap.launch(&paths, "test", 512 * 1024 * 1024, false, Box::new(|_: &str, _: &Value| {})).unwrap();
        assert_eq!(launched.guest_directories, (PathBuf::from("/data"), PathBuf::from("/cache")));
        assert_eq!(launched.export, None, "no export folder was approved");
        assert_eq!(launched.client.call("inspect", json!({}), Duration::from_secs(5)).unwrap(),
            json!({"data": "/data", "cache": "/cache"}), "the variables name the same folders as initialize");
        assert!(launched.process.alive());
        launched.client.stop();
        launched.client.stop();
        launched.process.terminate();
        launched.process.wait();
        assert!(!launched.process.alive(), "stopping the channel ended the sandbox");
        assert!(!launched.client.alive());
    }
}
