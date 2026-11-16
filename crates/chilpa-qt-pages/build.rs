use cxx_qt_build::{CxxQtBuilder, Interface, QmlModule};
use std::path::Path;
fn files(path: &Path, output: &mut Vec<String>) {
    if !path.is_dir() {
        return;
    }
    for entry in std::fs::read_dir(path).expect("readable QML directory") {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "qml") {
            output.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}
fn main() {
    let mut qml = Vec::new();
    files(Path::new("qml"), &mut qml);
    qml.sort();
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=cxx/pages.cpp");
    println!("cargo:rerun-if-changed=src/native_pages.rs");
    println!("cargo:rerun-if-changed=src/collection_model.rs");
    CxxQtBuilder::library(Interface::default())
        .cc_builder(|cc| {
            cc.file("cxx/pages.cpp");
        })
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qml_module(QmlModule {
            uri: "app.chilpa.pages",
            rust_files: &["src/native_pages.rs", "src/collection_model.rs"],
            qml_files: &qml,
            ..Default::default()
        })
        .build();
}
