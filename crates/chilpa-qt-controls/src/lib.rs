//! The ways to drive the player besides its own buttons: the keyboard
//! shortcuts with their sheet and editor, the timer that stops playback
//! and the A-B loop. The window's crate depends on this one; the facade is
//! the one `chilpa-qt-sources` holds.

pub mod bridge {
    pub mod controls;
    pub mod hotkeys;
}
