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
    let off = Protection { on: false, sidecars: false };
    assert!(off.writes_refused().is_empty() && off.sidecar_refused().is_empty());
    let on = Protection { on: true, sidecars: false };
    assert!(!on.writes_refused().is_empty());
    assert_eq!(on.sidecar_refused(), on.writes_refused());
    let sidecars = Protection { on: true, sidecars: true };
    assert!(!sidecars.writes_refused().is_empty());
    assert!(sidecars.sidecar_refused().is_empty());
    let child_alone = Protection { on: false, sidecars: true };
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
    assert_eq!(Protection::kept(), Protection { on: true, sidecars: true });
    assert!(!api.adjust(ENABLED, "maybe"));
    assert!(api.adjust(ENABLED, "false"));
    assert!(Protection::kept().writes_refused().is_empty());
}
