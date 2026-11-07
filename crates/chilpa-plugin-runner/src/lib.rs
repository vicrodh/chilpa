//! What the runner does before it becomes a process plugin, for a sandbox
//! that cannot bound the process itself: the Flatpak portal has no option
//! for resource limits.
//!
//! The options are prlimit's, `--as=N` or `--cpu=SOFT:HARD`, so a command
//! line moves from one to the other unchanged. A single number sets the soft
//! and the hard limit, and the plugin cannot raise either.
#![cfg(target_os = "linux")]

use std::ffi::OsString;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limit {
    pub resource: Resource,
    pub soft: u64,
    pub hard: u64,
}

/// A command line, read: the limits, then the program and its arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub limits: Vec<Limit>,
    pub program: OsString,
    pub args: Vec<OsString>,
}

/// Reads `[--LIMIT=N[:H]]... -- PROGRAM [ARGS]...`. Everything after the
/// first `--` belongs to the program, another `--` included.
pub fn plan(args: impl IntoIterator<Item = OsString>) -> Result<Plan, String> {
    let mut args = args.into_iter();
    let mut limits: Vec<Limit> = Vec::new();
    loop {
        let Some(arg) = args.next() else { return Err("no -- before the program".into()) };
        if arg == "--" { break; }
        let text = arg.to_str().ok_or_else(|| format!("{} is not an option", arg.to_string_lossy()))?;
        let (name, value) = text.strip_prefix("--").and_then(|option| option.split_once('='))
            .ok_or_else(|| format!("{text} is not an option"))?;
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
    Ok(Plan { limits, program, args: args.collect() })
}

/// Sets each limit on this process, which the program it becomes inherits.
pub fn apply(limits: &[Limit]) -> Result<(), (Resource, std::io::Error)> {
    for limit in limits {
        let value = libc::rlimit { rlim_cur: limit.soft as libc::rlim_t, rlim_max: limit.hard as libc::rlim_t };
        let set = unsafe {
            match limit.resource {
                Resource::AddressSpace => libc::setrlimit(libc::RLIMIT_AS, &value),
                Resource::Descriptors => libc::setrlimit(libc::RLIMIT_NOFILE, &value),
                Resource::CoreSize => libc::setrlimit(libc::RLIMIT_CORE, &value),
                Resource::Processes => libc::setrlimit(libc::RLIMIT_NPROC, &value),
                Resource::FileSize => libc::setrlimit(libc::RLIMIT_FSIZE, &value),
                Resource::CpuSeconds => libc::setrlimit(libc::RLIMIT_CPU, &value),
            }
        };
        if set != 0 { return Err((limit.resource, std::io::Error::last_os_error())); }
    }
    Ok(())
}

/// Marks every descriptor above the standard three to close when the program
/// starts, so a plugin holds nothing its starter left open by mistake.
pub fn seal_descriptors() -> std::io::Result<()> {
    let first: libc::c_uint = 3;
    let sealed = unsafe { libc::syscall(libc::SYS_close_range, first, libc::c_uint::MAX, libc::CLOSE_RANGE_CLOEXEC) };
    if sealed == 0 { return Ok(()); }
    // Kernels older than 5.11 lack the flag: one at a time, up to the soft
    // limit, where every open descriptor sits.
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 { return Err(std::io::Error::last_os_error()); }
    for fd in first as libc::c_int..limit.rlim_cur.min(1 << 20) as libc::c_int {
        // A number with no descriptor behind it answers EBADF, which is fine.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(args: &[&str]) -> Result<Plan, String> {
        plan(args.iter().map(OsString::from))
    }

    #[test]
    fn a_command_line_reads_as_prlimit_reads_it() {
        let plan = read(&["--as=1024", "--cpu=120:130", "--", "/plugin/plugin", "--", "-x"]).unwrap();
        assert_eq!(plan.limits, vec![Limit { resource: Resource::AddressSpace, soft: 1024, hard: 1024 },
            Limit { resource: Resource::CpuSeconds, soft: 120, hard: 130 }]);
        assert_eq!(plan.program, "/plugin/plugin");
        assert_eq!(plan.args, vec![OsString::from("--"), OsString::from("-x")], "what follows the program is its own");
        assert_eq!(read(&["--", "/bin/true"]).unwrap().limits, Vec::new());
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
        ] {
            assert_eq!(read(args), Err(problem.to_string()), "{args:?}");
        }
    }
}
