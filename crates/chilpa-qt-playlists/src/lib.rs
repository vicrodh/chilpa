//! The playlists' own screens and panels: the manager that lists and files
//! them, and the picker that adds tracks, mounted once in the window. The
//! work is the facade's; this crate is the surface.

pub mod bridge {
    pub mod add_to_playlist;
    pub mod manager;
}
