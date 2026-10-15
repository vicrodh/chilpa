//! The window's graphics at startup, the half that needs Qt: the startup
//! children that prove a backend presents frames, the list of Qt's own
//! Vulkan devices, and the bridge through which the window reports what
//! actually draws it and that it presented. The decisions are the facade's
//! (`chilpa_api::graphics_boot`), ported from QBZ.

pub mod bridge {
    pub mod graphics;
}

use chilpa_api::graphics_boot::{gpu, preflight};
use cxx_qt_lib::QGuiApplication;

unsafe extern "C" {
    fn chilpa_plasma_register_qml_type();
    fn chilpa_ribbon_register_qml_type();
    fn chilpa_line_bed_register_qml_type();
    fn chilpa_tunnel_flow_register_qml_type();
    fn chilpa_scope_trace_register_qml_type();
    /// Qt's Vulkan devices as JSON. See `cxx/probe.cpp`.
    fn chilpa_vulkan_devices_json() -> *const std::ffi::c_char;
    /// The probe window for a saved GPU. See `cxx/probe.cpp`.
    fn chilpa_gpu_preflight_window() -> i32;
    /// The probe window for Qt's default backend. See `cxx/probe.cpp`.
    fn chilpa_auto_preflight_window() -> i32;
}

/// Hands Qt's Vulkan devices to the facade. The application object must
/// exist and no window yet.
fn list_devices() {
    if !cfg!(target_os = "linux") {
        gpu::set_inventory_json("[]");
        return;
    }
    // SAFETY: called on the GUI thread after the application object exists;
    // the pointer is to a process-static buffer, copied before returning.
    let pointer = unsafe { chilpa_vulkan_devices_json() };
    let json = if pointer.is_null() {
        "[]".to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
    };
    gpu::set_inventory_json(&json);
}

/// When this process is one of the startup children, runs it and answers
/// its exit code. It must be asked first in `main`, before the single
/// instance, the facade, audio or plugins: a child does nothing else.
pub fn run_child_if_asked() -> Option<i32> {
    if preflight::auto_child_requested() {
        let app = QGuiApplication::new();
        // SAFETY: the child's GUI thread, its only application object, no
        // other window; the C++ side owns and destroys its probe window.
        let code = preflight::run_auto_child(|| unsafe { chilpa_auto_preflight_window() });
        drop(app);
        return Some(code);
    }
    if preflight::gpu_child_requested() {
        let app = QGuiApplication::new();
        list_devices();
        // SAFETY: as above, with Vulkan and the device's index already set.
        let code = gpu::run_child(|| unsafe { chilpa_gpu_preflight_window() });
        drop(app);
        return Some(code);
    }
    None
}

/// The saved GPU, once the application object exists and before the engine
/// builds the window: Qt's Vulkan instance can be listed now, and QRhi reads
/// the variables when the window is created.
pub fn apply_gpu_preference() {
    // These references retain the static objects whose startup functions
    // register the native QML types before the engine loads a scene.
    unsafe {
        chilpa_plasma_register_qml_type();
        chilpa_ribbon_register_qml_type();
        chilpa_line_bed_register_qml_type();
        chilpa_tunnel_flow_register_qml_type();
        chilpa_scope_trace_register_qml_type();
    }
    list_devices();
    gpu::apply_preference();
}
