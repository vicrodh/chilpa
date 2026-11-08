//! Sets a process plugin's resource limits, then becomes the plugin.
//!
//! `chilpa-plugin-runner [--as=N] [--nofile=N] [--core=N] [--nproc=N]
//!  [--fsize=N] [--cpu=SOFT[:HARD]] [--keep-env=NAME[,NAME]...] -- PROGRAM [ARGS]...`
//!
//! The Flatpak backend starts every process plugin through it, from the
//! application's `libexec` folder, because the portal that starts the
//! sandbox has no resource limits of its own. Descriptors above the
//! standard three are closed when the program starts, and only the
//! variables `--keep-env` names reach it.
//!
//! Exit status when the program never started: 2 for an invalid command
//! line, 126 when a limit or the descriptors could not be set, 127 when the
//! program could not be started.

#[cfg(target_os = "linux")]
fn main() {
    use std::os::unix::process::CommandExt;
    let plan = chilpa_plugin_runner::plan(std::env::args_os().skip(1)).unwrap_or_else(|problem| fail(2, &problem));
    if let Err(error) = chilpa_plugin_runner::seal_descriptors() {
        fail(126, &format!("could not close the inherited descriptors: {error}"));
    }
    if let Err((resource, error)) = chilpa_plugin_runner::apply(&plan.limits) {
        fail(126, &format!("could not set --{}: {error}", resource.option()));
    }
    let error = std::process::Command::new(&plan.program).args(&plan.args).env_clear()
        .envs(chilpa_plugin_runner::environment(&plan.keep)).exec();
    fail(127, &format!("could not start {}: {error}", plan.program.to_string_lossy()));
}

#[cfg(target_os = "linux")]
fn fail(code: i32, problem: &str) -> ! {
    eprintln!("chilpa-plugin-runner: {problem}");
    std::process::exit(code)
}

/// Only the Linux Flatpak backend starts plugins through it.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("chilpa-plugin-runner: this system starts process plugins without it");
    std::process::exit(2)
}
