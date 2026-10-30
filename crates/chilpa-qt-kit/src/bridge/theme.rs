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
        /// The theme chosen: Chilpa's "light", "dark", "oled" or "system",
        /// a theme of QBZ's list, "auto" or "custom".
        #[qproperty(QString, mode, READ = mode, WRITE = choose_mode, NOTIFY = mode_changed)]
        /// What the desktop asks for; QML binds it to the style hints.
        #[qproperty(bool, system_dark, READ = system_dark, WRITE = follow_system, NOTIFY = system_dark_changed)]
        /// True while the dynamic background shows through the chrome: the
        /// one predicate every surface that goes translucent reads.
        #[qproperty(bool, ambient, READ = ambient, WRITE = set_ambient, NOTIFY = ambient_changed)]
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

        /// The desktop's preferred fixed-width font for command text.
        #[qinvokable]
        fn fixed_font_family(self: &ThemeModel) -> QString;

        /// What GNOME, GTK or KDE say about dark colours, for when Qt's
        /// style hints report no scheme (a bare Wayland compositor).
        #[qinvokable]
        fn desktop_prefers_dark(self: &ThemeModel) -> bool;

        /// Replaces the accent with a `#RRGGBB` color and remembers it.
        /// Empty goes back to the theme's own.
        #[qinvokable]
        fn choose_accent(self: Pin<&mut ThemeModel>, hex: &QString);

        /// Reads the chosen theme again: the custom one was edited, or the
        /// auto one generated.
        #[qinvokable]
        fn refresh(self: Pin<&mut ThemeModel>);

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

        #[cxx_name = "ambient"]
        fn ambient(self: &ThemeModel) -> bool;
        #[cxx_name = "setAmbient"]
        fn set_ambient(self: Pin<&mut ThemeModel>, on: bool);
        #[qsignal]
        #[cxx_name = "ambientChanged"]
        fn ambient_changed(self: Pin<&mut ThemeModel>);
    }

    impl cxx_qt::Threading for ThemeModel {}
    impl cxx_qt::Initialize for ThemeModel {}
}

use crate::app;
use chilpa_api::theme::choice::{self, auto, custom};
use chilpa_api::theme::{Rgba, Theme};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QColor, QString};
use std::pin::Pin;

pub struct ThemeModelRust {
    rev: i32,
    dark: bool,
    mode: QString,
    system_dark: bool,
    ambient: bool,
    theme: Theme,
}

impl Default for ThemeModelRust {
    fn default() -> Self {
        let (mode, theme) = app::with(|api| (api.preference(choice::THEME), api.theme(false)));
        let mode = mode.unwrap_or_else(|| "system".to_string());
        Self { rev: 0, dark: theme.dark, mode: QString::from(&mode), system_dark: false, ambient: false, theme }
    }
}

fn qcolor(c: Rgba) -> QColor {
    QColor::from_rgba_f(c.r as f32, c.g as f32, c.b as f32, c.a as f32)
}

impl qobject::ThemeModel {
    pub fn desktop_prefers_dark(&self) -> bool {
        app::with(|api| api.desktop_prefers_dark())
    }

    pub fn fixed_font_family(&self) -> QString {
        unsafe extern "C" { fn chilpa_fixed_font_family() -> *const std::ffi::c_char; }
        // SAFETY: Qt keeps this immutable, NUL-terminated family for the process.
        let family = unsafe { std::ffi::CStr::from_ptr(chilpa_fixed_font_family()) };
        QString::from(family.to_str().unwrap_or("monospace"))
    }

    pub fn color(&self, token: &QString, _rev: i32) -> QColor {
        // Every token is checked when the theme loads, so a miss here is a
        // typo in Theme.qml: it shows as a transparent area, and the theme
        // test lists the names that exist.
        self.theme.color_under(&token.to_string(), self.rust().ambient).map(qcolor).unwrap_or_default()
    }

    pub fn ambient(&self) -> bool {
        self.rust().ambient
    }

    /// The background came on or went off: the text tokens a light theme
    /// hardens under it are read again.
    pub fn set_ambient(mut self: Pin<&mut Self>, on: bool) {
        if on == self.rust().ambient {
            return;
        }
        self.as_mut().rust_mut().ambient = on;
        self.as_mut().ambient_changed();
        self.bump();
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

    pub fn choose_accent(self: Pin<&mut Self>, hex: &QString) {
        let hex = hex.to_string();
        app::with(|api| api.remember_preference(choice::ACCENT, &hex));
        self.refresh();
    }

    pub fn mode(&self) -> QString {
        self.rust().mode.clone()
    }

    /// QBZ's `theme_set`: choosing Custom reads its file again, and the
    /// first time, before there is one, seeds it from the theme on screen,
    /// so Custom starts from what is shown and not from black. Choosing
    /// Auto generates it again.
    pub fn choose_mode(mut self: Pin<&mut Self>, mode: QString) {
        if mode == self.rust().mode {
            return;
        }
        let text = mode.to_string();
        if text == choice::CUSTOM {
            custom::forget();
            if !custom::exists() {
                custom::seed(self.rust().theme.seed_base());
            }
        }
        app::with(|api| api.remember_preference(choice::THEME, &text));
        self.as_mut().rust_mut().mode = mode;
        self.as_mut().mode_changed();
        self.as_mut().refresh();
        if text == choice::AUTO {
            self.regenerate();
        }
    }

    /// "Use current colors": the custom theme takes the colours on screen.
    pub fn seed_custom(self: Pin<&mut Self>) {
        custom::seed(self.rust().theme.seed_base());
        self.refresh();
    }

    /// Generates the auto theme off this thread; it is read again when done.
    pub fn regenerate(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        auto::regenerate(move || {
            let _ = thread.queue(|theme| theme.refresh());
        });
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
        self.refresh();
    }

    /// Loads the theme the mode and the system ask for, with the accent.
    pub fn refresh(mut self: Pin<&mut Self>) {
        let system_dark = self.rust().system_dark;
        let theme = app::with(|api| api.theme(system_dark));
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
    /// The auto theme starts from the colours the cache kept and is
    /// generated again behind them, as QBZ generates it at every start.
    fn initialize(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        THREAD.with(|slot| *slot.borrow_mut() = Some(thread));
        if self.rust().mode == QString::from(choice::AUTO) {
            self.regenerate();
        }
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
