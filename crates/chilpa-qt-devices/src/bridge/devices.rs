//! I publish progress separately from catalogs and retain the task when its screen closes.
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
        #[qproperty(QString, snapshot)]
        #[qproperty(QString, catalog)]
        #[qproperty(QString, page_rows)]
        #[qproperty(QString, browser)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, has_connected)]
        #[qproperty(i32, page)]
        type DeviceSync = super::DeviceSyncRust;
        #[qinvokable]
        fn enter(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn leave(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn poll(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn refresh(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn connections_changed(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn select_profile(self: Pin<&mut DeviceSync>, id: &QString);
        #[qinvokable]
        fn select_destination(self: Pin<&mut DeviceSync>, id: &QString);
        #[qinvokable]
        fn create_profile(self: Pin<&mut DeviceSync>, name: &QString, folder: &QString);
        #[qinvokable]
        fn toggle(self: Pin<&mut DeviceSync>, id: &QString);
        #[qinvokable]
        fn use_tracks(self: Pin<&mut DeviceSync>, ids: &QString);
        #[qinvokable]
        fn filter(self: Pin<&mut DeviceSync>, text: &QString);
        #[qinvokable]
        fn selection_kind(self: Pin<&mut DeviceSync>, kind: &QString);
        #[qinvokable]
        fn selection_page(self: Pin<&mut DeviceSync>, page: i32);
        #[qinvokable]
        fn removals(self: Pin<&mut DeviceSync>, enabled: bool);
        #[qinvokable]
        fn preview(self: Pin<&mut DeviceSync>, recover: bool);
        #[qinvokable]
        fn apply(self: Pin<&mut DeviceSync>, token: &QString, approve_removals: bool);
        #[qinvokable]
        fn cancel(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn change_page(self: Pin<&mut DeviceSync>, page: i32);
        #[qinvokable]
        fn browse(self: Pin<&mut DeviceSync>, source: &QString);
        #[qinvokable]
        fn open_browser_album(self: Pin<&mut DeviceSync>, id: &QString);
        #[qinvokable]
        fn browser_back(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn filter_browser(self: Pin<&mut DeviceSync>, text: &QString);
        #[qinvokable]
        fn browser_view(self: Pin<&mut DeviceSync>, view: &QString);
        #[qinvokable]
        fn browser_page(self: Pin<&mut DeviceSync>, page: i32);
        #[qinvokable]
        fn close_browser(self: Pin<&mut DeviceSync>);
        #[qinvokable]
        fn reload_browser(self: Pin<&mut DeviceSync>);
        #[qsignal]
        fn profile_created(self: Pin<&mut DeviceSync>);
    }
}
use chilpa_api::device_sync::{Service, UiError};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct DeviceSyncRust {
    snapshot: QString,
    catalog: QString,
    page_rows: QString,
    browser: QString,
    busy: bool,
    has_connected: bool,
    page: i32,
    service: Service,
    filter: String,
    selection_kind: String,
    selection_page: usize,
    published_revision: Option<u64>,
    viewers: usize,
    connection_pending: bool,
}
impl qobject::DeviceSync {
    fn run(mut self: Pin<&mut Self>, command: impl FnOnce(&mut Service) -> Result<(), UiError>) {
        let result = command(&mut self.as_mut().rust_mut().service);
        if let Err(error) = result {
            self.as_mut().rust_mut().service.show_error(error);
        }
        self.publish(false);
    }
    fn publish(mut self: Pin<&mut Self>, force: bool) {
        let status = QString::from(&self.rust().service.status_json());
        let busy = self.rust().service.status().busy;
        let revision = self.rust().service.revision();
        if self.rust().viewers > 0 && (force || self.rust().published_revision != Some(revision)) {
            let catalog = QString::from(&self.rust().service.catalog_page_json(&self.rust().filter, &self.rust().selection_kind, self.rust().selection_page));
            let rows = QString::from(&self.rust().service.changes_json(*self.page() as usize));
            let browser = QString::from(&self.rust().service.browser_json());
            let connected = self
                .rust()
                .service
                .destinations()
                .iter()
                .any(|d| d.id != "mounted");
            self.as_mut().rust_mut().published_revision = Some(revision);
            if self.catalog() != &catalog {
                self.as_mut().set_catalog(catalog);
            }
            if self.page_rows() != &rows {
                self.as_mut().set_page_rows(rows);
            }
            if self.browser() != &browser {
                self.as_mut().set_browser(browser);
            }
            if *self.has_connected() != connected {
                self.as_mut().set_has_connected(connected);
            }
        }
        if self.snapshot() != &status {
            self.as_mut().set_snapshot(status);
        }
        if *self.busy() != busy {
            self.as_mut().set_busy(busy);
        }
    }
    pub fn enter(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().viewers += 1;
        if let Err(error) = chilpa_qt_kit::app::with(|api| api.open_device_library()) {
            self.as_mut().rust_mut().service.show_error(error);
            self.publish(true);
            return;
        }
        self.run(Service::open_view);
    }
    pub fn leave(mut self: Pin<&mut Self>) {
        let viewers = self.rust().viewers.saturating_sub(1);
        self.as_mut().rust_mut().viewers = viewers;
        if viewers > 0 {
            return;
        }
        self.as_mut().rust_mut().connection_pending = false;
        self.as_mut().rust_mut().service.release_view();
        self.as_mut().rust_mut().filter = String::new();
        self.as_mut().rust_mut().selection_kind = String::new();
        self.as_mut().rust_mut().selection_page = 0;
        self.as_mut().set_catalog(QString::default());
        self.as_mut().set_page_rows(QString::default());
        self.as_mut().set_browser(QString::default());
        self.as_mut().rust_mut().published_revision = None;
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        let before = self.rust().service.status().clone();
        self.as_mut().rust_mut().service.poll();
        let created = before.phase == "pairing"
            && self.rust().service.status().phase == "idle"
            && before.profile != self.rust().service.status().profile;
        if self.rust().viewers > 0
            && !self.rust().service.status().busy
            && self.rust().service.status().error.is_none()
        {
            let result = self.as_mut().rust_mut().service.open_view();
            if let Err(error) = result {
                self.as_mut().rust_mut().service.show_error(error);
            }
        }
        if self.rust().viewers > 0 && self.rust().connection_pending && !self.rust().service.status().busy {
            self.as_mut().rust_mut().connection_pending = false;
            self.as_mut().run(Service::connections_changed);
        }
        self.as_mut().publish(false);
        if created {
            self.profile_created();
        }
    }
    pub fn connections_changed(mut self: Pin<&mut Self>) {
        if self.rust().viewers == 0 { return; }
        if self.rust().service.status().busy {
            self.as_mut().rust_mut().connection_pending = true;
        } else {
            self.run(Service::connections_changed);
        }
    }
    pub fn refresh(self: Pin<&mut Self>) {
        self.run(Service::refresh);
    }
    pub fn select_profile(self: Pin<&mut Self>, id: &QString) {
        self.run(|s| s.select_profile(&id.to_string()));
    }
    pub fn select_destination(self: Pin<&mut Self>, id: &QString) {
        self.run(|s| s.select_destination(&id.to_string()));
    }
    pub fn create_profile(self: Pin<&mut Self>, name: &QString, folder: &QString) {
        self.run(|s| s.add_profile(&name.to_string(), &folder.to_string()));
    }
    pub fn toggle(self: Pin<&mut Self>, id: &QString) {
        self.run(|s| s.toggle(&id.to_string()));
    }
    pub fn use_tracks(mut self: Pin<&mut Self>, ids: &QString) {
        self.as_mut().rust_mut().filter.clear();
        self.as_mut().rust_mut().selection_kind = "tracks".into();
        self.as_mut().rust_mut().selection_page = 0;
        let active = self.rust().viewers > 0;
        self.as_mut().run(|service| {
            service.stage_library_tracks(&ids.to_string())?;
            if active {
                service.open_view()?;
            }
            Ok(())
        });
    }
    pub fn filter(mut self: Pin<&mut Self>, text: &QString) {
        if self.rust().viewers == 0 { return; }
        self.as_mut().rust_mut().filter = text.to_string().chars().take(256).collect();
        self.as_mut().rust_mut().selection_page = 0;
        self.publish(true);
    }
    pub fn selection_kind(mut self: Pin<&mut Self>, kind: &QString) {
        if self.rust().viewers == 0 { return; }
        self.as_mut().rust_mut().selection_kind = match kind.to_string().as_str() {
            "playlists" => "playlists", "queries" => "queries", "tracks" => "tracks", _ => "all",
        }.into();
        self.as_mut().rust_mut().selection_page = 0;
        self.publish(true);
    }
    pub fn selection_page(mut self: Pin<&mut Self>, page: i32) {
        if self.rust().viewers == 0 { return; }
        self.as_mut().rust_mut().selection_page = page.max(0) as usize;
        self.publish(true);
    }
    pub fn removals(self: Pin<&mut Self>, enabled: bool) {
        self.run(|s| s.set_removals(enabled));
    }
    pub fn preview(mut self: Pin<&mut Self>, recover: bool) {
        self.as_mut().set_page(0);
        self.run(|s| s.preview(recover));
    }
    pub fn apply(self: Pin<&mut Self>, token: &QString, approve_removals: bool) {
        self.run(|s| s.apply(&token.to_string(), approve_removals));
    }
    pub fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().service.cancel();
        self.publish(false);
    }
    pub fn change_page(mut self: Pin<&mut Self>, page: i32) {
        if self.rust().viewers == 0 {
            return;
        }
        let maximum = self.rust().service.change_count().saturating_sub(1) / 200;
        self.as_mut()
            .set_page((page.max(0) as usize).min(maximum) as i32);
        let rows = QString::from(&self.rust().service.changes_json(*self.page() as usize));
        self.set_page_rows(rows);
    }
    pub fn browse(self: Pin<&mut Self>, source: &QString) {
        if self.rust().viewers == 0 {
            return;
        }
        self.run(|s| s.browse(&source.to_string()));
    }
    pub fn open_browser_album(self: Pin<&mut Self>, id: &QString) {
        if self.rust().viewers == 0 {
            return;
        }
        self.run(|s| s.open_browser_album(&id.to_string()));
    }
    pub fn browser_back(mut self: Pin<&mut Self>) {
        if self.rust().viewers == 0 {
            return;
        }
        self.as_mut().rust_mut().service.browser_back();
        self.publish(false);
    }
    pub fn filter_browser(mut self: Pin<&mut Self>, text: &QString) {
        if self.rust().viewers == 0 {
            return;
        }
        self.as_mut()
            .rust_mut()
            .service
            .filter_browser(&text.to_string());
        self.publish(false);
    }
    pub fn browser_view(self: Pin<&mut Self>, view: &QString) {
        if self.rust().viewers == 0 { return; }
        self.run(|s| s.browser_view(&view.to_string()));
    }
    pub fn browser_page(mut self: Pin<&mut Self>, page: i32) {
        if self.rust().viewers == 0 {
            return;
        }
        self.as_mut()
            .rust_mut()
            .service
            .browser_page(page.max(0) as usize);
        self.publish(false);
    }
    pub fn close_browser(mut self: Pin<&mut Self>) {
        if self.rust().viewers == 0 {
            return;
        }
        self.as_mut().rust_mut().service.close_browser();
        self.publish(false);
    }
    pub fn reload_browser(self: Pin<&mut Self>) {
        if self.rust().viewers == 0 { return; }
        self.run(Service::reload_browser);
    }
}
