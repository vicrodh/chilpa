//! Sets a process plugin's resource limits and environment, runs it, and
//! ends whatever it leaves behind before exiting itself.
//!
//! `chilpa-plugin-runner [--as=N] [--nofile=N] [--core=N] [--nproc=N]
//!  [--fsize=N] [--cpu=SOFT[:HARD]] [--keep-env=NAME[,NAME]...]
//!  [--export=PATH] [--ready=FD] -- PROGRAM [ARGS]...`
//!
//! The Flatpak backend starts every process plugin through it, from the
//! application's `libexec` folder, because the portal that starts the
//! sandbox has no resource limits of its own and stops answering for a
//! process once it exits. Descriptors above the standard three are closed
//! when the program starts, but for descriptor 3 when the program is told
//! `--export=/proc/self/fd/3`, and only the variables `--keep-env` names
//! reach it.
//!
//! Exit status: the program's, or 128 plus the signal that ended it or the
//! runner. When the program never started: 2 for an invalid command line,
//! 126 when a limit, the descriptors, the export folder, its confinement or
//! the runner's own setup could not be set, 127 when the program could not
//! be executed. With `--ready=FD`, nothing is written on FD in any of these
//! cases.

#[cfg(target_os = "linux")]
fn main() {
    use chilpa_plugin_runner::{confine::Confinement, Resource, EXPORT_DESCRIPTOR};
    use std::os::unix::process::CommandExt;
    let plan = chilpa_plugin_runner::plan(std::env::args_os().skip(1)).unwrap_or_else(|problem| fail(2, &problem));
    if let Err(error) = chilpa_plugin_runner::seal_descriptors() {
        fail(126, &format!("could not close the inherited descriptors: {error}"));
    }
    if let Some(fd) = plan.ready.filter(|fd| !chilpa_plugin_runner::is_open(*fd)) {
        fail(126, &format!("descriptor {fd}, for the ready line, is not open"));
    }
    if let Err(error) = chilpa_plugin_runner::adopt_orphans() {
        fail(126, &format!("could not adopt what the plugin leaves behind: {error}"));
    }
    // The process count is the program's alone: the runner still has it to start.
    let (own, program): (Vec<_>, Vec<_>) = plan.limits.iter().cloned().partition(|limit| limit.resource != Resource::Processes);
    if let Err((resource, error)) = chilpa_plugin_runner::apply(&own).and_then(|()| chilpa_plugin_runner::settable(&program)) {
        fail(126, &format!("could not set --{}: {error}", resource.option()));
    }
    let mut handed = false;
    if let Some(path) = &plan.export {
        let path = std::path::Path::new(path);
        if let Err(problem) = chilpa_plugin_runner::check_export(EXPORT_DESCRIPTOR, path) {
            fail(126, &format!("the export folder {} is out of the plugin's reach: {problem}", path.display()));
        }
        handed = chilpa_plugin_runner::through_descriptor(path);
    }
    if let Err(error) = chilpa_plugin_runner::hold_signals() {
        fail(126, &format!("could not hold its signals: {error}"));
    }
    let mut command = std::process::Command::new(&plan.program);
    command.args(&plan.args).env_clear().envs(chilpa_plugin_runner::environment(&plan.keep));
    // What the runner was started with leaves the plugin's reach before the
    // plugin exists. A mounted export folder's descriptor goes with it.
    let keep: Vec<libc::c_int> = handed.then_some(EXPORT_DESCRIPTOR).into_iter().chain(plan.ready).collect();
    if let Err(error) = chilpa_plugin_runner::conceal(&keep) {
        fail(126, &format!("could not hide what it was started with: {error}"));
    }
    // A plugin that keeps the descriptor would walk out of the folder through it.
    let confinement = handed.then(|| Confinement::beneath(EXPORT_DESCRIPTOR).unwrap_or_else(|problem| {
        fail(126, &format!("the export folder reaches the plugin as a descriptor, and the plugin cannot be kept inside it: {problem}"))
    }));
    let runner = std::process::id() as i32;
    unsafe {
        command.pre_exec(move || {
            chilpa_plugin_runner::release_signals()?;
            chilpa_plugin_runner::die_with(runner)?;
            if handed { chilpa_plugin_runner::hand_over_export()?; }
            chilpa_plugin_runner::apply(&program).map_err(|(_, error)| error)?;
            // Last, so nothing above runs confined.
            confinement.as_ref().map_or(Ok(()), Confinement::enforce)
        });
    }
    let started = command.spawn()
        .unwrap_or_else(|error| fail(127, &format!("could not start {}: {error}", plan.program.to_string_lossy())));
    let plugin = started.id() as i32;
    // Its ruleset and the descriptors kept for it are the plugin's now.
    drop(command);
    if let Some(fd) = plan.ready {
        if let Err(error) = chilpa_plugin_runner::announce(fd) {
            eprintln!("chilpa-plugin-runner: could not say the plugin started, so it is ended: {error}");
            unsafe { libc::kill(plugin, libc::SIGKILL); }
        }
    }
    if handed { unsafe { libc::close(EXPORT_DESCRIPTOR); } }
    chilpa_plugin_runner::release_stdio();
    std::process::exit(chilpa_plugin_runner::supervise(plugin));
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
