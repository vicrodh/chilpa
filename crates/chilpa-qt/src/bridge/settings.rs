//! The window's preferences that the window itself changes: density, the
//! sidebar's mode and width. Each one is read once at start and written
//! back when it changes (frontend ADR-0010, ADR-0011).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "compact" or "rich", for every track list.
        #[qproperty(QString, density)]
        /// "open", "collapsed" or "closed".
        #[qproperty(QString, sidebar_mode)]
        #[qproperty(i32, sidebar_width)]
        type SettingsModel = super::SettingsRust;
    }

    impl cxx_qt::Initialize for SettingsModel {}
}

use crate::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

const DENSITY: &str = "appearance.density";
const SIDEBAR_MODE: &str = "ui.sidebar.mode";
const SIDEBAR_WIDTH: &str = "ui.sidebar.width";

#[derive(Default)]
pub struct SettingsRust {
    density: QString,
    sidebar_mode: QString,
    sidebar_width: i32,
}

fn remember(key: &'static str, value: String) {
    app::with(|api| api.remember_preference(key, &value));
}

impl cxx_qt::Initialize for qobject::SettingsModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let (density, mode, width) = app::with(|api| {
            (api.preference(DENSITY), api.preference(SIDEBAR_MODE), api.preference(SIDEBAR_WIDTH))
        });
        {
            let mut this = self.as_mut().rust_mut();
            this.density = QString::from(density.as_deref().unwrap_or("compact"));
            this.sidebar_mode = QString::from(mode.as_deref().unwrap_or("open"));
            // Zero means the default width, which the theme knows.
            this.sidebar_width = width.and_then(|w| w.parse().ok()).unwrap_or(0);
        }
        self.as_mut().on_density_changed(|m| remember(DENSITY, m.density().to_string())).release();
        self.as_mut()
            .on_sidebar_mode_changed(|m| remember(SIDEBAR_MODE, m.sidebar_mode().to_string()))
            .release();
        self.as_mut()
            .on_sidebar_width_changed(|m| remember(SIDEBAR_WIDTH, m.sidebar_width().to_string()))
            .release();
    }
}
