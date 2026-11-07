//! What every Linux backend owes the compatibility worker, checked the same
//! way against each: it cannot fork, it has no network, a file beside its
//! package is out of reach, the root and the devices are read-only, and a
//! call that does not end in time revokes it. Whether its /tmp is bounded
//! depends on the backend, and the caller says what to expect.

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

/// Answers `inspect` with what it could and could not do, and never
/// answers `hang`.
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
 my $root_write=open(my $root,'>','/escape'); my $dev_write=open(my $dev,'>','/dev/escape');
 my $written=0; for my $i (1..24){open(my $file,'>',"/tmp/quota-$i") or last; my $n=syswrite($file,'x' x 1048576); $written+=$n||0; close $file; last if !defined($n) || $n<1048576}
 unlink "/tmp/quota-$_" for 1..24;
 my $t=JSON::PP::true; my $f=JSON::PP::false;
 my $result={fork_denied=>!defined($fork)?$t:$f,network_denied=>!$network?$t:$f,outside_hidden=>$hidden?$t:$f,
 root_readonly=>!$root_write?$t:$f,devices_readonly=>!$dev_write?$t:$f,temporary_bounded=>$written<24*1048576?$t:$f};
 my $b=encode_json({type=>'reply',id=>$m->{id},result=>$result}); print pack('N',length($b)),$b;
}
"#;

/// Makes `root/runtime/adapter` with the probe and `root/runtime/libraries`,
/// lets `launcher` build the backend over that runtime, then starts the
/// probe on a package under `root`.
pub(crate) fn an_isolated_worker_cannot_fork_or_use_network_and_timeout_revokes_it(
    root: &Path, temporary_bounded: bool, launcher: impl FnOnce(&Path) -> Box<dyn WorkerLauncher>) {
    let runtime = root.join("runtime");
    let guest = root.join("guest");
    for path in [runtime.join("adapter"), runtime.join("libraries/lib/perl5"), guest.clone()] { std::fs::create_dir_all(path).unwrap(); }
    std::fs::write(runtime.join("adapter/worker.pl"), PROBE).unwrap();
    let outside = root.join("outside");
    std::fs::write(&outside, "host-only").unwrap();
    let launcher = launcher(&runtime);
    let mut session = BridgeSession::new(launcher.launch(&guest).unwrap());
    let inspected = session.call("inspect", json!({"outside": outside}), Duration::from_secs(3), &mut Deny).unwrap();
    for property in ["fork_denied", "network_denied", "outside_hidden", "root_readonly", "devices_readonly"] {
        assert_eq!(inspected[property], true, "{property}: {inspected}");
    }
    assert_eq!(inspected["temporary_bounded"], temporary_bounded, "{inspected}");
    let start = Instant::now();
    assert_eq!(session.call("hang", json!({}), Duration::from_millis(100), &mut Deny), Err(BridgeError::Timeout));
    assert!(start.elapsed() < Duration::from_secs(2), "the timeout revoked the worker at once");
    assert!(!session.alive());
}
