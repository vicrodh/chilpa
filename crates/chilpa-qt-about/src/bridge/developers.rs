//! Manual developer snapshots and reviewed exports; no polling crosses Qt.
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
        #[qproperty(QString, diagnostics)]
        #[qproperty(QString, logs)]
        #[qproperty(QString, review)]
        #[qproperty(QString, status)]
        #[qproperty(QString, uploaded_url)]
        #[qproperty(bool, uploading)]
        type Developers = super::DevelopersRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut Developers>);
        #[qinvokable]
        fn refresh_logs(self: Pin<&mut Developers>, level: &QString, search: &QString);
        #[qinvokable]
        fn clear_logs(self: Pin<&mut Developers>);
        #[qinvokable]
        fn release(self: Pin<&mut Developers>);
        #[qinvokable]
        fn release_logs(self: Pin<&mut Developers>);
        #[qinvokable]
        fn prepare_report(self: Pin<&mut Developers>);
        #[qinvokable]
        fn close_report(self: Pin<&mut Developers>);
        #[qinvokable]
        fn copy_report(self: Pin<&mut Developers>);
        #[qinvokable]
        fn export_report(self: Pin<&mut Developers>);
        #[qinvokable]
        fn copy_logs(self: Pin<&mut Developers>, level: &QString, search: &QString);
        #[qinvokable]
        fn copy_bundle(self: Pin<&mut Developers>);
        #[qinvokable]
        fn upload_bundle(self: Pin<&mut Developers>);
        #[qinvokable]
        fn copy_url(self: Pin<&mut Developers>);
        #[qinvokable]
        fn log_url(self: &Developers) -> QString;
    }
    impl cxx_qt::Threading for Developers {}
}
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use std::{ffi::{CStr, CString}, pin::Pin};
use chilpa_qt_kit::app;
use chilpa_api::developer;
unsafe extern "C" {
    fn chilpa_developer_native_facts() -> *const std::ffi::c_char;
    fn chilpa_developer_clipboard(text: *const std::ffi::c_char);
}
#[derive(Default)]
pub struct DevelopersRust { diagnostics: QString, logs: QString, review: QString, status: QString, uploaded_url: QString, uploading: bool }
fn native_snapshot() -> String {
    // SAFETY: I call Qt on its UI thread and copy the static native buffer immediately.
    let native = unsafe { CStr::from_ptr(chilpa_developer_native_facts()) }.to_string_lossy();
    app::with(|api| api.developer_document(&native, env!("CHILPA_BUILD_DATE"), env!("CHILPA_BUILD_COMMIT")))
}
impl qobject::Developers {
    pub fn refresh(mut self: Pin<&mut Self>) {
        self.as_mut().set_diagnostics(QString::from(&native_snapshot()));
    }
    pub fn refresh_logs(mut self: Pin<&mut Self>, level: &QString, search: &QString) {
        self.as_mut().set_logs(QString::from(&developer::logs::document(&level.to_string(), &search.to_string())));
    }
    pub fn clear_logs(mut self: Pin<&mut Self>) {
        developer::logs::clear(); self.as_mut().refresh_logs(&QString::from("all"), &QString::default());
    }
    pub fn release(mut self: Pin<&mut Self>) {
        self.as_mut().set_diagnostics(QString::default()); self.as_mut().release_logs(); self.as_mut().close_report();
    }
    pub fn release_logs(mut self: Pin<&mut Self>) { self.as_mut().set_logs(QString::default()); }
    pub fn prepare_report(mut self: Pin<&mut Self>) {
        let report=developer::report(&native_snapshot());
        self.as_mut().set_review(QString::from(&report)); self.as_mut().set_status(QString::default());
    }
    pub fn close_report(mut self: Pin<&mut Self>) { self.as_mut().set_review(QString::default()); self.as_mut().set_status(QString::default()); }
    pub fn copy_report(mut self: Pin<&mut Self>) {
        let text = self.review().to_string();
        self.as_mut().copy_text(&text);
    }
    /// Every row the viewer's filter matches, as plain text.
    pub fn copy_logs(mut self: Pin<&mut Self>, level: &QString, search: &QString) {
        let text = developer::logs::filtered_text(&level.to_string(), &search.to_string());
        self.as_mut().copy_text(&text);
    }
    /// The shareable block: a header, the diagnostics report and the last log lines.
    pub fn copy_bundle(mut self: Pin<&mut Self>) {
        let text = developer::bundle(&developer::report(&native_snapshot()));
        self.as_mut().copy_text(&text);
    }
    pub fn copy_url(mut self: Pin<&mut Self>) {
        let text = self.uploaded_url().to_string();
        if !text.is_empty() { self.as_mut().copy_text(&text); }
    }
    fn copy_text(mut self: Pin<&mut Self>, text: &str) {
        if let Ok(text) = CString::new(text) {
            // SAFETY: the UTF-8 buffer lives through this synchronous UI-thread call.
            unsafe { chilpa_developer_clipboard(text.as_ptr()); }
            self.as_mut().set_status(QString::from(&app::t("Copied")));
        }
    }
    /// Posts the bundle to a public paste. The request runs on a thread of
    /// its own and the address is handed back to the window's thread; one
    /// upload at a time.
    pub fn upload_bundle(mut self: Pin<&mut Self>) {
        if *self.uploading() { return; }
        self.as_mut().set_uploading(true);
        self.as_mut().set_uploaded_url(QString::default());
        self.as_mut().set_status(QString::default());
        let bundle = developer::bundle(&developer::report(&native_snapshot()));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = developer::upload(&bundle);
            let _ = thread.queue(move |mut developers| {
                match result {
                    Ok(address) => developers.as_mut().set_uploaded_url(QString::from(&address)),
                    Err(error) => developers.as_mut().set_status(QString::from(&error)),
                }
                developers.set_uploading(false);
            });
        });
    }
    /// The system's save dialog blocks until it is answered, so it runs on a
    /// thread of its own and the reviewed text is written back on the
    /// window's thread. Cancelling it does nothing.
    pub fn export_report(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        let report = self.review().to_string();
        let title = app::t("Export diagnostics");
        std::thread::spawn(move || {
            let chosen = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).set_file_name("chilpa-diagnostics.txt").save_file());
            if let Some(file) = chosen {
                let path = file.path().to_path_buf();
                let _ = thread.queue(move |developers| {
                    let note = match developer::save_report(&path, &report) {
                        Ok(()) => app::t("Report saved"), Err(error) => error,
                    };
                    developers.set_status(QString::from(&note));
                });
            }
        });
    }
    pub fn log_url(&self) -> QString { QString::from(&developer::logs::file_url()) }
}
