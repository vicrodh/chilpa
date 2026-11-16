//! What every Linux backend owes the compatibility worker, checked the same
//! way against each: it cannot fork, it has no network, a file beside its
//! package is out of reach, `/usr` and `/app` are read-only, its package is
//! read-only and stays in place, and a call that does not end in time
//! revokes it. The root and `/dev` differ by backend, and the caller says
//! which to expect: read-only under bubblewrap; under the portal a private
//! tmpfs of each spawn's own, writable, that neither the host nor the next
//! worker sees. So does whether its /tmp is bounded.

use chilpa_plugin::bridge::{BridgeError, BridgeSession, ServiceOperation, WorkerLauncher, WorkerServices};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

/// Refuses every host service: the worker has to reach nothing through them.
pub(crate) struct Deny;

impl WorkerServices for Deny {
    fn handle(&mut self, _: ServiceOperation, _: Instant) -> Result<Value, BridgeError> {
        Err(BridgeError::Denied)
    }
}

/// What a backend gives the worker's root and /dev.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Root {
    /// Read-only: the worker writes nothing there.
    ReadOnly,
    /// Writable, but private to the spawn and gone with it.
    Private,
}

/// What differs between backends.
pub(crate) struct Expected {
    pub(crate) root: Root,
    /// Whether /tmp has a bound of its own.
    pub(crate) temporary_bounded: bool,
}

/// Answers `inspect` with what it could and could not do, and never
/// answers `hang`. `root_inherited` says whether `/escape` was there before
/// it tried to write it: what an earlier worker left.
pub(crate) const PROBE: &str = r#"
use JSON::PP; use Socket;
binmode STDIN; binmode STDOUT; $|=1;
sub exact { my($n)=@_; my $s=''; while(length($s)<$n){my $r=read(STDIN,my $b,$n-length($s)); exit unless $r; $s.=$b} $s }
while(1){
 my $n=unpack('N',exact(4)); my $m=decode_json(exact($n));
 if($m->{operation} eq 'hang'){while(1){}}
 my $fork=fork(); if(defined($fork) && !$fork){exit 0} waitpid($fork,0) if defined $fork;
 socket(my $socket,PF_INET,SOCK_STREAM,0); my $network=connect($socket,sockaddr_in(9,inet_aton('1.1.1.1')));
 my $outside=$m->{input}{outside}; my $hidden=!open(my $peek,'<',$outside);
 my $guest=$m->{input}{guest};
 my $inherited=(-e '/escape' || -e '/dev/escape');
 my $root_write=open(my $root,'>','/escape'); my $dev_write=open(my $dev,'>','/dev/escape');
 my $system_write=open(my $system,'>','/usr/escape'); my $app_write=open(my $app,'>','/app/escape');
 my $package_write=open(my $package,'>',"$guest/escape");
 my $moved=rename($guest,"$guest.moved"); rename("$guest.moved",$guest) if $moved;
 my $written=0; for my $i (1..24){open(my $file,'>',"/tmp/quota-$i") or last; my $n=syswrite($file,'x' x 1048576); $written+=$n||0; close $file; last if !defined($n) || $n<1048576}
 unlink "/tmp/quota-$_" for 1..24;
 my $t=JSON::PP::true; my $f=JSON::PP::false;
 my $result={fork_denied=>!defined($fork)?$t:$f,network_denied=>!$network?$t:$f,outside_hidden=>$hidden?$t:$f,
 root_readonly=>!$root_write?$t:$f,devices_readonly=>!$dev_write?$t:$f,root_inherited=>$inherited?$t:$f,
 system_readonly=>!$system_write?$t:$f,app_readonly=>!$app_write?$t:$f,package_readonly=>!$package_write?$t:$f,
 package_fixed=>!$moved?$t:$f,temporary_bounded=>$written<24*1048576?$t:$f};
 my $b=encode_json({type=>'reply',id=>$m->{id},result=>$result}); print pack('N',length($b)),$b;
}
"#;

/// Makes `root/runtime/adapter` with the probe and `root/runtime/libraries`,
/// lets `launcher` build the backend over that runtime, then starts the
/// probe on a package under `root`, twice, one worker after the other.
pub(crate) fn an_isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it(
    root: &Path, expected: Expected, launcher: impl FnOnce(&Path) -> Box<dyn WorkerLauncher>) {
    let runtime = root.join("runtime");
    let guest = root.join("guest");
    for path in [runtime.join("adapter"), runtime.join("libraries/lib/perl5"), guest.clone()] { std::fs::create_dir_all(path).unwrap(); }
    std::fs::write(runtime.join("adapter/worker.pl"), PROBE).unwrap();
    std::fs::write(guest.join("package.pl"), "1;\n").unwrap();
    let outside = root.join("outside");
    std::fs::write(&outside, "host-only").unwrap();
    let launcher = launcher(&runtime);
    let input = json!({"outside": outside, "guest": launcher.guest_root(&guest)});
    let mut first = BridgeSession::new(launcher.launch(&guest).unwrap());
    let inspected = first.call("inspect", input.clone(), Duration::from_secs(3), &mut Deny).unwrap();
    for property in ["fork_denied", "network_denied", "outside_hidden", "system_readonly", "app_readonly", "package_readonly", "package_fixed"] {
        assert_eq!(inspected[property], true, "{property}: {inspected}");
    }
    let read_only = expected.root == Root::ReadOnly;
    assert_eq!(inspected["root_readonly"], read_only, "the root: {inspected}");
    assert_eq!(inspected["devices_readonly"], read_only, "/dev: {inspected}");
    assert_eq!(inspected["temporary_bounded"], expected.temporary_bounded, "{inspected}");
    assert_eq!(inspected["root_inherited"], false, "{inspected}");
    // What it wrote in its root reached neither this side nor its package.
    assert!(!Path::new("/escape").exists() && !Path::new("/dev/escape").exists(), "the worker's root is not this side's");
    assert_eq!(std::fs::read_dir(&guest).unwrap().count(), 1, "its package is as it was handed over");
    assert!(guest.join("package.pl").is_file());

    // A second worker, while the first still runs: its root is its own, not
    // the first one's.
    let mut second = BridgeSession::new(launcher.launch(&guest).unwrap());
    let again = second.call("inspect", input, Duration::from_secs(3), &mut Deny).unwrap();
    assert_eq!(again["root_inherited"], false, "a worker never sees what another wrote in its root: {again}");
    drop(second);

    let start = Instant::now();
    assert_eq!(first.call("hang", json!({}), Duration::from_millis(100), &mut Deny), Err(BridgeError::Timeout));
    assert!(start.elapsed() < Duration::from_secs(2), "the timeout revoked the worker at once");
    assert!(!first.alive());
}
