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
        // Number 9 without close-on-exec, as a careless starter leaves it. The
        // flag is cleared by hand: when the file already is 9, dup2 does nothing.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(raw, 9) < 0 || libc::fcntl(9, libc::F_SETFD, 0) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
            });
        }
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

#[test]
fn only_the_variables_it_is_told_to_keep_reach_the_plugin() {
    let probe = r#"printf '%s %s' "${LEAKED-unset}" "${MARK-unset}""#;
    let shown = |args: &[&str]| -> String {
        let mut command = Command::new(RUNNER);
        command.args(args);
        for (name, value) in [("LEAKED", "from-the-starter"), ("MARK", "set")] { command.env(name, value); }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(shown(&["--keep-env=MARK", "--", "/bin/sh", "-c", probe]), "unset set");
    assert_eq!(shown(&["--", "/bin/sh", "-c", probe]), "unset unset", "nothing is kept unless named");
}

/// True while `pid` names a process that has not been reaped.
fn exists(pid: libc::pid_t) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Ends `pid` if a failed check left it behind.
fn end(pid: libc::pid_t) {
    if exists(pid) { unsafe { libc::kill(pid, libc::SIGKILL); } }
}

#[test]
fn nothing_the_plugin_leaves_behind_outlives_it() {
    // The sleeper gets a session of its own, then the shell exits at once.
    let output = run(&["--", "/bin/sh", "-c", "setsid sleep 30 </dev/null >/dev/null 2>&1 & echo $!"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let left: libc::pid_t = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    let survived = exists(left);
    end(left);
    assert!(!survived, "the runner ended what the plugin left before it exited itself");
}

#[test]
fn the_plugin_s_output_ends_when_the_plugin_closes_it() {
    use std::io::Read;
    let start = std::time::Instant::now();
    let mut runner = Command::new(RUNNER).args(["--", "/bin/sh", "-c", "exec >&-; sleep 2"])
        .stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut output = Vec::new();
    runner.stdout.take().unwrap().read_to_end(&mut output).unwrap();
    let closed = start.elapsed();
    let _ = runner.wait();
    assert!(closed < std::time::Duration::from_secs(1), "the runner holds no copy of the pipe; it closed after {closed:?}");
}

#[test]
fn ending_the_runner_ends_what_the_plugin_started() {
    use std::io::{BufRead, BufReader};
    let mut runner = Command::new(RUNNER).args(["--", "/bin/sh", "-c", "sleep 30 & echo $!; wait"])
        .stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(runner.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let started: libc::pid_t = line.trim().parse().unwrap();
    assert!(exists(started));
    unsafe { libc::kill(runner.id() as libc::pid_t, libc::SIGTERM); }
    let status = runner.wait().unwrap();
    let survived = exists(started);
    end(started);
    assert!(!survived, "a SIGTERM to the runner, as the portal's SIGINT when Chilpa leaves the bus, took the plugin's own children");
    assert!(!status.success());
}

#[test]
fn what_the_runner_was_started_with_stays_out_of_the_plugin_s_reach() {
    // The plugin runs as the runner's own user, so the runner's environment
    // and descriptors are one /proc away unless the runner hides them.
    let probe = r#"n=$(tr '\0' '\n' < /proc/$PPID/environ 2>/dev/null | grep -c '^LEAKED='); f=$(ls /proc/$PPID/fd 2>/dev/null | grep -cx 9); echo "$n $f""#;
    let file = std::fs::File::open("/dev/null").unwrap();
    let raw = file.as_raw_fd();
    let mut command = Command::new(RUNNER);
    command.args(["--", "/bin/sh", "-c", probe]);
    command.env("LEAKED", "from-the-starter");
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(raw, 9) < 0 || libc::fcntl(9, libc::F_SETFD, 0) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
        });
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "0 0", "neither the variable nor descriptor 9 is readable through the runner");
}

/// Starts the runner over `script` and reads the first line it prints.
fn started(args: &[&str], script: &str) -> (std::process::Child, String) {
    use std::io::{BufRead, BufReader};
    let mut runner = Command::new(RUNNER).args(args).args(["--", "/bin/sh", "-c", script])
        .stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(runner.stdout.take().unwrap()).read_line(&mut line).unwrap();
    (runner, line.trim().to_string())
}

/// True while `pid` names a process that runs: not reaped, and not a zombie.
fn running(pid: libc::pid_t) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
        .and_then(|stat| stat.rsplit_once(')').map(|(_, rest)| !rest.trim_start().starts_with('Z')))
        .unwrap_or(false)
}

#[test]
fn a_plugin_asked_to_end_hears_it_and_gets_a_moment_first() {
    let mark = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("runner-asked-{}", std::process::id()));
    let _ = std::fs::remove_file(&mark);
    let script = format!(r#"trap 'echo ended > "{}"; exit 0' TERM; echo ready; while :; do sleep 0.05; done"#, mark.display());
    let (mut runner, _) = started(&[], &script);
    unsafe { libc::kill(runner.id() as libc::pid_t, libc::SIGTERM); }
    runner.wait().unwrap();
    let ended = std::fs::read_to_string(&mark).unwrap_or_default();
    let _ = std::fs::remove_file(&mark);
    assert_eq!(ended.trim(), "ended", "the plugin got the SIGTERM before anything was killed");
}

#[test]
fn a_plugin_that_does_not_end_when_asked_is_killed_after_a_moment() {
    let (mut runner, line) = started(&[], "trap '' TERM; echo $$; exec sleep 30");
    let plugin: libc::pid_t = line.parse().unwrap();
    let start = std::time::Instant::now();
    unsafe { libc::kill(runner.id() as libc::pid_t, libc::SIGTERM); }
    runner.wait().unwrap();
    let waited = start.elapsed();
    let survived = running(plugin);
    end(plugin);
    assert!(!survived, "after the grace it was killed");
    assert!(waited >= std::time::Duration::from_millis(1500) && waited < std::time::Duration::from_secs(5), "it was given {waited:?}");
}
