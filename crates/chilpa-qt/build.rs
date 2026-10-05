use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::path::{Path, PathBuf};

/// Every file under `dir` that `keep` accepts, relative to the crate and
/// sorted, so the list is found on disk instead of written twice: adding a
/// component is adding its file.
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
    // A layer's `qmldir` declares its singletons, the fonts are the icons
    // and the .qsb files are compiled shaders. All of them travel in the same
    // resource tree as the QML.
    let mut resources = files("qml", &|p| p.file_name().is_some_and(|n| n == "qmldir") || p.extension().is_some_and(|e| e == "js"));
    resources.extend(files("assets", &|p| p.extension().is_some_and(|e| e == "ttf" || e == "qsb" || e == "svg")));
    // Only the files that declare a bridge: `mod.rs` just lists them.
    let bridges = files("src/bridge", &|p| {
        p.extension().is_some_and(|e| e == "rs")
            && std::fs::read_to_string(p)
                .is_ok_and(|s| s.lines().any(|l| l.trim_start().starts_with("#[cxx_qt::bridge]")))
    });
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=src/bridge");

    println!("cargo:rerun-if-changed=src/fonts.cpp");
    println!("cargo:rerun-if-changed=src/covers.cpp");
    println!("cargo:rerun-if-changed=src/modules.cpp");

    CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.chilpa",
            rust_files: &bridges,
            qml_files: &qml,
            qrc_files: &resources,
            ..Default::default()
        })
        .cc_builder(|cc| {
            cc.file("src/fonts.cpp");
            cc.file("src/covers.cpp");
            cc.file("src/modules.cpp");
        })
        .build();
}
