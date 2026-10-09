//! Which pages the Gallery shows besides its own: one per shell crate that
//! brings pieces of its own. A crate that joins adds its page here.
//!
//! cxx-qt builds a QML module only around at least one type, and this is
//! the Gallery's.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type GalleryPages = super::GalleryPagesRust;
        /// Where each crate's page is, in the binary's resources.
        #[qinvokable]
        fn pages(self: &GalleryPages) -> QStringList;
    }
}
use cxx_qt_lib::{QList, QString, QStringList};

/// The module of each crate that has a page, and the page's file.
const PAGES: [(&str, &str); 10] = [
    ("metadata", "GalleryMetadata"),
    ("sources", "GalleryPlugins"),
    ("controls", "GalleryControls"),
    ("lyrics", "GalleryLyrics"),
    ("ambient", "GalleryAmbient"),
    ("mini", "GalleryMini"),
    ("immersive", "GalleryImmersive"),
    ("waveform", "GalleryWaveform"),
    ("artwork", "GalleryArtwork"),
    ("about", "GalleryAbout"),
];

#[derive(Default)]
pub struct GalleryPagesRust;

impl qobject::GalleryPages {
    pub fn pages(&self) -> QStringList {
        let mut pages = QList::<QString>::default();
        for (module, page) in PAGES {
            pages.append(QString::from(&format!("qrc:/qt/qml/app/chilpa/{module}/qml/Gallery/{page}.qml")));
        }
        QStringList::from(&pages)
    }
}
