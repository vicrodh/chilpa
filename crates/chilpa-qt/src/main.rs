//! The desktop shell.
//!
//! Rust owns state and layered QML draws it (ADR-001). This is the only crate
//! that may hold Qt types.

mod app;
mod bridge;
mod browse;
mod format;
mod paths;
mod registry;
mod worker;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
// Only Linux has a desktop file to be named after, and only there is the
// name what makes the window pick up its icon.
#[cfg(target_os = "linux")]
use cxx_qt_lib::QString;
unsafe extern "C" {
    /// Registers a font from the resources as the application's default.
    /// See `fonts.cpp`.
    fn chilpa_use_font(resource: *const std::ffi::c_char) -> bool;
    /// Whether a desktop entry by that name is installed. See `fonts.cpp`.
    #[cfg(target_os = "linux")]
    fn chilpa_has_desktop_entry(name: *const std::ffi::c_char) -> bool;
    /// Hands the engine the covers' image provider. See `covers.cpp`.
    fn chilpa_install_covers(engine: *mut std::ffi::c_void);
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
const DESKTOP_ENTRY: &std::ffi::CStr = c"com.blitzfc.chilpa";

/// Where Inter lives in the resources.
const INTER: &std::ffi::CStr = c":/qt/qml/com/blitzfc/chilpa/assets/fonts/Inter.ttf";

fn main() {
    let mut app = QGuiApplication::new();
    // Named after its desktop entry only when one is installed: a build run
    // from the tree has none, and the portal refused it on every start.
    #[cfg(target_os = "linux")]
    if unsafe { chilpa_has_desktop_entry(DESKTOP_ENTRY.as_ptr()) } {
        QGuiApplication::set_desktop_file_name(&QString::from(DESKTOP_ENTRY.to_str().unwrap_or_default()));
    }

    // Inter unless the system's font was chosen: the design needs real
    // weights, and plenty of desktops set a font with one face that Qt does
    // not embolden.
    let font = app::with(|api| api.preference("appearance.font"));
    // SAFETY: the pointer is a NUL-terminated literal that outlives the call,
    // and the application object exists.
    if font.as_deref() != Some("system") && !unsafe { chilpa_use_font(INTER.as_ptr()) } {
        eprintln!("chilpa: the bundled font could not be loaded; using the system's");
    }

    // Sources have to exist before anything in the window asks for searches,
    // playlists or account state. Loading an empty folder is the normal case.
    app::with(|api| {
        api.load_plugins();
        // Listens go to ListenBrainz from now on, if an account is connected.
        api.start_scrobbling();
    });

    // CHILPA_GALLERY opens the Gallery instead of the window: every piece of
    // the interface in its states, in either theme.
    let root = if std::env::var_os("CHILPA_GALLERY").is_some() { "Gallery/Gallery.qml" } else { "Main.qml" };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // SAFETY: the engine is alive and a QQmlEngine; the provider it is
        // given is its own from then on.
        unsafe { chilpa_install_covers(engine.as_mut().get_unchecked_mut() as *mut QQmlApplicationEngine as *mut std::ffi::c_void) };
        engine.load(&QUrl::from(&format!("qrc:/qt/qml/com/blitzfc/chilpa/qml/{root}")));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
