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
        /// the queue's share of the panel when both are, in thousandths; zero
        /// is half.
        #[qproperty(bool, queue_open)]
        #[qproperty(bool, lyrics_open)]
        #[qproperty(i32, queue_share)]
        /// The Library screen: its filter columns shown or not, their
        /// height, and "tracks" or "grid".
        #[qproperty(bool, panes_open)]
        #[qproperty(i32, panes_height)]
        #[qproperty(QString, library_view)]
        /// The player bar's −10 s and +10 s.
        #[qproperty(bool, extra_controls)]
        /// The player bar steps away after a while without the mouse moving.
        #[qproperty(bool, hide_player)]
        /// A middle click on a destination opens it in a new tab.
        #[qproperty(bool, middle_click_tabs)]
        /// A click on a record opens its quick view, not its page.
        #[qproperty(bool, quick_view_click)]
        /// Sidebar playlist and folder artwork preferences.
        #[qproperty(bool, playlist_collage)]
        #[qproperty(bool, folder_colors)]
        #[qproperty(QString, album_header)]
        /// The Settings sections: ids, labels and the names of their glyphs.
        #[qproperty(QStringList, sections)]
        #[qproperty(QStringList, section_labels)]
        #[qproperty(QStringList, section_icons)]
        type SettingsModel = super::SettingsRust;

        /// Settles the sidebar where a drag released it: open at that width,
        /// collapsed or closed.
        #[qinvokable]
        fn place_sidebar(self: Pin<&mut SettingsModel>, width: f64, window_width: f64);

        /// Where a drag at `width` would land, for the sidebar to show while
        /// it is dragged: "open", "collapsed" or "closed".
        #[qinvokable]
        fn sidebar_mode_at(self: &SettingsModel, width: f64, window_width: f64) -> QString;

        /// The width open at `width` would have: `width`, kept between the
        /// minimum and 40 % of the window.
        #[qinvokable]
        fn sidebar_width_at(self: &SettingsModel, width: f64, window_width: f64) -> i32;
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
const QUEUE_SHARE: &str = "ui.rightPanel.queueShare";
const PANES_OPEN: &str = "ui.library.panesOpen";
const PANES_HEIGHT: &str = "ui.library.panesHeight";
const LIBRARY_VIEW: &str = "ui.library.view";
const EXTRA_CONTROLS: &str = "playback.extra";
const MIDDLE_CLICK_TABS: &str = "navigation.newtab";
const PLAYLIST_ARTWORK: &str = "navigation.playlistArtwork";
const FOLDER_COLORS: &str = "navigation.folderColors";
const ALBUM_HEADER: &str = "appearance.albumHeader";

#[derive(Default)]
pub struct SettingsRust {
    density: QString,
    sidebar_mode: QString,
    sidebar_width: i32,
    queue_open: bool,
    lyrics_open: bool,
    queue_share: i32,
    panes_open: bool,
    panes_height: i32,
    library_view: QString,
    extra_controls: bool,
    hide_player: bool,
    middle_click_tabs: bool,
    quick_view_click: bool,
    playlist_collage: bool,
    folder_colors: bool,
    album_header: QString,
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
    pub fn sidebar_mode_at(&self, width: f64, window_width: f64) -> QString {
        QString::from(chilpa_api::sidebar::settle(width, window_width).0)
    }

    pub fn sidebar_width_at(&self, width: f64, window_width: f64) -> i32 {
        chilpa_api::sidebar::settle(width, window_width).1
    }

    pub fn place_sidebar(mut self: Pin<&mut Self>, width: f64, window_width: f64) {
        let (mode, settled) = chilpa_api::sidebar::settle(width, window_width);
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
        let (density, mode, width) = app::with(|api| (api.preference(DENSITY), api.preference(SIDEBAR_MODE), api.preference(SIDEBAR_WIDTH)));
        let (queue, lyrics, queue_share) = app::with(|api| (api.preference(QUEUE_MODE), api.preference(LYRICS_OPEN), api.preference(QUEUE_SHARE)));
        let (panes, panes_height, view) = app::with(|api| (api.preference(PANES_OPEN), api.preference(PANES_HEIGHT), api.preference(LIBRARY_VIEW)));
        let (extra, middle, click, playlist_artwork, folder_colors, album_header) = app::with(|api| (api.preference(EXTRA_CONTROLS), api.preference(MIDDLE_CLICK_TABS), api.preference("navigation.albumClick"), api.preference(PLAYLIST_ARTWORK), api.preference(FOLDER_COLORS), api.preference(ALBUM_HEADER)));
        {
            let mut this = self.as_mut().rust_mut();
            let all = chilpa_api::settings_schema::SECTIONS;
            this.sections = strings(all.iter().map(|s| s.id.to_string()));
            this.section_labels = strings(all.iter().map(|s| app::t(s.label)));
            this.section_icons = strings(all.iter().map(|s| s.icon.to_string()));
            this.density = QString::from(density.as_deref().unwrap_or("compact"));
            this.sidebar_mode = QString::from(mode.as_deref().unwrap_or("open"));
            // Zero means the default width, which the theme knows.
            this.sidebar_width = width.and_then(|w| w.parse().ok()).unwrap_or(0);
            this.queue_open = queue.as_deref() == Some("side");
            this.lyrics_open = lyrics.as_deref() == Some("true");
            this.queue_share = queue_share.and_then(|h| h.parse().ok()).unwrap_or(0);
            this.panes_open = panes.as_deref() != Some("false");
            this.panes_height = panes_height.and_then(|h| h.parse().ok()).unwrap_or(0);
            this.library_view = QString::from(view.as_deref().unwrap_or("tracks"));
            this.extra_controls = extra.as_deref() == Some("true");
            this.hide_player = app::with(|api| api.preference("playback.autoHide")).as_deref() == Some("true");
            this.middle_click_tabs = middle.as_deref() != Some("false");
            this.quick_view_click = click.as_deref() == Some("quick");
            this.playlist_collage = playlist_artwork.as_deref() == Some("collage");
            this.folder_colors = folder_colors.as_deref() == Some("true");
            this.album_header = QString::from(album_header.as_deref().unwrap_or("full"));
        }
        self.as_mut().on_density_changed(|m| remember(DENSITY, m.density().to_string())).release();
        self.as_mut().on_sidebar_mode_changed(|m| remember(SIDEBAR_MODE, m.sidebar_mode().to_string())).release();
        self.as_mut().on_sidebar_width_changed(|m| remember(SIDEBAR_WIDTH, m.sidebar_width().to_string())).release();
        self.as_mut().on_queue_open_changed(|m| remember(QUEUE_MODE, if *m.queue_open() { "side" } else { "off" }.to_string())).release();
        self.as_mut().on_lyrics_open_changed(|m| remember(LYRICS_OPEN, m.lyrics_open().to_string())).release();
        self.as_mut().on_queue_share_changed(|m| remember(QUEUE_SHARE, m.queue_share().to_string())).release();
        self.as_mut().on_panes_open_changed(|m| remember(PANES_OPEN, m.panes_open().to_string())).release();
        self.as_mut().on_panes_height_changed(|m| remember(PANES_HEIGHT, m.panes_height().to_string())).release();
        self.as_mut().on_library_view_changed(|m| remember(LIBRARY_VIEW, m.library_view().to_string())).release();
        self.as_mut().on_extra_controls_changed(|m| remember(EXTRA_CONTROLS, m.extra_controls().to_string())).release();
        self.as_mut().on_middle_click_tabs_changed(|m| remember(MIDDLE_CLICK_TABS, m.middle_click_tabs().to_string())).release();
        self.as_mut().on_playlist_collage_changed(|m| remember(PLAYLIST_ARTWORK, if *m.playlist_collage() { "collage" } else { "icon" }.to_string())).release();
        self.as_mut().on_folder_colors_changed(|m| remember(FOLDER_COLORS, m.folder_colors().to_string())).release();
        self.as_mut().on_album_header_changed(|m| remember(ALBUM_HEADER, m.album_header().to_string())).release();
    }
}
