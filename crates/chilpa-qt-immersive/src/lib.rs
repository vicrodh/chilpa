//! The immersive view: an overlay over the window with a panel in focus
//! or a split of cover and panel, a floating player bar, a header that
//! picks the view and searches, and the visualizer's streams behind the
//! panels that move. The window's crate depends on this one; the facade
//! is the one `chilpa-qt-kit` holds.

pub mod bridge {
    pub mod immersive;
    pub mod visualizer;
}
