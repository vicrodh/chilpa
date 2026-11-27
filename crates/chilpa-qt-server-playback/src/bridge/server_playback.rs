//! A server's playback settings and the facade: they are read whole and
//! changed one at a time, and the facade refuses what it does not know.
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
        type ServerPlayback = super::ServerPlaybackRust;

        /// The server's settings as JSON: its name and kind, what it can be
        /// told, and what it is told.
        #[qinvokable]
        fn settings(self: &ServerPlayback, id: i64) -> QString;
        /// Changes one of them; false when the facade refused it.
        #[qinvokable]
        fn change(self: &ServerPlayback, id: i64, name: &QString, value: &QString) -> bool;
    }
}

use cxx_qt_lib::QString;

#[derive(Default)]
pub struct ServerPlaybackRust;

impl qobject::ServerPlayback {
    pub fn settings(&self, id: i64) -> QString {
        QString::from(chilpa_qt_kit::app::with(|api| api.server_playback_json(id)))
    }

    pub fn change(&self, id: i64, name: &QString, value: &QString) -> bool {
        chilpa_qt_kit::app::with(|api| api.set_server_playback(id, &name.to_string(), &value.to_string()))
    }
}
