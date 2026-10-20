//! What every shell crate stands on: the one facade they all talk to, the
//! theme, the translations, the primitives everything is drawn with, and
//! the helpers every bridge uses: the shared query worker and Qt's lists.
//! It depends on the facade and on nothing of the window.

pub mod app;
pub mod format;
pub mod worker;
pub mod album_pages;
pub mod album_rows;
pub mod track_roles;
pub mod bridge {
    pub mod i18n;
    pub mod prefs;
    pub mod pulse;
    pub mod queue_activity;
    pub mod theme;
}

unsafe extern "C" {
    fn chilpa_use_font(resource: *const std::ffi::c_char) -> bool;
    fn chilpa_use_japanese_fallback(resource: *const std::ffi::c_char) -> bool;
    #[cfg(target_os = "linux")]
    fn chilpa_has_desktop_entry(name: *const std::ffi::c_char) -> bool;
}

/// Makes a font of the resources the application's default, before any QML
/// is loaded (`cxx/fonts.cpp`). Needs the application object.
pub fn use_font(resource: &std::ffi::CStr) -> bool {
    // SAFETY: a NUL-terminated string that outlives the call, on the GUI
    // thread once the application object exists.
    unsafe { chilpa_use_font(resource.as_ptr()) }
}

/// Makes a font of the resources the one Japanese falls back to, whatever
/// the main font (`cxx/fonts.cpp`). Needs the application object.
pub fn use_japanese_fallback(resource: &std::ffi::CStr) -> bool {
    // SAFETY: as above.
    unsafe { chilpa_use_japanese_fallback(resource.as_ptr()) }
}

/// Whether the system has a desktop entry by that name (`cxx/fonts.cpp`).
#[cfg(target_os = "linux")]
pub fn has_desktop_entry(name: &std::ffi::CStr) -> bool {
    // SAFETY: as above.
    unsafe { chilpa_has_desktop_entry(name.as_ptr()) }
}
