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
        fn save_report(self: Pin<&mut Developers>, destination: &QString);
        #[qinvokable]
        fn log_url(self: &Developers) -> QString;
    }
}
use cxx_qt_lib::QString;
use std::{ffi::{CStr, CString}, pin::Pin};
use chilpa_qt_kit::app;
use chilpa_api::developer;
unsafe extern "C" {
    fn chilpa_developer_native_facts() -> *const std::ffi::c_char;
    fn chilpa_developer_clipboard(text: *const std::ffi::c_char);
}
#[derive(Default)]
pub struct DevelopersRust { diagnostics: QString, logs: QString, review: QString, status: QString }
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
        if let Ok(text)=CString::new(self.review().to_string()) {
            // SAFETY: the reviewed UTF-8 buffer lives through this synchronous UI-thread call.
            unsafe { chilpa_developer_clipboard(text.as_ptr()); }
            self.as_mut().set_status(QString::from(&app::t("Copied")));
        }
    }
    pub fn save_report(mut self: Pin<&mut Self>, destination: &QString) {
        let status = match developer::save_report(&destination.to_string(), &self.review().to_string()) {
            Ok(()) => app::t("Report saved"), Err(error) => error,
        };
        self.as_mut().set_status(QString::from(&status));
    }
    pub fn log_url(&self) -> QString { QString::from(&developer::logs::file_url()) }
}
