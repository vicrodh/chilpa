//! The metadata editor: its screen and the bridge to the facade's editor.
//! The window's crate depends on this one, shows its screen and asks it to
//! open on a track; this one knows nothing of the window.

pub mod bridge {
    pub mod metadata;
}
