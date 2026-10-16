//! Where this process's windows sit on a Plasma Wayland desktop.
//!
//! The wallpaper background paints the part of the desktop picture that
//! lies under the window, so moving the window slides the picture the way a
//! translucent window would. That needs the window's place on the screen,
//! and Wayland hides it from clients by design: Qt's window position is a
//! fiction there. KDE Plasma does tell a client where every window is,
//! through `org_kde_plasma_window_management`, the protocol its own task
//! manager and window switchers use, and this reads it back for the windows
//! of this process.
//!
//! Why a separate connection: Qt owns the process's Wayland display and
//! reads it from its own event loop; sharing it would mean a foreign event
//! queue dispatched from a thread Qt does not know about. A second client
//! connection is ordinary Wayland (any process may hold several), costs one
//! socket, and lives on its own thread that blocks in `dispatch` and only
//! wakes when the compositor has something to say. It reports through
//! `publish`: a JSON array of this process's windows with their absolute
//! geometry, republished only when that array changes.
//!
//! Who gets the protocol: KWin hands it only to a client whose executable
//! is named by a `.desktop` file whose `Exec` is that binary and which lists
//! the interface in `X-KDE-Wayland-Interfaces` (`packaging/linux/` carries
//! one). A binary run from somewhere no desktop file names never sees it:
//! the `bind` fails, one line is logged, and the window centres its crop,
//! the same picture as without this module, never a broken one. Other
//! compositors have no equivalent protocol and take the same fallback.
//!
//! The tracker starts lazily, on the first use of the wallpaper look on a
//! Wayland session, and runs for the life of the process: geometry events
//! arrive only when a window moves or resizes, so an idle tracker costs
//! nothing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_plasma::plasma_window_management::client::org_kde_plasma_window::{self, OrgKdePlasmaWindow};
use wayland_protocols_plasma::plasma_window_management::client::org_kde_plasma_window_management::{self, OrgKdePlasmaWindowManagement};

/// `window_with_uuid` arrived with version 13; `client_geometry` (the
/// surface without decorations, which is what the window measures) with 18.
/// Everything in between is tolerated: a 13 to 17 compositor reports the
/// frame geometry instead, the same rectangle for a frameless window.
const MIN_VERSION: u32 = 13;
const MAX_VERSION: u32 = 18;

static STARTED: AtomicBool = AtomicBool::new(false);

/// One window of this process, as the compositor reports it, in absolute
/// logical coordinates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct WindowRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Everything the compositor has said about one mapped window, until its
/// `pid` proves it is not ours (then the proxy is destroyed and the entry
/// dropped).
#[derive(Debug, Default)]
struct Tracked {
    pid: Option<u32>,
    /// Frame geometry, the version 13 to 17 fallback.
    frame: Option<(i32, i32, u32, u32)>,
    /// Surface geometry, version 18: preferred.
    client: Option<(i32, i32, u32, u32)>,
}

struct State {
    me: u32,
    windows: HashMap<String, Tracked>,
    dirty: bool,
    /// What was last published: a move that ends where it started, or a
    /// geometry event that repeats the frame, republishes nothing.
    published: String,
}

/// Starts the tracker once per process; every later call returns at once.
/// Outside a Wayland session there is nothing to track and nothing starts.
pub fn track(publish: impl Fn(String) + Send + 'static) {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(e) = std::thread::Builder::new().name("chilpa-wl-windows".into()).spawn(move || run(publish)) {
        log::warn!("wallpaper: the window tracker thread failed to start: {e}");
        STARTED.store(false, Ordering::SeqCst);
    }
}

fn run(publish: impl Fn(String)) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("wallpaper: no Wayland connection for the window tracker: {e}");
            return;
        }
    };
    let (globals, mut queue) = match registry_queue_init::<State>(&conn) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("wallpaper: the Wayland registry failed: {e}");
            return;
        }
    };
    let handle = queue.handle();
    let _manager: OrgKdePlasmaWindowManagement = match globals.bind(&handle, MIN_VERSION..=MAX_VERSION, ()) {
        Ok(m) => m,
        Err(e) => {
            log::info!("wallpaper: org_kde_plasma_window_management is not offered to this binary ({e}); the crop stays centred");
            return;
        }
    };
    let mut state = State { me: std::process::id(), windows: HashMap::new(), dirty: false, published: "[]".to_string() };
    loop {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            log::warn!("wallpaper: the window tracker stopped: {e}");
            publish("[]".to_string());
            return;
        }
        if state.dirty {
            state.dirty = false;
            let json = to_json(&state.mine());
            if json != state.published {
                state.published = json.clone();
                publish(json);
            }
        }
    }
}

impl State {
    /// This process's windows, in a stable order (position, then size) so
    /// two reports of the same layout serialise identically.
    fn mine(&self) -> Vec<WindowRect> {
        let mut out: Vec<WindowRect> = self
            .windows
            .values()
            .filter(|w| w.pid == Some(self.me))
            .filter_map(|w| {
                let (x, y, width, height) = w.client.or(w.frame)?;
                Some(WindowRect { x, y, width, height })
            })
            .collect();
        out.sort_by(|a, b| (a.x, a.y, a.width, a.height).cmp(&(b.x, b.y, b.width, b.height)));
        out
    }
}

/// The report: `[{"x":..,"y":..,"w":..,"h":..}, ...]`.
fn to_json(windows: &[WindowRect]) -> String {
    let items: Vec<String> = windows.iter().map(|r| format!("{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}", r.x, r.y, r.width, r.height)).collect();
    format!("[{}]", items.join(","))
}

// The registry: `registry_queue_init` collected the globals; nothing to do
// per event.
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<OrgKdePlasmaWindowManagement, ()> for State {
    fn event(state: &mut Self, manager: &OrgKdePlasmaWindowManagement, event: org_kde_plasma_window_management::Event, _: &(), _: &Connection, handle: &QueueHandle<Self>) {
        // A mapped window: ask for its object, keyed by the uuid the
        // compositor gave it. Every window on the desktop announces itself
        // here; the `pid` event sorts ours from the rest.
        if let org_kde_plasma_window_management::Event::WindowWithUuid { uuid, .. } = event {
            manager.get_window_by_uuid(uuid.clone(), handle, uuid.clone());
            state.windows.insert(uuid, Tracked::default());
        }
    }
}

impl Dispatch<OrgKdePlasmaWindow, String> for State {
    fn event(state: &mut Self, window: &OrgKdePlasmaWindow, event: org_kde_plasma_window::Event, uuid: &String, _: &Connection, _: &QueueHandle<Self>) {
        use org_kde_plasma_window::Event;
        let me = state.me;
        let Some(tracked) = state.windows.get_mut(uuid) else { return };
        match event {
            Event::PidChanged { pid } => tracked.pid = Some(pid),
            Event::Geometry { x, y, width, height } => {
                tracked.frame = Some((x, y, width, height));
                state.dirty |= tracked.pid == Some(me);
            }
            Event::ClientGeometry { x, y, width, height } => {
                tracked.client = Some((x, y, width, height));
                state.dirty |= tracked.pid == Some(me);
            }
            // The compositor has said what it knows about a new window (the
            // pid comes before this, the client geometry after). Someone
            // else's: let go of it now, so its later moves never reach here.
            Event::InitialState => {
                if tracked.pid != Some(me) {
                    state.windows.remove(uuid);
                    window.destroy();
                } else {
                    state.dirty = true;
                }
            }
            Event::Unmapped => {
                let mine = tracked.pid == Some(me);
                state.windows.remove(uuid);
                window.destroy();
                state.dirty |= mine;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_carries_each_window_with_short_keys() {
        let windows = vec![WindowRect { x: 640, y: 120, width: 1720, height: 980 }, WindowRect { x: -10, y: 0, width: 320, height: 96 }];
        assert_eq!(to_json(&windows), r#"[{"x":640,"y":120,"w":1720,"h":980},{"x":-10,"y":0,"w":320,"h":96}]"#);
        assert_eq!(to_json(&[]), "[]", "no windows is an empty array");
    }

    #[test]
    fn only_this_process_windows_are_reported_and_client_geometry_wins() {
        let mut state = State { me: 4242, windows: HashMap::new(), dirty: false, published: String::new() };
        state.windows.insert("a".into(), Tracked { pid: Some(4242), frame: Some((100, 100, 800, 600)), client: Some((104, 130, 792, 566)) });
        state.windows.insert("b".into(), Tracked { pid: Some(7), frame: Some((0, 0, 500, 500)), client: None });
        state.windows.insert("c".into(), Tracked { pid: Some(4242), frame: None, client: None });
        let mine = state.mine();
        assert_eq!(mine.len(), 1);
        assert_eq!((mine[0].x, mine[0].width), (104, 792));
    }
}
