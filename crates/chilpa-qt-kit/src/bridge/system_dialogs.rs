//! The system's file dialogs for QML (`crate::dialogs`): a file, a folder or
//! where to save, asked under a name the caller gives so that the answer
//! reaches the one that asked. Cancelling answers nothing.

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
        type SystemDialogs = super::SystemDialogsRust;

        /// The place chosen for what `asker` asked.
        #[qsignal]
        fn chosen(self: Pin<&mut SystemDialogs>, asker: &QString, path: &QString);

        /// A file to open. `filters` is JSON: `[{"name": "...", "extensions": ["..."]}]`.
        #[qinvokable]
        fn pick_file(self: Pin<&mut SystemDialogs>, asker: &QString, title: &QString, filters: &QString);

        /// A folder, starting from `from` when it is one.
        #[qinvokable]
        fn pick_folder(self: Pin<&mut SystemDialogs>, asker: &QString, title: &QString, from: &QString);

        /// Where to save, with a suggested name.
        #[qinvokable]
        fn save_file(self: Pin<&mut SystemDialogs>, asker: &QString, title: &QString, suggested: &QString, filters: &QString);
    }

    impl cxx_qt::Threading for SystemDialogs {}
}

use crate::dialogs::{self, Ask};
use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct SystemDialogsRust;

/// `[{"name": "Images", "extensions": ["png", "jpg"]}]` as the dialog takes it.
fn filters(json: &QString) -> Vec<(String, Vec<String>)> {
    let parsed: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
    parsed.as_array().map(|all| all.iter().map(|filter| {
        let name = filter["name"].as_str().unwrap_or_default().to_string();
        let extensions = filter["extensions"].as_array().map(|e| e.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        (name, extensions)
    }).collect()).unwrap_or_default()
}

impl qobject::SystemDialogs {
    fn answer(self: Pin<&mut Self>, asker: &QString, title: &QString, ask: Ask) {
        let asker = asker.clone();
        dialogs::ask(self.qt_thread(), title.to_string(), ask, move |this, path| {
            this.chosen(&asker, &QString::from(&dialogs::shown(&path)));
        });
    }
    pub fn pick_file(self: Pin<&mut Self>, asker: &QString, title: &QString, filters_json: &QString) {
        self.answer(asker, title, Ask::File { filters: filters(filters_json) });
    }
    pub fn pick_folder(self: Pin<&mut Self>, asker: &QString, title: &QString, from: &QString) {
        let from = Some(from.to_string()).filter(|from| !from.is_empty()).map(std::path::PathBuf::from);
        self.answer(asker, title, Ask::Folder { from });
    }
    pub fn save_file(self: Pin<&mut Self>, asker: &QString, title: &QString, suggested: &QString, filters_json: &QString) {
        self.answer(asker, title, Ask::Save { suggested: dialogs::file_name(&suggested.to_string()), filters: filters(filters_json) });
    }
}
