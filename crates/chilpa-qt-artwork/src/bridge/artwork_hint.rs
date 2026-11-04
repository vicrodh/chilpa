//! One cancellable edition-hint request, without a full album/track model.
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
        #[qproperty(i64, album_id)]
        #[qproperty(QString, document)]
        #[qproperty(bool, ready)]
        type ArtworkHintModel = super::ArtworkHintRust;
    }
    impl cxx_qt::Threading for ArtworkHintModel {}
    impl cxx_qt::Initialize for ArtworkHintModel {}
}
use chilpa_qt_kit::worker;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct ArtworkHintRust { album_id: i64, document: QString, ready: bool, generation: u64 }
impl qobject::ArtworkHintModel {
    fn read(mut self: Pin<&mut Self>) {
        let id = *self.album_id();
        let generation = self.rust().generation.wrapping_add(1);
        self.as_mut().rust_mut().generation = generation;
        self.as_mut().set_ready(false);
        if id == 0 { self.as_mut().set_document(QString::default()); self.set_ready(true); return; }
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let document = reader.album_artwork_hint(id);
            let _ = thread.queue(move |mut model| {
                if model.rust().generation == generation {
                    model.as_mut().set_document(QString::from(document));
                    model.set_ready(true);
                }
            });
        });
    }
}
impl cxx_qt::Initialize for qobject::ArtworkHintModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_album_id_changed(|model| model.read()).release();
        self.read();
    }
}
