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

    pub fn excluding(&self, text: &QString, column: &QString, value: &QString) -> QString {
        QString::from(&query::query_excluding(&text.to_string(), &column.to_string(), &value.to_string()))
    }
}
