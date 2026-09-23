use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa",
            rust_files: &["src/bridge.rs"],
            qml_files: &["qml/Main.qml"],
            ..Default::default()
        })
        .build();
}
