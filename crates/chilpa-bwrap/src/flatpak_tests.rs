use super::*;
use crate::fake_portal::{Exposed as Received, FakePortal, PrivateBus};
use crate::portal::{CLEAR_ENV, NO_NETWORK, SANDBOX, WATCH_BUS};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

const MEMORY: usize = 512 * 1024 * 1024;

/// A plugin that answers each call with what its environment names.
const TELLS_ITS_FOLDERS: &str = r#"#!/usr/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  printf '{"jsonrpc":"2.0","id":%s,"result":{"data":"%s","cache":"%s","home":"%s","temporary":"%s","cwd":"%s"}}\n' \
    "$id" "$CHILPA_PLUGIN_DATA_DIR" "$CHILPA_PLUGIN_CACHE_DIR" "$HOME" "$TMPDIR" "$(pwd)"
done
"#;

struct Fixture {
    root: tempfile::TempDir,
    paths: NativePaths,
    runner: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let folder = base.join("plugin");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("plugin"), TELLS_ITS_FOLDERS).unwrap();
        std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = base.join("runner");
        std::fs::write(&runner, "").unwrap();
        let paths = NativePaths { folder, data: base.join("data"), cache: base.join("cache"), export: None, private: Vec::new() };
        Self { root, paths, runner }
    }

    fn base(&self) -> PathBuf {
        self.root.path().canonicalize().unwrap()
    }
}

fn quiet() -> Notice {
    Box::new(|_: &str, _: &Value| {})
}

fn received(paths: &[PathBuf]) -> Vec<Received> {
    paths.iter().map(|path| Received { path: path.clone(), o_path: true }).collect()
}

fn byte_string(text: impl AsRef<OsStr>) -> Vec<u8> {
    let mut bytes = text.as_ref().as_bytes().to_vec();
    bytes.push(0);
    bytes
}

#[test]
fn a_plugin_gets_its_folders_as_descriptors_and_is_told_their_paths() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let fixture = Fixture::new();
    let base = fixture.base();
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    assert_eq!(flatpak.availability(), Ok(()));
    let launched = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()).unwrap();
    let (data, cache) = (base.join("data"), base.join("cache"));
    assert_eq!(launched.guest_directories, (data.clone(), cache.clone()));
    assert_eq!(launched.client.call("inspect", json!({}), Duration::from_secs(5)).unwrap(),
        json!({"data": data, "cache": cache, "home": data, "temporary": cache.join("tmp"), "cwd": data}),
        "the environment names the folders initialize names");

    let spawned = fake.spawned().remove(0);
    assert_eq!(spawned.flags, CLEAR_ENV | SANDBOX | WATCH_BUS | NO_NETWORK);
    assert_eq!(spawned.sandbox_flags, Some(0));
    assert_eq!(spawned.fds, vec![0, 1, 2]);
    assert_eq!(spawned.readable, received(&[base.join("plugin")]), "the package, read-only, as an O_PATH descriptor");
    assert_eq!(spawned.writable, received(&[data.clone(), cache.clone()]));
    assert_eq!(spawned.cwd, byte_string(&data));
    let program = base.join("plugin/plugin");
    assert_eq!(spawned.argv, [fixture.runner.as_os_str(), OsStr::new("--as=536870912"), OsStr::new("--nofile=256"),
        OsStr::new("--core=0"), OsStr::new("--"), program.as_os_str()].map(|arg| byte_string(arg)).to_vec(), "the limits bubblewrap sets with prlimit");
    let expected: HashMap<String, String> = environment(&data, &cache).unwrap().into_iter().map(|(name, value)| (name.to_string(), value)).collect();
    assert_eq!(spawned.env, expected);
    assert_eq!(spawned.env["XDG_DATA_HOME"], data.join(".local/share").display().to_string());

    assert!(launched.process.alive());
    launched.client.stop();
    assert!(!launched.process.alive(), "stopping the channel ended the process and waited for its exit");
    assert_eq!(fake.signals(), vec![(spawned.pid, libc::SIGKILL as u32, true)]);
}

#[test]
fn a_network_change_ends_the_plugin_then_starts_it_again_without() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let mut fixture = Fixture::new();
    let base = fixture.base();
    std::fs::create_dir(base.join("export")).unwrap();
    fixture.paths.export = Some(base.join("export"));
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    // What Natives::network_changed does: stop, which waits for the exit,
    // then start again with the new network.
    let online = flatpak.launch(&fixture.paths, "test", MEMORY, true, quiet()).unwrap();
    online.client.stop();
    assert!(!online.process.alive());
    fixture.paths.export = None;
    let offline = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()).unwrap();
    let spawned = fake.spawned();
    assert_eq!(spawned.len(), 2);
    assert_eq!(spawned[0].flags & NO_NETWORK, 0);
    assert_eq!(spawned[1].flags & NO_NETWORK, NO_NETWORK);
    assert_eq!(spawned[0].writable, received(&[base.join("data"), base.join("cache"), base.join("export")]), "an approved export folder");
    assert_eq!(spawned[1].writable, received(&[base.join("data"), base.join("cache")]));
    assert_eq!(fake.signals().len(), 1, "only the first one was ended");
    assert!(offline.process.alive());
    offline.client.stop();
}

#[test]
fn availability_needs_the_runner_and_a_portal_of_version_3() {
    let mut bus = PrivateBus::start();
    let fixture = Fixture::new();
    let missing = fixture.base().join("missing");
    assert_eq!(Flatpak::on(bus.bus(), missing.clone()).availability(), Err(Unavailable::MissingProgram(missing)));
    assert_eq!(Flatpak::on(bus.bus(), fixture.runner.clone()).availability(), Err(Unavailable::Portal), "nobody owns the portal's name");
    let old = PrivateBus::start();
    let _old = FakePortal::serve(&old, 2);
    assert_eq!(Flatpak::on(old.bus(), fixture.runner.clone()).availability(), Err(Unavailable::Portal), "version 2 has no descriptors to expose");
    let _fake = FakePortal::serve(&bus, 8);
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    assert_eq!(flatpak.availability(), Ok(()));
    let portal = flatpak.portal().unwrap();
    bus.end();
    assert!(portal.wait_lost(Duration::from_secs(3)), "the reader of exits saw the bus go");
    assert_eq!(flatpak.availability(), Err(Unavailable::Portal), "a lost bus is not connected to again");
    assert!(matches!(flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()), Err(BridgeError::Disconnected)));
}

#[test]
fn a_plugin_this_runtime_cannot_load_is_found_before_it_starts() {
    let fixture = Fixture::new();
    std::fs::write(fixture.paths.folder.join("plugin"), "not a program").unwrap();
    let flatpak = Flatpak::on(Bus::Address("unix:path=/nonexistent".into()), fixture.runner.clone());
    assert_eq!(flatpak.preflight(&fixture.paths.folder), Err(Incompatible::Format));
    std::fs::write(fixture.paths.folder.join("plugin"), "#!/nonexistent/interpreter\n").unwrap();
    assert_eq!(flatpak.preflight(&fixture.paths.folder), Err(Incompatible::Interpreter("/nonexistent/interpreter".into())));
}

#[test]
fn an_export_folder_that_now_lies_in_chilpa_s_own_is_refused_at_launch() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let mut fixture = Fixture::new();
    let base = fixture.base();
    let private = base.join("private");
    std::fs::create_dir_all(private.join("export")).unwrap();
    // The folder above the approved one was swapped for a link after the
    // grant: only the last component is opened without following links.
    std::os::unix::fs::symlink(&private, base.join("music")).unwrap();
    fixture.paths.private = vec![private.clone()];
    fixture.paths.export = Some(base.join("music/export"));
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    assert!(matches!(flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()), Err(BridgeError::Denied)));
    assert!(fake.spawned().is_empty(), "nothing was started");
    fixture.paths.private = vec![base.join("elsewhere")];
    std::fs::create_dir(base.join("elsewhere")).unwrap();
    let launched = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()).unwrap();
    assert_eq!(fake.spawned()[0].writable.last().map(|exposed| exposed.path.clone()), Some(private.join("export")),
        "the same folder, outside Chilpa's own, is handed over as what it is");
    launched.client.stop();
}

#[test]
fn a_folder_is_handed_over_only_as_itself_never_through_a_link() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().canonicalize().unwrap();
    std::fs::create_dir(base.join("real")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    assert_eq!(Exposed::open(&base.join("real")).unwrap().path, base.join("real"));
    assert!(Exposed::open(&base.join("link")).is_err(), "O_NOFOLLOW refuses the link");
    assert!(Exposed::open(&base.join("absent")).is_err());
}

/// The conformance bubblewrap passes, through the real portal. It runs only
/// inside the Chilpa Flatpak, and is the smoke for this backend there: build
/// this crate's tests with the runtime's SDK (org.kde.Sdk 6.11 with the Rust
/// extension), install chilpa-plugin-runner at /app/libexec, copy the test
/// binary under ~/.var/app/app.chilpa.Chilpa/data and run
/// `flatpak run --command=<that binary> app.chilpa.Chilpa --ignored --exact
/// flatpak::tests::a_native_plugin_sees_only_its_folders_through_the_portal`.
#[test]
#[ignore = "needs the real Flatpak portal: run it inside the Chilpa Flatpak"]
fn a_native_plugin_sees_only_its_folders_through_the_portal() {
    assert!(chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak, "this smoke runs inside the Chilpa Flatpak");
    // Only the application's own folders can be handed over from inside it.
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).expect("XDG_CACHE_HOME names the application's cache");
    let root = tempfile::tempdir_in(cache).unwrap();
    let base = root.path().canonicalize().unwrap();
    crate::conformance::a_native_plugin_sees_only_its_folders(&Flatpak::default(), &base, &|export| export.to_path_buf());
}
