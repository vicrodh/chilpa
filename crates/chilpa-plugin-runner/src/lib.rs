//! What the runner does around a process plugin, for a sandbox that cannot
//! bound the process itself: the Flatpak portal has no option for resource
//! limits, and once the process it started exits it forgets it, whatever
//! that process left running.
//!
//! The options are prlimit's, `--as=N` or `--cpu=SOFT:HARD`, so a command
//! line moves from one to the other unchanged. A single number sets the soft
//! and the hard limit, and the plugin cannot raise either. `--keep-env=A,B`
//! is the runner's own: the variables it names are the only ones the plugin
//! gets, so nothing else that reached the runner's environment goes on.
//!
//! The plugin runs as the runner's child, and the runner adopts whatever the
//! plugin leaves behind (a child subreaper). Asked to end, the runner hands
//! the signal on to the plugin and gives it two seconds. When the plugin
//! exits, or that time is up, everything still below the runner is ended and
//! reaped before the runner exits, so the exit the portal reports means
//! nothing of the plugin is left. Before the plugin starts, the runner
//! closes its own descriptors, wipes its environment and becomes
//! undumpable, so the plugin, which runs as the same user, reads neither
//! through /proc. The plugin dies with the runner, should it kill the
//! runner; what the plugin started itself is then left behind, a known limit
//! (packaging/flatpak/BUILD-NOTES.md, section 10). Resources (ADR-018): the
//! runner's own
//! process, asleep on its signals; it reads the process table only while it
//! ends what is left.
#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::io;
use std::time::{Duration, Instant};

/// A resource the runner bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    AddressSpace,
    Descriptors,
    CoreSize,
    Processes,
    FileSize,
    CpuSeconds,
}

impl Resource {
    const OPTIONS: [(&'static str, Resource); 6] = [("as", Resource::AddressSpace), ("nofile", Resource::Descriptors),
        ("core", Resource::CoreSize), ("nproc", Resource::Processes), ("fsize", Resource::FileSize), ("cpu", Resource::CpuSeconds)];

    fn named(option: &str) -> Option<Self> {
        Self::OPTIONS.iter().find(|(name, _)| *name == option).map(|(_, resource)| *resource)
    }

    /// The option that sets it, without its dashes.
    pub fn option(self) -> &'static str {
        Self::OPTIONS.iter().find(|(_, resource)| *resource == self).map(|(name, _)| *name).unwrap_or("")
    }

    /// `setrlimit` for this resource.
    fn set(self, value: &libc::rlimit) -> libc::c_int {
        unsafe {
            match self {
                Resource::AddressSpace => libc::setrlimit(libc::RLIMIT_AS, value),
                Resource::Descriptors => libc::setrlimit(libc::RLIMIT_NOFILE, value),
                Resource::CoreSize => libc::setrlimit(libc::RLIMIT_CORE, value),
                Resource::Processes => libc::setrlimit(libc::RLIMIT_NPROC, value),
                Resource::FileSize => libc::setrlimit(libc::RLIMIT_FSIZE, value),
                Resource::CpuSeconds => libc::setrlimit(libc::RLIMIT_CPU, value),
            }
        }
    }

    /// `getrlimit` for this resource.
    fn get(self, value: &mut libc::rlimit) -> libc::c_int {
        unsafe {
            match self {
                Resource::AddressSpace => libc::getrlimit(libc::RLIMIT_AS, value),
                Resource::Descriptors => libc::getrlimit(libc::RLIMIT_NOFILE, value),
                Resource::CoreSize => libc::getrlimit(libc::RLIMIT_CORE, value),
                Resource::Processes => libc::getrlimit(libc::RLIMIT_NPROC, value),
                Resource::FileSize => libc::getrlimit(libc::RLIMIT_FSIZE, value),
                Resource::CpuSeconds => libc::getrlimit(libc::RLIMIT_CPU, value),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limit {
    pub resource: Resource,
    pub soft: u64,
    pub hard: u64,
}

/// A command line, read: the limits, the variables kept, then the program
/// and its arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub limits: Vec<Limit>,
    /// The only variables the plugin's environment keeps.
    pub keep: Vec<OsString>,
    pub program: OsString,
    pub args: Vec<OsString>,
}

/// The option that names the variables kept.
const KEEP: &str = "keep-env";

/// Reads `[--LIMIT=N[:H]]... [--keep-env=NAME[,NAME]...] -- PROGRAM [ARGS]...`.
/// Everything after the first `--` belongs to the program, another `--`
/// included.
pub fn plan(args: impl IntoIterator<Item = OsString>) -> Result<Plan, String> {
    let mut args = args.into_iter();
    let mut limits: Vec<Limit> = Vec::new();
    let mut keep: Option<Vec<OsString>> = None;
    loop {
        let Some(arg) = args.next() else { return Err("no -- before the program".into()) };
        if arg == "--" { break; }
        let text = arg.to_str().ok_or_else(|| format!("{} is not an option", arg.to_string_lossy()))?;
        let (name, value) = text.strip_prefix("--").and_then(|option| option.split_once('='))
            .ok_or_else(|| format!("{text} is not an option"))?;
        if name == KEEP {
            if keep.is_some() { return Err(format!("--{KEEP} is given twice")); }
            let names: Vec<&str> = value.split(',').collect();
            if names.iter().any(|name| name.is_empty() || name.contains('=')) {
                return Err(format!("--{KEEP}: {value} is not a list of variable names"));
            }
            keep = Some(names.into_iter().map(OsString::from).collect());
            continue;
        }
        let resource = Resource::named(name).ok_or_else(|| format!("--{name} is not a limit"))?;
        if limits.iter().any(|limit| limit.resource == resource) { return Err(format!("--{name} is given twice")); }
        let number = |text: &str| text.parse::<u64>().map_err(|_| format!("--{name}: {text} is not a number"));
        let (soft, hard) = match value.split_once(':') {
            Some((soft, hard)) => (number(soft)?, number(hard)?),
            None => { let both = number(value)?; (both, both) }
        };
        if soft > hard { return Err(format!("--{name}: the soft limit is above the hard one")); }
        limits.push(Limit { resource, soft, hard });
    }
    let program = args.next().ok_or("no program after --")?;
    Ok(Plan { limits, keep: keep.unwrap_or_default(), program, args: args.collect() })
}

/// The plugin's whole environment: each variable in `keep` the runner has,
/// with its value, and nothing else.
pub fn environment(keep: &[OsString]) -> Vec<(OsString, OsString)> {
    keep.iter().filter_map(|name| std::env::var_os(name).map(|value| (name.clone(), value))).collect()
}

/// Sets each limit on this process, which the program it starts inherits.
/// Async-signal-safe, so a child can call it between fork and exec.
pub fn apply(limits: &[Limit]) -> Result<(), (Resource, io::Error)> {
    for limit in limits {
        let value = libc::rlimit { rlim_cur: limit.soft as libc::rlim_t, rlim_max: limit.hard as libc::rlim_t };
        if limit.resource.set(&value) != 0 {
            return Err((limit.resource, io::Error::last_os_error()));
        }
    }
    Ok(())
}

/// Whether each limit can be set by the program's starter, checked here
/// before it starts: a hard limit is only ever lowered.
pub fn settable(limits: &[Limit]) -> Result<(), (Resource, io::Error)> {
    for limit in limits {
        let mut current = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if limit.resource.get(&mut current) != 0 {
            return Err((limit.resource, io::Error::last_os_error()));
        }
        if current.rlim_max != libc::RLIM_INFINITY && limit.hard as libc::rlim_t > current.rlim_max {
            return Err((limit.resource, io::Error::from_raw_os_error(libc::EPERM)));
        }
    }
    Ok(())
}

/// Highest descriptor walked one by one when the open ones cannot be
/// listed: Linux's default ceiling on any descriptor number.
const WALKED: u64 = 1 << 20;
/// Past this ceiling, walking every number is no longer done.
const MOST_WALKED: u64 = 1 << 24;

/// Marks every descriptor above the standard three to close when the program
/// starts, so a plugin holds nothing its starter left open by mistake.
pub fn seal_descriptors() -> io::Result<()> {
    let first: libc::c_uint = 3;
    let sealed = unsafe { libc::syscall(libc::SYS_close_range, first, libc::c_uint::MAX, libc::CLOSE_RANGE_CLOEXEC) };
    if sealed == 0 { return Ok(()); }
    seal_one_by_one()
}

/// Kernels older than 5.11 lack the flag: each open descriptor, as this
/// process's descriptor table lists them. Without that list, every number
/// up to the hard limit or Linux's default ceiling, whichever is higher,
/// since a descriptor can sit above a limit lowered after it opened.
fn seal_one_by_one() -> io::Result<()> {
    if let Ok(open) = open_descriptors() {
        return open.into_iter().filter(|fd| *fd >= 3).try_for_each(seal);
    }
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 { return Err(io::Error::last_os_error()); }
    let last = (limit.rlim_max as u64).max(WALKED);
    if last > MOST_WALKED {
        return Err(io::Error::other(format!("the open descriptors cannot be listed, and {last} are too many to walk")));
    }
    (3..last as libc::c_int).try_for_each(seal)
}

/// The numbers of this process's open descriptors.
fn open_descriptors() -> io::Result<Vec<libc::c_int>> {
    let entries = std::fs::read_dir(chilpa_dirs::own_descriptors())?;
    // Read whole before any is touched: the listing holds one of its own.
    Ok(entries.flatten().filter_map(|entry| entry.file_name().to_str()?.parse().ok()).collect())
}

/// One descriptor closes when the program starts. A number with nothing
/// behind it is fine; any other refusal is not.
fn seal(fd: libc::c_int) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    let set = if flags < 0 { flags } else { unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } };
    if set >= 0 { return Ok(()); }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EBADF) { Ok(()) } else { Err(error) }
}

/// Takes out of the plugin's reach what the runner was started with. The
/// plugin runs as the runner's own user, and would otherwise read the
/// runner's environment and reopen its descriptors through /proc: every
/// descriptor above the standard three is closed, the environment's memory
/// is wiped, and the runner becomes undumpable, which closes its /proc
/// entries to processes of its user that lack CAP_SYS_PTRACE. Called once
/// the plugin's own environment is in hand, before the plugin exists. The
/// plugin is dumpable again: exec sets that back.
pub fn conceal() -> io::Result<()> {
    close_descriptors()?;
    forget_environment();
    let undumpable = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong) };
    if undumpable == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Closes every descriptor above the standard three.
fn close_descriptors() -> io::Result<()> {
    let first: libc::c_uint = 3;
    if unsafe { libc::syscall(libc::SYS_close_range, first, libc::c_uint::MAX, 0 as libc::c_uint) } == 0 { return Ok(()); }
    // Kernels older than 5.9: each one the descriptor table lists, or every
    // number when it cannot be listed, as the seal does.
    let open = match open_descriptors() {
        Ok(open) => open,
        Err(_) => {
            let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 { return Err(io::Error::last_os_error()); }
            let last = (limit.rlim_max as u64).max(WALKED);
            if last > MOST_WALKED {
                return Err(io::Error::other(format!("the open descriptors cannot be listed, and {last} are too many to walk")));
            }
            (3..last as libc::c_int).collect()
        }
    };
    for fd in open.into_iter().filter(|fd| *fd >= 3) { unsafe { libc::close(fd); } }
    Ok(())
}

/// Wipes the strings of this process's environment where the kernel shows
/// them in /proc, then lets go of them.
fn forget_environment() {
    extern "C" {
        static mut environ: *mut *mut libc::c_char;
    }
    unsafe {
        let mut entry = std::ptr::addr_of!(environ).read();
        while !entry.is_null() && !(*entry).is_null() {
            let text = *entry;
            std::ptr::write_bytes(text, 0, libc::strlen(text));
            entry = entry.add(1);
        }
        libc::clearenv();
    }
}

/// Makes this process the one that whatever the plugin leaves behind is
/// handed to when its parent goes.
pub fn adopt_orphans() -> io::Result<()> {
    let adopted = unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong) };
    if adopted == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// The signals the runner waits on: a child's exit, and being asked to end.
fn watched() -> libc::sigset_t {
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut set);
        for signal in [libc::SIGCHLD, libc::SIGINT, libc::SIGTERM, libc::SIGHUP] { libc::sigaddset(&mut set, signal); }
    }
    set
}

/// Holds the watched signals for the runner to wait on.
pub fn hold_signals() -> io::Result<()> {
    let set = watched();
    match unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) } {
        0 => Ok(()),
        error => Err(io::Error::from_raw_os_error(error)),
    }
}

/// Gives the program the signals the runner held. Async-signal-safe, for
/// between fork and exec.
pub fn release_signals() -> io::Result<()> {
    let mut empty: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe { libc::sigemptyset(&mut empty); }
    match unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut()) } {
        0 => Ok(()),
        error => Err(io::Error::from_raw_os_error(error)),
    }
}

/// Makes the plugin die with the runner, between fork and exec: a plugin
/// that kills the runner goes with it. What the plugin started itself has no
/// such tie and is left. Async-signal-safe.
pub fn die_with(runner: libc::pid_t) -> io::Result<()> {
    let tied = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong) };
    if tied != 0 { return Err(io::Error::last_os_error()); }
    // The runner went before the tie took, so the signal would never come.
    if unsafe { libc::getppid() } != runner { return Err(io::Error::from_raw_os_error(libc::ESRCH)); }
    Ok(())
}

/// Lets go of the standard input and output the plugin was started with,
/// so its pipes close when the plugin closes them, as if the runner had
/// become it. Standard error stays, for the runner's own messages.
pub fn release_stdio() {
    let null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO] {
        if null < 0 || unsafe { libc::dup2(null, fd) } < 0 { unsafe { libc::close(fd); } }
    }
    if null > libc::STDERR_FILENO { unsafe { libc::close(null); } }
}

/// How long a plugin asked to end has to do so before it is killed.
const GRACE: Duration = Duration::from_secs(2);

/// Waits until the plugin `plugin` exits or the runner is asked to end,
/// then ends and reaps everything still below the runner. Asked to end,
/// the runner first hands the signal on to the plugin, as if it were the
/// plugin, and gives it [`GRACE`] to exit. Returns the status to exit with:
/// the plugin's, or 128 plus the signal that asked. The watched signals must
/// be held.
pub fn supervise(plugin: libc::pid_t) -> i32 {
    let set = watched();
    let code = loop {
        let signal = unsafe { libc::sigwaitinfo(&set, std::ptr::null_mut()) };
        if signal < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) { continue; }
            break 1;
        }
        if signal == libc::SIGCHLD {
            if let Some(code) = reap(plugin) { break code; }
            continue;
        }
        unsafe { libc::kill(plugin, signal); }
        wait_for(plugin, GRACE, &set);
        break 128 + signal;
    };
    end_all();
    code
}

/// Waits up to `grace` for the plugin to exit, reaping whatever exits
/// meanwhile. A second ask to end stops the wait.
fn wait_for(plugin: libc::pid_t, grace: Duration, set: &libc::sigset_t) {
    let deadline = Instant::now() + grace;
    loop {
        if reap(plugin).is_some() { return; }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() { return; }
        let timeout = libc::timespec { tv_sec: left.as_secs() as libc::time_t, tv_nsec: left.subsec_nanos() as libc::c_long };
        let signal = unsafe { libc::sigtimedwait(set, std::ptr::null_mut(), &timeout) };
        if signal > 0 && signal != libc::SIGCHLD { return; }
    }
}

/// Reaps every child that has exited; the plugin's status, if it was one.
fn reap(plugin: libc::pid_t) -> Option<i32> {
    let mut found = None;
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid <= 0 { return found; }
        if pid == plugin { found = Some(exit_code(status)); }
    }
}

fn exit_code(status: libc::c_int) -> i32 {
    if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
    else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
    else { 1 }
}

/// How long ending what is left may take before the runner gives up on it.
const END_WAIT: Duration = Duration::from_secs(5);

/// Ends everything still below this process and reaps it, until nothing is
/// left. What a child started comes here when that child goes, so each
/// round reaches the next generation.
fn end_all() {
    let me = std::process::id() as libc::pid_t;
    let left = end_all_within(END_WAIT, || children_of(me), |child| unsafe { libc::kill(child, libc::SIGKILL); },
        |flags| { let mut status = 0; unsafe { libc::waitpid(-1, &mut status, flags) } });
    if !left.is_empty() {
        eprintln!("chilpa-plugin-runner: what the plugin started did not all end within {} s: {left:?}", END_WAIT.as_secs());
    }
}

/// Each round kills what `children` lists and reaps, without waiting, what
/// has exited; a child SIGKILL does not end at once, as one held in the
/// kernel, is never waited on past `bound`. What is still listed then is
/// returned. `reap` is waitpid(-1, ..) with the flags given.
fn end_all_within(bound: Duration, children: impl Fn() -> Vec<libc::pid_t>, kill: impl Fn(libc::pid_t),
    reap: impl Fn(libc::c_int) -> libc::pid_t) -> Vec<libc::pid_t> {
    let deadline = Instant::now() + bound;
    loop {
        for child in children() { kill(child); }
        loop {
            let reaped = reap(libc::WNOHANG);
            if reaped > 0 { continue; }
            if reaped == 0 { break; }
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::EINTR) => continue,
                // No children: nothing is left.
                _ => return Vec::new(),
            }
        }
        if Instant::now() >= deadline { return children(); }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The processes whose parent is `parent`, as the process table lists them.
fn children_of(parent: libc::pid_t) -> Vec<libc::pid_t> {
    let Ok(entries) = std::fs::read_dir(chilpa_dirs::process_table()) else { return Vec::new() };
    entries.flatten().filter_map(|entry| {
        let pid: libc::pid_t = entry.file_name().to_str()?.parse().ok()?;
        let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
        // "pid (name) state ppid ...": the name may hold spaces and parentheses.
        let ppid: libc::pid_t = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
        (ppid == parent).then_some(pid)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(args: &[&str]) -> Result<Plan, String> {
        plan(args.iter().map(OsString::from))
    }

    #[test]
    fn a_command_line_reads_as_prlimit_reads_it() {
        let plan = read(&["--as=1024", "--cpu=120:130", "--keep-env=PATH,HOME", "--", "/plugin/plugin", "--", "-x"]).unwrap();
        assert_eq!(plan.limits, vec![Limit { resource: Resource::AddressSpace, soft: 1024, hard: 1024 },
            Limit { resource: Resource::CpuSeconds, soft: 120, hard: 130 }]);
        assert_eq!(plan.keep, vec![OsString::from("PATH"), OsString::from("HOME")]);
        assert_eq!(plan.program, "/plugin/plugin");
        assert_eq!(plan.args, vec![OsString::from("--"), OsString::from("-x")], "what follows the program is its own");
        let bare = read(&["--", "/bin/true"]).unwrap();
        assert_eq!((bare.limits, bare.keep), (Vec::new(), Vec::new()));
        assert_eq!(Resource::FileSize.option(), "fsize");
    }

    #[test]
    fn a_malformed_command_line_says_what_is_wrong() {
        for (args, problem) in [
            (&["--as=1024"][..], "no -- before the program"),
            (&["--as=1024", "--"][..], "no program after --"),
            (&["--colour=8", "--", "/bin/true"][..], "--colour is not a limit"),
            (&["--as", "--", "/bin/true"][..], "--as is not an option"),
            (&["--as=lots", "--", "/bin/true"][..], "--as: lots is not a number"),
            (&["--as=8", "--as=9", "--", "/bin/true"][..], "--as is given twice"),
            (&["--cpu=9:8", "--", "/bin/true"][..], "--cpu: the soft limit is above the hard one"),
            (&["--keep-env=PATH", "--keep-env=HOME", "--", "/bin/true"][..], "--keep-env is given twice"),
            (&["--keep-env=PATH,,HOME", "--", "/bin/true"][..], "--keep-env: PATH,,HOME is not a list of variable names"),
        ] {
            assert_eq!(read(args), Err(problem.to_string()), "{args:?}");
        }
    }

    #[test]
    fn a_hard_limit_above_the_current_one_is_refused_before_anything_starts() {
        let mut current = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NPROC, &mut current) }, 0);
        let lower = Limit { resource: Resource::Processes, soft: 1, hard: 1 };
        assert!(settable(&[lower]).is_ok());
        if current.rlim_max != libc::RLIM_INFINITY {
            let higher = Limit { resource: Resource::Processes, soft: 1, hard: current.rlim_max as u64 + 1 };
            assert_eq!(settable(&[higher]).map_err(|(resource, _)| resource), Err(Resource::Processes));
        }
    }

    #[test]
    fn the_fallback_seals_a_descriptor_above_the_soft_limit() {
        use std::os::fd::AsRawFd;
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) }, 0);
        let fd = limit.rlim_cur.saturating_sub(1).min(4000) as libc::c_int;
        assert!(fd >= 128, "the soft limit leaves room for a high descriptor");
        // A descriptor opened before its starter lowered the soft limit under
        // it, which a loop up to that limit never reaches.
        let file = std::fs::File::open("/dev/null").unwrap();
        assert_eq!(unsafe { libc::dup2(file.as_raw_fd(), fd) }, fd, "dup2 leaves the copy without close-on-exec");
        let lowered = libc::rlimit { rlim_cur: fd as libc::rlim_t / 2, rlim_max: limit.rlim_max };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) }, 0);
        let sealed = seal_one_by_one();
        let restored = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        unsafe { libc::close(fd); }
        assert_eq!(restored, 0);
        sealed.unwrap();
        assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0, "descriptor {fd}, above the soft limit, closes when the plugin starts");
    }

    #[test]
    fn ending_what_is_left_never_waits_past_its_deadline() {
        // A child SIGKILL does not end in time, as one held in the kernel:
        // waiting for it to be reaped would block for as long as it is held.
        let stuck = 4_000_000;
        let start = Instant::now();
        let left = end_all_within(Duration::from_millis(200), || vec![stuck], |_| {}, |flags| {
            if flags & libc::WNOHANG == 0 { std::thread::sleep(Duration::from_secs(5)); stuck } else { 0 }
        });
        assert!(start.elapsed() < Duration::from_secs(1), "it waited {:?}", start.elapsed());
        assert_eq!(left, vec![stuck], "what is left at the deadline is said");
    }
}
