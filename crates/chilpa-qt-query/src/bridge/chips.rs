//! A Library query read into chips and edited as text: the window keeps the
//! query in `Library.queryText` and applies what these return.
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
        #[qproperty(bool, can_undo)]
        #[qproperty(QString, query_text)]
        type QueryChips = super::QueryChipsRust;

        /// I bind this bridge's queryText to Library.queryText to observe every replacement.
        #[qinvokable]
        fn observe(self: Pin<&mut QueryChips>, query: &QString);
        #[qinvokable]
        fn undo(self: Pin<&mut QueryChips>) -> QString;
        #[qinvokable]
        fn refine(self: &QueryChips, query: &QString) -> QString;
        #[qinvokable]
        fn refine_edit(self: Pin<&mut QueryChips>, query: &QString, edit: &QString) -> QString;
        /// A list that will ask suggestion rounds: the handle its rounds and
        /// their answers carry, until `close_suggestions` takes it back.
        #[qinvokable]
        fn open_suggestions(self: Pin<&mut QueryChips>) -> i32;
        #[qinvokable]
        fn close_suggestions(self: Pin<&mut QueryChips>, consumer: i32);
        /// I start a suggestion round for what a list has typed; `suggested`
        /// brings the answer to the list's latest round, never to an older one.
        #[qinvokable]
        fn suggest(self: Pin<&mut QueryChips>, consumer: i32, query: &QString, typed: &QString);
        /// The answer to a list's latest `suggest`, with the query and text
        /// it was asked for.
        #[qsignal]
        fn suggested(self: Pin<&mut QueryChips>, consumer: i32, query: QString, typed: QString, found: QString);
        #[qinvokable]
        fn notes(self: &QueryChips, query: &QString) -> QString;
        #[qinvokable]
        fn without_each(self: &QueryChips, query: &QString) -> QString;
        #[qinvokable]
        fn check(self: &QueryChips, query: &QString) -> QString;
        /// `{"any": bool, "chips": [{"label", "text", "negated"}]}`.
        #[qinvokable]
        fn read(self: &QueryChips, query: &QString) -> QString;
        /// The query without its chip `at`.
        #[qinvokable]
        fn without(self: Pin<&mut QueryChips>, query: &QString, at: i32) -> QString;
        /// The query with its chip `at` turned around.
        #[qinvokable]
        fn flipped(self: Pin<&mut QueryChips>, query: &QString, at: i32) -> QString;
        /// The query with its chips joined by "any" or by "all".
        #[qinvokable]
        fn joined(self: Pin<&mut QueryChips>, query: &QString, any: bool) -> QString;
        #[qinvokable]
        fn focus_rows(self: &QueryChips, query: &QString) -> QString;
        /// A JSON edit: kind, field and the values its control supplies.
        #[qinvokable]
        fn edit(self: Pin<&mut QueryChips>, query: &QString, edit: &QString) -> QString;
        /// The query that also keeps out a value of a Library column.
        #[qinvokable]
        fn excluding(self: Pin<&mut QueryChips>, query: &QString, column: &QString, value: &QString) -> QString;
    }
    impl cxx_qt::Initialize for QueryChips {}
    impl cxx_qt::Threading for QueryChips {}
}

use chilpa_api::library_query as query;
use cxx_qt_lib::QString;
use cxx_qt::{CxxQtType, Threading as _};
use std::pin::Pin;

#[derive(Default)]
pub struct QueryChipsRust {
    can_undo: bool,
    query_text: QString,
    history: crate::history::History,
    /// Each open list's latest suggestion round; an answer to an older one,
    /// or to a list that closed, is dropped.
    rounds: std::collections::HashMap<i32, u64>,
}

impl qobject::QueryChips {
    pub fn observe(mut self: Pin<&mut Self>, text: &QString) {
        self.as_mut().rust_mut().history.observe(&text.to_string());
        let can_undo=self.rust().history.can_undo(); self.as_mut().set_can_undo(can_undo);
    }
    fn applied(mut self: Pin<&mut Self>, before: &QString, after: String) -> QString {
        self.as_mut().rust_mut().history.applied(&before.to_string(),&after);
        let can_undo=self.rust().history.can_undo(); self.as_mut().set_can_undo(can_undo);
        QString::from(&after)
    }
    pub fn undo(mut self: Pin<&mut Self>) -> QString {
        let text=self.as_mut().rust_mut().history.undo();
        let can_undo=self.rust().history.can_undo(); self.as_mut().set_can_undo(can_undo);
        QString::from(&text)
    }
    pub fn refine(&self, text: &QString) -> QString { json(query::refine(&text.to_string())) }
    pub fn check(&self, text: &QString) -> QString { json(query::check(&text.to_string())) }
    pub fn refine_edit(self: Pin<&mut Self>, text: &QString, edit: &QString) -> QString {
        let (before,edit)=(text.to_string(),edit.to_string());
        let after=if serde_json::from_str::<serde_json::Value>(&edit).ok().is_some_and(|v| v["op"]=="literal") {
            chilpa_api::Reader::for_refine().map(|r| r.refine_edit(&before,&edit)).unwrap_or(before)
        } else { query::refine_edit(&before,&edit) };
        self.applied(text,after)
    }
    pub fn open_suggestions(self: Pin<&mut Self>) -> i32 {
        let consumer = i32::try_from(query::suggest_consumer()).unwrap_or(i32::MAX);
        self.rust_mut().rounds.insert(consumer, 0);
        consumer
    }
    pub fn close_suggestions(self: Pin<&mut Self>, consumer: i32) {
        self.rust_mut().rounds.remove(&consumer);
        query::suggest_release(consumer as u64);
    }
    pub fn suggest(mut self: Pin<&mut Self>, consumer: i32, text: &QString, typed: &QString) {
        let round = self.rust().rounds.get(&consumer).copied().unwrap_or(0) + 1;
        self.as_mut().rust_mut().rounds.insert(consumer, round);
        let (text, typed) = (text.to_string(), typed.to_string());
        let thread = self.qt_thread();
        query::suggest_later(consumer as u64, &text.clone(), &typed.clone(), move |found| {
            let _ = thread.queue(move |mut chips| {
                if chips.rust().rounds.get(&consumer) != Some(&round) { return; }
                chips.as_mut().suggested(consumer, QString::from(&text), QString::from(&typed), json(found));
            });
        });
    }
    pub fn notes(&self, text: &QString) -> QString {
        json(serde_json::to_value(chilpa_api::Reader::for_refine().map(|r| r.refine_notes(&text.to_string())).unwrap_or_default()).unwrap_or_default())
    }
    pub fn without_each(&self, text: &QString) -> QString {
        json(chilpa_api::Reader::for_refine().map(|r| r.refine_without_each(&text.to_string())).unwrap_or_else(|| serde_json::json!([])))
    }

    pub fn read(&self, text: &QString) -> QString {
        let (any, chips) = query::query_chips(&text.to_string());
        QString::from(&serde_json::json!({"any": any, "chips": chips}).to_string())
    }

    pub fn without(self: Pin<&mut Self>, text: &QString, at: i32) -> QString {
        self.applied(text, query::query_without(&text.to_string(), usize::try_from(at).unwrap_or(usize::MAX)))
    }

    pub fn flipped(self: Pin<&mut Self>, text: &QString, at: i32) -> QString {
        self.applied(text, query::query_flipped(&text.to_string(), usize::try_from(at).unwrap_or(usize::MAX)))
    }

    pub fn joined(self: Pin<&mut Self>, text: &QString, any: bool) -> QString {
        self.applied(text, query::query_joined(&text.to_string(), any))
    }

    pub fn focus_rows(&self, text: &QString) -> QString {
        QString::from(&serde_json::to_string(&query::focus_rows(&text.to_string())).unwrap_or_default())
    }

    pub fn edit(self: Pin<&mut Self>, text: &QString, edit: &QString) -> QString {
        self.applied(text, edit_query(&text.to_string(), &edit.to_string()))
    }

    pub fn excluding(self: Pin<&mut Self>, text: &QString, column: &QString, value: &QString) -> QString {
        self.applied(text, query::query_excluding(&text.to_string(), &column.to_string(), &value.to_string()))
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Edit {
    Value { field: String, value: String, state: query::ValueState },
    Range { field: String, from: Option<i64>, to: Option<i64> },
    Within { field: String, days: i64 },
    Missing { field: String, on: bool },
    Minimum { field: String, stars: Option<i64> },
}

fn edit_query(text: &str, edit: &str) -> String {
    match serde_json::from_str(edit) {
        Ok(Edit::Value { field, value, state }) => query::with_value(text, &field, &value, state),
        Ok(Edit::Range { field, from, to }) => query::with_range(text, &field, from, to),
        Ok(Edit::Within { field, days }) => query::with_within(text, &field, days),
        Ok(Edit::Missing { field, on }) => query::with_missing(text, &field, on),
        Ok(Edit::Minimum { field, stars }) => query::with_minimum(text, &field, stars),
        Err(_) => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::edit_query;

    #[test]
    fn json_edits_return_parser_text_and_reject_incomplete_requests() {
        for (edit, expected) in [
            (r#"{"kind":"value","field":"genre","value":"Jazz","state":"include"}"#, "genre:Jazz"),
            (r#"{"kind":"value","field":"genre","value":"Jazz","state":"exclude"}"#, "-genre:Jazz"),
            (r#"{"kind":"range","field":"year","from":1960,"to":1979}"#, "year:1960..1979"),
            (r#"{"kind":"within","field":"added","days":30}"#, "added:1m"),
            (r#"{"kind":"missing","field":"composer","on":true}"#, "composer:none"),
            (r#"{"kind":"minimum","field":"rating","stars":4}"#, "rating:4.."),
        ] { assert_eq!(edit_query("", edit), expected); }
        assert_eq!(edit_query("genre:Jazz", r#"{"kind":"value"}"#), "genre:Jazz");
        assert_eq!(edit_query("genre:Jazz", "{"), "genre:Jazz");
        assert_eq!(edit_query("year:1960..1979", r#"{"kind":"range","field":"year","from":null,"to":null}"#), "");
    }
}

fn json(value: serde_json::Value) -> QString { QString::from(&value.to_string()) }

impl cxx_qt::Initialize for qobject::QueryChips {
    fn initialize(mut self: Pin<&mut Self>) {
        let text=self.query_text().clone(); self.as_mut().observe(&text);
        self.as_mut().on_query_text_changed(|mut bridge| {
            let text=bridge.query_text().clone(); bridge.as_mut().observe(&text);
        }).release();
    }
}
