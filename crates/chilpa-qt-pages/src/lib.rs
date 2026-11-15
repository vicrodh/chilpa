//! Generic paginated collections and lazy embedded web surfaces.
pub(crate) use chilpa_qt_kit::app;
mod native_pages;

unsafe extern "C" {
    fn chilpa_prepare_embedded_pages();
}
/// Prepares shared graphics contexts before constructing the GUI application.
pub fn prepare() {
    unsafe { chilpa_prepare_embedded_pages() }
}
