use super::*;
use crate::fake_portal::{Exposed as Received, FakePortal, PrivateBus};
use crate::portal::{CLEAR_ENV, NO_NETWORK, SANDBOX, WATCH_BUS};
use crate::worker_conformance::Deny;
use chilpa_plugin::bridge::BridgeSession;
use serde_json::json;
use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;

/// Answers `ping` and never answers `hang`, in the bridge's framing.
const ECHO: &str = r#"
use JSON::PP;
binmode STDIN; binmode STDOUT; $|=1;
sub exact { my($n)=@_; my $s=''; while(length($s)<$n){my $r=read(STDIN,my $b,$n-length($s)); exit unless $r; $s.=$b} $s }
while(1){
 my $n=unpack('N',exact(4)); my $m=decode_json(exact($n));
 if($m->{operation} eq 'hang'){while(1){}}
 my $b=encode_json({type=>'reply',id=>$m->{id},result=>{pong=>JSON::PP::true}}); print pack('N',length($b)),$b;
}
"#;

/// A runtime with the echo worker and an empty module folder.
fn runtime(base: &Path) -> PathBuf {
    let runtime = base.join("runtime");
    for path in [runtime.join("adapter"), runtime.join("libraries/lib/perl5")] { std::fs::create_dir_all(path).unwrap(); }
    std::fs::write(runtime.join("adapter/worker.pl"), ECHO).unwrap();
    runtime
}

fn byte_string(text: impl AsRef<OsStr>) -> Vec<u8> {
    let mut bytes = text.as_ref().as_bytes().to_vec();
    bytes.push(0);
    bytes
}

#[test]
fn the_worker_starts_offline_with_its_limits_and_only_reads_its_package() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let root = tempfile::tempdir().unwrap();
    let base = root.path().canonicalize().unwrap();
    let runtime = runtime(&base);
    let guest = base.join("guest");
    std::fs::create_dir(&guest).unwrap();
    let runner = base.join("runner");
    std::fs::write(&runner, "").unwrap();
    // The host's Perl stands in for the application's: the fake portal runs it unsandboxed.
    let worker = FlatpakWorker::with(Arc::new(Flatpak::on(bus.bus(), runner.clone())),
        "/usr/bin/perl".into(), &runtime).unwrap();
    assert_eq!(worker.guest_root(&guest), guest.display().to_string(), "the package keeps its own path");
    let mut session = BridgeSession::new(worker.launch(&guest).unwrap());
    assert_eq!(session.call("ping", json!({}), Duration::from_secs(5), &mut Deny).unwrap(), json!({"pong": true}));

    let spawned = fake.spawned().remove(0);
    assert_eq!(spawned.flags, CLEAR_ENV | SANDBOX | WATCH_BUS | NO_NETWORK, "the worker never has a network of its own");
    assert!(spawned.writable.is_empty(), "nothing is writable");
    let received = |paths: &[PathBuf]| paths.iter().map(|path| Received { path: path.clone(), o_path: true }).collect::<Vec<_>>();
    assert_eq!(spawned.readable, received(&[guest.clone(), runtime.join("adapter"), runtime.join("libraries")]));
    assert_eq!(spawned.cwd, byte_string(&guest));
    let script = runtime.join("adapter/worker.pl");
    assert_eq!(spawned.argv, [runner.as_os_str(), OsStr::new("--as=268435456"), OsStr::new("--nproc=0"), OsStr::new("--nofile=64"),
        OsStr::new("--fsize=1048576"), OsStr::new("--cpu=120:120"), OsStr::new("--core=0"), OsStr::new("--keep-env=PATH,PERL5LIB"), OsStr::new("--"),
        OsStr::new("/usr/bin/perl"), script.as_os_str()].map(|arg| byte_string(arg)).to_vec(), "the limits bubblewrap sets with prlimit");
    assert_eq!(spawned.env, HashMap::from([("PATH".to_string(), "/usr/bin".to_string()),
        ("PERL5LIB".to_string(), runtime.join("libraries/lib/perl5").display().to_string())]));

    assert_eq!(session.call("hang", json!({}), Duration::from_millis(100), &mut Deny), Err(BridgeError::Timeout));
    assert_eq!(fake.signals(), vec![(spawned.pid, libc::SIGKILL as u32, true)], "the timeout revoked it through the portal");
}

#[test]
fn without_perl_or_its_runtime_there_is_no_worker_and_the_reason_names_what_is_missing() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().canonicalize().unwrap();
    let runner = base.join("runner");
    std::fs::write(&runner, "").unwrap();
    let flatpak = || Arc::new(Flatpak::on(crate::portal::Bus::Address("unix:path=/nonexistent".into()), runner.clone()));
    let runtime = runtime(&base);
    let missing = base.join("perl");
    assert!(matches!(FlatpakWorker::with(flatpak(), missing.clone(), &runtime), Err(Unavailable::MissingProgram(path)) if path == missing));
    let empty = base.join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(matches!(FlatpakWorker::with(flatpak(), "/usr/bin/perl".into(), &empty),
        Err(Unavailable::MissingProgram(path)) if path == empty.join("adapter/worker.pl")));
    assert!(matches!(FlatpakWorker::with(flatpak(), "/usr/bin/perl".into(), &runtime), Err(Unavailable::Portal)),
        "Perl and its runtime are there, the portal is not");
}

/// The conformance bubblewrap passes, through the real portal and the
/// application's Perl. It runs only inside the Chilpa Flatpak, built as the
/// process plugins' smoke is (see flatpak_tests.rs). Its /tmp has no bound
/// there, which the check expects.
#[test]
#[ignore = "needs the real Flatpak portal and Perl under /app: run it inside the Chilpa Flatpak"]
fn an_isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it_through_the_portal() {
    assert!(chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak, "this smoke runs inside the Chilpa Flatpak");
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).expect("XDG_CACHE_HOME names the application's cache");
    let root = tempfile::tempdir_in(cache).unwrap();
    let base = root.path().canonicalize().unwrap();
    crate::worker_conformance::an_isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it(&base, false,
        |runtime| Box::new(FlatpakWorker::with(crate::flatpak::shared(), PERL.into(), runtime).unwrap()));
}
