//! The window ends with Chilpa.
//!
//! Chilpa starts the window as the leader of a process group of its own, and
//! on a cancel, after its time limit, or when the window ends, kills that
//! whole group (`web_sign_in.rs` in chilpa-api). The engine's processes stay
//! in the group: Chromium only moves a child to another group when it is
//! asked to (`new_process_group` in its `base/process/launch_posix.cc`), and
//! its launcher does not ask. What is left is Chilpa itself ending: on Linux
//! the window waits for that on a process descriptor of its parent and then
//! ends the group it leads.

/// Watches the parent and ends the window's group when the parent ends.
#[cfg(target_os = "linux")]
pub fn follow() {
    // SAFETY: plain system calls on this process's own ids.
    let parent = unsafe { libc::getppid() };
    let watched = unsafe { libc::syscall(libc::SYS_pidfd_open, parent, 0) };
    let Ok(watched) = libc::c_int::try_from(watched) else { return };
    if watched < 0 {
        // A kernel older than 5.3, or a filter that refuses the call: the
        // kernel's parent-death signal then ends the window alone, and the
        // engine's processes end when their connection to it closes.
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
        return;
    }
    let started = std::thread::Builder::new().name("chilpa-sign-in-parent".into()).spawn(move || {
        let mut wait = libc::pollfd { fd: watched, events: libc::POLLIN, revents: 0 };
        // SAFETY: one descriptor this thread owns; poll blocks until the parent ends.
        while unsafe { libc::poll(&mut wait, 1, -1) } < 0 {
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
        }
        end_group();
    });
    if started.is_err() {
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
    }
}

/// Elsewhere the engine's processes end when their connection to the window
/// closes.
#[cfg(not(target_os = "linux"))]
pub fn follow() {}

/// Kills the group the window leads, itself included. A window that does not
/// lead its group (started by hand from a shell's pipeline, say) ends alone,
/// so it never kills processes it did not start.
#[cfg(target_os = "linux")]
fn end_group() -> ! {
    // SAFETY: signals to this process and to the group it leads.
    unsafe {
        if libc::getpgrp() == libc::getpid() {
            libc::kill(0, libc::SIGKILL);
        }
        libc::_exit(1)
    }
}
