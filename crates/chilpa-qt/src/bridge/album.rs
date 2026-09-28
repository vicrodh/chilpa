//! A record's header, as its page shows it.
//!
//! Every line the header carries is built here, so the page only places
//! them: "ALBUM · 2019", "Alternative · 11 songs · 55 min · FLAC · 24-bit /
//! 192 kHz", "2 discs". The tracks are a `TrackListModel` of their own.

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
        #[qproperty(i64, album_id)]
        #[qproperty(i64, selected_id)]
        #[qproperty(i32, content_rev)]
        #[qproperty(QString, versions)]
        #[qproperty(bool, found)]
        #[qproperty(QString, title)]
        #[qproperty(QString, artist)]
        #[qproperty(QUrl, art)]
        /// "ALBUM · 2019".
        #[qproperty(QString, kind_line)]
        /// Genre, songs, length and quality, joined.
        #[qproperty(QString, meta)]
        /// "2 discs", empty for one.
        #[qproperty(QString, discs)]
        /// Where the record's information comes from.
        #[qproperty(QString, info)]
        /// "11 songs · 55 min".
        #[qproperty(QString, summary)]
        /// The tier of its best file, for the quality icon.
        #[qproperty(i32, tier)]
        type AlbumDetailModel = super::AlbumDetailRust;
        #[qinvokable] fn select_version(self: Pin<&mut AlbumDetailModel>, value: &QString);
    }

    impl cxx_qt::Initialize for AlbumDetailModel {}
}

use crate::{app, format};
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct AlbumDetailRust {
    album_id: i64,
    selected_id: i64,
    content_rev: i32,
    versions: QString,
    found: bool,
    title: QString,
    artist: QString,
    art: QUrl,
    kind_line: QString,
    meta: QString,
    discs: QString,
    info: QString,
    summary: QString,
    tier: i32,
}

impl qobject::AlbumDetailModel {
    pub fn select_version(mut self: Pin<&mut Self>, value: &QString) {
        let Ok(version) = value.to_string().parse() else { return };
        let album = self.rust().album_id;
        if app::with(|api| api.select_album_version(album, version)) {
            self.as_mut().set_selected_id(version);
            self.as_mut().load();
            crate::registry::library(|library| library.content_changed());
        }
    }

    fn load(mut self: Pin<&mut Self>) {
        let id = self.rust().selected_id;
        let versions = app::with(|api| api.album_versions(id));
        self.as_mut().set_versions(QString::from(serde_json::to_string(&versions).unwrap_or_default()));
        let found = if id > 0 { app::with(|api| api.album_detail(id)) } else { None };
        let Some(d) = found else {
            self.as_mut().set_found(false);
            return;
        };
        let lines = app::with(|api| api.album_heading(&d, &format::span(d.duration_ms)));
        let art = if d.album.artwork_path.is_empty() {
            QUrl::default()
        } else {
            QUrl::from_local_file(&QString::from(&d.album.artwork_path))
        };
        self.as_mut().set_title(QString::from(&d.album.title));
        self.as_mut().set_artist(QString::from(&d.album.artist));
        self.as_mut().set_art(art);
        self.as_mut().set_kind_line(QString::from(&lines.kind));
        self.as_mut().set_meta(QString::from(&lines.meta));
        self.as_mut().set_discs(QString::from(&lines.discs));
        self.as_mut().set_info(QString::from(&lines.info));
        self.as_mut().set_summary(QString::from(&lines.summary));
        self.as_mut().set_tier(d.quality.tier.rank());
        self.as_mut().set_found(true);
    }
}

impl cxx_qt::Initialize for qobject::AlbumDetailModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_album_id_changed(|mut m| {
            let original = *m.album_id();
            let filtered = crate::browse::read(|s| s.filter.extra_filtered() && s.snapshot.album_ids.contains(&original));
            let id = if filtered { original } else { app::with(|api| api.selected_album_version(original)) };
            m.as_mut().set_selected_id(id);
            m.load();
        }).release();
        self.as_mut().on_content_rev_changed(|mut m| {
            let id = app::with(|api| api.selected_album_version(*m.selected_id()));
            m.as_mut().set_selected_id(id);
            m.load();
        }).release();
    }
}
