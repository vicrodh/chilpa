//! Sets a process plugin's resource limits and environment, runs it, and
//! ends whatever it leaves behind before exiting itself.
//!
//! `chilpa-plugin-runner [--as=N] [--nofile=N] [--core=N] [--nproc=N]
//!  [--fsize=N] [--cpu=SOFT[:HARD]] [--keep-env=NAME[,NAME]...]
//!  [--export=PATH] [--ready] -- PROGRAM [ARGS]...`
//!
//! The Flatpak backend starts every process plugin through it, from the
//! application's `libexec` folder, because the portal that starts the
//! sandbox has no resource limits of its own and stops answering for a
//! process once it exits. Descriptors above the standard three are closed
//! when the program starts, but for descriptor 3 with `--export`, and only
//! the variables `--keep-env` names reach it.
//!
//! Exit status: the program's, or 128 plus the signal that ended it or the
//! runner. When the program never started: 2 for an invalid command line,
//! 126 when a limit, the descriptors, the export folder or the runner's own
//! setup could not be set, 127 when the program could not be started. With
//! `--ready`, nothing is written on standard output in the first two cases.

#[cfg(target_os = "linux")]
fn main() {
    use chilpa_plugin_runner::Resource;
    use std::os::unix::process::CommandExt;
    let plan = chilpa_plugin_runner::plan(std::env::args_os().skip(1)).unwrap_or_else(|problem| fail(2, &problem));
    if let Err(error) = chilpa_plugin_runner::seal_descriptors() {
        fail(126, &format!("could not close the inherited descriptors: {error}"));
    }
    if let Err(error) = chilpa_plugin_runner::adopt_orphans() {
        fail(126, &format!("could not adopt what the plugin leaves behind: {error}"));
    }
    // The process count is the program's alone: the runner still has it to start.
    let (own, program): (Vec<_>, Vec<_>) = plan.limits.iter().cloned().partition(|limit| limit.resource != Resource::Processes);
    if let Err((resource, error)) = chilpa_plugin_runner::apply(&own).and_then(|()| chilpa_plugin_runner::settable(&program)) {
        fail(126, &format!("could not set --{}: {error}", resource.option()));
    }
    let export = plan.export.is_some();
    if let Some(path) = &plan.export {
        if let Err(problem) = chilpa_plugin_runner::check_export(chilpa_plugin_runner::EXPORT_DESCRIPTOR, std::path::Path::new(path)) {
            fail(126, &format!("the export folder {} is out of the plugin's reach: {problem}", path.to_string_lossy()));
        }
    }
    if let Err(error) = chilpa_plugin_runner::hold_signals() {
        fail(126, &format!("could not hold its signals: {error}"));
    }
    let mut command = std::process::Command::new(&plan.program);
    command.args(&plan.args).env_clear().envs(chilpa_plugin_runner::environment(&plan.keep));
    // What the runner was started with leaves the plugin's reach before the plugin exists.
    if let Err(error) = chilpa_plugin_runner::conceal(export) {
        fail(126, &format!("could not hide what it was started with: {error}"));
    }
    let runner = std::process::id() as i32;
    unsafe {
        command.pre_exec(move || {
            chilpa_plugin_runner::release_signals()?;
            chilpa_plugin_runner::die_with(runner)?;
            if export { chilpa_plugin_runner::hand_over_export()?; }
            chilpa_plugin_runner::apply(&program).map_err(|(_, error)| error)
        });
    }
    // Before the program exists, so nothing it writes comes ahead of it.
    if plan.ready {
        if let Err(error) = chilpa_plugin_runner::announce() {
            fail(126, &format!("could not say it is ready: {error}"));
        }
    }
    let started = command.spawn()
        .unwrap_or_else(|error| fail(127, &format!("could not start {}: {error}", plan.program.to_string_lossy())));
    chilpa_plugin_runner::release_stdio();
    std::process::exit(chilpa_plugin_runner::supervise(started.id() as i32));
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
