//! I expose the facade's processing snapshot to the player and quality view.
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
        #[qproperty(QString, preset_name)]
        #[qproperty(bool, available)]
        #[qproperty(bool, active)]
        type Dsp = super::DspRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut Dsp>);
        #[qinvokable]
        fn command(self: Pin<&mut Dsp>, action: &QString, input: &QString);
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct DspRust { snapshot: QString, preset_name: QString, available: bool, active: bool }
impl qobject::Dsp {
    pub fn command(self: Pin<&mut Self>, action: &QString, input: &QString) {
        chilpa_qt_kit::app::with(|api| api.dsp_action(&action.to_string(), &input.to_string())); self.refresh();
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        let snapshot = chilpa_qt_kit::app::with(|api| api.dsp_json());
        let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap_or_default();
        self.as_mut().set_available(value["available"].as_bool().unwrap_or(false));
        self.as_mut().set_active(value["active"].as_bool().unwrap_or(false));
        self.as_mut().set_preset_name(QString::from(value["preset"]["name"].as_str().unwrap_or("")));
        self.set_snapshot(QString::from(snapshot));
    }
}
