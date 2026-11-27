//! The stats documents as QML reads them, each one JSON document from the
//! facade: a period's report, a year's mosaic, a recap and a page of the
//! history; and the two ways
//! out of the screen, the history as CSV and the report as a picture.
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
        type Stats = super::StatsRust;

        /// The report of the period of `kind` ("day", "week", "month",
        /// "year" or "all") that holds `anchor`, a local date or empty for
        /// today, with weeks starting on `first_day` as `Qt.locale()` says
        /// it; `rev` is the language's, so its labels follow it.
        #[qinvokable]
        fn report(self: &Stats, kind: &QString, anchor: &QString, first_day: i32, rev: i32) -> QString;

        /// The mosaic of the year that holds `anchor`, `back` years before
        /// it: a square a day, laid out a week a column.
        #[qinvokable]
        fn mosaic(self: &Stats, anchor: &QString, back: i32, first_day: i32, rev: i32) -> QString;

        /// The recap of a month (1 to 12) or of a whole year (month 0); a
        /// year of 0 is the month before this one.
        #[qinvokable]
        fn recap(self: &Stats, year: i32, month: i32, rev: i32) -> QString;

        /// One page of the history, newest first: `page` from 1, with 20, 50
        /// or 100 listens, only those that hold every word of `query`, of the
        /// days from `from` to `to` ("2026-10-09"; empty for no bound), and
        /// the skipped ones only `with_skipped`.
        #[qinvokable]
        fn history(self: &Stats, page: i32, per_page: i32, query: &QString, from: &QString, to: &QString, with_skipped: bool, rev: i32) -> QString;

        /// One page of the ranking of `kind` ("artists", "albums" or
        /// "genres") over the days from `from` to `to`, most played first.
        #[qinvokable]
        fn ranking(self: &Stats, kind: &QString, from: &QString, to: &QString, page: i32, per_page: i32, rev: i32) -> QString;

        /// A bounded page of the persisted local analysis inventory.
        #[qinvokable]
        fn inventory(self: &Stats, filter: &QString, search: &QString, offset: i32, run: i64) -> QString;

        /// Asks where, and writes the whole history there as CSV.
        #[qinvokable]
        fn export_history(self: Pin<&mut Stats>);

        /// Where the screen leaves its picture before asking where it goes.
        #[qinvokable]
        fn picture_path(self: &Stats) -> QString;

        /// Asks where, and copies the picture at `from` there, named after
        /// `name`, the period it shows.
        #[qinvokable]
        fn save_picture(self: Pin<&mut Stats>, from: &QString, name: &QString);

        /// A file went out: what the screen says about it.
        #[qsignal]
        fn saved(self: Pin<&mut Stats>, note: QString);
    }

    impl cxx_qt::Threading for Stats {}
}

use chilpa_qt_kit::app;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use std::path::{Path, PathBuf};
use std::pin::Pin;

#[derive(Default)]
pub struct StatsRust;

impl qobject::Stats {
    pub fn report(&self, kind: &QString, anchor: &QString, first_day: i32, _rev: i32) -> QString {
        let (kind, anchor) = (kind.to_string(), anchor.to_string());
        QString::from(&app::with(|api| api.stats_report_json(&kind, &anchor, i64::from(first_day))))
    }

    pub fn mosaic(&self, anchor: &QString, back: i32, first_day: i32, _rev: i32) -> QString {
        let anchor = anchor.to_string();
        QString::from(&app::with(|api| api.stats_mosaic_json(&anchor, i64::from(back), i64::from(first_day))))
    }

    pub fn recap(&self, year: i32, month: i32, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.recap_json(i64::from(year), i64::from(month))))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn history(&self, page: i32, per_page: i32, query: &QString, from: &QString, to: &QString, with_skipped: bool, _rev: i32) -> QString {
        let (query, from, to) = (query.to_string(), from.to_string(), to.to_string());
        QString::from(&app::with(|api| api.listening_history_json(i64::from(page), i64::from(per_page), &query, &from, &to, with_skipped)))
    }

    pub fn ranking(&self, kind: &QString, from: &QString, to: &QString, page: i32, per_page: i32, _rev: i32) -> QString {
        let (kind, from, to) = (kind.to_string(), from.to_string(), to.to_string());
        QString::from(&app::with(|api| api.listening_ranking_json(&kind, &from, &to, i64::from(page), i64::from(per_page))))
    }

    pub fn inventory(&self, filter: &QString, search: &QString, offset: i32, run: i64) -> QString {
        let (filter, search) = (filter.to_string(), search.to_string());
        QString::from(app::with(|api| api.analysis_inventory_for_run(&filter, &search, offset.max(0) as usize, run).to_string()))
    }

    pub fn export_history(self: Pin<&mut Self>) {
        let title = app::t("Export listening history");
        let suggested = format!("{}.csv", app::t("Listening history"));
        self.ask(title, suggested, |api, to| api.export_listening_history(to));
    }

    pub fn picture_path(&self) -> QString {
        QString::from(&app::with(|api| api.stats_picture_path()).display().to_string())
    }

    pub fn save_picture(self: Pin<&mut Self>, from: &QString, name: &QString) {
        let from = PathBuf::from(from.to_string());
        let title = app::t("Save as an image");
        let suggested = format!("{} {}.png", app::t("Listening stats"), name.to_string().replace('/', "-"));
        self.ask(title, suggested, move |api, to| api.save_art(&from, to).then(|| api.t("Image saved")));
    }

    /// Asks where with the system's save dialog and saves there.
    /// Cancelling it does nothing.
    fn ask(self: Pin<&mut Self>, title: String, suggested: String, then: impl FnOnce(&mut chilpa_api::Chilpa, &Path) -> Option<String> + Send + 'static) {
        let ask = chilpa_qt_kit::dialogs::Ask::Save { suggested, filters: Vec::new() };
        chilpa_qt_kit::dialogs::ask(self.qt_thread(), title, ask, move |stats, to| {
            if let Some(note) = app::with(|api| then(api, &to)) {
                stats.saved(QString::from(&note));
            }
        });
    }
}
