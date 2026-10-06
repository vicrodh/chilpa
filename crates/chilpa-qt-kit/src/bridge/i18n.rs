//! Translation, seen from the window.
//!
//! QML asks for a string by its English msgid, which is also the fallback:
//! a missing translation shows a correct English sentence, not a key.
//!
//! Every call takes `rev` as its last argument and ignores it. It is there so
//! a binding written as `I18n.tr("Play", I18n.rev)` depends on `rev` and runs
//! again when the language changes; without it the window would keep the old
//! language until it was rebuilt.

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
        /// Bumped on every language change.
        #[qproperty(i32, rev)]
        /// The language in use, as a two letter code.
        #[qproperty(QString, language)]
        type I18n = super::I18nRust;

        #[qinvokable]
        fn tr(self: &I18n, msgid: &QString, rev: i32) -> QString;

        /// Singular or plural, chosen by `n`.
        #[qinvokable]
        fn tr_n(self: &I18n, singular: &QString, plural: &QString, n: i32, rev: i32) -> QString;

        /// A count: the msgid carries one `{}` and it is filled with `n`.
        #[qinvokable]
        fn tr_count(
            self: &I18n,
            singular: &QString,
            plural: &QString,
            n: i32,
            rev: i32,
        ) -> QString;

        /// Each `{}` in the msgid is filled with the next argument.
        #[qinvokable]
        fn tr_args(self: &I18n, msgid: &QString, args: &QStringList, rev: i32) -> QString;

        /// Chooses the language and remembers it. Empty goes back to what the
        /// environment asks for.
        #[qinvokable]
        fn choose(self: Pin<&mut I18n>, code: &QString);
    }
}

use crate::app;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

pub struct I18nRust {
    rev: i32,
    language: QString,
}

impl Default for I18nRust {
    fn default() -> Self {
        Self { rev: 0, language: QString::from(app::with(|api| api.language())) }
    }
}

impl qobject::I18n {
    pub fn tr(&self, msgid: &QString, _rev: i32) -> QString {
        QString::from(&app::t(&msgid.to_string()))
    }

    pub fn tr_n(&self, singular: &QString, plural: &QString, n: i32, _rev: i32) -> QString {
        let (one, many) = (singular.to_string(), plural.to_string());
        QString::from(&app::with(|api| api.tn(&one, &many, i64::from(n))))
    }

    pub fn tr_count(&self, singular: &QString, plural: &QString, n: i32, _rev: i32) -> QString {
        let (one, many, shown) = (singular.to_string(), plural.to_string(), n.to_string());
        QString::from(&app::with(|api| api.tf(&one, &many, i64::from(n), &[&shown])))
    }

    pub fn tr_args(&self, msgid: &QString, args: &QStringList, _rev: i32) -> QString {
        let owned: Vec<String> =
            QList::<QString>::from(args).iter().map(ToString::to_string).collect();
        let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
        let msgid = msgid.to_string();
        QString::from(&app::with(|api| api.t_args(&msgid, &borrowed)))
    }

    pub fn choose(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        let now = app::with(|api| {
            api.choose_language(&code);
            api.language()
        });
        self.as_mut().set_language(QString::from(now));
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}
