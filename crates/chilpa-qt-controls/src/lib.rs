//! The ways to drive the player besides its own buttons: the keyboard
//! shortcuts with their sheet and editor, the timer that stops playback
//! and the A-B loop. The window's crate depends on this one; the facade is
//! the one `chilpa-qt-kit` holds.

pub mod bridge {
    pub mod controls;
    pub mod commands;
    pub mod hotkeys;
    pub mod now_about;
    pub mod queues;
    pub mod place;
    pub mod player_layout;
    pub mod search;
}
