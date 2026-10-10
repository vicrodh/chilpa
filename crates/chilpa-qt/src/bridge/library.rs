//! Library filtering and its worker. Column choices invalidate their right;
//! generation tags discard stale answers. State survives navigation.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, rev)]
        #[qproperty(i32, content_rev)]
        /// Sources' listings and records changed; apart, their covers.
        #[qproperty(i32, remote_rev)]
        #[qproperty(i32, remote_art_rev)]
        #[qproperty(bool, busy)]
        #[qproperty(QString, summary)]
        #[qproperty(QString, year)]
        #[qproperty(QString, genre)]
        #[qproperty(QString, label)] #[qproperty(QString, composer)]
        #[qproperty(QString, rating)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, album)]
        #[qproperty(QStringList, source_keys)] #[qproperty(QStringList, source_labels)]
        #[qproperty(QStringList, source_counts)] #[qproperty(QStringList, excluded_sources)]
        #[qproperty(QStringList, source_icons)]
        #[qproperty(QStringList, qualities)] #[qproperty(QStringList, formats)]
        #[qproperty(bool, extra_filtered)]
        #[qproperty(bool, group_versions)]
        #[qproperty(bool, artist_group_versions)]
        #[qproperty(QStringList, queries)]
        /// How the tracks are ordered: "artist", "album", "year", "added",
        /// "title", "plays" or "duration".
        #[qproperty(QString, order, READ = order, WRITE = choose_order, NOTIFY = order_changed)]
        /// How the Albums screen orders its records, remembered: "artist",
        /// "title", "year", "added", "played" or "last_played".
        #[qproperty(QString, album_order)]
        type Library = super::LibraryRust;

        #[cxx_name = "order"]
        fn order(self: &Library) -> QString;
        #[cxx_name = "setOrder"]
        fn choose_order(self: Pin<&mut Library>, order: QString);
        #[qsignal]
        #[cxx_name = "orderChanged"]
        fn order_changed(self: Pin<&mut Library>);

        /// Chooses in a column ("year", "genre", "rating", "artist", ...) and
        /// clears the ones to its right. An empty key is "All".
        #[qinvokable]
        fn choose(self: Pin<&mut Library>, column: &QString, key: &QString, label: &QString);

        /// Clears every column.
        #[qinvokable]
        fn clear(self: Pin<&mut Library>);

        #[qinvokable] fn toggle_filter(self: Pin<&mut Library>, kind: &QString, key: &QString);
        #[qinvokable] fn reset_extra(self: Pin<&mut Library>);
        #[qinvokable] fn group_albums(self: Pin<&mut Library>, on: bool);
        #[qinvokable] fn group_artist_albums(self: Pin<&mut Library>, on: bool);
        #[qinvokable] fn search_column(self: Pin<&mut Library>, column: &QString, query: &QString);
    }

    impl cxx_qt::Threading for Library {}
    impl cxx_qt::Initialize for Library {}
}

use crate::{app, browse, worker};
use chilpa_api::{Facets, Filter, Snapshot, TrackOrder};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;
use std::rc::Rc;

#[derive(Default)]
pub struct LibraryRust {
    rev: i32,
    content_rev: i32,
    remote_rev: i32,
    remote_art_rev: i32,
    busy: bool,
    summary: QString,
    year: QString,
    genre: QString,
    label: QString,
    composer: QString,
    rating: QString,
    artist: QString,
    album: QString,
    source_keys: QStringList, source_labels: QStringList, source_counts: QStringList, source_icons: QStringList,
    excluded_sources: QStringList, qualities: QStringList, formats: QStringList,
    extra_filtered: bool,
    group_versions: bool,
    artist_group_versions: bool,
    queries: QStringList,
    order: QString,
    album_order: QString,
    generation: u64,
}

const ORDER_KEY: &str = "ui.library.order";

const ORDER: [&str; 7] = ["year", "genre", "label", "composer", "rating", "artist", "album"];

fn strings(items: impl IntoIterator<Item = String>) -> QStringList {
    let mut list = QList::<QString>::default();
    for item in items { list.append(QString::from(&item)); }
    QStringList::from(&list)
}

impl qobject::Library {
    const SETTERS: [fn(Pin<&mut Self>, QString); 7] = [Self::set_year, Self::set_genre, Self::set_label, Self::set_composer, Self::set_rating, Self::set_artist, Self::set_album];

    pub fn group_artist_albums(mut self: Pin<&mut Self>, on: bool) {
        if app::with(|api| api.set_group_artist_versions(on)) {
            self.as_mut().set_artist_group_versions(on);
            self.content_changed();
        }
    }

    pub fn group_albums(mut self: Pin<&mut Self>, on: bool) {
        if app::with(|api| api.set_group_album_versions(on)) {
            self.as_mut().set_group_versions(on);
            self.content_changed();
        }
    }

    pub fn search_column(mut self: Pin<&mut Self>, column: &QString, query: &QString) {
        let Some(at) = ORDER.iter().position(|c| *c == column.to_string()) else { return };
        let queries = browse::write(|s| {
            s.filter.queries[at] = query.to_string();
            s.filter.queries.clone()
        });
        self.as_mut().set_queries(strings(queries));
        self.refresh();
    }

    pub fn choose(mut self: Pin<&mut Self>, column: &QString, key: &QString, label: &QString) {
        let column = column.to_string();
        let Some(at) = ORDER.iter().position(|c| *c == column) else { return };
        let key = key.to_string();
        let keys: Vec<String> = (!key.is_empty()).then_some(key).into_iter().collect();
        let labels = if keys.is_empty() { Vec::new() } else { vec![label.to_string()] };
        self.as_mut().choose_keys(keys, labels, at);
    }

    /// Applies one column's Excel-style selection and clears its right side.
    pub fn choose_keys(mut self: Pin<&mut Self>, keys: Vec<String>, labels: Vec<String>, at: usize) {
        browse::write(|state| state.filter.choose(at, &keys));
        let shown = QString::from(&labels.join(", "));
        // The column chosen shows its choice; every one to its right is cleared.
        for (column, set) in Self::SETTERS.iter().enumerate().skip(at.min(Self::SETTERS.len() - 1)) {
            set(self.as_mut(), if column == at { shown.clone() } else { QString::default() });
        }
        self.refresh();
    }

    pub fn clear(mut self: Pin<&mut Self>) {
        browse::write(|s| s.filter = Filter::default());
        self.as_mut().set_queries(strings(vec![String::new(); ORDER.len()]));
        for set in Self::SETTERS {
            set(self.as_mut(), QString::default());
        }
        self.as_mut().set_excluded_sources(QStringList::default());
        self.as_mut().set_qualities(QStringList::default());
        self.as_mut().set_formats(QStringList::default());
        self.as_mut().set_extra_filtered(false);
        self.refresh();
    }

    pub fn toggle_filter(mut self: Pin<&mut Self>, kind: &QString, key: &QString) {
        let (kind, key) = (kind.to_string(), key.to_string());
        browse::write(|state| state.filter.toggle_extra(&kind, &key));
        let (sources, qualities, formats) = browse::read(|state| (
            state.filter.excluded_sources.clone(), state.filter.qualities.clone(), state.filter.formats.clone()
        ));
        let active = !(sources.is_empty() && qualities.is_empty() && formats.is_empty());
        self.as_mut().set_excluded_sources(strings(sources));
        self.as_mut().set_qualities(strings(qualities));
        self.as_mut().set_formats(strings(formats));
        self.as_mut().set_extra_filtered(active);
        self.refresh();
    }

    pub fn reset_extra(mut self: Pin<&mut Self>) {
        browse::write(|state| state.filter.reset_extra());
        self.as_mut().set_excluded_sources(QStringList::default());
        self.as_mut().set_qualities(QStringList::default());
        self.as_mut().set_formats(QStringList::default());
        self.as_mut().set_extra_filtered(false);
        self.refresh();
    }

    pub fn order(&self) -> QString {
        self.rust().order.clone()
    }

    pub fn choose_order(mut self: Pin<&mut Self>, order: QString) {
        if order == self.rust().order {
            return;
        }
        let name = order.to_string();
        app::with(|api| api.remember_preference(ORDER_KEY, &name));
        browse::write(|s| s.order = TrackOrder::from_name(&name));
        self.as_mut().rust_mut().order = order;
        self.as_mut().order_changed();
        self.refresh();
    }

    /// The library changed under the screens: read it again and tell them.
    pub fn content_changed(mut self: Pin<&mut Self>) {
        let next = self.content_rev().wrapping_add(1);
        self.as_mut().set_content_rev(next);
        self.refresh();
    }

    /// Asks the worker for the columns and the snapshot of this filter.
    fn refresh(mut self: Pin<&mut Self>) {
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        self.as_mut().set_busy(true);
        let (filter, order) = browse::read(|s| (s.filter.clone(), s.order));
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let facets = reader.facets(&filter);
            let snapshot = reader.snapshot(&filter, order);
            let _ = thread.queue(move |library| library.arrive(generation, facets, snapshot));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, facets: Facets, snapshot: Snapshot) {
        if generation != self.rust().generation {
            return;
        }
        let totals = snapshot.totals;
        self.as_mut().set_source_keys(strings(facets.sources.iter().map(|entry| entry.key.clone())));
        self.as_mut().set_source_labels(strings(facets.sources.iter().map(|entry| entry.label.clone())));
        self.as_mut().set_source_counts(strings(facets.sources.iter().map(|entry| entry.albums.to_string())));
        self.as_mut().set_source_icons(strings(facets.sources.iter().map(|entry| entry.icon.clone())));
        browse::write(|s| {
            s.facets = Rc::new(facets);
            s.snapshot = Rc::new(snapshot);
        });
        let (albums, songs) = (totals.albums.to_string(), totals.tracks.to_string());
        let line = app::with(|api| {
            format!(
                "{} · {}",
                api.tf("{} album", "{} albums", totals.albums as i64, &[&albums]),
                api.tf("{} song", "{} songs", totals.tracks as i64, &[&songs])
            )
        });
        self.as_mut().set_summary(QString::from(&line));
        self.as_mut().set_busy(false);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::Library {
    fn initialize(mut self: Pin<&mut Self>) {
        crate::registry::register_library(self.qt_thread());
        let grouped = app::with(|api| api.group_album_versions());
        self.as_mut().set_group_versions(grouped);
        let grouped = app::with(|api| api.group_artist_versions());
        self.as_mut().set_artist_group_versions(grouped);
        self.as_mut().set_queries(strings(browse::read(|s| s.filter.queries.clone())));
        let name = app::with(|api| api.preference(ORDER_KEY)).unwrap_or_else(|| "artist".to_string());
        browse::write(|s| s.order = TrackOrder::from_name(&name));
        self.as_mut().rust_mut().order = QString::from(&name);
        let albums = app::with(|api| api.album_order());
        self.as_mut().rust_mut().album_order = QString::from(&albums);
        self.as_mut()
            .on_album_order_changed(|m| {
                let order = m.album_order().to_string();
                app::with(|api| api.set_album_order(&order));
            })
            .release();
        self.refresh();
    }
}
