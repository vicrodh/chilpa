//! I publish one committed catalog summary to every Refine surface, on demand.
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
        #[qproperty(QString, catalog)]
        #[qproperty(i32, rev)]
        type SavedQueries = super::SavedQueriesRust;
        #[qinvokable] fn load(self: Pin<&mut SavedQueries>) -> QString;
        #[qinvokable] fn save(self: Pin<&mut SavedQueries>, name: &QString, text: &QString) -> QString;
        #[qinvokable] fn rename(self: Pin<&mut SavedQueries>, id: &QString, name: &QString) -> QString;
        #[qinvokable] fn remove(self: Pin<&mut SavedQueries>, id: &QString) -> QString;
        #[qinvokable] fn expression(self: &SavedQueries, id: &QString) -> QString;
        #[qinvokable] fn previews(self: &SavedQueries) -> QString;
    }
}
use chilpa_api::saved_queries as saved;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct SavedQueriesRust { catalog: QString, rev: i32 }

impl qobject::SavedQueries {
    fn publish(mut self: Pin<&mut Self>, result: Result<saved::Catalog, String>) -> QString {
        match result {
            Err(error) => QString::from(&error),
            Ok(catalog) => {
                let json = QString::from(&catalog.summary_json());
                if self.catalog() != &json {
                    self.as_mut().set_catalog(json);
                    let next = self.rev().wrapping_add(1);
                    self.as_mut().set_rev(next);
                }
                QString::default()
            }
        }
    }
    pub fn load(self: Pin<&mut Self>) -> QString { self.publish(saved::list()) }
    pub fn save(self: Pin<&mut Self>, name: &QString, text: &QString) -> QString {
        self.publish(saved::save(&name.to_string(), &text.to_string()))
    }
    pub fn rename(self: Pin<&mut Self>, id: &QString, name: &QString) -> QString {
        self.publish(saved::rename(&id.to_string(), &name.to_string()))
    }
    pub fn remove(self: Pin<&mut Self>, id: &QString) -> QString {
        self.publish(saved::remove(&id.to_string()))
    }
    pub fn expression(&self, id: &QString) -> QString {
        let result = match saved::expression(&id.to_string()) {
            Ok(text) => serde_json::json!({"expression":text}),
            Err(error) => serde_json::json!({"error":error}),
        };
        QString::from(&result.to_string())
    }
    /// I lend one bounded snapshot to an open dialog, without retaining it here.
    pub fn previews(&self) -> QString {
        let result = match saved::list() {
            Ok(catalog) => serde_json::json!({"entries": catalog.entries}),
            Err(error) => serde_json::json!({"error": error}),
        };
        QString::from(&result.to_string())
    }
}
