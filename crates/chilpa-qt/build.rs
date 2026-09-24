use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// The QML files compiled into the binary. The ones starting with `Layout`
/// are window layouts, and the list the bridge offers the user comes from
/// them: that way adding a layout is adding a file, and there is no second
/// list that can fall behind.
const QML: &[&str] = &[
    "qml/Main.qml",
    // The layouts.
    "qml/LayoutClassic.qml",
    "qml/LayoutCompact.qml",
    // The pieces a layout can place wherever it likes.
    "qml/OutputPanel.qml",
    "qml/ActionsPanel.qml",
    "qml/TransportPanel.qml",
    "qml/NowPlayingPanel.qml",
    "qml/QueuePanel.qml",
    "qml/LyricsPanel.qml",
    "qml/VisualizerPanel.qml",
    "qml/SearchBox.qml",
    "qml/LayoutSelector.qml",
    "qml/LanguageSelector.qml",
    "qml/Library.qml",
    "qml/ArtistGrid.qml",
    "qml/StatsPanel.qml",
    "qml/PlaylistsPanel.qml",
    "qml/WizardPanel.qml",
    "qml/PluginsPanel.qml",
    "qml/TrackList.qml",
    "qml/Results.qml",
];

/// Writes the list of layouts for the bridge to include.
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
    .expect("the list of layouts is written");
}

fn main() {
    write_layouts();
    CxxQtBuilder::new()
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa",
            // The bridge's module lives in `bridge/mod.rs`: cxx-qt only looks at the
            // file carrying `#[cxx_qt::bridge]`, and the `impl`s spread across its
            // siblings are not needed.
            rust_files: &["src/bridge/mod.rs"],
            qml_files: QML,
            ..Default::default()
        })
        .build();
}
