//! I expose the importer's asynchronous workflow to its screen.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" { include!("cxx-qt-lib/qstring.h"); type QString = cxx_qt_lib::QString; }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, snapshot)]
        type PlaylistImport = super::PlaylistImportRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut PlaylistImport>);
        #[qinvokable]
        fn command(self: Pin<&mut PlaylistImport>, action: &QString, payload: &QString);
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct PlaylistImportRust { snapshot: QString }
impl qobject::PlaylistImport {
    pub fn refresh(self: Pin<&mut Self>) { let snapshot=crate::app::with(|api| api.playlist_import_json()); self.set_snapshot(QString::from(snapshot)); }
    pub fn command(mut self: Pin<&mut Self>, action: &QString, payload: &QString) {
        crate::app::with(|api| api.playlist_import_command(&action.to_string(), &payload.to_string()));
        self.as_mut().refresh();
    }
}
