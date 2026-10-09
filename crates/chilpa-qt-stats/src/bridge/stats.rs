//! The stats documents as QML reads them, each one JSON document from the
//! facade: a period's report and a page of the history.
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

        /// The report of `period` ("week", "month", "year" or "all"),
        /// `back` periods before the one that ends today; `rev` is the
        /// language's, so its labels follow it.
        #[qinvokable]
        fn report(self: &Stats, period: &QString, back: i32, rev: i32) -> QString;

        /// One page of the history, newest first: `page` from 1, with 20, 50
        /// or 100 listens.
        #[qinvokable]
        fn history(self: &Stats, page: i32, per_page: i32, rev: i32) -> QString;
    }
}

use chilpa_qt_kit::app;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct StatsRust;

impl qobject::Stats {
    pub fn report(&self, period: &QString, back: i32, _rev: i32) -> QString {
        let period = period.to_string();
        QString::from(&app::with(|api| api.stats_report_json(&period, i64::from(back))))
    }

    pub fn history(&self, page: i32, per_page: i32, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.listening_history_json(i64::from(page), i64::from(per_page))))
    }
}
