//! The facade parses commands; the existing window action dispatcher applies them.
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
        type CommandLine = super::CommandsRust;
        #[qinvokable]
        fn prepare(self: &CommandLine, input: &QString) -> QString;
        #[qinvokable]
        fn complete(self: &CommandLine, input: &QString) -> QString;
        #[qinvokable]
        fn history(self: &CommandLine) -> QString;
    }
}
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
#[derive(Default)]
pub struct CommandsRust;
impl qobject::CommandLine {
    pub fn prepare(&self, input: &QString) -> QString {
        QString::from(app::with(|api| api.command_line_json(&input.to_string())))
    }
    pub fn complete(&self, input: &QString) -> QString {
        QString::from(app::with(|api| {
            api.command_complete_json(&input.to_string())
        }))
    }
    pub fn history(&self) -> QString {
        QString::from(app::with(|api| api.command_history_json()))
    }
}
