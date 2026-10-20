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
        type QueryChips = super::QueryChipsRust;

        /// `{"any": bool, "chips": [{"label", "text", "negated"}]}`.
        #[qinvokable]
        fn read(self: &QueryChips, query: &QString) -> QString;
        /// The query without its chip `at`.
        #[qinvokable]
        fn without(self: &QueryChips, query: &QString, at: i32) -> QString;
        /// The query with its chip `at` turned around.
        #[qinvokable]
        fn flipped(self: &QueryChips, query: &QString, at: i32) -> QString;
        /// The query with its chips joined by "any" or by "all".
        #[qinvokable]
        fn joined(self: &QueryChips, query: &QString, any: bool) -> QString;
        #[qinvokable]
        fn focus_rows(self: &QueryChips, query: &QString) -> QString;
        /// A JSON edit: kind, field and the values its control supplies.
        #[qinvokable]
        fn edit(self: &QueryChips, query: &QString, edit: &QString) -> QString;
        /// The query that also keeps out a value of a Library column.
        #[qinvokable]
        fn excluding(self: &QueryChips, query: &QString, column: &QString, value: &QString) -> QString;
    }
}

use chilpa_api::library_query as query;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct QueryChipsRust;

impl qobject::QueryChips {
    pub fn read(&self, text: &QString) -> QString {
        let (any, chips) = query::query_chips(&text.to_string());
        QString::from(&serde_json::json!({"any": any, "chips": chips}).to_string())
    }

    pub fn without(&self, text: &QString, at: i32) -> QString {
        QString::from(&query::query_without(&text.to_string(), usize::try_from(at).unwrap_or(usize::MAX)))
    }

    pub fn flipped(&self, text: &QString, at: i32) -> QString {
        QString::from(&query::query_flipped(&text.to_string(), usize::try_from(at).unwrap_or(usize::MAX)))
    }

    pub fn joined(&self, text: &QString, any: bool) -> QString {
        QString::from(&query::query_joined(&text.to_string(), any))
    }

    pub fn focus_rows(&self, text: &QString) -> QString {
        QString::from(&serde_json::to_string(&query::focus_rows(&text.to_string())).unwrap_or_default())
    }

    pub fn edit(&self, text: &QString, edit: &QString) -> QString {
        QString::from(&edit_query(&text.to_string(), &edit.to_string()))
    }

    pub fn excluding(&self, text: &QString, column: &QString, value: &QString) -> QString {
        QString::from(&query::query_excluding(&text.to_string(), &column.to_string(), &value.to_string()))
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
