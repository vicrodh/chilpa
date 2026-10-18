//! The update inbox only serializes after its worker revision changes.
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
        type PluginUpdates = super::PluginUpdatesRust;
        #[qinvokable] fn refresh(self: Pin<&mut PluginUpdates>);
        #[qinvokable] fn command(self: Pin<&mut PluginUpdates>, action: &QString, input: &QString);
    }
}
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use cxx_qt::CxxQtType;
use std::pin::Pin;

pub struct PluginUpdatesRust { snapshot: QString, revision: u64 }
impl Default for PluginUpdatesRust {
    fn default() -> Self { Self { snapshot: QString::default(), revision: u64::MAX } }
}
impl qobject::PluginUpdates {
    pub fn refresh(mut self: Pin<&mut Self>) {
        if let Some((revision, snapshot)) = app::with(|api| api.plugin_notifications(self.rust().revision)) {
            self.as_mut().rust_mut().revision = revision;
            self.set_snapshot(QString::from(snapshot));
        }
    }
    pub fn command(self: Pin<&mut Self>, action: &QString, input: &QString) {
        app::with(|api| api.plugin_updates_command(&action.to_string(), &input.to_string()));
    }
}
