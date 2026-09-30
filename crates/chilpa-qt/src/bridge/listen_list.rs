//! I expose the facade's buffers; the side panel remains the live queue.
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
        type ListenList = super::ListenListRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut ListenList>);
        #[qinvokable]
        fn command(self: Pin<&mut ListenList>, action: &QString, id: &QString, text: &QString);
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct ListenListRust { snapshot: QString, revision: i32 }
impl qobject::ListenList {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let snapshot = QString::from(crate::app::with(|api| api.listen_json()));
        if self.snapshot() != &snapshot {
            let next = self.revision().wrapping_add(1);
            self.as_mut().set_snapshot(snapshot);
            self.set_revision(next);
        }
    }
    pub fn command(mut self: Pin<&mut Self>, action: &QString, id: &QString, text: &QString) {
        crate::app::with(|api| api.listen_command(&action.to_string(), &id.to_string(), &text.to_string()));
        if action.to_string() == "save" { crate::registry::playlists(|lists| lists.changed()); }
        self.as_mut().refresh();
    }
}
