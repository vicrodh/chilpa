//! The Library screen's filter, and the worker that answers it.
//!
//! Choosing in a column clears the columns to its right, then asks the
//! worker for the four columns and the snapshot of the tracks. Results come
//! back tagged with the generation that asked for them, so an answer to a
//! filter that has since changed is dropped instead of flashing on screen.
//! The filter lives on across navigation: leaving the screen and coming
//! back finds it as it was.

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
        #[qml_singleton]
        /// Bumped when new columns and a new snapshot are in.
        #[qproperty(i32, rev)]
        /// Bumped when the library itself changed: a scan ended, a folder
        /// was forgotten. Every screen that shows the library binds to it.
        #[qproperty(i32, content_rev)]
        /// True while the worker is answering.
        #[qproperty(bool, busy)]
        /// "22 albums · 177 songs".
        #[qproperty(QString, summary)]
        /// What each column has chosen, for the breadcrumb. Empty is "All".
        #[qproperty(QString, year)]
        #[qproperty(QString, genre)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, album)]
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

        /// Chooses in a column ("year", "genre", "artist", "album") and
        /// clears the ones to its right. An empty key is "All".
        #[qinvokable]
        fn choose(self: Pin<&mut Library>, column: &QString, key: &QString, label: &QString);

        /// Clears every column.
        #[qinvokable]
        fn clear(self: Pin<&mut Library>);
    }

    impl cxx_qt::Threading for Library {}
    impl cxx_qt::Initialize for Library {}
}

use crate::{app, browse, worker};
use chilpa_api::{Facets, Filter, Snapshot, TrackOrder};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Default)]
pub struct LibraryRust {
    rev: i32,
    content_rev: i32,
    busy: bool,
    summary: QString,
    year: QString,
    genre: QString,
    artist: QString,
    album: QString,
    order: QString,
    album_order: QString,
    generation: u64,
}

const ORDER_KEY: &str = "ui.library.order";

/// An order by its name, and back.
fn order_of(name: &str) -> TrackOrder {
    match name {
        "album" => TrackOrder::Album,
        "year" => TrackOrder::Year,
        "added" => TrackOrder::Added,
        "title" => TrackOrder::Title,
        "plays" => TrackOrder::Plays,
        "duration" => TrackOrder::Duration,
        _ => TrackOrder::Artist,
    }
}

const ORDER: [&str; 4] = ["year", "genre", "artist", "album"];

impl qobject::Library {
    pub fn choose(mut self: Pin<&mut Self>, column: &QString, key: &QString, label: &QString) {
        let column = column.to_string();
        let Some(at) = ORDER.iter().position(|c| *c == column) else { return };
        let key = key.to_string();
        browse::write(|s| {
            let f = &mut s.filter;
            match at {
                0 => f.year = key.parse().ok(),
                1 => f.genre = (!key.is_empty()).then(|| key.clone()),
                2 => f.artist = (!key.is_empty()).then(|| key.clone()),
                _ => f.album = key.parse().ok(),
            }
            // What is to the right was chosen among what this column left;
            // with a new choice here it may not exist any more.
            if at < 1 {
                f.genre = None;
            }
            if at < 2 {
                f.artist = None;
            }
            if at < 3 {
                f.album = None;
            }
        });
        let shown = if key.is_empty() { QString::default() } else { label.clone() };
        let empty = QString::default;
        match at {
            0 => {
                self.as_mut().set_year(shown);
                self.as_mut().set_genre(empty());
                self.as_mut().set_artist(empty());
                self.as_mut().set_album(empty());
            }
            1 => {
                self.as_mut().set_genre(shown);
                self.as_mut().set_artist(empty());
                self.as_mut().set_album(empty());
            }
            2 => {
                self.as_mut().set_artist(shown);
                self.as_mut().set_album(empty());
            }
            _ => self.as_mut().set_album(shown),
        }
        self.refresh();
    }

    pub fn clear(mut self: Pin<&mut Self>) {
        browse::write(|s| s.filter = Filter::default());
        for set in [Self::set_year, Self::set_genre, Self::set_artist, Self::set_album] {
            set(self.as_mut(), QString::default());
        }
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
        browse::write(|s| s.order = order_of(&name));
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
        let name = app::with(|api| api.preference(ORDER_KEY)).unwrap_or_else(|| "artist".to_string());
        browse::write(|s| s.order = order_of(&name));
        self.as_mut().rust_mut().order = QString::from(&name);
        let albums = app::with(|api| api.album_sort().name().to_string());
        self.as_mut().rust_mut().album_order = QString::from(&albums);
        self.as_mut()
            .on_album_order_changed(|m| {
                let order = chilpa_api::AlbumSort::from_name(&m.album_order().to_string());
                app::with(|api| api.set_album_sort(order));
            })
            .release();
        self.refresh();
    }
}
