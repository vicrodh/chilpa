//! A shown genre image owns one cancellable request for its default card or
//! header. Property changes coalesce in one Qt event; no network or disk I/O
//! runs on the GUI thread. Inactive, it holds no request and no source.
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
        #[qproperty(QString, key)]
        #[qproperty(bool, header)]
        #[qproperty(bool, active)]
        #[qproperty(QString, source)]
        type GenreArtModel = super::GenreArtRust;
    }
    impl cxx_qt::Threading for GenreArtModel {}
    impl cxx_qt::Initialize for GenreArtModel {}
}
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct GenreArtRust {
    key: QString, header: bool, active: bool, source: QString,
    scheduled: bool, request: Option<chilpa_api::GenreArtRequest>,
    generation: chilpa_qt_kit::album_pages::Generation,
}

impl qobject::GenreArtModel {
    fn schedule(mut self: Pin<&mut Self>) {
        self.rust().generation.next();
        self.as_mut().rust_mut().request = None;
        self.as_mut().set_source(QString::default());
        if !*self.active() || self.rust().scheduled { return; }
        self.as_mut().rust_mut().scheduled = true;
        let _ = self.qt_thread().queue(|mut model| {
            model.as_mut().rust_mut().scheduled = false;
            model.read();
        });
    }

    fn read(mut self: Pin<&mut Self>) {
        let key = self.key().to_string();
        if !*self.active() || key.is_empty() { return; }
        let ticket = self.rust().generation.next();
        let thread = self.qt_thread();
        let request = chilpa_api::request_genre_art(&key, *self.header(), move |path| {
            if !ticket.current() || path.is_empty() { return; }
            let _ = thread.queue(move |mut model| {
                if !ticket.current() || !*model.active() { return; }
                model.as_mut().set_source(QString::from(QUrl::from_local_file(&QString::from(path)).to_string()));
            });
        });
        self.as_mut().rust_mut().request = request;
    }
}

impl cxx_qt::Initialize for qobject::GenreArtModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_key_changed(|model| model.schedule()).release();
        self.as_mut().on_header_changed(|model| model.schedule()).release();
        self.as_mut().on_active_changed(|model| model.schedule()).release();
        self.schedule();
    }
}
