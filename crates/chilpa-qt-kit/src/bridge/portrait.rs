//! A visible image owns one cancellable metadata request. Property changes
//! coalesce in one Qt event; no network or disk I/O runs on the GUI thread.
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
        #[qproperty(QString, name)]
        #[qproperty(QString, mbid)]
        #[qproperty(bool, active)]
        #[qproperty(QString, source)]
        #[qproperty(QString, credit)]
        #[qproperty(QString, page)]
        type PortraitModel = super::PortraitRust;
        #[qinvokable]
        fn cancel(self: Pin<&mut PortraitModel>);
    }
    impl cxx_qt::Threading for PortraitModel {}
    impl cxx_qt::Initialize for PortraitModel {}
}
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct PortraitRust {
    name:QString, mbid:QString, active:bool, source:QString, credit:QString, page:QString,
    scheduled:bool, request:Option<chilpa_api::PortraitRequest>,
    generation:crate::album_pages::Generation,
}
impl qobject::PortraitModel {
    pub fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().clear();
        self.set_active(false);
    }
    fn clear(mut self: Pin<&mut Self>) {
        self.rust().generation.next();
        self.as_mut().rust_mut().request = None;
        self.as_mut().set_source(QString::default());
        self.as_mut().set_credit(QString::default());
        self.set_page(QString::default());
    }
    fn schedule(mut self: Pin<&mut Self>) {
        self.as_mut().clear();
        if !*self.active() { return; }
        if self.rust().scheduled { return; }
        self.as_mut().rust_mut().scheduled = true;
        let _ = self.qt_thread().queue(|mut model| {
            model.as_mut().rust_mut().scheduled = false;
            model.read();
        });
    }
    fn read(mut self: Pin<&mut Self>) {
        if !*self.active() || self.name().to_string().trim().is_empty() { return; }
        let ticket = self.rust().generation.next();
        let thread = self.qt_thread();
        let request = chilpa_api::request_artist_portrait(&self.name().to_string(), &self.mbid().to_string(), move |image| {
            if !ticket.current() { return; }
            let _ = thread.queue(move |mut model| {
                if !ticket.current() || !*model.active() || image.path.is_empty() { return; }
                model.as_mut().set_source(QString::from(QUrl::from_local_file(&QString::from(image.path)).to_string()));
                let credit = [image.info.credit,image.info.license,image.info.license_url].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ");
                model.as_mut().set_credit(QString::from(credit));
                model.set_page(QString::from(image.info.page));
            });
        });
        self.as_mut().rust_mut().request = request;
    }
}
impl cxx_qt::Initialize for qobject::PortraitModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_name_changed(|model| model.schedule()).release();
        self.as_mut().on_mbid_changed(|model| model.schedule()).release();
        self.as_mut().on_active_changed(|model| model.schedule()).release();
        self.schedule();
    }
}
