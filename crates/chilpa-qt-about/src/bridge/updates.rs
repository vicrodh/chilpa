//! I expose the facade's update worker without doing process or network work in QML.
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
        type Updates = super::UpdatesRust;
        #[qinvokable] fn refresh(self: Pin<&mut Updates>);
        #[qinvokable] fn check(self: Pin<&mut Updates>, automatic: bool);
        #[qinvokable] fn download(self: Pin<&mut Updates>);
        #[qinvokable] fn cancel(self: Pin<&mut Updates>);
        #[qinvokable] fn set_automatic(self: Pin<&mut Updates>, on: bool);
        #[qinvokable] fn restart(self: Pin<&mut Updates>) -> bool;
    }
}
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct UpdatesRust { snapshot: QString }

impl qobject::Updates {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let next = QString::from(app::with(|api| api.appimage_updates_json()));
        if *self.snapshot() != next { self.set_snapshot(next); }
    }
    pub fn check(mut self: Pin<&mut Self>, automatic: bool) { app::with(|api| api.appimage_updates_check(automatic)); self.as_mut().refresh(); }
    pub fn download(mut self: Pin<&mut Self>) { app::with(|api| api.appimage_updates_download()); self.as_mut().refresh(); }
    pub fn cancel(self: Pin<&mut Self>) { app::with(|api| api.appimage_updates_cancel()); }
    pub fn set_automatic(mut self: Pin<&mut Self>, on: bool) { app::with(|api| api.appimage_updates_automatic(on)); self.as_mut().refresh(); }
    pub fn restart(self: Pin<&mut Self>) -> bool { app::with(|api| api.appimage_updates_restart()) }
}
