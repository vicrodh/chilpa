//! The thread that feeds the device goes ahead of everything else.
//!
//! Between a block arriving and the device running dry there is a third of
//! a second at most, and a thread of ordinary priority is held back for
//! longer than that behind a build or a scan on the same machine: that is
//! how a DAC ran dry at DSD256 with the music sitting right there in
//! memory. The sound servers run their threads in real time for this
//! reason, through RealtimeKit; a program that writes to the card itself
//! has to ask for the same.
//!
//! Asked for once per thread, the first time it opens a device. First the
//! scheduler directly, which works where the user may (the audio group,
//! or a limit set for them); then RealtimeKit on the system bus, which
//! grants it to any desktop user and kills a real-time thread that stops
//! blocking, so the limit it asks for is set first; and failing both, a
//! higher ordinary priority from RealtimeKit. Nothing here can fail the
//! device: what was not granted is written down and the music plays as it
//! did.

use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
use dbus::blocking::{Connection, Proxy};
use std::cell::Cell;
use std::time::Duration;

/// The priority asked for, within what RealtimeKit allows (20 by default).
const PRIORITY: u32 = 20;
/// The ordinary priority asked for when real time is refused: the same
/// RealtimeKit gives a sound server's clients.
const NICE: i32 = -11;

/// What the thread got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevation {
    /// Real time, at that priority, straight from the scheduler.
    Scheduled(u32),
    /// Real time, at that priority, granted by RealtimeKit.
    Granted(u32),
    /// Not real time, but ahead of the rest, at that nice level.
    Nicer(i32),
    /// Nothing: as it was.
    AsItWas,
}

thread_local! {
    /// What the calling thread holds now, if anything.
    static HELD: Cell<Option<Elevation>> = const { Cell::new(None) };
}

/// Puts the calling thread ahead while it feeds a device. Asked twice, it
/// says what the first time got.
pub fn elevate() -> Elevation {
    if let Some(got) = HELD.get() {
        return got;
    }
    let started = std::time::Instant::now();
    let got = scheduled().or_else(granted).or_else(nicer).unwrap_or(Elevation::AsItWas);
    HELD.set(Some(got));
    chilpa_audio::diagnostics::trace("alsa", "thread_priority", format_args!("got={got:?} after={:?}", started.elapsed()));
    got
}

/// Puts the calling thread back among the others, once it no longer feeds
/// a device. A real-time thread that stops blocking on a device and goes
/// on writing to a file at full speed would starve the machine, and
/// RealtimeKit kills a real-time thread that runs past its limit without
/// blocking; so what was asked for is given back with the device.
pub fn relax() {
    let Some(got) = HELD.take() else { return };
    let ordinary = libc::sched_param { sched_priority: 0 };
    // SAFETY: a thread id of zero is the calling thread; giving priority up
    // needs no permission.
    let back = match got {
        Elevation::Scheduled(_) | Elevation::Granted(_) => {
            unsafe { libc::sched_setscheduler(0, libc::SCHED_OTHER, &ordinary) == 0 }
        }
        Elevation::Nicer(_) => unsafe { libc::setpriority(libc::PRIO_PROCESS, thread_id() as libc::id_t, 0) == 0 },
        Elevation::AsItWas => true,
    };
    chilpa_audio::diagnostics::trace("alsa", "thread_priority", format_args!("gave_back={got:?} ok={back}"));
}

/// The calling thread's id, as the kernel and RealtimeKit know it.
fn thread_id() -> u64 {
    // SAFETY: gettid takes no arguments and cannot fail.
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

/// Real time from the scheduler itself: it says yes where the user's
/// limits allow a real-time priority, and no with EPERM anywhere else.
fn scheduled() -> Option<Elevation> {
    let max = unsafe { libc::sched_get_priority_max(libc::SCHED_RR) };
    if max < 1 {
        return None;
    }
    let priority = (PRIORITY as i32).min(max);
    let param = libc::sched_param { sched_priority: priority };
    // SAFETY: a thread id of zero is the calling thread, and the struct is
    // whole.
    let set = unsafe { libc::sched_setscheduler(0, libc::SCHED_RR, &param) };
    (set == 0).then_some(Elevation::Scheduled(priority as u32))
}

/// A proxy for RealtimeKit on the system bus, when it is there.
fn kit(connection: &Connection) -> Proxy<'_, &Connection> {
    connection.with_proxy("org.freedesktop.RealtimeKit1", "/org/freedesktop/RealtimeKit1", Duration::from_secs(2))
}

/// Real time granted by RealtimeKit, within the time it allows a thread
/// to run without blocking, which it has to be told the thread accepts.
fn granted() -> Option<Elevation> {
    let connection = Connection::new_system().ok()?;
    let kit = kit(&connection);
    let most: i32 = kit.get("org.freedesktop.RealtimeKit1", "MaxRealtimePriority").ok()?;
    let longest: i64 = kit.get("org.freedesktop.RealtimeKit1", "RTTimeUSecMax").ok()?;
    if most < 1 || longest < 1 {
        return None;
    }
    let limit = libc::rlimit { rlim_cur: longest as libc::rlim_t, rlim_max: longest as libc::rlim_t };
    // SAFETY: the struct is whole; the limit applies to this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_RTTIME, &limit) } != 0 {
        return None;
    }
    let priority = (PRIORITY as i32).min(most) as u32;
    kit.method_call("org.freedesktop.RealtimeKit1", "MakeThreadRealtime", (thread_id(), priority)).map(|(): ()| Elevation::Granted(priority)).ok()
}

/// Ahead of ordinary threads, from RealtimeKit, when real time was refused.
fn nicer() -> Option<Elevation> {
    let connection = Connection::new_system().ok()?;
    let kit = kit(&connection);
    let lowest: i32 = kit.get("org.freedesktop.RealtimeKit1", "MinNiceLevel").ok()?;
    let nice = NICE.max(lowest);
    kit.method_call("org.freedesktop.RealtimeKit1", "MakeThreadHighPriority", (thread_id(), nice)).map(|(): ()| Elevation::Nicer(nice)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever this machine grants, asking twice on one thread asks once,
    /// and the answer is the same answer; given back, it can be asked again.
    #[test]
    fn a_thread_asks_once_and_gives_back() {
        let first = elevate();
        let second = elevate();
        assert_eq!(first, second);
        relax();
        assert_eq!(HELD.get(), None);
        relax();
        assert_eq!(elevate(), first);
        relax();
    }

    #[test]
    fn the_id_is_the_threads_own() {
        let here = thread_id();
        let there = std::thread::spawn(thread_id).join().unwrap();
        assert_ne!(here, 0);
        assert_ne!(here, there);
    }
}
