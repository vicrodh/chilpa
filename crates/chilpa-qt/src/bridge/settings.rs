//! The preferences, gathered in one place.
//!
//! The window used to carry them in its top bar, one control at a time.
//! They are still read from the same properties, so a layout that wants a
//! selector where it always was keeps working; what this adds is somewhere
//! to find all of them without hunting.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

/// The visualizers that ship, as id and the msgid of their name.
///
/// The id is what is kept in the preferences and what `VisualizerPanel`
/// compares against, so it never depends on the chosen language.
pub(super) const VISUALIZERS: &[(&str, &str)] = &[
    ("wave", chilpa_api::Chilpa::mark("Wave")),
    ("spectrum", chilpa_api::Chilpa::mark("Spectrum")),
    ("goniometer", chilpa_api::Chilpa::mark("Goniometer")),
];

impl Chilpa {
    /// Reads the folders the library is made of.
    pub fn refresh_library_folders(mut self: Pin<&mut Self>) {
        let folders = self.as_mut().rust_mut().api.roots();
        let mut l = QList::<QString>::default();
        for folder in folders {
            l.append(QString::from(&folder.display().to_string()));
        }
        self.as_mut().set_library_folders(QStringList::from(&l));
    }

    /// Forgets a folder and refreshes what the panel shows.
    pub fn forget_library_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let folder = folder.to_string();
        if self.as_mut().rust_mut().api.forget_folder(&folder) {
            self.as_mut().refresh_library_folders();
            self.as_mut().refresh_albums();
        }
    }
}
