//! One plugin manager's revision cursor and native file dialogs.
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
        type PluginCenterModel = super::PluginCenterModelRust;
        /// Returns an empty string when the document has not changed.
        #[qinvokable]
        fn plugins_json(self: Pin<&mut PluginCenterModel>) -> QString;
        #[qinvokable]
        fn plugin_command(self: &PluginCenterModel, action: &QString, input: &QString);
    }
    impl cxx_qt::Threading for PluginCenterModel {}
}
use crate::app;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct PluginCenterModelRust { revision: Option<u64> }

impl qobject::PluginCenterModel {
    pub fn plugins_json(mut self: Pin<&mut Self>) -> QString {
        let Some((revision, document)) = app::with(|api| api.plugin_center_snapshot(self.rust().revision)) else { return QString::default(); };
        self.as_mut().rust_mut().revision = Some(revision);
        QString::from(document)
    }
    pub fn plugin_command(&self, action: &QString, input: &QString) {
        let action = action.to_string();
        if !matches!(action.as_str(), "choose_icon" | "choose_package" | "choose_export_folder") {
            return app::with(|api| api.plugin_center_command(&action, &input.to_string()));
        }
        let package = action == "choose_package";
        let export = action == "choose_export_folder";
        let title = if package { app::t("Install plugin from ZIP") } else if export { app::t("Choose download folder") } else { app::t("Choose plugin icon") };
        let input = input.to_string();
        let filter = |name: &str, extensions: &[&str]| vec![(name.to_string(), extensions.iter().map(|e| e.to_string()).collect())];
        let ask = if package { chilpa_qt_kit::dialogs::Ask::File { filters: filter("ZIP", &["zip"]) } }
            else if export { chilpa_qt_kit::dialogs::Ask::Folder { from: None } }
            else { chilpa_qt_kit::dialogs::Ask::File { filters: filter("Images", &["svg", "png", "jpg", "jpeg"]) } };
        chilpa_qt_kit::dialogs::ask(self.qt_thread(), title, ask, move |_, path| {
            if let Ok(mut input) = serde_json::from_str::<serde_json::Value>(&input) {
                input["path"] = cxx_qt_lib::QUrl::from_local_file(&QString::from(path.display().to_string())).to_string().to_string().into();
                let action = if package { "install_local" } else if export { "export_folder" } else { "icon" };
                app::with(|api| api.plugin_center_command(action, &input.to_string()));
            }
        });
    }
}
