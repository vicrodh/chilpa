//! The About window as its own QML module (`app.chilpa.about`): the bridge
//! that hands it the facade's document, and the window itself. The build
//! stamps its date and commit for the Build row. Each shell crate stays
//! inside the shell's budgets (ADR-001).
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

/// The build's date and short commit, for the About window's Build row:
/// the date from `SOURCE_DATE_EPOCH` when a reproducible build sets it, else
/// the clock; the commit from git, empty in a tree without one. Neither can
/// fail the build. It runs again when HEAD or the branch it names moves.
fn stamp() {
    let git = |args: &[&str]| {
        std::process::Command::new("git").args(args).output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(git(&["rev-parse", "--git-path", &branch]));
    }
    // A path that does not exist would make every build run this again.
    for path in watched.into_iter().flatten().filter(|p| Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let epoch = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.trim().parse::<i64>().ok())
        .or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64))
        .unwrap_or(0);
    println!("cargo:rustc-env=CHILPA_BUILD_DATE={}", day_of(epoch));
    println!("cargo:rustc-env=CHILPA_BUILD_COMMIT={}", git(&["rev-parse", "--short=9", "HEAD"]).unwrap_or_default());
}

/// Seconds since 1970 as `YYYY-MM-DD` in UTC (Howard Hinnant's
/// days-to-civil), empty for none.
fn day_of(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }
    let z = epoch.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn main() {
    stamp();
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
            uri: "app.chilpa.about",
            rust_files: &bridges,
            qml_files: &qml,
            qrc_files: &resources,
            ..Default::default()
        })
        .build();
}
