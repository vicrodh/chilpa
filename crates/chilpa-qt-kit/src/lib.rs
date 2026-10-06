//! What every shell crate stands on: the one facade they all talk to, the
//! theme, the translations and the primitives everything is drawn with.
//! It depends on the facade and on nothing of the window.

pub mod app;
pub mod bridge {
    pub mod i18n;
    pub mod theme;
}
