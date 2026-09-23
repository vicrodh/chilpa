use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// Los archivos QML que se compilan al binario. Los que empiezan con
/// `Layout` son acomodos de la ventana, y de ahí sale la lista que el puente
/// le ofrece al usuario: así agregar un acomodo es agregar un archivo, y no
/// hay una segunda lista que se pueda quedar atrás.
const QML: &[&str] = &[
    "qml/Main.qml",
    // Los acomodos.
    "qml/LayoutClassic.qml",
    "qml/LayoutCompact.qml",
    // Las piezas que un acomodo puede colocar donde quiera.
    "qml/OutputPanel.qml",
    "qml/ActionsPanel.qml",
    "qml/TransportPanel.qml",
    "qml/NowPlayingPanel.qml",
    "qml/QueuePanel.qml",
    "qml/LyricsPanel.qml",
    "qml/VisualizerPanel.qml",
    "qml/SearchBox.qml",
    "qml/LayoutSelector.qml",
    "qml/Library.qml",
    "qml/ArtistGrid.qml",
    "qml/StatsPanel.qml",
    "qml/PlaylistsPanel.qml",
    "qml/WizardPanel.qml",
    "qml/PluginsPanel.qml",
    "qml/TrackList.qml",
    "qml/Results.qml",
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
