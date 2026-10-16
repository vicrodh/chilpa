//! What sources bring to the window: their pages, the start that waits for
//! them, and searching them. The window's crate depends on this one.

pub(crate) use chilpa_qt_kit::app;
pub mod bridge {
    pub mod sources;
    pub mod importer;
    pub mod open_media;
}
