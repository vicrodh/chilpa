//! The window's preferences that the window itself changes: density, the
//! sidebar's mode and width. Each one is read once at start and written
//! back when it changes (frontend ADR-0010, ADR-0011).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
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
        /// The right panel: the queue and the lyrics, each open or not, and
        /// the queue's share of the panel when both are.
        #[qproperty(bool, queue_open)]
        #[qproperty(bool, lyrics_open)]
        #[qproperty(i32, queue_height)]
        /// The Settings sections: ids, labels and the names of their glyphs.
        #[qproperty(QStringList, sections)]
        #[qproperty(QStringList, section_labels)]
        #[qproperty(QStringList, section_icons)]
        type SettingsModel = super::SettingsRust;

        /// Settles the sidebar where a drag released it: open at that width,
        /// collapsed or closed.
        #[qinvokable]
        fn place_sidebar(self: Pin<&mut SettingsModel>, width: f64, window_width: f64);
    }

    impl cxx_qt::Threading for SettingsModel {}
    impl cxx_qt::Initialize for SettingsModel {}
}

use crate::app;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

const DENSITY: &str = "appearance.density";
const SIDEBAR_MODE: &str = "ui.sidebar.mode";
const SIDEBAR_WIDTH: &str = "ui.sidebar.width";
const QUEUE_MODE: &str = "ui.queue.mode";
const LYRICS_OPEN: &str = "ui.lyrics.open";
const QUEUE_HEIGHT: &str = "ui.rightPanel.queueHeight";

#[derive(Default)]
pub struct SettingsRust {
    density: QString,
    sidebar_mode: QString,
    sidebar_width: i32,
    queue_open: bool,
    lyrics_open: bool,
    queue_height: i32,
    sections: QStringList,
    section_labels: QStringList,
    section_icons: QStringList,
}

fn strings(items: impl Iterator<Item = String>) -> QStringList {
    let mut l = QList::<QString>::default();
    for i in items {
        l.append(QString::from(&i));
    }
    QStringList::from(&l)
}

impl qobject::SettingsModel {
    pub fn place_sidebar(mut self: Pin<&mut Self>, width: f64, window_width: f64) {
        let (mode, settled) = crate::sidebar::settle(width, window_width);
        if settled > 0 {
            self.as_mut().set_sidebar_width(settled);
        }
        self.as_mut().set_sidebar_mode(QString::from(mode));
    }
}

fn remember(key: &'static str, value: String) {
    app::with(|api| api.remember_preference(key, &value));
}

impl cxx_qt::Initialize for qobject::SettingsModel {
    fn initialize(mut self: Pin<&mut Self>) {
        crate::registry::register_settings(self.qt_thread());
        let (density, mode, width) = app::with(|api| {
            (api.preference(DENSITY), api.preference(SIDEBAR_MODE), api.preference(SIDEBAR_WIDTH))
        });
        let (queue, lyrics, queue_height) = app::with(|api| {
            (api.preference(QUEUE_MODE), api.preference(LYRICS_OPEN), api.preference(QUEUE_HEIGHT))
        });
        {
            let mut this = self.as_mut().rust_mut();
            let all = crate::settings_schema::SECTIONS;
            this.sections = strings(all.iter().map(|s| s.id.to_string()));
            this.section_labels = strings(all.iter().map(|s| app::t(s.label)));
            this.section_icons = strings(all.iter().map(|s| s.icon.to_string()));
            this.density = QString::from(density.as_deref().unwrap_or("compact"));
            this.sidebar_mode = QString::from(mode.as_deref().unwrap_or("open"));
            // Zero means the default width, which the theme knows.
            this.sidebar_width = width.and_then(|w| w.parse().ok()).unwrap_or(0);
            this.queue_open = queue.as_deref() == Some("side");
            this.lyrics_open = lyrics.as_deref() == Some("true");
            this.queue_height = queue_height.and_then(|h| h.parse().ok()).unwrap_or(0);
        }
        self.as_mut().on_density_changed(|m| remember(DENSITY, m.density().to_string())).release();
        self.as_mut()
            .on_sidebar_mode_changed(|m| remember(SIDEBAR_MODE, m.sidebar_mode().to_string()))
            .release();
        self.as_mut()
            .on_sidebar_width_changed(|m| remember(SIDEBAR_WIDTH, m.sidebar_width().to_string()))
            .release();
        self.as_mut()
            .on_queue_open_changed(|m| remember(QUEUE_MODE, if *m.queue_open() { "side" } else { "off" }.to_string()))
            .release();
        self.as_mut().on_lyrics_open_changed(|m| remember(LYRICS_OPEN, m.lyrics_open().to_string())).release();
        self.as_mut()
            .on_queue_height_changed(|m| remember(QUEUE_HEIGHT, m.queue_height().to_string()))
            .release();
    }
}
