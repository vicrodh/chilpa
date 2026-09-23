//! El shell de escritorio.
//!
//! Hoy es un smoke test visual: elegir salida, abrir un archivo y ver la
//! posición avanzar con el veredicto de bit-perfect. La decisión de Widgets
//! contra Quick sigue abierta, así que esta ventana es desechable; lo que no
//! lo es son la fachada y el puente, que ya tienen la forma del ADR-001.
//!
//! Este crate es el ÚNICO que puede tener tipos de Qt.

mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

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
