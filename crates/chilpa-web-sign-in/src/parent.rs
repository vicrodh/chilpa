//! The window ends with Chilpa.
//!
//! Chilpa starts the window as the leader of a process group of its own, and
//! on a cancel, after its time limit, or when the window ends, kills that
//! whole group (`web_sign_in.rs` in chilpa-api). The engine's processes stay
//! in the group: Chromium only moves a child to another group when it is
//! asked to (`new_process_group` in its `base/process/launch_posix.cc`), and
//! its launcher does not ask. What is left is Chilpa itself ending: the
//! window waits for that, on a process descriptor of its parent on Linux and
//! on a kernel event queue on macOS, and then ends the group it leads.

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

/// Watches the parent and ends the window's group when the parent ends.
/// macOS has neither process descriptors nor a parent-death signal; a parent
/// that cannot be watched leaves the window to be closed by hand.
#[cfg(target_os = "macos")]
pub fn follow() {
    // SAFETY: a plain system call on this process's own ids.
    let parent = unsafe { libc::getppid() };
    let _ = std::thread::Builder::new().name("chilpa-sign-in-parent".into()).spawn(move || {
        match ended(parent) {
            Ok(()) => end_group(),
            // The parent ended before it could be watched.
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => end_group(),
            Err(_) => {}
        }
    });
}

/// Elsewhere the engine's processes end when their connection to the window
/// closes.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn follow() {}

/// Blocks until the process ends. `ESRCH` when there is no such process,
/// which is also what a process that already ended gives.
#[cfg(target_os = "macos")]
fn ended(process: libc::pid_t) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::os::fd::{FromRawFd, OwnedFd};
    let Ok(ident) = usize::try_from(process) else { return Err(Error::from_raw_os_error(libc::ESRCH)) };
    // SAFETY: a new descriptor, owned (and closed) by `_queue` from here on.
    let queue = unsafe { libc::kqueue() };
    if queue < 0 {
        return Err(Error::last_os_error());
    }
    let _queue = unsafe { OwnedFd::from_raw_fd(queue) };
    let mut event = libc::kevent {
        ident,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD | libc::EV_ONESHOT,
        fflags: libc::NOTE_EXIT,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // With no room for events, a change the kernel refuses comes back as -1
    // and errno instead of as an event.
    // SAFETY: one change read from `event`, nothing written.
    if unsafe { libc::kevent(queue, &event, 1, std::ptr::null_mut(), 0, std::ptr::null()) } < 0 {
        return Err(Error::last_os_error());
    }
    loop {
        // SAFETY: room for one event in `event`; blocks until it comes.
        match unsafe { libc::kevent(queue, std::ptr::null(), 0, &mut event, 1, std::ptr::null()) } {
            1 if event.filter == libc::EVFILT_PROC && event.fflags & libc::NOTE_EXIT != 0 => return Ok(()),
            -1 if Error::last_os_error().kind() == ErrorKind::Interrupted => {}
            -1 => return Err(Error::last_os_error()),
            _ => {}
        }
    }
}

/// Kills the group the window leads, itself included. A window that does not
/// lead its group (started by hand from a shell's pipeline, say) ends alone,
/// so it never kills processes it did not start.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn end_group() -> ! {
    // SAFETY: signals to this process and to the group it leads.
    unsafe {
        if libc::getpgrp() == libc::getpid() {
            libc::kill(0, libc::SIGKILL);
        }
        libc::_exit(1)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::ended;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn a_watched_process_is_seen_ending() {
        let mut child = Command::new("/bin/sleep").arg("0.3").spawn().expect("sleep starts");
        let pid = libc::pid_t::try_from(child.id()).unwrap();
        let began = Instant::now();
        ended(pid).expect("the child is watched until it ends");
        assert!(began.elapsed() >= Duration::from_millis(200), "returned before the child ended: {:?}", began.elapsed());
        assert!(began.elapsed() < Duration::from_secs(5), "{:?}", began.elapsed());
        child.wait().unwrap();
    }

    #[test]
    fn a_process_that_is_gone_cannot_be_watched() {
        let mut child = Command::new("/usr/bin/true").spawn().expect("true starts");
        let pid = libc::pid_t::try_from(child.id()).unwrap();
        child.wait().unwrap();
        assert_eq!(ended(pid).map_err(|error| error.raw_os_error()), Err(Some(libc::ESRCH)));
        assert_eq!(ended(-1).map_err(|error| error.raw_os_error()), Err(Some(libc::ESRCH)));
    }
}
