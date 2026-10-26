//! I expose device synchronization through the facade and a separate QML module.
pub mod bridge {
    pub mod devices;
}

/// I pass the mount notification endpoint to the native adapter without duplicating its convention.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[no_mangle]
pub extern "C" fn chilpa_mount_events_file() -> *const std::ffi::c_char {
    chilpa_dirs::mount_events_file().as_ptr()
}
