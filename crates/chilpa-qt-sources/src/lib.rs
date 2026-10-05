//! What sources bring to the window: their pages, the start that waits for
//! them, and searching them. The window's crate depends on this one; the
//! facade both talk to is held here, in [`app`], so there is one.

pub mod app;
pub mod bridge {
    pub mod sources;
    pub mod importer;
}
