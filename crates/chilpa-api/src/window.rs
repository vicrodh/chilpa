//! What the window asks of the system it runs on.

/// Puts the system's own window controls, where the window keeps them,
/// `target` points under its top. False while there is no window yet: the
/// caller asks again.
pub fn center_window_controls(target: f64) -> bool {
    chilpa_appkit::center_window_controls(target)
}
