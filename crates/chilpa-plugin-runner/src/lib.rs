//! What the runner does before it becomes a process plugin, for a sandbox
//! that cannot bound the process itself: the Flatpak portal has no option
//! for resource limits.
//!
//! The options are prlimit's, `--as=N` or `--cpu=SOFT:HARD`, so a command
//! line moves from one to the other unchanged. A single number sets the soft
//! and the hard limit, and the plugin cannot raise either. `--keep-env=A,B`
//! is the runner's own: the variables it names are the only ones the plugin
//! gets, so nothing else that reached the runner's environment goes on.
#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::io;

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

/// Sets each limit on this process, which the program it becomes inherits.
pub fn apply(limits: &[Limit]) -> Result<(), (Resource, io::Error)> {
    for limit in limits {
        let value = libc::rlimit { rlim_cur: limit.soft as libc::rlim_t, rlim_max: limit.hard as libc::rlim_t };
        if limit.resource.set(&value) != 0 {
            return Err((limit.resource, io::Error::last_os_error()));
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
}
