//! Window-thread calls into the facade that take too long, written down.
//!
//! The window freezes while one of its calls into the facade runs. Which
//! call it was is known only while it happens, so each call says where it
//! came from, and what takes longer than a frame should is written to
//! `stalls.log` in the cache folder: when it ends, and, if it runs past a
//! second, while it still runs, in case it never ends.

use std::io::Write;
use std::panic::Location;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Longer than this is a stall the listener can see.
const NOTICED: Duration = Duration::from_millis(250);
/// A call still running after this is written before it ends.
const STUCK: Duration = Duration::from_secs(1);
/// The log is started over past this size.
const LOG_MOST: u64 = 1 << 20;

struct Running {
    since: Instant,
    at: &'static Location<'static>,
    reported: bool,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// Marks a window-thread call in progress until it is dropped.
pub struct Guard(());

/// Called at the start of every window-thread call into the facade.
pub fn enter(at: &'static Location<'static>) -> Guard {
    watcher();
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { since: Instant::now(), at, reported: false });
    Guard(())
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Some(done) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        let took = done.since.elapsed();
        if took >= NOTICED {
            write(&format!("{}ms {}:{}", took.as_millis(), done.at.file(), done.at.line()));
        }
    }
}

/// Writes a call that is still running past a second, once.
fn watcher() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("chilpa-stall".into()).spawn(|| loop {
            std::thread::sleep(Duration::from_millis(200));
            let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(call) = running.as_mut().filter(|c| !c.reported && c.since.elapsed() >= STUCK) {
                call.reported = true;
                let line = format!("still running after {}ms {}:{}", call.since.elapsed().as_millis(), call.at.file(), call.at.line());
                drop(running);
                write(&line);
            }
        });
    });
}

fn write(line: &str) {
    let path = chilpa_dirs::cache().join("stalls.log");
    eprintln!("stall: {line}");
    if std::fs::metadata(&path).map(|m| m.len() > LOG_MOST).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let when = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{when} {line}");
    }
}
