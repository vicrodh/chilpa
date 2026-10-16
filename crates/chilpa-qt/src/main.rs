//! The desktop shell.
//!
//! Rust owns state and layered QML draws it (ADR-001). This crate and
//! `chilpa-qt-sources` are the only ones that may hold Qt types.

use chilpa_qt_kit::{app, format, worker};
// Linked for their QML modules, which `modules.cpp` registers: a crate
// nothing here names is left out of the binary, and its module with it.
use chilpa_qt_ambient as _;
use chilpa_qt_mini as _;
use chilpa_qt_dsp as _;
use chilpa_qt_immersive as _;
use chilpa_qt_waveform as _;
use chilpa_qt_artwork as _;
use chilpa_qt_about as _;
use chilpa_qt_stats as _;
use chilpa_qt_link as _;
use chilpa_qt_playlists as _;
use chilpa_qt_wizard as _;
use chilpa_qt_controls as _;
use chilpa_qt_gallery as _;
use chilpa_qt_lyrics as _;
use chilpa_qt_sources as _;
mod bridge;
mod browse;
mod registry;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
// On Linux the desktop entry associates the window with its launcher.
#[cfg(target_os = "linux")]
use cxx_qt_lib::QString;
unsafe extern "C" {
    /// Hands the engine the covers' image provider. See `covers.cpp`.
    fn chilpa_install_covers(engine: *mut std::ffi::c_void);
    /// Registers the shell modules and sets the window icon. See `modules.cpp`.
    fn chilpa_install_shell();
}

#[no_mangle]
pub unsafe extern "C" fn chilpa_network_allowed(address: *const std::ffi::c_char) -> bool {
    unsafe { std::ffi::CStr::from_ptr(address) }.to_str().is_ok_and(chilpa_api::network_allowed)
}

/// Makes a derivative off the UI thread. The C strings are valid and `out`
/// has room for `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn chilpa_cover(
    path: *const std::ffi::c_char,
    size: u32,
    out: *mut std::ffi::c_char,
    capacity: std::ffi::c_ulong,
) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    write_path(chilpa_api::cover_derivative(std::path::Path::new(path), size), out, capacity)
}
#[no_mangle]
pub unsafe extern "C" fn chilpa_contained_cover(path: *const std::ffi::c_char, size: u32, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    write_path(chilpa_api::contained_cover(std::path::Path::new(path), size), out, capacity)
}
#[no_mangle]
pub unsafe extern "C" fn chilpa_atmosphere(path: *const std::ffi::c_char, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    write_path(chilpa_api::header_atmosphere(std::path::Path::new(path)), out, capacity)
}
fn write_path(made: Option<std::path::PathBuf>, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Some(made) = made else { return false };
    let bytes = made.to_string_lossy().into_owned().into_bytes();
    if bytes.len() + 1 > capacity as usize {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len());
        *out.add(bytes.len()) = 0;
    }
    true
}

/// The desktop entry the window is named after, where there is one.
#[cfg(target_os = "linux")]
const DESKTOP_ENTRY: &std::ffi::CStr = c"app.chilpa.Chilpa";

/// Where Inter lives in the resources.
const INTER: &std::ffi::CStr = c":/qt/qml/app/chilpa/kit/assets/fonts/Inter.ttf";

fn main() {
    use chilpa_api::graphics_boot;
    // The interface size, before any Qt object; the startup children
    // inherit it. QBZ's order from here to the window.
    graphics_boot::apply_interface_size();
    // A startup child proves a backend and does nothing else: it never
    // reaches the single instance, the facade, audio or plugins.
    if let Some(code) = chilpa_qt_graphics::run_child_if_asked() {
        std::process::exit(code);
    }
    // Before anything is opened: a second launch shows the first and ends.
    if !chilpa_api::Chilpa::only_instance() { return; }
    // The saved GPU, the renderer preference and managed Auto, before the
    // application object builds its backend.
    graphics_boot::gpu::preflight_saved_gpu_at_boot();
    graphics_boot::apply_renderer_preference();
    #[cfg(target_os = "linux")]
    if let Err(reason) = graphics_boot::preflight::auto_at_boot() {
        eprintln!("renderer: the start stopped before the window: {reason}");
        std::process::exit(78);
    }
    // Before anything is restored: a start that dies restoring degrades the
    // next one instead of trapping it.
    chilpa_api::startup_chain::arm();
    // An image installs its entry before Qt looks for one below.
    chilpa_api::appimage_desktop::integrate();
    let mut app = QGuiApplication::new();
    // Named after its desktop entry only when one is installed: a build run
    // from the tree has none, and the portal refused it on every start.
    #[cfg(target_os = "linux")]
    if chilpa_qt_kit::has_desktop_entry(DESKTOP_ENTRY) {
        QGuiApplication::set_desktop_file_name(&QString::from(DESKTOP_ENTRY.to_str().unwrap_or_default()));
    }

    // Qt's Vulkan devices can be listed now, and no window exists yet.
    chilpa_qt_graphics::apply_gpu_preference();

    // SAFETY: I register static plugins once, before loading their resources or QML.
    unsafe { chilpa_install_shell() };

    // Inter unless the system's font was chosen: the design needs real
    // weights, and plenty of desktops set a font with one face that Qt does
    // not embolden.
    let font = app::with(|api| api.preference("appearance.font"));
    if font.as_deref() != Some("system") && !chilpa_qt_kit::use_font(INTER) {
        eprintln!("chilpa: the bundled font could not be loaded; using the system's");
    }

    // Sources load independently; guest initialization cannot hold up the window.
    app::with(|api| {
        api.start_plugins();
        // Listens go to ListenBrainz from now on, if an account is connected.
        api.start_scrobbling();
        // My servers: their tracks play, sync and are watched (ADR-017).
        api.start_servers();
        api.start_media_controls();
    });

    // CHILPA_GALLERY opens the component gallery.
    let root = if std::env::var_os("CHILPA_GALLERY").is_some() { "gallery/qml/Gallery/Gallery.qml" } else { "qml/Main.qml" };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // SAFETY: the engine is alive and a QQmlEngine; the provider it is
        // given is its own from then on.
        unsafe { chilpa_install_covers(engine.as_mut().get_unchecked_mut() as *mut QQmlApplicationEngine as *mut std::ffi::c_void) };
        let url = std::env::var_os("CHILPA_QML_SMOKE")
            .map(|path| QUrl::from_local_file(&cxx_qt_lib::QString::from(std::path::PathBuf::from(path).to_string_lossy().as_ref())))
            .unwrap_or_else(|| QUrl::from(&format!("qrc:/qt/qml/app/chilpa/{root}")));
        engine.load(&url);
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }

    // The window is gone, so the process ends now, whatever is still
    // running: a process that outlives its window is one nobody can see or
    // close. What is left gets a moment to finish in order; past that, what
    // was being waited for is said and the process leaves without it.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        eprintln!("chilpa: closing took more than 3 s (the player, the card or a request still pending); leaving without waiting");
        std::process::exit(0);
    });
    drop(engine);
    app::shutdown();
    std::process::exit(0);
}
