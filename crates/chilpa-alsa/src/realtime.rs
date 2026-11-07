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
//! blocking, so the limit it asks for is set first; failing that, a
//! higher ordinary priority from RealtimeKit. Inside a Flatpak the system
//! bus does not reach RealtimeKit, so the same two asks then go to the
//! desktop portal on the session bus (`org.freedesktop.portal.Realtime`),
//! which hands them on to RealtimeKit with the sandbox's process and thread
//! ids turned into the host's. Nothing here can fail the device: what was
//! not granted is written down and the music plays as it did.

use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
use dbus::blocking::{Connection, Proxy};
use std::cell::Cell;
use std::sync::Mutex;
use std::time::Duration;

/// The priority asked for, within what RealtimeKit allows (20 by default).
const PRIORITY: u32 = 20;
/// The ordinary priority asked for when real time is refused: the same
/// RealtimeKit gives a sound server's clients.
const NICE: i32 = -11;
/// How long one question over D-Bus may take before it is given up on.
const PATIENCE: Duration = Duration::from_secs(2);

/// RealtimeKit's name, object and interface share one string.
const KIT: &str = "org.freedesktop.RealtimeKit1";
/// The portal's interface, on the desktop portal's object.
const REALTIME: &str = "org.freedesktop.portal.Realtime";

/// What the thread got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevation {
    /// Real time, at that priority, straight from the scheduler.
    Scheduled(u32),
    /// Real time, at that priority, granted by RealtimeKit, asked directly
    /// or through the portal.
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

/// The session bus, where the portal is: opened the first time the portal
/// is asked and kept for the threads that ask after it.
static PORTAL: Shared<fn() -> Result<Connection, dbus::Error>> =
    Shared::new(Connection::new_session as fn() -> Result<Connection, dbus::Error>);

/// Puts the calling thread ahead while it feeds a device. Asked twice, it
/// says what the first time got.
pub fn elevate() -> Elevation {
    if let Some(got) = HELD.get() {
        return got;
    }
    let started = std::time::Instant::now();
    let got = ask(scheduled, Connection::new_system, &PORTAL);
    HELD.set(Some(got));
    chilpa_audio::diagnostics::trace("alsa", "thread_priority", format_args!("got={got:?} after={:?}", started.elapsed()));
    got
}

/// Asks, in order, until something is granted: the scheduler, RealtimeKit
/// on the system bus for real time and then for a nicer level, and only
/// when the system bus gave nothing, the portal for the same two.
///
/// The portal comes last on purpose: on a desktop where the system bus
/// answers, nothing here asks the session bus anything.
fn ask<O>(
    scheduler: impl FnOnce() -> Option<Elevation>,
    system: impl Fn() -> Result<Connection, dbus::Error>,
    portal: &Shared<O>,
) -> Elevation
where
    O: Fn() -> Result<Connection, dbus::Error>,
{
    scheduler()
        .or_else(|| granted(&system().ok()?))
        .or_else(|| nicer(&system().ok()?))
        .or_else(|| {
            let got = portal.with(|connection| granted_by_portal(connection).or_else(|| nicer_by_portal(connection)));
            chilpa_audio::diagnostics::trace("alsa", "thread_priority", format_args!("portal={got:?}"));
            got
        })
        .unwrap_or(Elevation::AsItWas)
}

/// A connection opened the first time it is needed and kept for the asks
/// after it.
struct Shared<O> {
    open: O,
    held: Mutex<Option<Connection>>,
}

impl<O> Shared<O> {
    const fn new(open: O) -> Self {
        Shared { open, held: Mutex::new(None) }
    }
}

impl<O: Fn() -> Result<Connection, dbus::Error>> Shared<O> {
    /// `work` on the connection, opened now if it is not open yet. One ask
    /// at a time, each bounded by [`PATIENCE`] per question. A connection
    /// through which nothing was granted is let go, so a portal that was
    /// restarted, or a bus that went away, is reached again by the next
    /// ask instead of failing every one after it.
    fn with(&self, work: impl FnOnce(&Connection) -> Option<Elevation>) -> Option<Elevation> {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if held.is_none() {
            *held = (self.open)().ok();
        }
        let got = work(held.as_ref()?);
        if got.is_none() {
            *held = None;
        }
        got
    }
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

/// The calling thread's id, as the kernel and RealtimeKit know it. Inside
/// a Flatpak it is the id in the sandbox's own process namespace, which is
/// what the portal expects to be handed.
fn thread_id() -> u64 {
    // SAFETY: gettid takes no arguments and cannot fail.
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

/// This process's id, in the same namespace as [`thread_id`].
fn process_id() -> u64 {
    u64::from(std::process::id())
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
    connection.with_proxy(KIT, "/org/freedesktop/RealtimeKit1", PATIENCE)
}

/// A proxy for the desktop portal on the session bus, when it is there.
fn portal(connection: &Connection) -> Proxy<'_, &Connection> {
    connection.with_proxy("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", PATIENCE)
}

/// The real-time priority to ask `grantor` for, within what it allows, once
/// the time a thread may run without blocking has been set to its limit,
/// which RealtimeKit requires before it grants anything. Its three numbers
/// are the same properties on RealtimeKit and on the portal.
fn real_time_within(grantor: &Proxy<'_, &Connection>, interface: &str) -> Option<u32> {
    let most: i32 = grantor.get(interface, "MaxRealtimePriority").ok()?;
    let longest: i64 = grantor.get(interface, "RTTimeUSecMax").ok()?;
    if most < 1 || longest < 1 {
        return None;
    }
    let limit = libc::rlimit { rlim_cur: longest as libc::rlim_t, rlim_max: longest as libc::rlim_t };
    // SAFETY: the struct is whole; the limit applies to this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_RTTIME, &limit) } != 0 {
        return None;
    }
    Some((PRIORITY as i32).min(most) as u32)
}

/// The nice level to ask `grantor` for, within what it allows.
fn nice_within(grantor: &Proxy<'_, &Connection>, interface: &str) -> Option<i32> {
    let lowest: i32 = grantor.get(interface, "MinNiceLevel").ok()?;
    Some(NICE.max(lowest))
}

/// Real time granted by RealtimeKit, within the time it allows a thread
/// to run without blocking, which it has to be told the thread accepts.
fn granted(connection: &Connection) -> Option<Elevation> {
    let kit = kit(connection);
    let priority = real_time_within(&kit, KIT)?;
    kit.method_call(KIT, "MakeThreadRealtime", (thread_id(), priority)).map(|(): ()| Elevation::Granted(priority)).ok()
}

/// Ahead of ordinary threads, from RealtimeKit, when real time was refused.
fn nicer(connection: &Connection) -> Option<Elevation> {
    let kit = kit(connection);
    let nice = nice_within(&kit, KIT)?;
    kit.method_call(KIT, "MakeThreadHighPriority", (thread_id(), nice)).map(|(): ()| Elevation::Nicer(nice)).ok()
}

/// Real time through the portal, which takes the process and the thread as
/// this process sees them and maps both before it asks RealtimeKit.
fn granted_by_portal(connection: &Connection) -> Option<Elevation> {
    let portal = portal(connection);
    let priority = real_time_within(&portal, REALTIME)?;
    portal
        .method_call(REALTIME, "MakeThreadRealtimeWithPID", (process_id(), thread_id(), priority))
        .map(|(): ()| Elevation::Granted(priority))
        .ok()
}

/// A nicer level through the portal, when it refused real time.
fn nicer_by_portal(connection: &Connection) -> Option<Elevation> {
    let portal = portal(connection);
    let nice = nice_within(&portal, REALTIME)?;
    portal
        .method_call(REALTIME, "MakeThreadHighPriorityWithPID", (process_id(), thread_id(), nice))
        .map(|(): ()| Elevation::Nicer(nice))
        .ok()
}

#[cfg(test)]
#[path = "realtime_tests.rs"]
mod tests;
