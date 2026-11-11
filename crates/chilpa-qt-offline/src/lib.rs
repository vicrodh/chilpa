//! Offline music: the page that says what the servers' music keeps on this
//! device, its limit and its folders, and the menus' way to it. The work is
//! the facade's (`chilpa_api` `server_offline`); this crate is the surface.

pub mod bridge {
    pub mod offline;
}
