//! The desktop shell.
//!
//! Rust owns the state and QML draws it (ADR-001). The QML tree under `qml/`
//! is layered -- Theme, Primitives, Components, Patterns, Screens, then the
//! window -- and `tools/lint` checks the layering, the per-file budgets and
//! that no color or measure is written outside the theme.
//!
//! This crate is the ONLY one that may hold Qt types.

mod app;
mod bridge;
mod browse;
mod format;
mod navigation;
mod theme;
mod worker;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
// Only Linux has a desktop file to be named after, and only there is the
// name what makes the window pick up its icon.
#[cfg(target_os = "linux")]
use cxx_qt_lib::QString;

fn main() {
    let mut app = QGuiApplication::new();
    #[cfg(target_os = "linux")]
    QGuiApplication::set_desktop_file_name(&QString::from("com.blitzfc.chilpa"));

    // CHILPA_GALLERY opens the Gallery instead of the window: every piece of
    // the interface in its states, in either theme.
    let root = if std::env::var_os("CHILPA_GALLERY").is_some() { "Gallery/Gallery.qml" } else { "Main.qml" };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(&format!("qrc:/qt/qml/com/blitzfc/chilpa/qml/{root}")));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
