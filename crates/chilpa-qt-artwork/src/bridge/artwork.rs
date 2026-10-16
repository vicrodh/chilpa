//! The covers and pictures the listener chooses, as QML reads them: whether
//! a record has one, the picture chosen for an artist, the address a cover
//! came from, and the system's dialogs to choose one and to save one. The
//! copy, where it is kept and what shows it are the facade's
//! (`chilpa_api::Chilpa::choose_album_art`); `rev` moves with every choice.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, rev)]
        type Artwork = super::ArtworkRust;

        /// Whether the record shows a cover the listener chose.
        #[qinvokable]
        fn album_chosen(self: &Artwork, album_id: i64, rev: i32) -> bool;
        /// The web address the record's cover came from, empty for one on
        /// disk.
        #[qinvokable]
        fn album_address(self: &Artwork, album_id: i64, rev: i32) -> QString;
        /// Asks for an image and gives it to the record.
        #[qinvokable]
        fn choose_album(self: Pin<&mut Artwork>, album_id: i64);
        /// Takes the chosen cover away; the one its files bring comes back.
        #[qinvokable]
        fn forget_album(self: Pin<&mut Artwork>, album_id: i64);
        /// Asks where, and copies the cover the record shows there.
        #[qinvokable]
        fn save_album(self: Pin<&mut Artwork>, album_id: i64, title: &QString);
        /// The picture chosen for the artist, or an empty address.
        #[qinvokable]
        fn artist(self: &Artwork, name: &QString, rev: i32) -> QUrl;
        #[qinvokable]
        fn choose_artist(self: Pin<&mut Artwork>, name: &QString);
        #[qinvokable]
        fn forget_artist(self: Pin<&mut Artwork>, name: &QString);
        /// Asks where, and copies the picture the artist's page shows.
        #[qinvokable]
        fn save_artist(self: Pin<&mut Artwork>, name: &QString, shown: &QUrl);
    }

    impl cxx_qt::Threading for Artwork {}
}

use chilpa_qt_kit::app;
use cxx_qt::Threading as _;
use cxx_qt_lib::{QString, QUrl};
use std::path::PathBuf;
use std::pin::Pin;

#[derive(Default)]
pub struct ArtworkRust {
    rev: i32,
}

/// What the picker offers: what the window can draw.
const KINDS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

impl qobject::Artwork {
    pub fn album_chosen(&self, album_id: i64, _rev: i32) -> bool {
        app::with(|api| api.album_art_chosen(album_id))
    }

    pub fn album_address(&self, album_id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.album_art_address(album_id)))
    }

    pub fn choose_album(self: Pin<&mut Self>, album_id: i64) {
        self.pick(app::t("Choose a cover"), move |api, path| api.choose_album_art(album_id, Some(path)));
    }

    pub fn forget_album(self: Pin<&mut Self>, album_id: i64) {
        app::with(|api| api.choose_album_art(album_id, None));
        self.follow();
    }

    pub fn save_album(self: Pin<&mut Self>, album_id: i64, title: &QString) {
        let title = title.to_string();
        if let Some((from, name)) = app::with(|api| api.album_art_to_save(album_id, &title)) {
            self.save(app::t("Save the cover"), from, name);
        }
    }

    pub fn artist(&self, name: &QString, _rev: i32) -> QUrl {
        let path = app::with(|api| api.artist_art(&name.to_string()));
        if path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(&path)) }
    }

    pub fn choose_artist(self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        self.pick(app::t("Choose an image"), move |api, path| api.choose_artist_art(&name, Some(path)));
    }

    pub fn forget_artist(self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        app::with(|api| api.choose_artist_art(&name, None));
        self.follow();
    }

    pub fn save_artist(self: Pin<&mut Self>, name: &QString, shown: &QUrl) {
        let Some(shown) = shown.to_local_file().map(|file| PathBuf::from(file.to_string())) else { return };
        let name = name.to_string();
        if let Some((from, suggested)) = app::with(|api| api.artist_art_to_save(&name, &shown)) {
            self.save(app::t("Save the image"), from, suggested);
        }
    }

    /// The revision the facade is at, so whatever reads it asks again.
    fn follow(self: Pin<&mut Self>) {
        let rev = app::with(|api| api.artwork_rev());
        self.set_rev(rev);
    }

    /// The system's picker blocks until it is answered, so it runs on a
    /// thread of its own and hands the file back to the window's thread,
    /// where the facade lives. Cancelling it does nothing.
    fn pick(self: Pin<&mut Self>, title: String, then: impl FnOnce(&mut chilpa_api::Chilpa, &std::path::Path) -> bool + Send + 'static) {
        let thread = self.qt_thread();
        let images = app::t("Images");
        std::thread::spawn(move || {
            let picked = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).add_filter(&images, &KINDS).pick_file());
            if let Some(file) = picked {
                let path = file.path().to_path_buf();
                let _ = thread.queue(move |artwork| {
                    app::with(|api| then(api, &path));
                    artwork.follow();
                });
            }
        });
    }

    /// Asks where, on a thread of its own, and copies there from the
    /// window's thread.
    fn save(self: Pin<&mut Self>, title: String, from: PathBuf, suggested: String) {
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let chosen = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).set_file_name(&suggested).save_file());
            if let Some(to) = chosen {
                let to = to.path().to_path_buf();
                let _ = thread.queue(move |_| {
                    app::with(|api| api.save_art(&from, &to));
                });
            }
        });
    }
}
