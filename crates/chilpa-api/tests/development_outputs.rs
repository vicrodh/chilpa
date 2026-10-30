//! I keep file capture out of normal device pickers while retaining the development route.
#![cfg(target_os = "linux")]

#[test]
fn file_output_is_listed_only_with_the_development_flag() {
    let profile = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", profile.path().join("config"));
    std::env::set_var("XDG_DATA_HOME", profile.path().join("data"));
    std::env::set_var("XDG_CACHE_HOME", profile.path().join("cache"));
    std::env::remove_var("CHILPA_DEV");
    // Listed as the core would list a machine with a card and the file
    // stack, without asking this machine's sound servers: the flag is read
    // when a list is read, and that is what is checked here.
    use chilpa_core::backends::{Fidelity, Stack, StackKind};
    let stack = |kind| Stack { kind, present: true, missing: String::new(), provider: String::new(), fidelity: Fidelity::Exact, caveat: String::new() };
    let output = |id: &str| chilpa_core::Output { id: id.into(), label: String::new(), detail: String::new(), bit_perfect: true };
    let app = chilpa_api::Chilpa::with_listed(
        vec![stack(StackKind::Alsa), stack(StackKind::File)],
        vec![output("alsa:hw:1,0"), output("file:capture")],
    );
    // The stand-in answers on the listing's thread, so this waits once.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !app.outputs_listed() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(app.outputs_listed(), "the stand-in did not answer within ten seconds");
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
