use super::*;
use crate::Chilpa;

/// True in the child that runs the test in a profile of its own: the
/// switch is a preference, and another test must not read it turned on.
fn isolated(name: &str) -> bool {
    const CHILD: &str = "CHILPA_LIBRARY_PROTECTION_TEST";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return true;
    }
    let profile = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"]).env(CHILD, name)
        .envs(crate::test_support::profile_environment(profile.path())).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    false
}

fn row(api: &mut Chilpa, key: &str) -> crate::settings_rows::SettingsRow {
    api.settings_rows("library", &crate::settings_rows::RowNotes::default()).0.into_iter().find(|row| row.key == key).expect(key)
}

#[test]
fn refusals_follow_the_switch_and_its_child() {
    let off = Protection { on: false, sidecars: false, ignore: false };
    assert!(off.writes_refused().is_empty() && off.sidecar_refused().is_empty());
    let on = Protection { on: true, sidecars: false, ignore: false };
    assert!(!on.writes_refused().is_empty());
    assert_eq!(on.sidecar_refused(), on.writes_refused());
    let sidecars = Protection { on: true, sidecars: true, ignore: false };
    assert!(!sidecars.writes_refused().is_empty());
    assert!(sidecars.sidecar_refused().is_empty());
    let child_alone = Protection { on: false, sidecars: true, ignore: false };
    assert!(child_alone.writes_refused().is_empty() && child_alone.sidecar_refused().is_empty());
}

#[test]
fn both_start_off_and_the_child_waits_for_its_parent() {
    if !isolated("library_protection::tests::both_start_off_and_the_child_waits_for_its_parent") { return; }
    let mut api = Chilpa::new();
    assert_eq!(row(&mut api, ENABLED).value, "false");
    assert_eq!(row(&mut api, SIDECARS).value, "false");
    assert_eq!(row(&mut api, SIDECARS).control, "fixed", "nothing to allow while the parent is off");
    assert!(!api.library_protected());
    assert!(api.adjust(ENABLED, "true"));
    assert!(api.library_protected());
    assert_eq!(row(&mut api, SIDECARS).control, "toggle");
    assert!(api.adjust(SIDECARS, "true"));
    assert!(api.protected_sidecars());
    assert_eq!(Protection::kept(), Protection { on: true, sidecars: true, ignore: false });
    assert!(!api.adjust(ENABLED, "maybe"));
    assert!(api.adjust(ENABLED, "false"));
    assert!(Protection::kept().writes_refused().is_empty());
}

#[test]
fn sidecars_are_ignored_only_while_they_are_not_allowed() {
    assert!(Protection { on: true, sidecars: false, ignore: true }.ignores_sidecars());
    assert!(!Protection { on: true, sidecars: true, ignore: true }.ignores_sidecars(), "allowed, they are read");
    assert!(!Protection { on: false, sidecars: false, ignore: true }.ignores_sidecars(), "unprotected, they are read");
    assert!(!Protection { on: true, sidecars: false, ignore: false }.ignores_sidecars());
}

#[test]
fn the_ignore_switch_shows_with_sidecars_refused_and_tells_the_library() {
    if !isolated("library_protection::tests::the_ignore_switch_shows_with_sidecars_refused_and_tells_the_library") { return; }
    let mut api = Chilpa::new();
    let shown = |api: &mut Chilpa| api.settings_rows("library", &crate::settings_rows::RowNotes::default()).0.iter().any(|row| row.key == IGNORE);
    assert!(!shown(&mut api), "nothing to ignore while sidecars are read");
    assert!(api.adjust(ENABLED, "true"));
    assert!(shown(&mut api), "protected without sidecars, they can be left unread");
    assert_eq!(row(&mut api, IGNORE).value, "false");
    assert!(!chilpa_library::sidecar::ignored());
    assert!(api.adjust(IGNORE, "true"));
    assert!(chilpa_library::sidecar::ignored(), "the library leaves them unread");
    assert!(api.adjust(SIDECARS, "true"));
    assert!(!shown(&mut api));
    assert!(!chilpa_library::sidecar::ignored(), "allowed again, they are read again");
    assert!(api.adjust(SIDECARS, "false"));
    assert!(chilpa_library::sidecar::ignored(), "the switch kept its place");
}

#[test]
fn a_protected_library_refuses_the_folder_batch_even_with_sidecars() {
    if !isolated("library_protection::tests::a_protected_library_refuses_the_folder_batch_even_with_sidecars") { return; }
    let mut api = Chilpa::new();
    assert!(api.adjust(ENABLED, "true"));
    assert!(api.adjust(SIDECARS, "true"));
    assert!(!api.metadata_batch_save());
    assert_eq!(api.metadata_batch_view()["error"], Protection::kept().writes_refused());
    assert_eq!(api.metadata_batch_view()["protected"], true);
}

#[test]
fn a_protected_library_deletes_nothing_from_a_server() {
    if !isolated("library_protection::tests::a_protected_library_deletes_nothing_from_a_server") { return; }
    let mut api = Chilpa::new();
    assert!(api.adjust(ENABLED, "true"));
    let said: serde_json::Value = serde_json::from_str(&api.server_media_delete("track", 1, "fixture", 1)).unwrap();
    assert_eq!(said["error"], Protection::kept().writes_refused());
}
