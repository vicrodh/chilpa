//! I publish a record's header and share its committed track page.

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
        #[qproperty(i32, remote_rev)]
        #[qproperty(i32, tracks_rev)]
        #[qproperty(QString, versions)]
        #[qproperty(bool, found)]
        #[qproperty(QString, title)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, artist_information)]
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
    impl cxx_qt::Threading for AlbumDetailModel {}
}

use crate::{app, worker};
use chilpa_qt_kit::album_pages;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct AlbumDetailRust {
    album_id: i64,
    selected_id: i64,
    content_rev: i32,
    remote_rev: i32,
    source_revision: Option<(i32, i32)>,
    tracks_rev: i32,
    generation: album_pages::Generation,
    pending: bool,
    requested_id: i64,
    page: Option<std::rc::Rc<chilpa_api::AlbumPage>>,
    versions: QString,
    found: bool,
    title: QString,
    artist: QString,
    artist_information: QString,
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
        let Ok(version) = value.to_string().parse::<i64>() else { return };
        let valid = self.rust().page.as_ref().is_some_and(|page| page.versions.iter().any(|choice| choice.id == version));
        if !valid { return; }
        let library = crate::registry::library_thread();
        // A source's copy, or the library's in place of one: the card's
        // choice among the sources is the facade's to keep.
        if version < 0 || self.rust().selected_id < 0 {
            app::with(|api| api.choose_album_copy(version));
            if let Some(library) = &library { let _ = library.queue(|library| library.content_changed()); }
        }
        self.as_mut().rust_mut().requested_id = version;
        if version < 0 {
            self.schedule();
            return;
        }
        let album = version;
        let ticket = self.rust().generation.next();
        let thread = self.qt_thread();
        worker::submit_album(move |reader| {
            // Explicit choices persist even if a refresh or navigation cancels presentation.
            let accepted = reader.select_album_version(album, version);
            let page = (accepted && ticket.current()).then(|| reader.album_page(version, true)).flatten();
            let _ = thread.queue(move |mut model| {
                if !ticket.current() { return; }
                model.as_mut().rust_mut().requested_id = 0;
                if accepted { model.apply(page); } else { model.schedule(); }
            });
            if accepted {
                if let Some(library) = library { let _ = library.queue(|library| library.content_changed()); }
            }
        });
    }

    fn schedule(mut self: Pin<&mut Self>) {
        self.rust().generation.next();
        if self.rust().pending { return; }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.load());
    }
    fn load(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = false;
        let original = self.rust().album_id;
        let selected = if self.rust().requested_id != 0 { self.rust().requested_id } else { self.rust().selected_id };
        let id = if selected != 0 { selected } else { original };
        let retain = crate::browse::read(|state| state.filter.extra_filtered() && state.snapshot.album_ids.contains(&original));
        let ticket = self.rust().generation.next();
        if id <= 0 {
            let (revision, page) = app::with(|api| (api.source_entity_revision(id), api.cached_album_page(id)));
            self.as_mut().rust_mut().source_revision = Some(revision);
            self.apply(page);
            return;
        }
        let thread = self.qt_thread();
        worker::submit_album(move |reader| {
            if !ticket.current() { return; }
            let page = reader.album_page(id, retain);
            let _ = thread.queue(move |model| { if ticket.current() { model.apply(page); } });
        });
    }
    fn apply(mut self: Pin<&mut Self>, page: Option<chilpa_api::AlbumPage>) {
        let Some(mut page) = page else {
            self.as_mut().rust_mut().page = None;
            self.as_mut().set_found(false);
            self.as_mut().set_selected_id(0);
            return;
        };
        let versions = std::mem::take(&mut page.versions);
        page.versions = app::with(|api| api.album_copies(page.detail.album.id, versions));
        if self.rust().page.as_deref() == Some(&page) { return; }
        let (token, page) = album_pages::keep(page);
        self.as_mut().rust_mut().page = Some(page.clone());
        let (d, lines) = (&page.detail, &page.heading);
        self.as_mut().set_versions(QString::from(serde_json::to_string(&page.versions).unwrap_or_default()));
        self.as_mut().set_tracks_rev(token);
        self.as_mut().set_selected_id(d.album.id);
        let art = if d.album.artwork_path.is_empty() {
            QUrl::default()
        } else {
            QUrl::from_local_file(&QString::from(&d.album.artwork_path))
        };
        self.as_mut().set_title(QString::from(&d.album.title));
        self.as_mut().set_artist(QString::from(&d.album.artist));
        self.as_mut().set_artist_information(QString::from(&page.artist_information));
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
        self.as_mut().on_album_id_changed(|mut model| {
            model.as_mut().rust_mut().requested_id = 0;
            model.as_mut().rust_mut().page = None;
            model.as_mut().rust_mut().source_revision = None;
            model.as_mut().set_found(false);
            model.as_mut().set_selected_id(0);
            model.schedule();
        }).release();
        self.as_mut().on_content_rev_changed(|model| model.schedule()).release();
        self.as_mut().on_remote_rev_changed(|model| {
            let id = model.rust().album_id;
            if id < 0 && model.rust().source_revision != Some(app::with(|api| api.source_entity_revision(id))) { model.schedule(); }
        }).release();
    }
}
