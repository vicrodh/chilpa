//! The runner's export folder and ready line, against the runner itself: the
//! plugin reaches the folder held at descriptor 3 through the path it is
//! told, and a path that is not that folder starts nothing.
#![cfg(target_os = "linux")]

use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RUNNER: &str = env!("CARGO_BIN_EXE_chilpa-plugin-runner");

/// A folder of its own under the test's temporary folder.
fn folder(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("runner-export-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Runs the runner with `held` at descriptor 3, as the Flatpak backend hands
/// the export folder over.
fn run_holding(held: &Path, args: &[&str]) -> Output {
    let file = std::fs::File::open(held).unwrap();
    let raw = file.as_raw_fd();
    let mut command = Command::new(RUNNER);
    command.args(args);
    unsafe {
        command.pre_exec(move || {
            if raw != 3 && libc::dup2(raw, 3) < 0 { return Err(std::io::Error::last_os_error()); }
            if libc::fcntl(3, libc::F_SETFD, 0) < 0 { return Err(std::io::Error::last_os_error()); }
            Ok(())
        });
    }
    command.output().unwrap()
}

#[test]
fn the_plugin_writes_into_the_export_folder_through_the_path_it_is_told() {
    let chosen = folder("chosen");
    let by_descriptor = run_holding(&chosen, &["--export=/proc/self/fd/3", "--",
        "/bin/sh", "-c", "touch /proc/self/fd/3/written && sh -c 'touch /proc/self/fd/3/by-a-child'"]);
    assert!(by_descriptor.status.success(), "{}", String::from_utf8_lossy(&by_descriptor.stderr));
    assert!(chosen.join("written").is_file() && chosen.join("by-a-child").is_file(), "its descriptor stays open for it and its children");
    let path = chosen.display().to_string();
    let by_path = run_holding(&chosen, &[&format!("--export={path}"), "--", "/bin/sh", "-c", "touch \"$1/by-path\"", "sh", &path]);
    assert!(by_path.status.success(), "{}", String::from_utf8_lossy(&by_path.stderr));
    assert!(chosen.join("by-path").is_file());
    std::fs::remove_dir_all(&chosen).unwrap();
}

#[test]
fn an_export_path_that_is_not_the_folder_handed_over_starts_nothing() {
    let (chosen, other) = (folder("handed"), folder("elsewhere"));
    let marker = other.join("started");
    let probe = ["--", "/bin/sh", "-c", "touch \"$1\"", "sh", marker.to_str().unwrap()];
    for export in [other.display().to_string(), other.join("absent").display().to_string()] {
        let option = format!("--export={export}");
        let args: Vec<&str> = [option.as_str(), "--ready"].into_iter().chain(probe).collect();
        let output = run_holding(&chosen, &args);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(126), "{export}: {errors}");
        assert!(errors.starts_with(&format!("chilpa-plugin-runner: the export folder {export} is out of the plugin's reach: ")), "{errors}");
        assert!(output.stdout.is_empty(), "a refused start never says it is ready");
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
fn the_ready_line_comes_before_anything_the_plugin_writes() {
    let output = Command::new(RUNNER).args(["--ready", "--", "/bin/echo", "from the plugin"]).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\nfrom the plugin\n");
    let quiet = Command::new(RUNNER).args(["--", "/bin/echo", "from the plugin"]).output().unwrap();
    assert_eq!(quiet.stdout, b"from the plugin\n", "without --ready the output is the plugin's alone");
}
