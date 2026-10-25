fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let library = pkg_config::Config::new()
        .range_version("1.0"..)
        .probe("libpipewire-0.3")
        .expect("PipeWire development headers >= 1.0 are required on Linux");
    let mut build = cc::Build::new();
    build
        .std("gnu11")
        .warnings(true)
        .flag_if_supported("-fno-strict-aliasing");
    for include in library.include_paths {
        build.include(include);
    }
    for (key, value) in library.defines {
        build.define(&key, value.as_deref());
    }
    for file in [
        "native/dsd_format.c",
        "native/dsd_session.c",
        "native/dsd_stream.c",
        "native/dsd_test.c",
    ] {
        println!("cargo:rerun-if-changed={file}");
        build.file(file);
    }
    println!("cargo:rerun-if-changed=native/dsd.h");
    build.compile("chilpa_pipewire_dsd");
}
