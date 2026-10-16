//! I keep file capture out of normal device pickers while retaining the development route.
#![cfg(target_os = "linux")]

#[test]
fn file_output_is_listed_only_with_the_development_flag() {
    let profile = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", profile.path().join("config"));
    std::env::set_var("XDG_DATA_HOME", profile.path().join("data"));
    std::env::set_var("XDG_CACHE_HOME", profile.path().join("cache"));
    std::env::remove_var("CHILPA_DEV");
    let app = chilpa_api::Chilpa::new();
    assert!(!app.stacks().iter().any(|stack| stack.key == "file"));
    assert!(app.stack_outputs("file").is_empty());
    assert!(!app.outputs().iter().any(|output| output.id.starts_with("file:")));
    std::env::set_var("CHILPA_DEV", "1");
    assert!(app.stacks().iter().any(|stack| stack.key == "file"));
    assert!(app.stack_outputs("file").iter().any(|output| output.id == "file:capture"));
    assert!(app.outputs().iter().any(|output| output.id == "file:capture"));
    std::env::remove_var("CHILPA_DEV");
    assert!(!app.stacks().iter().any(|stack| stack.key == "file"));
}
