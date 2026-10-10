//! The graphics at startup as its own QML module (`app.chilpa.graphics`):
//! the bridge that reports what draws the window, the probe that watches it
//! present, and the C++ that lists Qt's Vulkan devices and opens the probe
//! window of the startup children (ported from QBZ's
//! `cxx/qt_vulkan_probe.cpp`).
use cxx_qt_build::{CxxQtBuilder, Interface, QmlModule};
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// One answer of qmake, the same tool cxx-qt-build asks.
fn qmake(variable: &str) -> Option<String> {
    let candidates = std::env::var("QMAKE").ok().into_iter().chain(["qmake6".to_string(), "qmake".to_string()]);
    for qmake in candidates {
        if let Ok(out) = Command::new(&qmake).args(["-query", variable]).output() {
            let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Qt's private headers, which `QVulkanDefaultInstance` lives in: the version
/// root (private headers include their siblings as `<QtGui/private/..>`) and
/// the module's own folder under it. A machine without them builds the
/// probe without the device list, which then lists none and leaves Auto.
fn private_includes() -> Vec<String> {
    println!("cargo:rerun-if-env-changed=QMAKE");
    let (Some(headers), Some(version)) = (qmake("QT_INSTALL_HEADERS"), qmake("QT_VERSION")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for module in ["QtCore", "QtGui", "QtQml", "QtQuick"] {
        let root = format!("{headers}/{module}/{version}");
        if Path::new(&root).is_dir() {
            out.push(root.clone());
        }
        let own = format!("{root}/{module}");
        if Path::new(&own).is_dir() {
            out.push(own);
        }
    }
    out
}

fn main() {
    let qml = files("qml", &|p| p.extension().is_some_and(|e| e == "qml"));
    let bridges = files("src/bridge", &|p| p.extension().is_some_and(|e| e == "rs"));
    let resources = files("qml", &|p| p.file_name().is_some_and(|n| n == "qmldir"));
    let includes = private_includes();
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=src/bridge");
    println!("cargo:rerun-if-changed=cxx/probe.cpp");
    CxxQtBuilder::library(Interface::default())
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qml_module(QmlModule {
            uri: "app.chilpa.graphics",
            rust_files: &bridges,
            qml_files: &qml,
            qrc_files: &resources,
            ..Default::default()
        })
        .cc_builder(move |cc| {
            for include in &includes {
                cc.include(include);
            }
            cc.file("cxx/probe.cpp");
        })
        .build();
}
