//! What every shell crate stands on: the one facade they all talk to, the
//! theme, the translations, the primitives everything is drawn with, and
//! the helpers every bridge uses: the shared query worker and Qt's lists.
//! It depends on the facade and on nothing of the window.

pub mod app;
pub mod format;
pub mod worker;
pub mod bridge {
    pub mod i18n;
    pub mod prefs;
    pub mod pulse;
    pub mod theme;
}
