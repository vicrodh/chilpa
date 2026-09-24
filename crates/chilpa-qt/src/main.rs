//! The desktop shell.
//!
//! Today it is a visual smoke test: choose an output, open a file and watch
//! the position advance with the bit-perfect verdict. The Widgets versus
//! Quick decision is still open, so this window is disposable; what is not
//! are the facade and the bridge, which already have ADR-001's shape.
//!
//! This crate is the ONLY one that may hold Qt types.

mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
// Only Linux has a desktop file to be named after, and only there is the
// name what makes the window pick up its icon.
#[cfg(target_os = "linux")]
use cxx_qt_lib::QString;

fn main() {
    let mut app = QGuiApplication::new();
    #[cfg(target_os = "linux")]
    QGuiApplication::set_desktop_file_name(&QString::from("com.blitzfc.chilpa"));

    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/blitzfc/chilpa/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
