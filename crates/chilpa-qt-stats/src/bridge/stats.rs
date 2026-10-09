//! The stats documents as QML reads them, each one JSON document from the
//! facade: a period's report and a page of the history; and the two ways
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

        /// One page of the history, newest first: `page` from 1, with 20, 50
        /// or 100 listens, only those that hold every word of `query`.
        #[qinvokable]
        fn history(self: &Stats, page: i32, per_page: i32, query: &QString, rev: i32) -> QString;

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

    pub fn history(&self, page: i32, per_page: i32, query: &QString, _rev: i32) -> QString {
        let query = query.to_string();
        QString::from(&app::with(|api| api.listening_history_json(i64::from(page), i64::from(per_page), &query)))
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

    /// The system's save dialog blocks until it is answered, so it runs on
    /// a thread of its own and hands the place back to the window's thread,
    /// where the facade lives. Cancelling it does nothing.
    fn ask(self: Pin<&mut Self>, title: String, suggested: String, then: impl FnOnce(&mut chilpa_api::Chilpa, &Path) -> Option<String> + Send + 'static) {
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let chosen = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).set_file_name(&suggested).save_file());
            if let Some(to) = chosen {
                let to = to.path().to_path_buf();
                let _ = thread.queue(move |stats| {
                    if let Some(note) = app::with(|api| then(api, &to)) {
                        stats.saved(QString::from(&note));
                    }
                });
            }
        });
    }
}
