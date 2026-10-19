//! The theme picker and its options, QBZ 2.1.2's (`AppearanceSettings.qml`,
//! `CustomThemeEditor.qml`, `QbzColorPicker.qml`): the list with its
//! light/dark filter, the auto theme's source, and the custom theme's base
//! colours with their picker. The themes and their colours are the facade's
//! (`chilpa_api::theme`); this crate is the surface.

pub mod bridge {
    pub mod editor;
}
