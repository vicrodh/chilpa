//! The image provider's C ABI. Derivatives and network policy stay in the facade.
use chilpa_qt_kit::format;

/// # Safety
/// `address` must be a valid NUL-terminated C string for this call.
#[no_mangle]
pub unsafe extern "C" fn chilpa_network_allowed(address: *const std::ffi::c_char) -> bool {
    unsafe { std::ffi::CStr::from_ptr(address) }.to_str().is_ok_and(chilpa_api::network_allowed)
}

/// Makes a derivative off the UI thread.
/// # Safety
/// `path` is a valid C string; `out` has room for `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn chilpa_cover(path: *const std::ffi::c_char, size: u32, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    unsafe { format::write_path(chilpa_api::cover_derivative(std::path::Path::new(path), size), out, capacity) }
}
/// # Safety
/// `path` is a valid C string; `out` has room for `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn chilpa_contained_cover(path: *const std::ffi::c_char, size: u32, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    unsafe { format::write_path(chilpa_api::contained_cover(std::path::Path::new(path), size), out, capacity) }
}
/// # Safety
/// `path` is a valid C string; `out` has room for `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn chilpa_atmosphere(path: *const std::ffi::c_char, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else { return false };
    unsafe { format::write_path(chilpa_api::header_atmosphere(std::path::Path::new(path)), out, capacity) }
}
