//! About Chilpa: the window that says what this is, which build it is and
//! whom it thanks. The document is the facade's (`chilpa_api`); this crate
//! adds the build's stamp and draws it.

pub mod bridge {
    pub mod about;
    pub mod updates;
    pub mod plugin_updates;
}
