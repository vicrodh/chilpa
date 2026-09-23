use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa",
            rust_files: &["src/bridge.rs"],
            qml_files: &[
                "qml/Main.qml",
                // Los acomodos. Agregar uno es agregar su archivo aquí.
                "qml/LayoutClasico.qml",
                // Las piezas que un acomodo puede colocar donde quiera.
                "qml/PanelSalida.qml",
                "qml/PanelAcciones.qml",
                "qml/PanelTransporte.qml",
                "qml/PanelAhoraSuena.qml",
                "qml/PanelCola.qml",
                "qml/CajaDeBusqueda.qml",
                "qml/Biblioteca.qml",
                "qml/Resultados.qml",
            ],
            ..Default::default()
        })
        .build();
}
