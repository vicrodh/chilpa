//! The window's graphics at startup, the half that needs Qt: the startup
//! children that prove a backend presents frames, the list of Qt's own
//! Vulkan devices, and the bridge through which the window reports what
//! actually draws it and that it presented. The decisions are the facade's
//! (`chilpa_api::graphics_boot`), ported from QBZ.

pub mod bridge {
    pub mod graphics;
}

use chilpa_api::graphics_boot::{gpu, inventory, preflight};
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

/// Reads this process's Qt Vulkan instance, after its application object
/// exists and before a window does.
fn devices_json() -> String {
    if !cfg!(target_os = "linux") {
        return "[]".to_string();
    }
    // SAFETY: called on the GUI thread after the application object exists;
    // the pointer is to a process-static buffer, copied before returning.
    let pointer = unsafe { chilpa_vulkan_devices_json() };
    if pointer.is_null() {
        "[]".to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
    }
}

/// When this process is one of the startup children, runs it and answers
/// its exit code. It must be asked first in `main`, before the single
/// instance, the facade, audio or plugins: a child does nothing else.
pub fn run_child_if_asked() -> Option<i32> {
    if inventory::child_requested() {
        let app = QGuiApplication::new();
        let code = inventory::run_child(devices_json);
        drop(app);
        return Some(code);
    }
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
        gpu::set_inventory_json(&devices_json());
        // SAFETY: as above, with Vulkan and the device's index already set.
        let code = gpu::run_child(|| unsafe { chilpa_gpu_preflight_window() });
        drop(app);
        return Some(code);
    }
    None
}

/// The saved GPU, once the application object exists and before the engine
/// builds the window. An approved saved device needs this process's Qt
/// instance order; Auto obtains only Settings rows in a disposable child.
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
    if gpu::needs_local_inventory() {
        gpu::set_inventory_json(&devices_json());
    } else {
        inventory::collect();
    }
    gpu::apply_preference();
}
