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
    assert_eq!(spawned.fds, vec![0, 1, 2, 4], "the runner's ready line comes on a descriptor of its own");
    assert_eq!(spawned.readable, received(&[base.join("plugin")]), "the package, read-only, as an O_PATH descriptor");
    assert_eq!(spawned.writable, received(&[data.clone(), cache.clone()]));
    assert_eq!(spawned.cwd, byte_string(&data));
    let program = base.join("plugin/plugin");
    let keep = "--keep-env=PATH,HOME,XDG_CACHE_HOME,XDG_CONFIG_HOME,XDG_DATA_HOME,XDG_STATE_HOME,TMPDIR,CHILPA_PLUGIN_DATA_DIR,CHILPA_PLUGIN_CACHE_DIR,SSL_CERT_DIR";
    assert_eq!(spawned.argv, [fixture.runner.as_os_str(), OsStr::new("--as=536870912"), OsStr::new("--nofile=256"),
        OsStr::new("--core=0"), OsStr::new(keep), OsStr::new("--ready=4"), OsStr::new("--"), program.as_os_str()].map(|arg| byte_string(arg)).to_vec(),
        "the limits bubblewrap sets with prlimit, the runner keeps only the variables it is started with, and says when the plugin runs");
    assert_eq!(launched.export, None, "no export folder was approved");
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
    assert_eq!(spawned[0].fds, vec![0, 1, 2, 3, 4], "held at descriptor 3 too, for the runner to check");
    assert!(spawned[0].argv.contains(&byte_string(format!("--export={}", base.join("export").display()))));
    assert_eq!(online.export, Some(base.join("export")), "at its own path, where the portal mounts it");
    assert_eq!(spawned[1].writable, received(&[base.join("data"), base.join("cache")]));
    assert_eq!(spawned[1].fds, vec![0, 1, 2, 4]);
    assert!(!spawned[1].argv.iter().any(|arg| arg.starts_with(b"--export=")));
    assert_eq!(fake.signals().len(), 1, "only the first one was ended");
    assert!(offline.process.alive());
    offline.client.stop();
}

/// A plugin that touches `written` in the export folder each call names.
const WRITES_ITS_EXPORT: &str = r#"#!/usr/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  export=$(printf '%s' "$line" | sed -E 's/.*"export":"([^"]*)".*/\1/')
  wrote=false; touch "$export/written" 2>/dev/null && wrote=true
  printf '{"jsonrpc":"2.0","id":%s,"result":{"wrote":%s}}\n' "$id" "$wrote"
done
"#;

#[test]
fn a_documents_portal_folder_goes_over_as_its_descriptor_and_is_named_by_it() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let mut fixture = Fixture::new();
    let base = fixture.base();
    std::fs::write(fixture.paths.folder.join("plugin"), WRITES_ITS_EXPORT).unwrap();
    let chosen = base.join("doc/a1b2c3/Music");
    std::fs::create_dir_all(&chosen).unwrap();
    fixture.paths.export = Some(chosen.clone());
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone()).with_documents(vec![base.join("doc")]);
    let launched = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()).unwrap();
    let told = chilpa_dirs::own_descriptors().join("3");
    assert_eq!(launched.export, Some(told.clone()), "the plugin reaches it through the descriptor it holds");
    let spawned = fake.spawned().remove(0);
    assert_eq!(spawned.writable, received(&[base.join("data"), base.join("cache")]), "the portal is not asked to mount what it cannot");
    assert_eq!(spawned.fds, vec![0, 1, 2, 3, 4]);
    assert!(spawned.argv.contains(&byte_string(format!("--export={}", told.display()))));
    assert_eq!(launched.client.call("write", json!({"export": told}), Duration::from_secs(5)).unwrap(), json!({"wrote": true}));
    assert!(chosen.join("written").is_file(), "the write landed in the chosen folder");
    launched.client.stop();
}

#[test]
fn a_documents_portal_folder_is_never_handed_over_on_a_kernel_that_cannot_confine_the_plugin() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let mut fixture = Fixture::new();
    let base = fixture.base();
    let chosen = base.join("doc/a1b2c3/Music");
    std::fs::create_dir_all(&chosen).unwrap();
    fixture.paths.export = Some(chosen);
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone()).with_documents(vec![base.join("doc")]).without_confinement();
    assert!(matches!(flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()), Err(BridgeError::Denied)));
    assert!(fake.spawned().is_empty(), "nothing was started");
    // A folder the portal mounts needs no confinement.
    std::fs::create_dir(base.join("export")).unwrap();
    fixture.paths.export = Some(base.join("export"));
    let launched = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()).unwrap();
    launched.client.stop();
}

#[test]
fn an_export_folder_the_plugin_could_not_write_is_refused_before_anything_starts() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let mut fixture = Fixture::new();
    let base = fixture.base();
    let sealed = base.join("sealed");
    std::fs::create_dir(&sealed).unwrap();
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o555)).unwrap();
    fixture.paths.export = Some(sealed.clone());
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    let result = flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet());
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
    if unsafe { libc::geteuid() } == 0 { return; }
    assert!(matches!(result, Err(BridgeError::Denied)));
    assert!(fake.spawned().is_empty(), "nothing was started");
}

#[test]
fn a_start_its_runner_refuses_fails_at_launch() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    fake.refuse_starts();
    let fixture = Fixture::new();
    let flatpak = Flatpak::on(bus.bus(), fixture.runner.clone());
    let start = std::time::Instant::now();
    assert!(matches!(flatpak.launch(&fixture.paths, "test", MEMORY, false, quiet()), Err(BridgeError::Disconnected)),
        "no ready line, no plugin: the launch fails instead of handing over a channel to nothing");
    assert!(start.elapsed() < Duration::from_secs(5), "its output closing is seen at once, not at the deadline");
    assert_eq!(fake.spawned().len(), 1);
    assert!(fake.running().is_empty(), "what was spawned is gone");
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

const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";

/// Adds `folder` to the Documents portal for this application, as the file
/// chooser does with a folder outside the sandbox. Returns the document and
/// the path this sandbox has for it, canonical, as an approved export folder
/// is recorded.
fn documents_add(bus: &zbus::blocking::Connection, folder: &Path) -> (String, PathBuf) {
    const DIRECTORY: u32 = 8;
    // The Documents portal takes an O_PATH descriptor only without
    // O_NOFOLLOW, and grants writing only through one.
    let name = CString::new(folder.as_os_str().as_bytes()).unwrap();
    let raw = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    assert!(raw >= 0, "{} opens", folder.display());
    let held = unsafe { OwnedFd::from_raw_fd(raw) };
    let reply = bus.call_method(Some(DOCUMENTS), DOCUMENTS_PATH, Some(DOCUMENTS), "AddFull",
        &(vec![zbus::zvariant::Fd::from(held.as_fd())], DIRECTORY, "", Vec::<&str>::new())).unwrap();
    let (ids, _): (Vec<String>, HashMap<String, zbus::zvariant::OwnedValue>) = reply.body().deserialize().unwrap();
    let mount: Vec<u8> = bus.call_method(Some(DOCUMENTS), DOCUMENTS_PATH, Some(DOCUMENTS), "GetMountPoint", &())
        .unwrap().body().deserialize().unwrap();
    let mount = PathBuf::from(OsStr::from_bytes(mount.strip_suffix(&[0]).unwrap_or(&mount)));
    let path = mount.join(&ids[0]).join(folder.file_name().unwrap()).canonicalize().unwrap();
    (ids[0].clone(), path)
}

fn documents_delete(bus: &zbus::blocking::Connection, id: &str) {
    bus.call_method(Some(DOCUMENTS), DOCUMENTS_PATH, Some(DOCUMENTS), "Delete", &(id,)).unwrap();
}

/// A plugin that touches `written` in the export folder each call names and
/// tries to list what lies two folders above it, Chilpa's side of the
/// Documents portal.
const WRITES_AND_LOOKS_ABOVE: &str = r#"#!/usr/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  export=$(printf '%s' "$line" | sed -E 's/.*"export":"([^"]*)".*/\1/')
  wrote=false; touch "$export/written" 2>/dev/null && wrote=true
  above=false; ls "$export/../.." >/dev/null 2>&1 && above=true
  printf '{"jsonrpc":"2.0","id":%s,"result":{"wrote":%s,"above":%s}}\n' "$id" "$wrote" "$above"
done
"#;

/// The Landlock version this kernel offers, if any.
fn landlock() -> Option<i64> {
    let version = unsafe { libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<u8>(), 0usize, 1u32) };
    (version > 0).then_some(version)
}

/// An export folder chosen through the file chooser, through the real
/// portals: the plugin writes there while the grant lasts and cannot walk
/// out of it, and every way it cannot write is a start that fails or a
/// write that is refused, never a plugin told of a folder it cannot reach.
/// Without Landlock 3 the runner cannot confine it, and nothing starts. Run
/// inside the Chilpa Flatpak as the smoke above is.
#[test]
#[ignore = "needs the real Flatpak and Documents portals: run it inside the Chilpa Flatpak"]
fn a_documents_folder_takes_the_plugin_s_writes_until_it_is_revoked_through_the_portal() {
    assert!(chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak, "this smoke runs inside the Chilpa Flatpak");
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).expect("XDG_CACHE_HOME names the application's cache");
    let root = tempfile::tempdir_in(cache).unwrap();
    let base = root.path().canonicalize().unwrap();
    let folder = base.join("plugin");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("plugin"), WRITES_AND_LOOKS_ABOVE).unwrap();
    std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let bus = zbus::blocking::Connection::session().unwrap();
    let chosen = base.join("chosen");
    std::fs::create_dir(&chosen).unwrap();
    let (id, path) = documents_add(&bus, &chosen);
    let mut paths = NativePaths { folder, data: base.join("data"), cache: base.join("cache"), export: Some(path), private: Vec::new() };
    let flatpak = Flatpak::default();

    if landlock().is_some_and(|version| version >= 3) {
        let launched = flatpak.launch(&paths, "test", MEMORY, false, quiet()).unwrap();
        let told = launched.export.clone().expect("an approved export folder is named");
        assert_eq!(told, chilpa_dirs::own_descriptors().join("3"));
        assert_eq!(launched.client.call("write", json!({"export": told}), Duration::from_secs(5)).unwrap(), json!({"wrote": true, "above": false}),
            "it writes in the folder, and the Documents portal's other folders are out of its reach");
        assert!(chosen.join("written").is_file(), "the write landed in the chosen folder");
        std::fs::remove_file(chosen.join("written")).unwrap();
        // Revoked while the plugin runs: its next write is refused.
        documents_delete(&bus, &id);
        assert_eq!(launched.client.call("write", json!({"export": told}), Duration::from_secs(5)).unwrap()["wrote"], json!(false));
        assert!(!chosen.join("written").exists());
        launched.client.stop();
    } else {
        assert!(matches!(flatpak.launch(&paths, "test", MEMORY, false, quiet()), Err(BridgeError::Disconnected)),
            "a plugin the runner cannot keep inside the folder is not started");
        assert!(!chosen.join("written").exists());
        documents_delete(&bus, &id);
    }
    // Revoked before a start: the folder is not there, and nothing starts.
    assert!(flatpak.launch(&paths, "test", MEMORY, false, quiet()).is_err());

    // A folder the plugin could not write: refused before anything starts.
    let sealed = base.join("sealed");
    std::fs::create_dir(&sealed).unwrap();
    let (sealed_id, sealed_path) = documents_add(&bus, &sealed);
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o555)).unwrap();
    paths.export = Some(sealed_path);
    let refused = flatpak.launch(&paths, "test", MEMORY, false, quiet());
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
    documents_delete(&bus, &sealed_id);
    assert!(matches!(refused, Err(BridgeError::Denied)), "a read-only folder is refused at launch");

    // A folder the portal skips: this sandbox's own /tmp, which the host does
    // not have at that path. The runner finds it missing and the launch fails.
    let skipped = tempfile::tempdir_in("/tmp").unwrap();
    paths.export = Some(skipped.path().canonicalize().unwrap());
    assert!(matches!(flatpak.launch(&paths, "test", MEMORY, false, quiet()), Err(BridgeError::Disconnected)),
        "a folder the portal did not mount fails the start instead of reaching the plugin as a missing path");
}
