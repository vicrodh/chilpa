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
        let (thread, input) = (self.qt_thread(), input.to_string());
        std::thread::spawn(move || {
            let dialog = rfd::AsyncFileDialog::new().set_title(&title);
            let dialog = if package { dialog.add_filter("ZIP", &["zip"]) }
                else if export { dialog } else { dialog.add_filter("Images", &["svg", "png", "jpg", "jpeg"]) };
            let picked = if export { pollster::block_on(dialog.pick_folder()) } else { pollster::block_on(dialog.pick_file()) };
            if let (Some(file), Ok(mut input)) = (picked, serde_json::from_str::<serde_json::Value>(&input)) {
                input["path"] = cxx_qt_lib::QUrl::from_local_file(&QString::from(file.path().display().to_string())).to_string().to_string().into();
                let action = if package { "install_local" } else if export { "export_folder" } else { "icon" };
                let _ = thread.queue(move |_| app::with(|api| api.plugin_center_command(action, &input.to_string())));
            }
        });
    }
}
