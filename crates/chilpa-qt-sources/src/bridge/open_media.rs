//! Native pickers feed one temporary media session without importing it.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h"); type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h"); type QUrl = cxx_qt_lib::QUrl;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, snapshot)]
        #[qproperty(QString, error)]
        #[qproperty(QUrl, art)]
        #[qproperty(bool, loading)]
        #[qproperty(i32, revision)]
        type OpenMedia = super::OpenMediaRust;
        #[qinvokable]
        fn pick(self: Pin<&mut OpenMedia>, kind: &QString);
        #[qinvokable]
        fn open_path(self: Pin<&mut OpenMedia>, kind: &QString, path: &QString);
        #[qinvokable]
        fn open_requested(self: Pin<&mut OpenMedia>);
        #[qinvokable]
        fn close(self: Pin<&mut OpenMedia>);
        #[qinvokable]
        fn refresh(self: Pin<&mut OpenMedia>);
        #[qsignal]
        fn opened(self: Pin<&mut OpenMedia>);
    }
    impl cxx_qt::Threading for OpenMedia {}
}
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;
#[derive(Default)]
pub struct OpenMediaRust {
    snapshot: QString, error: QString, art: QUrl, loading: bool, revision: i32, generation: u64,
}
impl qobject::OpenMedia {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let snapshot = crate::app::with(|api| api.media_json());
        let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap_or_default();
        self.as_mut().set_art(QUrl::from_local_file(&QString::from(value["art"].as_str().unwrap_or_default())));
        let snapshot = QString::from(snapshot);
        if self.snapshot() != &snapshot {
            let revision = self.revision().wrapping_add(1);
            self.as_mut().set_snapshot(snapshot);
            self.set_revision(revision);
        }
    }
    pub fn close(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().generation += 1;
        crate::app::with(|api| api.close_media());
        self.as_mut().set_loading(false);
        self.as_mut().set_error(QString::default());
        self.refresh();
    }
    pub fn open_path(mut self: Pin<&mut Self>, kind: &QString, path: &QString) {
        self.as_mut().rust_mut().generation += 1;
        let generation = self.rust().generation;
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::default());
        let (kind, path) = (kind.to_string(), path.to_string());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = chilpa_api::open_media::read(&kind, &path);
            let _ = thread.queue(move |mut model| {
                if model.rust().generation != generation { return; }
                model.as_mut().set_loading(false);
                match result {
                    Ok(media) => {
                        crate::app::with(|api| api.open_media(media));
                        model.as_mut().refresh();
                        model.opened();
                    }
                    Err(error) => model.set_error(QString::from(error)),
                }
            });
        });
    }
    /// Opens what a launch asked for: the files and folders a file manager
    /// handed over, as one temporary media.
    pub fn open_requested(self: Pin<&mut Self>) {
        if let Some(paths) = crate::app::with(|api| api.take_open_request()) {
            self.open_path(&QString::from("launch"), &QString::from(paths));
        }
    }
    pub fn pick(mut self: Pin<&mut Self>, kind: &QString) {
        if self.rust().loading { return; }
        let kind = kind.to_string();
        if kind == "cd" {
            let drive = crate::app::with(|api| api.cd_drives().first().cloned().unwrap_or_default());
            self.open_path(&QString::from(kind), &QString::from(drive));
            return;
        }
        self.as_mut().rust_mut().generation += 1;
        let generation = self.rust().generation;
        self.as_mut().set_loading(true);
        let title = crate::app::t(if kind == "sacd" { "Open SACD…" } else { "Open a folder" });
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let dialog = rfd::AsyncFileDialog::new().set_title(&title);
            let picked = if kind == "sacd" { pollster::block_on(dialog.add_filter("SACD", &["iso", "ISO"]).pick_file()) }
                else { pollster::block_on(dialog.pick_folder()) };
            let path = picked.map(|p| p.path().to_string_lossy().into_owned());
            let _ = thread.queue(move |mut model| {
                if model.rust().generation != generation { return; }
                model.as_mut().set_loading(false);
                if let Some(path) = path { model.open_path(&QString::from(kind), &QString::from(path)); }
            });
        });
    }
}
