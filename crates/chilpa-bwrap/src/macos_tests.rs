use super::*;
use std::{io::{Read, Write}, net::{TcpListener, UdpSocket}, time::{Duration, Instant}};

fn paths(root: &Path, executable: &str) -> NativePaths {
    let folder = root.join("package");
    std::fs::create_dir(&folder).unwrap();
    std::fs::copy(executable, folder.join("plugin")).unwrap();
    // A copied platform binary needs its own signature at the fixture path.
    let signed = Command::new("/usr/bin/codesign").args(["--force", "--sign", "-"])
        .arg(folder.join("plugin")).output().unwrap();
    assert!(signed.status.success(), "{}", String::from_utf8_lossy(&signed.stderr));
    NativePaths { folder, data: root.join("data"), cache: root.join("cache"), export: None, private: Vec::new() }
}

#[test]
fn private_folders_are_writable_but_the_package_and_other_files_are_sealed() {
    let root = tempfile::Builder::new().prefix("chilpa-path-\"-(param)-").tempdir().unwrap();
    let paths = paths(root.path(), "/bin/bash");
    let outside = root.path().join("outside");
    std::fs::write(&outside, "private\n").unwrap();
    let mut child = command(&paths, false).unwrap();
    child.arg("-c").arg(r#"
        printf 'ok' > "$HOME/written" || exit 1
        printf 'ok' > "$CHILPA_PLUGIN_CACHE_DIR/written" || exit 2
        if read -r secret < "$1"; then exit 3; fi
        if printf 'bad' > "$2/written"; then exit 4; fi
        ulimit -n > "$HOME/descriptor-limit"
        read -r limit < "$HOME/descriptor-limit"
        [ "$limit" = 256 ] || exit 5
        ulimit -c > "$HOME/core-limit"
        read -r limit < "$HOME/core-limit"
        [ "$limit" = 0 ] || exit 6
        [ "$HOME" = "$CHILPA_PLUGIN_DATA_DIR" ] || exit 7
        exit 0
    "#).arg("fixture").arg(&outside).arg(&paths.folder);
    let result = child.output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(paths.data.join("written").is_file());
    assert!(paths.cache.join("written").is_file());
    assert!(!paths.folder.join("written").exists());
}

#[test]
fn execution_and_forking_are_denied_after_the_worker_starts() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path(), "/bin/bash");
    let result = command(&paths, false).unwrap().args(["-c", "exec /usr/bin/true"]).output().unwrap();
    assert!(!result.status.success(), "the worker must not execute another program");
    let result = command(&paths, false).unwrap().args(["-c", "(:) & wait"]).output().unwrap();
    assert!(!result.status.success(), "the worker must not fork a descendant");
}

#[test]
fn an_export_folder_requires_the_hosts_explicit_grant() {
    let root = tempfile::tempdir().unwrap();
    let mut paths = paths(root.path(), "/bin/bash");
    let export = root.path().join("export");
    std::fs::create_dir(&export).unwrap();
    let script = "printf exported > \"$1/written\"";
    let denied = command(&paths, false).unwrap().args(["-c", script, "fixture"]).arg(&export).output().unwrap();
    assert!(!denied.status.success());
    paths.export = Some(export.clone());
    let allowed = command(&paths, false).unwrap().args(["-c", script, "fixture"]).arg(&export).output().unwrap();
    assert!(allowed.status.success(), "{}", String::from_utf8_lossy(&allowed.stderr));
    assert_eq!(std::fs::read_to_string(export.join("written")).unwrap(), "exported");
}

#[test]
fn an_executable_symlink_cannot_escape_the_package() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path(), "/bin/bash");
    std::fs::remove_file(paths.folder.join("plugin")).unwrap();
    std::os::unix::fs::symlink("/bin/sh", paths.folder.join("plugin")).unwrap();
    assert!(matches!(command(&paths, false), Err(BridgeError::Denied)));
}

#[test]
fn replaced_private_roots_cannot_redirect_a_later_launch() {
    for name in ["data", "cache", "export"] {
        let root = tempfile::tempdir().unwrap();
        let mut paths = paths(root.path(), "/bin/bash");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let replaced = root.path().join(name);
        std::os::unix::fs::symlink(&outside, &replaced).unwrap();
        if name == "export" { paths.export = Some(replaced); }
        assert!(matches!(command(&paths, false), Err(BridgeError::Denied)), "replaced {name} root");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }
}

#[test]
fn offline_workers_cannot_connect_to_external_addresses() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path(), "/usr/bin/curl");
    let route = UdpSocket::bind("0.0.0.0:0").unwrap();
    route.connect("192.0.2.1:9").unwrap();
    let address = route.local_addr().unwrap().ip();
    assert!(!address.is_loopback() && !address.is_unspecified());
    let listener = TcpListener::bind((address, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}:{}/", address, listener.local_addr().unwrap().port());
    // TEST-NET-1 is not a local interface. A sandbox denial is EPERM rather
    // than a timeout; localhost intentionally remains usable for audio IPC.
    let denied = command(&paths, false).unwrap().args(["--verbose", "--max-time", "2", "--noproxy", "*", "http://192.0.2.1:9/"]).output().unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("Operation not permitted"),
        "the kernel must deny the connection: {:?}: {}", denied.status, String::from_utf8_lossy(&denied.stderr));
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut request = [0; 2048];
                    socket.read(&mut request).unwrap();
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("local fixture did not receive the online request: {error}"),
            }
        }
    });
    let allowed = command(&paths, true).unwrap().args(["--silent", "--max-time", "3", "--noproxy", "*", &url]).output().unwrap();
    server.join().unwrap();
    assert!(allowed.status.success(), "{:?}: {}", allowed.status, String::from_utf8_lossy(&allowed.stderr));
    assert_eq!(allowed.stdout, b"ok");
}

/// Where `trust_probe` finds its chain; set only when a test starts it inside
/// the sandbox.
const TRUST_FIXTURE: &str = "CHILPA_TEST_TRUST_FIXTURE";
const REFUSED: &str = "the system's certificate verifier refused";

/// A test anchor and a server certificate it signs, as DER files in `folder`.
fn chain(folder: &Path) {
    let openssl = |arguments: &[&str]| {
        let made = Command::new("/usr/bin/openssl").args(arguments).current_dir(folder).output().unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    };
    std::fs::write(folder.join("leaf.cnf"), "subjectAltName=DNS:plugin.test\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n").unwrap();
    openssl(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "anchor.key", "-out", "anchor.pem", "-days", "30",
        "-subj", "/CN=Chilpa Test Anchor", "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "keyUsage=critical,keyCertSign"]);
    openssl(&["x509", "-in", "anchor.pem", "-outform", "DER", "-out", "anchor.der"]);
    openssl(&["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "leaf.key", "-out", "leaf.csr", "-subj", "/CN=plugin.test"]);
    openssl(&["x509", "-req", "-in", "leaf.csr", "-CA", "anchor.pem", "-CAkey", "anchor.key", "-CAcreateserial", "-days", "30",
        "-extfile", "leaf.cnf", "-outform", "DER", "-out", "leaf.der"]);
}

/// Security.framework's trust evaluation, the call a platform certificate
/// verifier makes for each HTTPS handshake. `Err` carries the system's code.
fn evaluate(leaf: &[u8], anchor: &[u8], host: &str) -> Result<(), isize> {
    use std::ffi::c_void;
    type Object = *const c_void;
    #[repr(C)]
    struct ArrayCallBacks { version: isize, retain: Object, release: Object, describe: Object, equal: Object }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFTypeArrayCallBacks: ArrayCallBacks;
        fn CFDataCreate(allocator: Object, bytes: *const u8, length: isize) -> Object;
        fn CFStringCreateWithBytes(allocator: Object, bytes: *const u8, length: isize, encoding: u32, external: u8) -> Object;
        fn CFArrayCreate(allocator: Object, values: *const Object, count: isize, callbacks: *const ArrayCallBacks) -> Object;
        fn CFErrorGetCode(error: Object) -> isize;
    }
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecCertificateCreateWithData(allocator: Object, data: Object) -> Object;
        fn SecPolicyCreateSSL(server: u8, host: Object) -> Object;
        fn SecTrustCreateWithCertificates(certificates: Object, policies: Object, trust: *mut Object) -> i32;
        fn SecTrustSetAnchorCertificates(trust: Object, anchors: Object) -> i32;
        fn SecTrustSetNetworkFetchAllowed(trust: Object, allowed: u8) -> i32;
        fn SecTrustEvaluateWithError(trust: Object, error: *mut Object) -> bool;
    }
    const UTF8: u32 = 0x0800_0100;
    // SAFETY: plain Core Foundation and Security calls on objects made here;
    // they live until the test process ends.
    unsafe {
        let certificate = |bytes: &[u8]| SecCertificateCreateWithData(std::ptr::null(), CFDataCreate(std::ptr::null(), bytes.as_ptr(), bytes.len() as isize));
        let array = |items: &[Object]| CFArrayCreate(std::ptr::null(), items.as_ptr(), items.len() as isize, &kCFTypeArrayCallBacks);
        let (leaf, anchor) = (certificate(leaf), certificate(anchor));
        assert!(!leaf.is_null() && !anchor.is_null(), "the fixture certificates do not parse");
        let host = CFStringCreateWithBytes(std::ptr::null(), host.as_ptr(), host.len() as isize, UTF8, 0);
        let mut trust = std::ptr::null();
        assert_eq!(SecTrustCreateWithCertificates(array(&[leaf]), SecPolicyCreateSSL(1, host), &mut trust), 0);
        assert_eq!(SecTrustSetAnchorCertificates(trust, array(&[anchor])), 0);
        assert_eq!(SecTrustSetNetworkFetchAllowed(trust, 0), 0);
        let mut error = std::ptr::null();
        if SecTrustEvaluateWithError(trust, &mut error) { Ok(()) } else { Err(if error.is_null() { 0 } else { CFErrorGetCode(error) }) }
    }
}

/// Started inside the sandbox by the test below; on its own it does nothing.
#[test]
#[ignore = "runs inside the sandbox, started by online_workers_can_verify_a_server_certificate"]
fn trust_probe() {
    let Some(folder) = std::env::var_os(TRUST_FIXTURE).map(PathBuf::from) else { return };
    let read = |name: &str| std::fs::read(folder.join(name)).unwrap();
    if let Err(code) = evaluate(&read("leaf.der"), &read("anchor.der"), "plugin.test") {
        panic!("{REFUSED}: {code}");
    }
}

#[test]
fn online_workers_can_verify_a_server_certificate() {
    let root = tempfile::tempdir().unwrap();
    let probe = std::env::current_exe().unwrap();
    let paths = paths(root.path(), probe.to_str().unwrap());
    std::fs::create_dir_all(&paths.data).unwrap();
    chain(&paths.data);
    let fixture = paths.data.canonicalize().unwrap();
    let run = |network| {
        let ran = command(&paths, network).unwrap().args(["--exact", "macos::tests::trust_probe", "--ignored", "--test-threads=1"])
            .env(TRUST_FIXTURE, &fixture).output().unwrap();
        (ran.status.success(), format!("{}{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr)))
    };
    // The sandbox is what refuses an offline worker, so the probe would also
    // catch a policy that forgets the verifier for an online one.
    let (trusted, said) = run(false);
    assert!(!trusted && said.contains(REFUSED), "an offline worker reached the verifier: {said}");
    let (trusted, said) = run(true);
    assert!(trusted, "a plugin's HTTPS client cannot verify a server certificate: {said}");
}

#[test]
fn the_budget_monitor_reads_physical_memory_instead_of_address_space() {
    assert!(footprint(std::process::id()).is_some_and(|bytes| bytes > 0));
}
