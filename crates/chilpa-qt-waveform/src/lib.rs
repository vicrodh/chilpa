//! The seek bar's waveform: the player bar stands tall and its seek shows
//! how loud the track is along its length. The window's crate depends on
//! this one; the pass and the cache are the facade's (`chilpa_api`).

pub mod bridge {
    pub mod waveform;
}
