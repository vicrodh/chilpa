//! The system's own file dialogs, the one way Chilpa asks for a file or a
//! folder: the desktop portal on Linux, the native panels on macOS and
//! Windows. A bridge calls [`ask`]; QML calls the `SystemDialogs` singleton,
//! which calls it too. Nothing else opens a file dialog.
//!
//! The dialog blocks until it is answered, so it runs on a thread of its own
//! and hands the place back to the window's thread, where the facade lives.
//! Cancelling it calls nothing.

use core::pin::Pin;
use std::path::{Path, PathBuf};

/// What is asked for. A filter is a name and the extensions it takes.
pub enum Ask {
    File { filters: Vec<(String, Vec<String>)> },
    Folder { from: Option<PathBuf> },
    Save { suggested: String, filters: Vec<(String, Vec<String>)> },
}

/// Opens the dialog and runs `then` on the window's thread with the place
/// chosen.
pub fn ask<T>(thread: cxx_qt::CxxQtThread<T>, title: String, ask: Ask, then: impl FnOnce(Pin<&mut T>, PathBuf) + Send + 'static)
where
    T: cxx_qt::Threading + 'static,
{
    std::thread::spawn(move || {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(&title);
        let filtered = |mut dialog: rfd::AsyncFileDialog, filters: &[(String, Vec<String>)]| {
            for (name, extensions) in filters {
                dialog = dialog.add_filter(name, extensions);
            }
            dialog
        };
        let chosen = match ask {
            Ask::File { filters } => pollster::block_on(filtered(dialog, &filters).pick_file()),
            Ask::Folder { from } => {
                if let Some(from) = from.filter(|from| from.is_dir()) {
                    dialog = dialog.set_directory(from);
                }
                pollster::block_on(dialog.pick_folder())
            }
            Ask::Save { suggested, filters } => pollster::block_on(filtered(dialog, &filters).set_file_name(&suggested).save_file()),
        };
        if let Some(chosen) = chosen {
            let path = chosen.path().to_path_buf();
            let _ = thread.queue(move |qobject| then(qobject, path));
        }
    });
}

/// A file name the dialog can suggest: no folder separators in it.
pub fn file_name(name: &str) -> String {
    name.replace(['/', '\\'], "-")
}

/// The place, as QML writes it back to a setting.
pub fn shown(path: &Path) -> String {
    path.display().to_string()
}
