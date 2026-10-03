//! I expose the sources that are browsed: where the window stands and what it sees.
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
        #[qproperty(QString, snapshot)]
        #[qproperty(i32, revision)]
        type Sources = super::SourcesRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut Sources>);
        #[qinvokable]
        fn command(self: Pin<&mut Sources>, action: &QString, id: &QString, text: &QString);
        /// A listing's page as JSON: title, state, folders, memberships.
        #[qinvokable]
        fn art(self: &Sources, plugin: &QString, image: &QString, rev: i32) -> QString;
        #[qinvokable]
        fn artist_key(self: &Sources, id: i64, path: &QString) -> QString;
        #[qinvokable]
        fn card(self: &Sources, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn page(self: &Sources, key: &QString, rev: i32) -> QString;
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct SourcesRust { snapshot: QString, revision: i32 }
impl qobject::Sources {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let snapshot = QString::from(crate::app::with(|api| api.sources_json()));
        if self.snapshot() != &snapshot {
            let next = self.revision().wrapping_add(1);
            self.as_mut().set_snapshot(snapshot);
            self.set_revision(next);
        }
    }
    pub fn art(&self, plugin: &QString, image: &QString, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.native_image(&plugin.to_string(), &image.to_string()))) }
    pub fn artist_key(&self, id: i64, path: &QString) -> QString { QString::from(crate::app::with(|api| api.source_artist_key(id, &path.to_string()).unwrap_or_default())) }
    pub fn card(&self, id: i64, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.native_card_info(id))) }
    pub fn page(&self, key: &QString, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.source_page(&key.to_string()))) }
    pub fn command(mut self: Pin<&mut Self>, action: &QString, id: &QString, text: &QString) {
        crate::app::with(|api| api.sources_command(&action.to_string(), &id.to_string(), &text.to_string()));
        self.as_mut().refresh();
    }
}
