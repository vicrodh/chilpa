//! What the search flyout lists (design/04 10): the top artist, records and
//! songs, or more of one kind when a chip narrows it.
//!
//! Each keystroke asks the worker, tagged, and an answer for text already
//! typed over is dropped. What was found is kept whole, so switching chips
//! rebuilds the rows without asking again.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, text)]
        /// "all", "artists", "albums" or "songs".
        #[qproperty(QString, scope)]
        /// Rows that are results, headers excluded.
        #[qproperty(i32, count)]
        type SearchModel = super::SearchRust;

        /// Opens what the row is: an artist's page, a record's page; a song
        /// plays, with the songs listed after it.
        #[qinvokable]
        fn activate(self: Pin<&mut SearchModel>, row: i32);

        /// Plays everything the row's artist has.
        #[qinvokable]
        fn play(self: Pin<&mut SearchModel>, row: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SearchModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SearchModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SearchModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SearchModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SearchModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for SearchModel {}
    impl cxx_qt::Initialize for SearchModel {}
}

use crate::{app, format, registry, worker};
use chilpa_api::{Album, Artist, Chilpa, Found, TrackOrder, TrackRow};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::pin::Pin;

const ROLES: &[&str] = &["kind", "title", "detail", "art", "duration"];
const USER_ROLE: i32 = 256;
/// As many of each kind as a chip shows; "All" shows fewer.
const LIMIT: usize = 12;

/// A row of the flyout. The numbers are what QML's `kind` reads.
enum Hit {
    Header(String),
    Top(Artist),
    Artist(Artist),
    Album(Album),
    Track(TrackRow),
}

impl Hit {
    fn kind(&self) -> i32 {
        match self {
            Hit::Header(_) => 0,
            Hit::Top(_) => 1,
            Hit::Artist(_) => 2,
            Hit::Album(_) => 3,
            Hit::Track(_) => 4,
        }
    }
}

#[derive(Default)]
pub struct SearchRust {
    text: QString,
    scope: QString,
    count: i32,
    found: Found,
    rows: Vec<Hit>,
    generation: u64,
}

fn artist_detail(a: &Artist) -> String {
    let n = a.album_count.to_string();
    let records = app::with(|api| api.tf("{} record", "{} records", a.album_count as i64, &[&n]));
    format!("{} · {records}", app::t("Artist"))
}

fn album_detail(a: &Album) -> String {
    let mut parts = vec![app::t("Album"), a.artist.clone()];
    parts.extend(a.year.map(|y| y.to_string()));
    parts.join(" · ")
}

fn art(path: &str) -> QVariant {
    let url = if path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(path)) };
    QVariant::from(&url)
}

/// The rows a scope shows of what was found.
fn rows(found: &Found, scope: &str) -> Vec<Hit> {
    let mut out = Vec::new();
    let mut section = |label: &str, hits: Vec<Hit>| {
        if !hits.is_empty() {
            out.push(Hit::Header(app::t(label)));
            out.extend(hits);
        }
    };
    match scope {
        "artists" => section(Chilpa::mark("Artists"), found.artists.iter().cloned().map(Hit::Artist).collect()),
        "albums" => section(Chilpa::mark("Albums"), found.albums.iter().cloned().map(Hit::Album).collect()),
        "songs" => section(Chilpa::mark("Songs"), found.tracks.iter().cloned().map(Hit::Track).collect()),
        _ => {
            section(Chilpa::mark("Top result"), found.artists.iter().take(1).cloned().map(Hit::Top).collect());
            section(Chilpa::mark("Albums"), found.albums.iter().take(3).cloned().map(Hit::Album).collect());
            section(Chilpa::mark("Songs"), found.tracks.iter().take(4).cloned().map(Hit::Track).collect());
        }
    }
    out
}

impl qobject::SearchModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(hit) = self.rust().rows.get(index.row().max(0) as usize) else { return QVariant::default() };
        let text = |s: &str| QVariant::from(&QString::from(s));
        match (role - USER_ROLE, hit) {
            (0, h) => QVariant::from(&h.kind()),
            (1, Hit::Header(label)) => text(label),
            (1, Hit::Top(a) | Hit::Artist(a)) => text(&a.name),
            (1, Hit::Album(a)) => text(&a.title),
            (1, Hit::Track(t)) => text(&t.title),
            (2, Hit::Top(a) | Hit::Artist(a)) => text(&artist_detail(a)),
            (2, Hit::Album(a)) => text(&album_detail(a)),
            (2, Hit::Track(t)) => text(&format!("{} — {}", t.artist, t.album)),
            (3, Hit::Top(a) | Hit::Artist(a)) => art(&a.artwork_path),
            (3, Hit::Album(a)) => art(&a.artwork_path),
            (3, Hit::Track(t)) => art(&t.artwork_path),
            (4, Hit::Track(t)) => text(&format::clock(t.duration_ms)),
            // A role a row does not have is still of its type: QML binds it
            // to a string or a url, and an empty variant reads "undefined".
            (3, _) => art(""),
            _ => text(""),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, name) in ROLES.iter().enumerate() {
            roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    pub fn activate(self: Pin<&mut Self>, row: i32) {
        let row = row.max(0) as usize;
        match self.rust().rows.get(row) {
            Some(Hit::Top(a) | Hit::Artist(a)) => {
                let name = a.name.clone();
                registry::navigation(move |nav| nav.open_artist(&QString::from(&name)));
            }
            Some(Hit::Album(a)) => {
                let id = a.id;
                registry::navigation(move |nav| nav.go(&QString::from("album"), id));
            }
            Some(Hit::Track(t)) => {
                let songs: Vec<&TrackRow> =
                    self.rust().rows.iter().filter_map(|h| if let Hit::Track(t) = h { Some(t) } else { None }).collect();
                let start = songs.iter().position(|s| s.id == t.id).unwrap_or(0);
                let paths: Vec<String> = songs.iter().map(|s| s.path.clone()).collect();
                app::with(|api| api.play_from(&paths, start));
            }
            _ => {}
        }
    }

    pub fn play(self: Pin<&mut Self>, row: i32) {
        let Some(Hit::Top(a) | Hit::Artist(a)) = self.rust().rows.get(row.max(0) as usize) else { return };
        let name = a.name.clone();
        app::with(|api| {
            let paths: Vec<String> =
                api.artist_tracks(&name, TrackOrder::Artist, 100_000).into_iter().map(|t| t.path).collect();
            api.play_from(&paths, 0);
        });
    }

    fn ask(mut self: Pin<&mut Self>) {
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        let text = self.rust().text.to_string();
        if text.trim().is_empty() {
            self.arrive(generation, Found::default());
            return;
        }
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let found = reader.find(&text, LIMIT);
            let _ = thread.queue(move |model| model.arrive(generation, found));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, found: Found) {
        if generation != self.rust().generation {
            return;
        }
        self.as_mut().rust_mut().found = found;
        self.rebuild();
    }

    fn rebuild(mut self: Pin<&mut Self>) {
        let rows = rows(&self.rust().found, &self.rust().scope.to_string());
        let count = rows.iter().filter(|h| !matches!(h, Hit::Header(_))).count() as i32;
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = rows;
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_count(count);
    }
}

impl cxx_qt::Initialize for qobject::SearchModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().scope = QString::from("all");
        self.as_mut().on_text_changed(|m| m.ask()).release();
        self.as_mut().on_scope_changed(|m| m.rebuild()).release();
    }
}
