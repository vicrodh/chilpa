//! The disposable children that prove a backend presents frames before the
//! real window uses it, ported from QBZ (`renderer_preflight_linux.rs` and
//! the preflight half of `renderer_qt.rs`).
//!
//! A child is this same executable started with a marker in its
//! environment: the window's `main` sees the marker before anything else
//! (before the single-instance check, the facade, audio or plugins), opens
//! a tiny probe window and prints a proof line when frames went out. Its
//! failures, a compositor that kills the connection included, end only the
//! child.

use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const AUTO_CHILD_ENV: &str = "CHILPA_INTERNAL_AUTO_PREFLIGHT_CHILD";
const AUTO_PROOF: &str = "CHILPA_AUTO_PREFLIGHT_OK";
const GPU_CHILD_ENV: &str = "CHILPA_INTERNAL_GPU_PREFLIGHT_CHILD";
const GPU_IDENTITY_ENV: &str = "CHILPA_INTERNAL_GPU_PREFLIGHT_IDENTITY";
const GPU_NAME_ENV: &str = "CHILPA_INTERNAL_GPU_PREFLIGHT_NAME";
const GPU_PROOF: &str = "CHILPA_GPU_PREFLIGHT_OK=";
pub(super) const GPU_NOT_FOUND: i32 = 41;
pub(super) const GPU_NOT_SELECTABLE: i32 = 42;
/// How long a child has before it is killed and reaped.
const WATCHDOG: Duration = Duration::from_secs(8);

static SOFTWARE_NOTICE: AtomicBool = AtomicBool::new(false);

pub(super) fn take_software_notice() -> bool {
    SOFTWARE_NOTICE.swap(false, Ordering::SeqCst)
}

fn marked(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value.to_string_lossy().trim() == "1")
}

/// This process is the managed-Auto child.
pub fn auto_child_requested() -> bool {
    marked(AUTO_CHILD_ENV)
}

/// This process is the saved-GPU child.
pub fn gpu_child_requested() -> bool {
    marked(GPU_CHILD_ENV)
}

/// This process is one of the children, which never open the log of their
/// own nor touch anything of the parent's.
pub fn child_requested() -> bool {
    auto_child_requested() || gpu_child_requested()
}

/// The managed-Auto child's half: `present` opens the probe window on Qt's
/// default backend (or on software, which the parent set) and answers 0
/// when three frames went out.
pub fn run_auto_child(present: impl FnOnce() -> i32) -> i32 {
    std::env::remove_var(AUTO_CHILD_ENV);
    let result = present();
    if result == 0 {
        print_proof(AUTO_PROOF);
    }
    result
}

/// The device the saved-GPU child was asked to prove, taken out of the
/// environment so nothing it starts inherits it.
pub(super) fn take_gpu_child_target() -> (String, String) {
    let identity = std::env::var(GPU_IDENTITY_ENV).unwrap_or_default();
    let name = std::env::var(GPU_NAME_ENV).unwrap_or_default();
    for variable in [GPU_CHILD_ENV, GPU_IDENTITY_ENV, GPU_NAME_ENV] {
        std::env::remove_var(variable);
    }
    (name, identity)
}

fn print_proof(line: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

pub(super) fn print_gpu_proof(identity: &str) {
    print_proof(&format!("{GPU_PROOF}{identity}"));
}

/// The identity the GPU child proved, from its whole output line: a line
/// that only contains the marker is not a proof.
pub(super) fn approved_identity(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .find_map(|line| line.strip_prefix(GPU_PROOF))
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_string)
}

/// The last lines a failed child said, for the log.
pub(super) fn output_tail(output: &Output) -> String {
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(6)..].join(" | ");
    tail.chars().take(1200).collect()
}

/// Runs a child under the watchdog. Its output goes to anonymous temporary
/// files, not pipes: a driver that talks a lot must not block the child
/// before it can report. Past the budget the child is killed and reaped;
/// the second value says it was.
fn run_child(command: &mut Command, budget: Duration) -> Result<(Output, bool), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut stdout = tempfile::tempfile().map_err(|e| format!("child stdout: {e}"))?;
    let mut stderr = tempfile::tempfile().map_err(|e| format!("child stderr: {e}"))?;
    let out = stdout.try_clone().map_err(|e| e.to_string())?;
    let err = stderr.try_clone().map_err(|e| e.to_string())?;
    // A desktop's activation token is single-use: the invisible probe must
    // never consume the one meant to raise the real window.
    for variable in chilpa_dirs::activation_variables() {
        command.env_remove(variable);
    }
    let started = std::time::Instant::now();
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|error| format!("cannot spawn the child: {error}"))?;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < budget => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot watch the child: {error}"));
            }
        }
    }
    let status = child.wait().map_err(|error| format!("cannot collect the child: {error}"))?;
    fn tail(file: &mut std::fs::File) -> Result<Vec<u8>, String> {
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(65536))).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(65536).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
    Ok((Output { status, stdout: tail(&mut stdout)?, stderr: tail(&mut stderr)? }, timed_out))
}

fn this_executable() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("cannot resolve the executable: {error}"))
}

pub(super) fn spawn_gpu_child(name: &str, identity: &str) -> Result<(Output, bool), String> {
    let mut command = Command::new(this_executable()?);
    command.env(GPU_CHILD_ENV, "1").env(GPU_IDENTITY_ENV, identity).env(GPU_NAME_ENV, name);
    run_child(&mut command, WATCHDOG)
}

/// The managed-Auto child: no arguments (a link belongs to the parent), the
/// parent's environment, and software when asked.
fn auto_command(executable: &std::path::Path, software: bool) -> Command {
    let mut command = Command::new(executable);
    command.env(AUTO_CHILD_ENV, "1");
    if software {
        command.env(super::QUICK_ENV, "software");
    }
    command
}

fn auto_proof(result: &(Output, bool)) -> Result<(), String> {
    let (output, timed_out) = result;
    if *timed_out {
        return Err("the child went past the 8-second watchdog (killed and reaped)".into());
    }
    if output.status.success() && String::from_utf8_lossy(&output.stdout).lines().any(|line| line == AUTO_PROOF) {
        return Ok(());
    }
    Err(format!("child status {}; presentation not proved; {}", output.status, output_tail(output)))
}

/// Auto, then software once at most. A child that cannot be started or
/// watched is a problem of the machinery, not a proof of bad graphics: it
/// is reported and nothing is rewritten. `true` means software.
fn decide(mut probe: impl FnMut(bool) -> Result<(Output, bool), String>) -> Result<bool, String> {
    match auto_proof(&probe(false)?) {
        Ok(()) => Ok(false),
        Err(reason) => {
            eprintln!("renderer: Auto presentation failed: {reason}; trying software once");
            auto_proof(&probe(true)?).map_err(|reason| format!("software presentation also failed: {reason}"))?;
            Ok(true)
        }
    }
}

/// A backend already chosen by the environment, the renderer row included
/// (it has set its variable by now).
fn external_backend(qsg: &str, quick: &str, chosen: &str) -> bool {
    !qsg.trim().is_empty() || !quick.trim().is_empty() || (!chosen.trim().is_empty() && !chosen.trim().eq_ignore_ascii_case("auto"))
}

/// Managed Auto, after the saved GPU and the renderer preference and before
/// the facade, audio or the application object. When Qt's default cannot
/// present here and software can, this launch alone runs in software: no
/// sentinel and no preference changes, so a driver that recovers gets
/// another Auto attempt at the next start. `Err` is a launch that should
/// not go on: not even software presents.
pub fn auto_at_boot() -> Result<(), String> {
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    if external_backend(&env(super::RHI_ENV), &env(super::QUICK_ENV), &env(super::RENDERER_ENV)) {
        eprintln!("renderer: an explicit backend policy is kept; managed Auto probe skipped");
        return Ok(());
    }
    if super::gpu::any_approved() {
        // The window will use the device a child already proved.
        return Ok(());
    }
    if matches!(env("QT_QPA_PLATFORM").trim().split(':').next(), Some("offscreen" | "minimal" | "vnc")) {
        eprintln!("renderer: no desktop platform; managed presentation probe skipped");
        return Ok(());
    }
    let executable = this_executable()?;
    let start = std::time::Instant::now();
    let software = decide(|software| {
        let started = std::time::Instant::now();
        let result = run_child(&mut auto_command(&executable, software), WATCHDOG);
        eprintln!(
            "renderer: {} presentation probe done in {} ms",
            if software { "software" } else { "Auto" },
            started.elapsed().as_millis()
        );
        result
    })?;
    if software {
        std::env::set_var(super::QUICK_ENV, "software");
        SOFTWARE_NOTICE.store(true, Ordering::SeqCst);
        eprintln!("renderer: software presentation proved; rescue for this launch only ({} ms)", start.elapsed().as_millis());
    } else {
        eprintln!("renderer: Auto presentation proved; Qt's default backend kept ({} ms)", start.elapsed().as_millis());
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn outcome(code: i32, marker: bool, timeout: bool) -> (Output, bool) {
        let stdout = if marker { format!("{AUTO_PROOF}\n").into_bytes() } else { Vec::new() };
        (Output { status: std::process::ExitStatus::from_raw(code), stdout, stderr: Vec::new() }, timeout)
    }

    #[test]
    fn auto_failure_and_its_retry_are_this_launch_only_and_finite() {
        let mut attempts = Vec::new();
        assert!(!decide(|soft| {
            attempts.push(soft);
            Ok(outcome(0, true, false))
        })
        .unwrap());
        assert_eq!(attempts, [false], "a healthy Auto needs one child");
        for (code, marker, timeout) in [(6, false, false), (0, false, false), (0, true, true)] {
            attempts.clear();
            assert!(decide(|soft| {
                attempts.push(soft);
                Ok(if soft { outcome(0, true, false) } else { outcome(code, marker, timeout) })
            })
            .unwrap());
            assert_eq!(attempts, [false, true]);
        }
        attempts.clear();
        assert!(decide(|soft| {
            attempts.push(soft);
            Ok(outcome(6, false, false))
        })
        .is_err());
        assert_eq!(attempts, [false, true], "software is tried once, never more");
    }

    #[test]
    fn an_explicit_policy_ignores_empty_and_whitespace_values() {
        for value in ["", " ", "\t"] {
            assert!(!external_backend(value, value, value));
            assert!(!external_backend(value, value, " Auto "));
        }
        for (qsg, quick, chosen) in [("opengl", "", ""), ("", " software ", ""), ("", "", "gpu")] {
            assert!(external_backend(qsg, quick, chosen));
        }
    }

    #[test]
    fn a_spawn_failure_is_reported_without_a_second_child() {
        let mut attempts = 0;
        let result = decide(|_| {
            attempts += 1;
            Err("fixture spawn failure".into())
        });
        assert_eq!(attempts, 1);
        assert!(result.unwrap_err().contains("spawn failure"));
        let mut missing = auto_command(std::path::Path::new("/no such chilpa executable"), false);
        assert!(run_child(&mut missing, Duration::from_millis(80)).is_err());
    }

    #[test]
    fn the_gpu_proof_needs_a_whole_line_and_an_identity() {
        assert_eq!(approved_identity(b"noise\nCHILPA_GPU_PREFLIGHT_OK=vk:device-uuid\nmore\n").as_deref(), Some("vk:device-uuid"));
        assert!(approved_identity(b"CHILPA_GPU_PREFLIGHT_OK=   \n").is_none());
        assert!(approved_identity(b"prefix CHILPA_GPU_PREFLIGHT_OK=vk:x\n").is_none());
    }

    #[test]
    fn a_child_gets_no_arguments_no_token_and_a_watchdog() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::Builder::new().prefix("chilpa preflight ").tempdir().unwrap();
        let shell = which_sh();
        let executable = dir.path().join("fixture with spaces");
        symlink(&shell, &executable).unwrap();
        let mut command = auto_command(&executable, false);
        assert_eq!(command.get_args().count(), 0, "the parent's link never reaches the child");
        let tokens = chilpa_dirs::activation_variables();
        let check = tokens.iter().map(|t| format!("test -z \"${t}\"")).collect::<Vec<_>>().join(" && ");
        command
            .env(tokens[0], "a token")
            .args(["-c", &format!("{check} || exit 9; head -c 131072 /dev/zero >&2; echo {AUTO_PROOF}")]);
        let result = run_child(&mut command, Duration::from_secs(5)).unwrap();
        assert!(auto_proof(&result).is_ok(), "{:?}", result.0.status);
        assert_eq!(result.0.stderr.len(), 65536, "only the tail is kept");

        // `exec` replaces the shell, so there is exactly one child, as with
        // the real executable; past the budget it is killed and reaped.
        let pid_path = dir.path().join("pid");
        let mut command = auto_command(&executable, false);
        command.env("FIXTURE_PID", &pid_path).args(["-c", "echo $$ > \"$FIXTURE_PID\"; exec sleep 30"]);
        let start = std::time::Instant::now();
        let result = run_child(&mut command, Duration::from_millis(200)).unwrap();
        assert!(result.1, "the watchdog fired");
        assert!(start.elapsed() < Duration::from_secs(2));
        let pid = std::fs::read_to_string(pid_path).unwrap();
        let alive = Command::new("kill").args(["-0", pid.trim()]).stderr(std::process::Stdio::null()).status().unwrap();
        assert!(!alive.success(), "the timed-out child must be reaped");
    }

    fn which_sh() -> std::path::PathBuf {
        let out = Command::new("sh").args(["-c", "command -v sh"]).output().unwrap();
        std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
    }
}
