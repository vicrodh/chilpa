//! Linux isolation for a single-process compatibility worker.
#![cfg(target_os = "linux")]

use chilpa_plugin::{bridge::BridgeError, pipe_transport::PipeTransport};
use std::{path::{Path, PathBuf}, process::{Command, Stdio}};

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
        let adjacent = std::env::current_exe().ok()?.parent()?.join("compat-runtime");
        let root = if adjacent.join("adapter/worker.pl").is_file() { adjacent.as_path() } else { root };
        let paths = Self { sandbox: "/usr/bin/bwrap".into(), system: "/usr".into(),
            libraries: root.join("libraries"), adapter: root.join("adapter") };
        (paths.sandbox.is_file() && paths.adapter.join("worker.pl").is_file()
            && paths.libraries.join("lib/perl5").is_dir()).then_some(paths)
    }
}

impl chilpa_plugin::bridge::WorkerLauncher for RuntimePaths {
    fn guest_root(&self) -> &str { "/guest" }

    fn inspection_revision(&self) -> Option<String> {
        use sha2::{Digest, Sha256};
        fn hash_tree(path: &Path, hash: &mut Sha256) -> std::io::Result<()> {
            let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                hash.update(entry.file_name().as_encoded_bytes());
                if entry.file_type()?.is_dir() { hash_tree(&entry.path(), hash)?; }
                else { hash.update(std::fs::read(entry.path())?); }
            }
            Ok(())
        }
        let mut hash = Sha256::new();
        hash_tree(&self.adapter, &mut hash).ok()?;
        hash_tree(&self.libraries, &mut hash).ok()?;
        Some(format!("{:x}", hash.finalize()))
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
            "/usr/bin/prlimit"])
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

/// Starts a worker from the one thread that lives as long as Chilpa.
///
/// `--die-with-parent` is a parent-death signal, and Linux sends it when the
/// thread that started the child ends, not the process. Workers started at
/// startup, on a thread that then finished, were killed with it and every
/// first call found them gone.
fn spawn(command: Command) -> std::io::Result<std::process::Child> {
    type Ask = (Command, std::sync::mpsc::Sender<std::io::Result<std::process::Child>>);
    static LAUNCHER: std::sync::OnceLock<Option<std::sync::Mutex<std::sync::mpsc::Sender<Ask>>>> = std::sync::OnceLock::new();
    let launcher = LAUNCHER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Ask>();
        std::thread::Builder::new().name("chilpa-worker-launcher".into())
            .spawn(move || for (mut command, reply) in rx { let _ = reply.send(command.spawn()); })
            .ok().map(|_| std::sync::Mutex::new(tx))
    });
    let gone = || std::io::Error::other("the worker launcher is not running");
    let (reply, answer) = std::sync::mpsc::channel();
    launcher.as_ref().ok_or_else(gone)?.lock().unwrap_or_else(|e| e.into_inner()).send((command, reply)).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
}

fn canonical_directory(path: &Path) -> Result<PathBuf, BridgeError> {
    let path = path.canonicalize().map_err(|_| BridgeError::Disconnected)?;
    if !path.is_dir() { return Err(BridgeError::Disconnected); }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_plugin::bridge::{BridgeSession, ServiceOperation, WorkerServices};
    use serde_json::{json, Value};
    use std::time::{Duration, Instant};

    struct Deny;
    impl WorkerServices for Deny {
        fn handle(&mut self, _: ServiceOperation, _: Instant) -> Result<Value, BridgeError> { Err(BridgeError::Denied) }
    }

    #[test]
    fn isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it() {
        let root = tempfile::tempdir().unwrap();
        let adapter = root.path().join("adapter");
        let libraries = root.path().join("libraries");
        let guest = root.path().join("guest");
        for path in [&adapter, &libraries, &guest] { std::fs::create_dir(path).unwrap(); }
        std::fs::write(root.path().join("outside"), "host-only").unwrap();
        let fixture = r#"
use JSON::PP; use Socket;
binmode STDIN; binmode STDOUT; $|=1;
sub exact { my($n)=@_; my $s=''; while(length($s)<$n){my $r=read(STDIN,my $b,$n-length($s)); exit unless $r; $s.=$b} $s }
while(1){
 my $n=unpack('N',exact(4)); my $m=decode_json(exact($n));
 if($m->{operation} eq 'hang'){while(1){}}
 my $fork=fork(); if(defined($fork) && !$fork){exit 0} waitpid($fork,0) if defined $fork;
 socket(my $socket,PF_INET,SOCK_STREAM,0); my $network=connect($socket,sockaddr_in(9,inet_aton('1.1.1.1')));
 my $root_write=open(my $root,'>','/escape'); my $dev_write=open(my $dev,'>','/dev/escape');
 my $written=0; for my $i (1..24){open(my $file,'>',"/tmp/quota-$i") or last; my $n=syswrite($file,'x' x 1048576); $written+=$n||0; close $file; last if !defined($n) || $n<1048576}
 my $result={fork_denied=>!defined($fork)?JSON::PP::true:JSON::PP::false,network_denied=>!$network?JSON::PP::true:JSON::PP::false,home_hidden=>!-d '/home'?JSON::PP::true:JSON::PP::false,
 root_readonly=>!$root_write?JSON::PP::true:JSON::PP::false,devices_readonly=>!$dev_write?JSON::PP::true:JSON::PP::false,temporary_bounded=>$written<24*1048576?JSON::PP::true:JSON::PP::false};
 my $b=encode_json({type=>'reply',id=>$m->{id},result=>$result}); print pack('N',length($b)),$b;
}
"#;
        std::fs::write(adapter.join("worker.pl"), fixture).unwrap();
        let paths = RuntimePaths { sandbox: "/usr/bin/bwrap".into(), system: "/usr".into(), libraries, adapter };
        let mut session = BridgeSession::new(launch(&paths, &guest, 256 * 1024 * 1024).unwrap());
        assert_eq!(session.call("inspect", json!({}), Duration::from_secs(3), &mut Deny).unwrap(),
            json!({"fork_denied":true,"network_denied":true,"home_hidden":true,"root_readonly":true,"devices_readonly":true,"temporary_bounded":true}));
        let start = Instant::now();
        assert_eq!(session.call("hang", json!({}), Duration::from_millis(100), &mut Deny), Err(BridgeError::Timeout));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
