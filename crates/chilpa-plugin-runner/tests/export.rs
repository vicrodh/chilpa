//! The runner's export folder and ready line, against the runner itself: the
//! plugin reaches the folder held at descriptor 3 through the path it is
//! told and through nothing else, a path that is not that folder starts
//! nothing, and the ready line comes only once the plugin's program runs.
#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RUNNER: &str = env!("CARGO_BIN_EXE_chilpa-plugin-runner");
/// Where the runner is told to write its ready line in these tests.
const READY: libc::c_int = 4;

/// A folder of its own under the test's temporary folder.
fn folder(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("runner-export-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// What a run printed, how it ended, and what came on its ready descriptor.
struct Run {
    output: Output,
    ready: Vec<u8>,
}

impl Run {
    fn errors(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }
}

/// Runs `command` with `held` at descriptor 3, as the Flatpak backend hands
/// the export folder over, and a pipe at descriptor 4 for the ready line.
fn run_with(mut command: Command, held: Option<&Path>) -> Run {
    // Copies far above 3 and 4, so placing one never overwrites the other.
    let high = |fd: libc::c_int| -> std::os::fd::OwnedFd {
        let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 100) };
        assert!(copy >= 100);
        unsafe { std::os::fd::FromRawFd::from_raw_fd(copy) }
    };
    let file = held.map(|held| high(std::fs::File::open(held).unwrap().as_raw_fd()));
    let raw = file.as_ref().map(|file| file.as_raw_fd());
    let (mut reader, piped) = std::io::pipe().unwrap();
    let writer = high(piped.as_raw_fd());
    drop(piped);
    let ready = writer.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            for (fd, number) in raw.map(|raw| (raw, 3)).into_iter().chain([(ready, READY)]) {
                if fd != number && libc::dup2(fd, number) < 0 { return Err(std::io::Error::last_os_error()); }
                if libc::fcntl(number, libc::F_SETFD, 0) < 0 { return Err(std::io::Error::last_os_error()); }
            }
            Ok(())
        });
    }
    let output = command.output().unwrap();
    drop(writer);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    Run { output, ready: bytes }
}

fn runner(args: &[&str]) -> Command {
    let mut command = Command::new(RUNNER);
    command.args(args);
    command
}

#[test]
fn a_mounted_export_folder_is_reached_by_its_path_and_its_descriptor_never_reaches_the_plugin() {
    let chosen = folder("chosen");
    let path = chosen.display().to_string();
    let run = run_with(runner(&[&format!("--export={path}"), "--", "/bin/sh", "-c",
        "touch \"$1/by-path\" && sh -c 'touch \"$1/by-a-child\"' sh \"$1\" && { [ -e /proc/self/fd/3 ] && echo open || echo closed; }", "sh", &path]),
        Some(&chosen));
    assert!(run.output.status.success(), "{}", run.errors());
    assert!(chosen.join("by-path").is_file() && chosen.join("by-a-child").is_file());
    assert_eq!(String::from_utf8_lossy(&run.output.stdout).trim(), "closed",
        "a descriptor resolved from walks out of the folder with `..`: the plugin is not given it");
    std::fs::remove_dir_all(&chosen).unwrap();
}

/// The Landlock version this kernel offers, if any.
fn landlock() -> Option<i32> {
    let version = unsafe { libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<u8>(), 0usize, 1u32) };
    (version > 0).then_some(version as i32)
}

/// Bubblewrap's options for a root of its own that shows this system's
/// programs, libraries and their loader's cache in /etc, and nothing of the
/// test's folders.
fn own_root() -> Command {
    let mut command = Command::new("/usr/bin/bwrap");
    command.args(["--unshare-all", "--die-with-parent", "--ro-bind", "/usr", "/usr", "--ro-bind", "/etc", "/etc",
        "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
    for top in ["/bin", "/sbin", "/lib", "/lib64"] {
        match std::fs::read_link(top) {
            Ok(target) => { command.arg("--symlink").arg(target).arg(top); }
            Err(_) if Path::new(top).is_dir() => { command.args(["--ro-bind", top, top]); }
            Err(_) => {}
        }
    }
    command.args(["--ro-bind", RUNNER, "/runner"]);
    command
}

#[test]
fn a_plugin_given_the_export_folder_s_descriptor_cannot_walk_out_of_it() {
    assert!(Path::new("/usr/bin/bwrap").is_file(), "this test gives the runner a root of its own with bubblewrap");
    let base = folder("walk");
    let (chosen, outside) = (base.join("chosen"), base.join("outside"));
    for dir in [&chosen, &outside] { std::fs::create_dir(dir).unwrap(); }
    std::fs::write(outside.join("secret"), "outside the export folder").unwrap();
    // The descriptor is the plugin's only way to the test's folders: its
    // parent, here and in Chilpa's sandbox, is one `..` away.
    let probe = r#"
touch /proc/self/fd/3/written && echo wrote
cat /proc/self/fd/3/../outside/secret 2>/dev/null && echo read-outside
ls /proc/self/fd/3/.. >/dev/null 2>&1 && echo listed-outside
touch /proc/self/fd/3/../outside/planted 2>/dev/null && echo wrote-outside
cd /proc/self/fd/3/../.. 2>/dev/null && ls >/dev/null 2>&1 && echo listed-above
true"#;
    let mut command = own_root();
    command.args(["--", "/runner", "--export=/proc/self/fd/3", "--ready=4", "--", "/bin/sh", "-c", probe]);
    let run = run_with(command, Some(&chosen));
    let said = String::from_utf8_lossy(&run.output.stdout).into_owned();
    assert!(!said.contains("outside") && !said.contains("above"), "the plugin got out of the folder: {said}");
    assert!(!outside.join("planted").exists());
    match landlock() {
        Some(version) if version >= 3 => {
            assert!(run.output.status.success(), "{}", run.errors());
            assert_eq!(said.trim(), "wrote", "the folder itself is the plugin's");
            assert_eq!(run.ready, b"\n");
        }
        _ => {
            assert_eq!(run.output.status.code(), Some(126), "{}", run.errors());
            assert!(run.errors().starts_with("chilpa-plugin-runner: the export folder reaches the plugin as a descriptor, and the plugin cannot be kept inside it: "),
                "{}", run.errors());
            assert!(said.is_empty() && !chosen.join("written").exists(), "a kernel that cannot confine it starts nothing");
            assert!(run.ready.is_empty(), "a refused start never says it is ready");
        }
    }
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn an_export_path_that_is_not_the_folder_handed_over_starts_nothing() {
    let (chosen, other) = (folder("handed"), folder("elsewhere"));
    let marker = other.join("started");
    let probe = ["--", "/bin/sh", "-c", "touch \"$1\"", "sh", marker.to_str().unwrap()];
    for export in [other.display().to_string(), other.join("absent").display().to_string()] {
        let option = format!("--export={export}");
        let args: Vec<&str> = [option.as_str(), "--ready=4"].into_iter().chain(probe).collect();
        let run = run_with(runner(&args), Some(&chosen));
        assert_eq!(run.output.status.code(), Some(126), "{export}: {}", run.errors());
        assert!(run.errors().starts_with(&format!("chilpa-plugin-runner: the export folder {export} is out of the plugin's reach: ")), "{}", run.errors());
        assert!(run.ready.is_empty(), "a refused start never says it is ready");
        assert!(!marker.exists(), "the plugin did not start");
    }
    // No descriptor 3 at all.
    let mut bare = Command::new(RUNNER);
    bare.arg(format!("--export={}", chosen.display())).args(probe);
    unsafe { bare.pre_exec(|| { libc::close(3); Ok(()) }); }
    let output = bare.output().unwrap();
    assert_eq!(output.status.code(), Some(126));
    assert!(!marker.exists());
    std::fs::remove_dir_all(&chosen).unwrap();
    std::fs::remove_dir_all(&other).unwrap();
}

#[test]
fn the_ready_line_comes_on_its_own_descriptor_once_the_program_runs() {
    let run = run_with(runner(&["--ready=4", "--", "/bin/sh", "-c", "echo from the plugin; [ -e /proc/self/fd/4 ] && echo open || echo closed"]), None);
    assert!(run.output.status.success(), "{}", run.errors());
    assert_eq!(run.ready, b"\n");
    assert_eq!(run.output.stdout, b"from the plugin\nclosed\n", "the plugin's output is its own, and the ready descriptor is not the plugin's");
    let quiet = Command::new(RUNNER).args(["--", "/bin/echo", "from the plugin"]).output().unwrap();
    assert_eq!(quiet.stdout, b"from the plugin\n");
}

#[test]
fn a_program_that_cannot_be_executed_never_gets_a_ready_line() {
    // Replaced after it was checked, as between the backend's preflight and
    // the exec: not there any more, or no longer executable.
    let base = folder("unexecutable");
    let plain = base.join("plugin");
    std::fs::write(&plain, "#!/bin/sh\necho started\n").unwrap();
    for program in [base.join("absent"), plain] {
        let run = run_with(runner(&["--ready=4", "--", program.to_str().unwrap()]), None);
        assert_eq!(run.output.status.code(), Some(127), "{}: {}", program.display(), run.errors());
        assert!(run.ready.is_empty(), "{}: the exec failed, so the start is refused, not a dead plugin", program.display());
        assert!(run.output.stdout.is_empty());
    }
    let unopened = runner(&["--ready=9", "--", "/bin/true"]).output().unwrap();
    assert_eq!(unopened.status.code(), Some(126), "a ready descriptor that is not open stops the start");
    std::fs::remove_dir_all(&base).unwrap();
}
