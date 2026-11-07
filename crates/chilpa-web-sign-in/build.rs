//! Compiles the window (`cxx/window.cpp`) against Qt WebEngine when the
//! `webengine` feature asks for it; without the feature nothing is built and
//! the executable answers that it has no web engine.

fn main() {
    println!("cargo:rerun-if-changed=cxx/window.cpp");
    #[cfg(feature = "webengine")]
    window();
}

#[cfg(feature = "webengine")]
fn window() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    // 6.9 is the first release with QWebEngineProfileBuilder, which gives the
    // profile its folders before it writes anything.
    let qt = pkg_config::Config::new()
        .atleast_version("6.9")
        .cargo_metadata(false)
        .probe("Qt6WebEngineWidgets")
        .unwrap_or_else(|error| panic!("the webengine feature needs Qt6WebEngineWidgets 6.9 or later: {error}"));
    let mut cc = cc::Build::new();
    cc.cpp(true).std("c++17").pic(true).file("cxx/window.cpp");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        cc.flag("/Zc:__cplusplus").flag("/permissive-").flag("/utf-8");
    }
    for path in &qt.include_paths {
        cc.include(path);
    }
    for (name, value) in &qt.defines {
        cc.define(name, value.as_deref());
    }
    cc.compile("chilpa_sign_in_window");
    // Qt goes after the window's archive: a linker that drops a library no
    // earlier object needs would drop Qt if it came first.
    for path in &qt.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for library in &qt.libs {
        println!("cargo:rustc-link-lib={library}");
    }
}
