//! The seek bar's waveform, as its own QML module (`app.chilpa.waveform`):
//! the bars under the player bar when it stands tall, and the bridge that
//! reads the document the facade keeps. Each shell crate stays inside the
//! shell's budgets (ADR-001).
use cxx_qt_build::{CxxQtBuilder, Interface, QmlModule};
use std::path::{Path, PathBuf};

fn files(dir: &str, keep: &dyn Fn(&Path) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::from(dir)];
    pending.retain(|p| p.is_dir());
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("the directory is readable") {
            let path = entry.expect("the entry is readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if keep(&path) {
                out.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn main() {
    let qml = files("qml", &|p| p.extension().is_some_and(|e| e == "qml"));
    let bridges = files("src/bridge", &|p| p.extension().is_some_and(|e| e == "rs"));
    // A layer's `qmldir` declares its singletons.
    let resources = files("qml", &|p| p.file_name().is_some_and(|n| n == "qmldir"));
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=src/bridge");
    CxxQtBuilder::library(Interface::default())
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qml_module(QmlModule {
            uri: "app.chilpa.waveform",
            rust_files: &bridges,
            qml_files: &qml,
            qrc_files: &resources,
            ..Default::default()
        })
        .build();
}
