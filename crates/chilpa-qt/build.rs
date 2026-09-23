use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// Los archivos QML que se compilan al binario. Los que empiezan con
/// `Layout` son acomodos de la ventana, y de ahí sale la lista que el puente
/// le ofrece al usuario: así agregar un acomodo es agregar un archivo, y no
/// hay una segunda lista que se pueda quedar atrás.
const QML: &[&str] = &[
    "qml/Main.qml",
    // Los acomodos.
    "qml/LayoutClasico.qml",
    "qml/LayoutCompacto.qml",
    // Las piezas que un acomodo puede colocar donde quiera.
    "qml/PanelSalida.qml",
    "qml/PanelAcciones.qml",
    "qml/PanelTransporte.qml",
    "qml/PanelAhoraSuena.qml",
    "qml/PanelCola.qml",
    "qml/PanelLetra.qml",
    "qml/PanelVisualizador.qml",
    "qml/CajaDeBusqueda.qml",
    "qml/SelectorDeLayout.qml",
    "qml/Biblioteca.qml",
    "qml/RejaDeArtistas.qml",
    "qml/PanelEstadisticas.qml",
    "qml/PanelListas.qml",
    "qml/PanelAsistente.qml",
    "qml/PanelPlugins.qml",
    "qml/ListaDePistas.qml",
    "qml/Resultados.qml",
];

/// Escribe la lista de acomodos para que el puente la incluya.
fn write_layouts() {
    let names: Vec<String> = QML
        .iter()
        .filter_map(|f| f.strip_prefix("qml/Layout"))
        .filter_map(|f| f.strip_suffix(".qml"))
        .map(|n| format!("    \"{n}\",\n"))
        .collect();
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("layouts.rs");
    std::fs::write(
        out,
        format!("pub const LAYOUTS: &[&str] = &[\n{}];\n", names.concat()),
    )
    .expect("se escribe la lista de acomodos");
}

fn main() {
    write_layouts();
    CxxQtBuilder::new()
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa",
            rust_files: &["src/bridge.rs"],
            qml_files: QML,
            ..Default::default()
        })
        .build();
}
