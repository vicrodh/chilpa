//! I expose the sources that are browsed: where the window stands and what it sees.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
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
        fn art(self: &Sources, plugin: &QString, image: &QString, rev: i32) -> QUrl;
        #[qinvokable]
        fn artist_key(self: &Sources, id: i64, path: &QString) -> QString;
        #[qinvokable]
        fn reference_card(self: &Sources, plugin: &QString, reference: &QString) -> i64;
        #[qinvokable]
        fn card(self: &Sources, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn art_colors(self: &Sources, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn album_libraries(self: &Sources, rev: i32) -> QString;
        #[qinvokable]
        fn track_libraries(self: &Sources, rev: i32) -> QString;
        #[qinvokable]
        fn home_libraries(self: &Sources, rev: i32) -> QString;
        #[qinvokable]
        fn page(self: &Sources, key: &QString, rev: i32) -> QString;
        /// What MusicBrainz and Last.fm know of an artist by name, for the
        /// page's Network sidebar; `loading` until it is read.
        #[qinvokable]
        fn network(self: &Sources, name: &QString, rev: i32) -> QString;
    }
}
use cxx_qt_lib::{QString, QUrl};
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
    pub fn art(&self, plugin: &QString, image: &QString, _rev: i32) -> QUrl {
        let path = crate::app::with(|api| api.native_image(&plugin.to_string(), &image.to_string()));
        if path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(path)) }
    }
    pub fn artist_key(&self, id: i64, path: &QString) -> QString { QString::from(crate::app::with(|api| api.source_artist_key(id, &path.to_string()).unwrap_or_default())) }
    pub fn reference_card(&self, plugin: &QString, reference: &QString) -> i64 { crate::app::with(|api| api.native_reference_card(&plugin.to_string(), &reference.to_string())) }
    pub fn card(&self, id: i64, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.native_card_info(id))) }
    pub fn art_colors(&self, id: i64, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.native_card_art_colors(id))) }
    pub fn track_libraries(&self, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.track_libraries())) }
    pub fn home_libraries(&self, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.home_libraries())) }
    pub fn album_libraries(&self, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.album_libraries())) }
    pub fn page(&self, key: &QString, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.source_page(&key.to_string()))) }
    pub fn network(&self, name: &QString, _rev: i32) -> QString { QString::from(crate::app::with(|api| api.artist_network(&name.to_string()))) }
    pub fn command(mut self: Pin<&mut Self>, action: &QString, id: &QString, text: &QString) {
        crate::app::with(|api| api.sources_command(&action.to_string(), &id.to_string(), &text.to_string()));
        self.as_mut().refresh();
    }
}
