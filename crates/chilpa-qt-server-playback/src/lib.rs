//! What a server is told of what plays from it, and later how it streams:
//! the dialog a server's menu opens. The work is the facade's (`chilpa_api`
//! `server_playback_json`); this crate is the surface.

pub mod bridge {
    pub mod server_playback;
}
