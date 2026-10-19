//! What the theme's options show and what they change. The facade keeps the
//! choices and the custom theme's colours; the window's theme is told to
//! read them again, or to generate the auto theme again.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The options as one JSON document (`theme_options_json`): the
        /// list's filter and its light themes, the auto theme's source, the
        /// custom theme's colours, and which boxes are folded.
        #[qproperty(QString, state)]
        type ThemeEditor = super::ThemeEditorRust;

        /// Reads the options again: the theme changed, or the language.
        #[qinvokable]
        fn reload(self: Pin<&mut ThemeEditor>);
        /// The list's filter: 0 every theme, 1 the dark ones, 2 the light.
        #[qinvokable]
        fn set_filter(self: Pin<&mut ThemeEditor>, filter: i32);
        /// The auto theme's source, by its place in the list.
        #[qinvokable]
        fn set_source(self: Pin<&mut ThemeEditor>, index: i32);
        /// The image the auto theme reads; it becomes the source.
        #[qinvokable]
        fn set_image(self: Pin<&mut ThemeEditor>, path: &QString);
        /// Generates the auto theme again.
        #[qinvokable]
        fn regenerate(self: Pin<&mut ThemeEditor>);
        /// Folds or opens the box of "auto" or "custom".
        #[qinvokable]
        fn fold(self: Pin<&mut ThemeEditor>, which: &QString, folded: bool);
        /// The custom theme takes the colours on screen.
        #[qinvokable]
        fn use_current_colors(self: Pin<&mut ThemeEditor>);
        /// The custom theme's polarity.
        #[qinvokable]
        fn set_dark(self: Pin<&mut ThemeEditor>, dark: bool);
        /// One of the custom theme's base colours, `#rrggbb`: shown at once,
        /// written when the edits stop.
        #[qinvokable]
        fn set_color(self: Pin<&mut ThemeEditor>, key: &QString, hex: &QString);
        /// Writes the custom theme now: a drag ended, a colour was typed.
        #[qinvokable]
        fn flush(self: Pin<&mut ThemeEditor>);
        /// A colour from hue, saturation, value and alpha, each 0 to 1: what
        /// the colour picker paints.
        #[qinvokable]
        fn hsv(self: &ThemeEditor, hue: f64, saturation: f64, value: f64, alpha: f64) -> QColor;
    }

    impl cxx_qt::Threading for ThemeEditor {}
}

use chilpa_api::theme::choice::custom;
use chilpa_qt_kit::app;
use chilpa_qt_kit::bridge::theme;
use cxx_qt::Threading as _;
use cxx_qt_lib::{QColor, QString};
use std::pin::Pin;

pub struct ThemeEditorRust {
    state: QString,
}

impl Default for ThemeEditorRust {
    fn default() -> Self {
        Self { state: QString::from(&app::with(|api| api.theme_options_json())) }
    }
}

/// HSV to RGB, every channel 0 to 1.
fn rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
    let h = hue.clamp(0.0, 1.0) * 6.0;
    let (s, v) = (saturation.clamp(0.0, 1.0), value.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r + m, g + m, b + m)
}

impl qobject::ThemeEditor {
    pub fn reload(self: Pin<&mut Self>) {
        let state = app::with(|api| api.theme_options_json());
        self.set_state(QString::from(&state));
    }

    pub fn set_filter(self: Pin<&mut Self>, filter: i32) {
        app::with(|api| api.choose_theme_filter(filter));
        self.reload();
    }

    pub fn set_source(self: Pin<&mut Self>, index: i32) {
        if app::with(|api| api.choose_auto_theme_source(index)) {
            theme::ask(|theme| theme.regenerate());
        }
        self.reload();
    }

    pub fn set_image(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        if app::with(|api| api.choose_auto_theme_image(&path)) {
            theme::ask(|theme| theme.regenerate());
        }
        self.reload();
    }

    pub fn regenerate(self: Pin<&mut Self>) {
        theme::ask(|theme| theme.regenerate());
    }

    pub fn fold(self: Pin<&mut Self>, which: &QString, folded: bool) {
        let which = which.to_string();
        app::with(|api| api.fold_theme_options(&which, folded));
        self.reload();
    }

    /// The theme on screen is the window theme's to read, so the seed is
    /// taken there and these options read again after it.
    pub fn use_current_colors(self: Pin<&mut Self>) {
        let editor = self.qt_thread();
        theme::ask(move |theme| {
            theme.seed_custom();
            let _ = editor.queue(|editor| editor.reload());
        });
    }

    pub fn set_dark(self: Pin<&mut Self>, dark: bool) {
        if custom::set_dark(dark) {
            theme::ask(|theme| theme.refresh());
            self.reload();
        }
    }

    pub fn set_color(self: Pin<&mut Self>, key: &QString, hex: &QString) {
        if custom::set(&key.to_string(), &hex.to_string()) {
            theme::ask(|theme| theme.refresh());
            self.reload();
        }
    }

    pub fn flush(self: Pin<&mut Self>) {
        custom::flush();
    }

    pub fn hsv(&self, hue: f64, saturation: f64, value: f64, alpha: f64) -> QColor {
        let (r, g, b) = rgb(hue, saturation, value);
        QColor::from_rgba_f(r as f32, g as f32, b as f32, alpha.clamp(0.0, 1.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::rgb;

    #[test]
    fn the_picker_paints_the_hues_and_grays_it_names() {
        assert_eq!(rgb(0.0, 1.0, 1.0), (1.0, 0.0, 0.0));
        let (r, g, b) = rgb(1.0 / 3.0, 1.0, 1.0);
        assert!(r.abs() < 1e-9 && (g - 1.0).abs() < 1e-9 && b.abs() < 1e-9, "green");
        assert_eq!(rgb(0.5, 0.0, 0.5), (0.5, 0.5, 0.5));
        assert_eq!(rgb(1.0, 1.0, 1.0), (1.0, 0.0, 0.0), "the strip's last stop is red again");
    }
}
