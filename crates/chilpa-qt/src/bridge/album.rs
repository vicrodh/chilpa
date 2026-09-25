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

struct Lines {
    kind: String,
    meta: String,
    discs: String,
    info: String,
    summary: String,
}

impl qobject::AlbumDetailModel {
    fn load(mut self: Pin<&mut Self>) {
        let id = self.rust().album_id;
        let found = if id > 0 { app::with(|api| api.album_detail(id)) } else { None };
        let Some(d) = found else {
            self.as_mut().set_found(false);
            return;
        };
        let lines = app::with(|api| {
            let songs = d.album.track_count.to_string();
            let songs = api.tf("{} song", "{} songs", d.album.track_count as i64, &[&songs]);
            let length = format::span(d.duration_ms);
            let mut meta: Vec<String> = Vec::new();
            if !d.genre.is_empty() {
                meta.push(d.genre.clone());
            }
            meta.extend([songs.clone(), length.clone()]);
            let quality = d.quality.long();
            if !quality.is_empty() {
                meta.push(quality);
            }
            let kind = api.t("Album").to_uppercase();
            let n = d.discs.to_string();
            Lines {
                kind: match d.album.year {
                    Some(y) => format!("{kind} · {y}"),
                    None => kind,
                },
                meta: meta.join(" · "),
                discs: if d.discs > 1 { api.tf("{} disc", "{} discs", d.discs as i64, &[&n]) } else { String::new() },
                info: api.t("Info: Embedded tags"),
                summary: format!("{songs} · {length}"),
            }
        });
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
        self.as_mut().on_album_id_changed(|m| m.load()).release();
    }
}
