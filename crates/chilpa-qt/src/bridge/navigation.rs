//! The tabs and where each one is, as QML reads them (frontend ADR-0005).
//!
//! The history lives in [`crate::navigation`]; this object only publishes
//! it. A tab's title and icon are decided here: a record's tab is named
//! after the record.

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
        #[qproperty(QStringList, tab_titles)]
        /// The name of each tab's icon in `Icons`.
        #[qproperty(QStringList, tab_icons)]
        #[qproperty(i32, active)]
        /// The active tab's screen: "home", "library", "album"...
        #[qproperty(QString, screen)]
        /// Which record, when the screen is one.
        #[qproperty(i64, route_id)]
        #[qproperty(bool, can_back)]
        #[qproperty(bool, can_forward)]
        /// The Settings section on screen, when the screen is Settings.
        #[qproperty(QString, section)]
        /// The route's text: the folder, for Folders; the section, for
        /// Settings.
        #[qproperty(QString, route_key)]
        type Navigation = super::NavigationRust;

        /// Opens a folder in the Folders screen, inside the active tab. An
        /// empty path is the top: the library's folders.
        #[qinvokable]
        fn open_folder(self: Pin<&mut Navigation>, path: &QString);

        /// Opens an artist's page inside the active tab.
        #[qinvokable]
        fn open_artist(self: Pin<&mut Navigation>, name: &QString);

        /// Opens a Settings section inside the active tab.
        #[qinvokable]
        fn open_section(self: Pin<&mut Navigation>, id: &QString);

        /// Goes somewhere inside the active tab.
        #[qinvokable]
        fn go(self: Pin<&mut Navigation>, screen: &QString, id: i64);

        /// Opens somewhere in a new tab, right of the active one.
        #[qinvokable]
        fn open(self: Pin<&mut Navigation>, screen: &QString, id: i64);

        #[qinvokable]
        fn back(self: Pin<&mut Navigation>);

        #[qinvokable]
        fn forward(self: Pin<&mut Navigation>);

        #[qinvokable]
        fn activate(self: Pin<&mut Navigation>, index: i32);

        #[qinvokable]
        fn close(self: Pin<&mut Navigation>, index: i32);
    }

    impl cxx_qt::Threading for Navigation {}
    impl cxx_qt::Initialize for Navigation {}
}

use crate::app;
use crate::navigation::{Route, Screen, Tabs};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

#[derive(Default)]
pub struct NavigationRust {
    tab_titles: QStringList,
    tab_icons: QStringList,
    active: i32,
    screen: QString,
    route_id: i64,
    can_back: bool,
    can_forward: bool,
    section: QString,
    route_key: QString,
    tabs: Tabs,
}

fn route(screen: &QString, id: i64) -> Option<Route> {
    Screen::from_name(&screen.to_string()).map(|screen| Route { screen, id, key: String::new() })
}

/// A tab's title and icon.
fn label(r: &Route) -> (String, &'static str) {
    match r.screen {
        Screen::Home => (app::t("Home"), "home"),
        Screen::Library => (app::t("Library"), "columns3"),
        Screen::Artists => (app::t("Artists"), "microphone2"),
        Screen::Albums => (app::t("Albums"), "disc"),
        Screen::Settings => (app::t("Settings"), "settings"),
        Screen::LibraryFolders => (app::t("Library folders"), "folder"),
        Screen::Artist => (r.key.clone(), "user"),
        Screen::Folders => match std::path::Path::new(&r.key).file_name() {
            Some(name) => (name.to_string_lossy().into_owned(), "folder"),
            None => (app::t("Folders"), "folder"),
        },
        Screen::Album => {
            let title = app::with(|api| api.album(r.id).map(|a| a.title)).unwrap_or_default();
            (title, "disc")
        }
        Screen::Playlist => {
            let name = app::with(|api| api.playlist(r.id).map(|p| p.name)).unwrap_or_default();
            (name, "playlist")
        }
    }
}

impl qobject::Navigation {
    pub fn go(mut self: Pin<&mut Self>, screen: &QString, id: i64) {
        if let Some(to) = route(screen, id) {
            self.as_mut().rust_mut().tabs.go(to);
            self.publish();
        }
    }

    pub fn open(mut self: Pin<&mut Self>, screen: &QString, id: i64) {
        if let Some(to) = route(screen, id) {
            self.as_mut().rust_mut().tabs.open(to);
            self.publish();
        }
    }

    pub fn open_section(mut self: Pin<&mut Self>, id: &QString) {
        let section = crate::settings_schema::section(&id.to_string()).id;
        self.as_mut().rust_mut().tabs.go(Route { screen: Screen::Settings, id: 0, key: section.to_string() });
        self.publish();
    }

    pub fn open_artist(mut self: Pin<&mut Self>, name: &QString) {
        let key = name.to_string();
        if !key.is_empty() {
            self.as_mut().rust_mut().tabs.go(Route { screen: Screen::Artist, id: 0, key });
            self.publish();
        }
    }

    pub fn open_folder(mut self: Pin<&mut Self>, path: &QString) {
        let key = path.to_string().trim_end_matches('/').to_string();
        self.as_mut().rust_mut().tabs.go(Route { screen: Screen::Folders, id: 0, key });
        self.publish();
    }

    pub fn back(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().tabs.back();
        self.publish();
    }

    pub fn forward(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().tabs.forward();
        self.publish();
    }

    pub fn activate(mut self: Pin<&mut Self>, index: i32) {
        self.as_mut().rust_mut().tabs.activate(index.max(0) as usize);
        self.publish();
    }

    pub fn close(mut self: Pin<&mut Self>, index: i32) {
        self.as_mut().rust_mut().tabs.close(index.max(0) as usize);
        self.publish();
    }

    /// Reads the tabs' titles again: something they name was renamed.
    pub fn refresh_labels(self: Pin<&mut Self>) {
        self.publish();
    }

    /// Forgets a place that was deleted, from every tab and history.
    pub fn left_behind(mut self: Pin<&mut Self>, screen: &str, id: i64) {
        if let Some(screen) = Screen::from_name(screen) {
            self.as_mut().rust_mut().tabs.forget(&Route { screen, id, key: String::new() });
            self.publish();
        }
    }

    /// Sets every property from the tabs. The screen goes last: it is what
    /// swaps the page, and by then the route id it reads is the new one.
    fn publish(mut self: Pin<&mut Self>) {
        let tabs = self.rust().tabs.clone();
        let (mut titles, mut icons) = (QList::<QString>::default(), QList::<QString>::default());
        for r in tabs.routes() {
            let (title, icon) = label(&r);
            titles.append(QString::from(&title));
            icons.append(QString::from(icon));
        }
        let now = tabs.current();
        self.as_mut().set_tab_titles(QStringList::from(&titles));
        self.as_mut().set_tab_icons(QStringList::from(&icons));
        self.as_mut().set_active(tabs.active() as i32);
        self.as_mut().set_can_back(tabs.can_back());
        self.as_mut().set_can_forward(tabs.can_forward());
        self.as_mut().set_route_id(now.id);
        let section = if now.screen == Screen::Settings { now.key.as_str() } else { "" };
        self.as_mut().set_section(QString::from(section));
        self.as_mut().set_route_key(QString::from(&now.key));
        self.as_mut().set_screen(QString::from(now.screen.name()));
    }
}

impl cxx_qt::Initialize for qobject::Navigation {
    fn initialize(self: Pin<&mut Self>) {
        crate::registry::register_navigation(self.qt_thread());
        self.publish();
    }
}
