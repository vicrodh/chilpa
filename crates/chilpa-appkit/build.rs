//! The Dock's reopen event is an Apple event that only Objective-C can
//! register for; the file is compiled when the target is a Mac.
fn main() {
    println!("cargo:rerun-if-changed=cxx/macos_reopen.mm");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    cc::Build::new().file("cxx/macos_reopen.mm").flag("-fobjc-arc").compile("chilpa_macos_reopen");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Carbon");
}
