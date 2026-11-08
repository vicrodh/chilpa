//! Keeps a plugin that holds the export folder's descriptor inside that
//! folder and its own sandbox.
//!
//! A folder the portal cannot mount reaches the plugin only as descriptor 3,
//! a directory opened in Chilpa's sandbox. A path resolved from it walks
//! that sandbox's tree, not the plugin's: `/proc/self/fd/3/..` is the
//! folder's parent there, and from it everything Chilpa can reach. No mount
//! can be made inside a Flatpak to cut that tree, so the plugin is confined
//! by what the kernel lets an unprivileged process do to itself:
//!
//! - Landlock, version 3 or later: every file access is refused unless the
//!   file lies beneath the plugin's own root or beneath the export folder.
//!   Version 3 is the first that also covers truncating a file by its path.
//! - A seccomp filter: no UNIX socket can be created, so no socket of
//!   Chilpa's sandbox (its session bus, its display, its sound server) can be
//!   reached by path, whatever Landlock version, and no directory outside
//!   can be watched with inotify. A pair of connected sockets is still made
//!   with `socketpair`.
//!
//! Without Landlock 3 the plugin is not started. What neither covers, a
//! known limit (packaging/flatpak/BUILD-NOTES.md, section 10): a path
//! outside the folder that the plugin guesses can still be looked up, its
//! metadata read, and its mode, times and extended attributes changed;
//! listing, reading, writing, creating, removing, renaming, truncating and
//! running anything there is refused.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

/// The first Landlock version that bounds `truncate` by path.
pub const MIN_LANDLOCK: i32 = 3;

const CREATE_RULESET_VERSION: u32 = 1;
const RULE_PATH_BENEATH: libc::c_int = 1;

/// The file access rights a Landlock `version` knows. Each is handled, and
/// granted only beneath the two folders.
pub fn handled_access(version: i32) -> u64 {
    // EXECUTE through MAKE_SYM, version 1.
    let mut rights = (1u64 << 13) - 1;
    if version >= 2 { rights |= 1 << 13; } // REFER
    if version >= 3 { rights |= 1 << 14; } // TRUNCATE
    if version >= 5 { rights |= 1 << 15; } // IOCTL_DEV
    if version >= 9 { rights |= 1 << 16; } // RESOLVE_UNIX
    rights
}

/// The Landlock version this kernel offers, or why there is none.
pub fn landlock_version() -> Result<i32, io::Error> {
    let version = unsafe {
        libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<u8>(), 0usize, CREATE_RULESET_VERSION)
    };
    if version < 0 { Err(io::Error::last_os_error()) } else { Ok(version as i32) }
}

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
}

#[repr(C, packed)]
struct PathBeneath {
    allowed_access: u64,
    parent_fd: i32,
}

/// The ruleset and the filter, made before the plugin is started and
/// applied between fork and exec.
pub struct Confinement {
    ruleset: OwnedFd,
    filter: Vec<libc::sock_filter>,
}

impl Confinement {
    /// For a plugin whose export folder is `export`, a descriptor it keeps.
    /// Fails when the kernel cannot confine it: the plugin must then not
    /// start.
    pub fn beneath(export: libc::c_int) -> Result<Self, String> {
        let version = landlock_version().map_err(|error| format!("this kernel has no Landlock ({error})"))?;
        if version < MIN_LANDLOCK {
            return Err(format!("this kernel's Landlock is version {version}, and {MIN_LANDLOCK} is needed"));
        }
        let filter = filter().ok_or("no seccomp filter is written for this architecture")?;
        let rights = handled_access(version);
        let attr = RulesetAttr { handled_access_fs: rights };
        let raw = unsafe {
            libc::syscall(libc::SYS_landlock_create_ruleset, &attr as *const RulesetAttr, std::mem::size_of::<RulesetAttr>(), 0u32)
        };
        if raw < 0 { return Err(format!("the Landlock ruleset cannot be made: {}", io::Error::last_os_error())); }
        let ruleset = unsafe { OwnedFd::from_raw_fd(raw as libc::c_int) };
        let root = unsafe { libc::open(c"/".as_ptr(), libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if root < 0 { return Err(format!("its own root cannot be opened: {}", io::Error::last_os_error())); }
        let root = unsafe { OwnedFd::from_raw_fd(root) };
        for (folder, name) in [(root.as_raw_fd(), "its own root"), (export, "the export folder")] {
            let rule = PathBeneath { allowed_access: rights, parent_fd: folder };
            let added = unsafe {
                libc::syscall(libc::SYS_landlock_add_rule, ruleset.as_raw_fd(), RULE_PATH_BENEATH, &rule as *const PathBeneath, 0u32)
            };
            if added < 0 { return Err(format!("{name} cannot be granted: {}", io::Error::last_os_error())); }
        }
        Ok(Self { ruleset, filter })
    }

    /// Applies both to this process, between fork and exec, after every
    /// other step of the plugin's setup. Async-signal-safe.
    pub fn enforce(&self) -> io::Result<()> {
        let no_new_privileges = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong) };
        if no_new_privileges != 0 { return Err(io::Error::last_os_error()); }
        if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, self.ruleset.as_raw_fd(), 0u32) } != 0 {
            return Err(io::Error::last_os_error());
        }
        install(&self.filter)
    }
}

/// Installs `filter` on this process. Async-signal-safe.
pub fn install(filter: &[libc::sock_filter]) -> io::Result<()> {
    let program = libc::sock_fprog { len: filter.len() as libc::c_ushort, filter: filter.as_ptr() as *mut libc::sock_filter };
    let installed = unsafe {
        libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER as libc::c_ulong, &program as *const libc::sock_fprog as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong)
    };
    if installed != 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

// Classic BPF, as seccomp reads it.
const LOAD_WORD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS, the first two being 0
const JUMP_EQUAL: u16 = 0x05 | 0x10; // BPF_JMP | BPF_JEQ | BPF_K
#[cfg(target_arch = "x86_64")]
const JUMP_AT_LEAST: u16 = 0x05 | 0x30; // BPF_JMP | BPF_JGE | BPF_K
const RETURN: u16 = 0x06; // BPF_RET | BPF_K
// Offsets in struct seccomp_data; the low half of the first argument on a
// little-endian machine.
const NUMBER: u32 = 0;
const ARCHITECTURE: u32 = 4;
const FIRST_ARGUMENT: u32 = 16;

#[cfg(target_arch = "x86_64")]
const NATIVE: u32 = 0xc000_003e; // AUDIT_ARCH_X86_64
#[cfg(target_arch = "aarch64")]
const NATIVE: u32 = 0xc000_00b7; // AUDIT_ARCH_AARCH64

fn statement(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt: 0, jf: 0, k }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

/// The filter: `socket(AF_UNIX, ..)` and `inotify_add_watch` answer EACCES;
/// a call made through another ABI of this machine, such as `int 0x80` on
/// x86-64, answers ENOSYS; everything else is allowed.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub fn filter() -> Option<Vec<libc::sock_filter>> {
    let refuse = libc::SECCOMP_RET_ERRNO | libc::EACCES as u32;
    let foreign = libc::SECCOMP_RET_ERRNO | libc::ENOSYS as u32;
    let mut program = vec![
        statement(LOAD_WORD, ARCHITECTURE),
        jump(JUMP_EQUAL, NATIVE, 1, 0),
        statement(RETURN, foreign),
        statement(LOAD_WORD, NUMBER),
    ];
    // The x32 ABI shares the architecture and sets this bit in the number.
    #[cfg(target_arch = "x86_64")]
    program.extend([jump(JUMP_AT_LEAST, 0x4000_0000, 0, 1), statement(RETURN, foreign)]);
    program.extend([
        jump(JUMP_EQUAL, libc::SYS_inotify_add_watch as u32, 0, 1),
        statement(RETURN, refuse),
        jump(JUMP_EQUAL, libc::SYS_socket as u32, 0, 3),
        statement(LOAD_WORD, FIRST_ARGUMENT),
        jump(JUMP_EQUAL, libc::AF_UNIX as u32, 0, 1),
        statement(RETURN, refuse),
        statement(RETURN, libc::SECCOMP_RET_ALLOW),
    ]);
    Some(program)
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn filter() -> Option<Vec<libc::sock_filter>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `probe` in a child that installed the filter; its exit code.
    fn filtered(probe: fn() -> i32) -> i32 {
        let program = filter().expect("a filter for this architecture");
        match unsafe { libc::fork() } {
            0 => {
                let code = if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong) } != 0
                    || install(&program).is_err() { 100 } else { probe() };
                unsafe { libc::_exit(code) }
            }
            child => {
                assert!(child > 0, "fork");
                let mut status = 0;
                assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
                assert!(libc::WIFEXITED(status), "the probe exited by itself: {status}");
                libc::WEXITSTATUS(status)
            }
        }
    }

    /// 0 when the call failed with EACCES, 1 when it succeeded, 2 otherwise.
    fn outcome(result: libc::c_int) -> i32 {
        if result >= 0 { return 1; }
        if io::Error::last_os_error().raw_os_error() == Some(libc::EACCES) { 0 } else { 2 }
    }

    #[test]
    fn the_filter_refuses_unix_sockets_and_watches_but_not_pairs_or_the_network() {
        assert_eq!(filtered(|| outcome(unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) })), 0, "no UNIX socket");
        assert_eq!(filtered(|| outcome(unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) })), 0);
        assert_eq!(filtered(|| {
            let watcher = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if watcher < 0 { return 2; }
            outcome(unsafe { libc::inotify_add_watch(watcher, c"/".as_ptr(), libc::IN_CREATE) })
        }), 0, "no inotify watch");
        assert_eq!(filtered(|| {
            let mut pair = [0; 2];
            outcome(unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, pair.as_mut_ptr()) })
        }), 1, "a connected pair is still made");
        assert_eq!(filtered(|| outcome(unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) })), 1, "the network is the sandbox's to give");
        assert_eq!(filtered(|| outcome(unsafe { libc::getpid() })), 1);
    }

    #[test]
    fn each_landlock_version_handles_the_rights_it_knows() {
        assert_eq!(handled_access(1), 0x1fff);
        assert_eq!(handled_access(2), 0x3fff);
        assert_eq!(handled_access(3), 0x7fff, "truncate from version 3");
        assert_eq!(handled_access(4), 0x7fff);
        assert_eq!(handled_access(5), 0xffff, "device ioctls from version 5");
        assert_eq!(handled_access(9), 0x1ffff, "pathname UNIX sockets from version 9");
        assert_eq!(std::mem::size_of::<PathBeneath>(), 12, "the kernel's packed layout");
    }

    #[test]
    fn without_landlock_3_nothing_is_confined_and_it_says_why() {
        let file = std::fs::File::open("/").unwrap();
        match landlock_version() {
            Ok(version) if version >= MIN_LANDLOCK => assert!(Confinement::beneath(file.as_raw_fd()).is_ok()),
            Ok(version) => assert_eq!(Confinement::beneath(file.as_raw_fd()).err(),
                Some(format!("this kernel's Landlock is version {version}, and {MIN_LANDLOCK} is needed"))),
            Err(_) => assert!(Confinement::beneath(file.as_raw_fd()).err().is_some_and(|problem| problem.starts_with("this kernel has no Landlock ("))),
        }
    }
}
