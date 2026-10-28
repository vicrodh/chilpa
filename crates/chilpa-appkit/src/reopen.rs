//! The Dock's click on a hidden Chilpa, which closing the window leaves
//! running: the application is asked to open again and the window comes back.

use std::sync::OnceLock;

static ON_REOPEN: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "C" {
    fn chilpa_install_macos_reopen_handler(present: extern "C" fn());
}

extern "C" fn reopened() {
    if let Some(handler) = ON_REOPEN.get() {
        handler();
    }
}

/// Calls `handler` when the Dock or the Finder asks Chilpa to open again.
/// On the thread AppKit runs on, once; a second call changes nothing.
pub fn on_reopen(handler: impl Fn() + Send + Sync + 'static) {
    if ON_REOPEN.set(Box::new(handler)).is_ok() {
        // SAFETY: the callback is a plain function that outlives the process's
        // use of it, and the installer runs on the main thread.
        unsafe { chilpa_install_macos_reopen_handler(reopened) };
    }
}
