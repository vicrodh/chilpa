//! The tabs and where each one is, as QML reads them (frontend ADR-0005).
//!
//! The history lives in [`crate::navigation`]; this object only publishes
//! it. What a tab is called is the facade's to say.

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

        /// Goes to a screen that is told apart by a text, like a source by
        /// its plugin's id.
        #[qinvokable]
        fn go_key(self: Pin<&mut Navigation>, screen: &QString, key: &QString);

        /// Opens an artist's page inside the active tab.
        #[qinvokable]
        fn open_artist(self: Pin<&mut Navigation>, name: &QString);

        /// Opens the page of one of the artists a credit names: the first
        /// is 0. A place that is none of them opens nothing.
        #[qinvokable]
        fn open_credit(self: Pin<&mut Navigation>, credit: &QString, place: i32);

        /// Where each artist of a credit is inside it, as "0,5,7,22"; empty
        /// when it names one.
        #[qinvokable]
        fn credit_stops(self: &Navigation, credit: &QString) -> QString;

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
use chilpa_api::navigation::{Route, Screen, Tabs};
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
    app::with(|api| api.route_to(&screen.to_string(), id))
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
        let section = chilpa_api::settings_schema::section(&id.to_string()).id;
        self.as_mut().rust_mut().tabs.go(Route { screen: Screen::Settings, id: 0, key: section.to_string() });
        self.publish();
    }

    pub fn go_key(mut self: Pin<&mut Self>, screen: &QString, key: &QString) {
        if let Some(screen) = Screen::from_name(&screen.to_string()).filter(|_| !key.is_empty()) {
            self.as_mut().rust_mut().tabs.go(Route { screen, id: 0, key: key.to_string() });
            self.publish();
        }
    }

    pub fn open_artist(self: Pin<&mut Self>, name: &QString) {
        self.go_key(&QString::from("artist"), name);
    }

    pub fn open_credit(self: Pin<&mut Self>, credit: &QString, place: i32) {
        let names = app::with(|api| api.credit_names(&credit.to_string()));
        if let Some(name) = usize::try_from(place).ok().and_then(|place| names.get(place)) {
            self.open_artist(&QString::from(name));
        }
    }

    pub fn credit_stops(&self, credit: &QString) -> QString {
        QString::from(&app::with(|api| api.credit_stops(&credit.to_string())))
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
            let (title, icon) = app::with(|api| api.tab_label(&r));
            titles.append(QString::from(&title));
            icons.append(QString::from(&icon));
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
        app::with(|api| api.keep_tabs(&tabs));
    }
}

impl cxx_qt::Initialize for qobject::Navigation {
    fn initialize(mut self: Pin<&mut Self>) {
        crate::registry::register_navigation(self.qt_thread());
        if let Some(tabs) = app::with(|api| api.restored_tabs()) {
            self.as_mut().rust_mut().tabs = tabs;
        }
        self.publish();
    }
}
