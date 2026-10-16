//! The mini player: a second, frameless window with a card that shows what
//! plays in one of five surfaces over a footer of controls. The window's
//! crate depends on this one; the facade is the one `chilpa-qt-kit` holds.

pub mod bridge {
    pub mod mini;
}
