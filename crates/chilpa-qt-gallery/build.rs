//! The Gallery, as its own QML module (`com.blitzfc.chilpa.gallery`): the
//! second window that shows every piece of the interface in its states. It
//! is for whoever builds Chilpa, so it does not weigh on the window's
//! crate (ADR-001).
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
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=src/bridge");
    CxxQtBuilder::library(Interface::default())
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa.gallery",
            rust_files: &bridges,
            qml_files: &qml,
            ..Default::default()
        })
        .build();
}
