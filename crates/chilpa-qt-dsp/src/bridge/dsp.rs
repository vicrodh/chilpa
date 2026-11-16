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
        fn watch_view(self: Pin<&mut Dsp>, watching: bool);
        #[qinvokable]
        fn export_presets(self: Pin<&mut Dsp>, input: &QString) -> QString;
        #[qinvokable]
        fn command(self: Pin<&mut Dsp>, action: &QString, input: &QString);
    }
}
use cxx_qt_lib::QString;
use std::pin::Pin;
#[derive(Default)]
pub struct DspRust { snapshot: QString, preset_name: QString, available: bool, active: bool, viewers: usize }
impl qobject::Dsp {
    pub fn watch_view(mut self: Pin<&mut Self>, watching: bool) {
        use cxx_qt::CxxQtType;
        let viewers = if watching { self.viewers.saturating_add(1) } else { self.viewers.saturating_sub(1) };
        self.as_mut().rust_mut().viewers = viewers;
        if viewers == 0 && !self.snapshot.is_empty() { self.as_mut().set_snapshot(QString::default()); }
        if watching { self.refresh(); }
    }
    pub fn export_presets(self: Pin<&mut Self>, input: &QString) -> QString {
        QString::from(chilpa_qt_kit::app::with(|api| api.dsp_export(&input.to_string())))
    }
    pub fn command(self: Pin<&mut Self>, action: &QString, input: &QString) {
        chilpa_qt_kit::app::with(|api| api.dsp_action(&action.to_string(), &input.to_string())); self.refresh();
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        if self.viewers == 0 {
            let (available, active, name) = chilpa_qt_kit::app::with(|api| api.dsp_status());
            let name = QString::from(name);
            if self.available != available { self.as_mut().set_available(available); }
            if self.active != active { self.as_mut().set_active(active); }
            if self.preset_name != name { self.set_preset_name(name); }
            return;
        }
        let snapshot = chilpa_qt_kit::app::with(|api| api.dsp_json());
        let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap_or_default();
        let available = value["available"].as_bool().unwrap_or(false);
        let active = value["active"].as_bool().unwrap_or(false);
        let name = QString::from(value["preset"]["name"].as_str().unwrap_or(""));
        let snapshot = QString::from(snapshot);
        if self.available != available { self.as_mut().set_available(available); }
        if self.active != active { self.as_mut().set_active(active); }
        if self.preset_name != name { self.as_mut().set_preset_name(name); }
        if self.snapshot != snapshot { self.set_snapshot(snapshot); }
    }
}
