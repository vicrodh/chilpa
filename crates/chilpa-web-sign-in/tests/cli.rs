//! The executable's arguments and exit status, without opening a window.

use std::process::Command;

fn window(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_chilpa-web-sign-in")).args(arguments).output().expect("the executable starts")
}

#[test]
fn arguments_it_does_not_know_print_the_usage() {
    for arguments in [
        &["--url", "https://login.example/a", "--until", "https://example/r", "--other", "x"][..],
        &["--url", "https://login.example/a", "--until"],
        &["--url", "http://login.example/a", "--until", "https://example/r"],
        &[],
    ] {
        let output = window(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("usage: chilpa-web-sign-in "), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
    }
}

#[cfg(not(feature = "webengine"))]
#[test]
fn a_build_without_the_engine_says_so() {
    let output = window(&["--url", "https://login.example/a", "--until", "https://example/r", "--title", "Source"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
}
