//! The runner against itself: the limits it sets are the ones the program
//! it becomes reads, and they hold.
#![cfg(target_os = "linux")]

use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Output};

const RUNNER: &str = env!("CARGO_BIN_EXE_chilpa-plugin-runner");

fn run(args: &[&str]) -> Output {
    Command::new(RUNNER).args(args).output().unwrap()
}

/// The soft and hard columns of one row of /proc/self/limits.
fn row(limits: &str, name: &str) -> (String, String) {
    let line = limits.lines().find(|line| line.starts_with(name)).unwrap_or_else(|| panic!("no {name} in {limits}"));
    let mut columns = line[name.len()..].split_whitespace();
    (columns.next().unwrap().to_string(), columns.next().unwrap().to_string())
}

#[test]
fn each_limit_reaches_the_program_it_becomes() {
    let output = run(&["--as=1073741824", "--nofile=64", "--core=0", "--nproc=4096", "--fsize=1048576", "--cpu=120:130",
        "--", "/bin/cat", "/proc/self/limits"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let limits = String::from_utf8(output.stdout).unwrap();
    for (name, soft, hard) in [("Max address space", "1073741824", "1073741824"), ("Max open files", "64", "64"),
        ("Max core file size", "0", "0"), ("Max processes", "4096", "4096"), ("Max file size", "1048576", "1048576"),
        ("Max cpu time", "120", "130")] {
        assert_eq!(row(&limits, name), (soft.to_string(), hard.to_string()), "{name}");
    }
}

#[test]
fn a_write_past_the_file_size_limit_is_stopped() {
    let file = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("runner-fsize-{}", std::process::id()));
    let output = run(&["--fsize=4096", "--", "/bin/sh", "-c", "head -c 8192 /dev/zero > \"$1\"", "sh", file.to_str().unwrap()]);
    let written = std::fs::metadata(&file).map(|metadata| metadata.len());
    let _ = std::fs::remove_file(&file);
    assert!(!output.status.success(), "the write past the limit ended its writer");
    assert_eq!(written.unwrap(), 4096);
}

#[test]
fn a_descriptor_its_starter_left_open_does_not_reach_the_plugin() {
    let probe = "[ -e /proc/self/fd/9 ] && echo open || echo closed";
    let file = std::fs::File::open("/dev/null").unwrap();
    let raw = file.as_raw_fd();
    let leaked = |command: &mut Command| -> String {
        // dup2 leaves the copy without close-on-exec, as a careless starter would.
        unsafe { command.pre_exec(move || if libc::dup2(raw, 9) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }); }
        String::from_utf8(command.output().unwrap().stdout).unwrap().trim().to_string()
    };
    assert_eq!(leaked(Command::new("/bin/sh").args(["-c", probe])), "open", "the probe sees a leaked descriptor");
    assert_eq!(leaked(Command::new(RUNNER).args(["--nofile=64", "--", "/bin/sh", "-c", probe])), "closed");
}

#[test]
fn a_bad_command_line_or_a_missing_program_starts_nothing() {
    for (args, code) in [(&["--as=lots", "--", "/bin/true"][..], 2), (&["--nofile=8"][..], 2), (&["--as=8", "--"][..], 2),
        (&["--nofile=8", "--", "/nonexistent/plugin"][..], 127)] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(code), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("chilpa-plugin-runner: "), "{args:?}");
    }
}
