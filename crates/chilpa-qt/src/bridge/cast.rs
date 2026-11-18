//! I draw the facade's cast snapshot and dispatch picker actions.
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
        #[qproperty(QString, destination)]
        #[qproperty(bool, active)]
        type Cast = super::CastRust;
        #[qinvokable]
        fn picker(self: Pin<&mut Cast>, open: bool);
        #[qinvokable]
        fn act(self: Pin<&mut Cast>, action: &QString, protocol: &QString, id: &QString);
        #[qinvokable]
        fn refresh(self: Pin<&mut Cast>);
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct CastRust { snapshot: QString, destination: QString, active: bool }
impl qobject::Cast {
    pub fn picker(self: Pin<&mut Self>, open: bool) { crate::app::with(|api| api.cast_picker(open)); self.refresh(); }
    pub fn act(self: Pin<&mut Self>, action: &QString, protocol: &QString, id: &QString) {
        crate::app::with(|api| api.cast_act(&action.to_string(), &protocol.to_string(), &id.to_string())); self.refresh();
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        let (snapshot, destination) = crate::app::with(|api| (api.cast_json(), api.cast_destination()));
        self.as_mut().set_snapshot(QString::from(&snapshot));
        self.as_mut().set_active(destination.is_some());
        self.set_destination(QString::from(destination.as_deref().unwrap_or("")));
    }
}
