//! The active theme, as QML reads it.
//!
//! `Theme.qml` turns each token into a typed property by calling `color`
//! once; `rev` is what makes those bindings run again when the theme, the
//! system scheme or the accent changes, so the window repaints in place.

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
        /// Bumped on every change of colors.
        #[qproperty(i32, rev)]
        /// True when the theme in use is a dark one.
        #[qproperty(bool, dark)]
        /// "light", "dark" or "system", as chosen.
        #[qproperty(QString, mode, READ = mode, WRITE = choose_mode, NOTIFY = mode_changed)]
        /// What the desktop asks for; QML binds it to the style hints.
        #[qproperty(bool, system_dark, READ = system_dark, WRITE = follow_system, NOTIFY = system_dark_changed)]
        type ThemeModel = super::ThemeModelRust;

        #[qinvokable]
        fn color(self: &ThemeModel, token: &QString, rev: i32) -> QColor;

        /// One of the colors a cover shows while it loads.
        #[qinvokable]
        fn placeholder(self: &ThemeModel, index: i32, rev: i32) -> QColor;

        /// The color of one layer of a named shadow.
        #[qinvokable]
        fn shadow_color(self: &ThemeModel, name: &QString, layer: i32, rev: i32) -> QColor;

        /// The system's window controls, that far under the top; false to ask again.
        #[qinvokable]
        fn center_window_controls(self: &ThemeModel, middle: f64) -> bool;

        /// Replaces the accent with a `#RRGGBB` color and remembers it.
        /// Empty goes back to the theme's own.
        #[qinvokable]
        fn choose_accent(self: Pin<&mut ThemeModel>, hex: &QString);

        #[cxx_name = "mode"]
        fn mode(self: &ThemeModel) -> QString;
        #[cxx_name = "setMode"]
        fn choose_mode(self: Pin<&mut ThemeModel>, mode: QString);
        #[qsignal]
        #[cxx_name = "modeChanged"]
        fn mode_changed(self: Pin<&mut ThemeModel>);

        #[cxx_name = "systemDark"]
        fn system_dark(self: &ThemeModel) -> bool;
        #[cxx_name = "setSystemDark"]
        fn follow_system(self: Pin<&mut ThemeModel>, dark: bool);
        #[qsignal]
        #[cxx_name = "systemDarkChanged"]
        fn system_dark_changed(self: Pin<&mut ThemeModel>);
    }

    impl cxx_qt::Threading for ThemeModel {}
    impl cxx_qt::Initialize for ThemeModel {}
}

use crate::app;
use chilpa_api::theme::{for_choice, parse_color, Rgba, Theme};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QColor, QString};
use std::pin::Pin;

const MODE_KEY: &str = "appearance.theme";
const ACCENT_KEY: &str = "appearance.accent";

pub struct ThemeModelRust {
    rev: i32,
    dark: bool,
    mode: QString,
    system_dark: bool,
    theme: Theme,
}

impl Default for ThemeModelRust {
    fn default() -> Self {
        let (mode, accent) = app::with(|api| (api.preference(MODE_KEY), api.preference(ACCENT_KEY)));
        let mode = mode.unwrap_or_else(|| "system".to_string());
        let mut theme = Theme::built_in(for_choice(&mode, false));
        theme.set_accent(accent.as_deref().and_then(parse_color));
        Self { rev: 0, dark: theme.dark, mode: QString::from(&mode), system_dark: false, theme }
    }
}

fn qcolor(c: Rgba) -> QColor {
    QColor::from_rgba_f(c.r as f32, c.g as f32, c.b as f32, c.a as f32)
}

impl qobject::ThemeModel {
    pub fn color(&self, token: &QString, _rev: i32) -> QColor {
        // Every token is checked when the theme loads, so a miss here is a
        // typo in Theme.qml: it shows as a transparent area, and the theme
        // test lists the names that exist.
        self.theme.color(&token.to_string()).map(qcolor).unwrap_or_default()
    }

    pub fn center_window_controls(&self, middle: f64) -> bool {
        chilpa_api::window::center_window_controls(middle)
    }

    pub fn placeholder(&self, index: i32, _rev: i32) -> QColor {
        qcolor(self.theme.placeholder(index.unsigned_abs() as usize))
    }

    pub fn shadow_color(&self, name: &QString, layer: i32, _rev: i32) -> QColor {
        let layer = layer.max(0) as usize;
        self.theme.shadow(&name.to_string(), layer).map(|s| qcolor(s.color)).unwrap_or_default()
    }

    pub fn choose_accent(mut self: Pin<&mut Self>, hex: &QString) {
        let hex = hex.to_string();
        app::with(|api| api.remember_preference(ACCENT_KEY, &hex));
        self.as_mut().rust_mut().theme.set_accent(parse_color(&hex));
        self.bump();
    }

    pub fn mode(&self) -> QString {
        self.rust().mode.clone()
    }

    pub fn choose_mode(mut self: Pin<&mut Self>, mode: QString) {
        if mode == self.rust().mode {
            return;
        }
        let text = mode.to_string();
        app::with(|api| api.remember_preference(MODE_KEY, &text));
        self.as_mut().rust_mut().mode = mode;
        self.as_mut().mode_changed();
        self.apply();
    }

    pub fn system_dark(&self) -> bool {
        self.rust().system_dark
    }

    pub fn follow_system(mut self: Pin<&mut Self>, dark: bool) {
        if dark == self.rust().system_dark {
            return;
        }
        self.as_mut().rust_mut().system_dark = dark;
        self.as_mut().system_dark_changed();
        self.apply();
    }

    /// Loads the theme the mode and the system ask for, keeping the accent.
    fn apply(mut self: Pin<&mut Self>) {
        let id = for_choice(&self.rust().mode.to_string(), self.rust().system_dark);
        if id == self.rust().theme.id() {
            return;
        }
        let accent = app::with(|api| api.preference(ACCENT_KEY));
        let mut theme = Theme::built_in(id);
        theme.set_accent(accent.as_deref().and_then(parse_color));
        let dark = theme.dark;
        self.as_mut().rust_mut().theme = theme;
        self.as_mut().set_dark(dark);
        self.bump();
    }

    fn bump(mut self: Pin<&mut Self>) {
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::ThemeModel {
    fn initialize(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        THREAD.with(|slot| *slot.borrow_mut() = Some(thread));
    }
}

thread_local! {
    /// The theme's thread, for whoever has to ask it something.
    static THREAD: std::cell::RefCell<Option<cxx_qt::CxxQtThread<qobject::ThemeModel>>> = const { std::cell::RefCell::new(None) };
}

/// Queues `f` on the theme, from another object: the settings page
/// changing the mode or the accent. Dropped if the theme has not started,
/// which only happens before the window is up.
pub fn ask(f: impl FnOnce(Pin<&mut qobject::ThemeModel>) + Send + 'static) {
    THREAD.with(|slot| {
        if let Some(thread) = slot.borrow().as_ref() {
            let _ = thread.queue(f);
        }
    });
}
