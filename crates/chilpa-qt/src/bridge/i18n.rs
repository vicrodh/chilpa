//! Translation, seen from the window.
//!
//! QML asks for a string with its English msgid, which is also the fallback:
//! a missing translation shows a correct English sentence and not a
//! placeholder.
//!
//! Every call takes `rev` as its last argument and does nothing with it. It
//! is there so a QML binding written as `Chilpa.translate("Play",
//! Chilpa.trRev)` depends on `trRev`, and re-evaluates when the language
//! changes. Without it the binding would have no reason to run again and the
//! window would keep the old language until it was rebuilt.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {
    pub fn translate(&self, msgid: &QString, _rev: i32) -> QString {
        QString::from(&self.api.t(&msgid.to_string()))
    }

    pub fn translate_n(
        &self,
        singular: &QString,
        plural: &QString,
        n: i32,
        _rev: i32,
    ) -> QString {
        QString::from(&self.api.tn(&singular.to_string(), &plural.to_string(), i64::from(n)))
    }

    /// The common case of a count: the msgid carries one `{}` and it is
    /// filled with `n`. "1 track" and "4 tracks" out of one call.
    pub fn translate_count(
        &self,
        singular: &QString,
        plural: &QString,
        n: i32,
        _rev: i32,
    ) -> QString {
        let shown = n.to_string();
        QString::from(&self.api.tf(
            &singular.to_string(),
            &plural.to_string(),
            i64::from(n),
            &[&shown],
        ))
    }

    pub fn translate_args(&self, msgid: &QString, args: &QStringList, _rev: i32) -> QString {
        let owned: Vec<String> =
            QList::<QString>::from(args).iter().map(ToString::to_string).collect();
        let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
        QString::from(&self.api.t_args(&msgid.to_string(), &borrowed))
    }

    /// Chooses the language and remembers it. An empty code goes back to
    /// what the environment asks for.
    ///
    /// Bumping `tr_rev` is what makes every binding re-read its string; the
    /// window does not have to be rebuilt.
    pub fn choose_language(mut self: Pin<&mut Self>, code: &QString) {
        let owned = code.to_string();
        self.as_mut().rust_mut().api.choose_language(&owned);
        let now = QString::from(self.api.language());
        self.as_mut().set_language(now);
        let next = self.tr_rev().wrapping_add(1);
        self.as_mut().set_tr_rev(next);
        // The lists whose labels are built on the Rust side do not hang off
        // a QML binding, so bumping `tr_rev` is not enough for them: they
        // are rebuilt here.
        self.retranslate();
    }

    /// Rebuilds the labels this side owns, in the language chosen now.
    fn retranslate(mut self: Pin<&mut Self>) {
        let mut views = QList::<QString>::default();
        for v in super::VIEWS {
            views.append(QString::from(&self.api.t(v.1)));
        }
        self.as_mut().set_album_views(QStringList::from(&views));

        let mut drawings = QList::<QString>::default();
        for (id, name) in super::settings::VISUALIZERS {
            drawings.append(QString::from(&format!("{id}\t{}", self.api.t(name))));
        }
        self.as_mut().set_visualizers(QStringList::from(&drawings));
    }
}
