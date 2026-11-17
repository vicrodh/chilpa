//! The sidebar layout through the facade, in a disposable profile.
use std::process::Command;

fn row(api: &mut chilpa_api::Chilpa) -> chilpa_api::SettingsRow {
    api.settings_rows("navigation", &chilpa_api::RowNotes::default())
        .0.into_iter().find(|row| row.key == "navigation.sidebar").expect("the sidebar row")
}

const DEFAULT: &str = "home,library,artists,albums,tracks,genres,composers,compositions,folders,openMedia,deviceSync";

#[test]
fn the_layout_starts_whole_and_keeps_a_new_order_across_runs() {
    isolated("the_layout_starts_whole_and_keeps_a_new_order_across_runs", || {
        let mut api = chilpa_api::Chilpa::new();
        let first = row(&mut api);
        assert_eq!(first.control, "entries");
        assert_eq!(first.value, DEFAULT);
        let moved = "albums,home,-library,artists,tracks,genres,composers,compositions,folders,openMedia,deviceSync";
        assert!(api.adjust("navigation.sidebar", moved));
        drop(api);
        let mut reopened = chilpa_api::Chilpa::new();
        assert_eq!(row(&mut reopened).value, moved);
        assert_eq!(reopened.sidebar_layout(), moved);
    });
}

#[test]
fn a_layout_with_nothing_shown_or_not_whole_is_refused() {
    isolated("a_layout_with_nothing_shown_or_not_whole_is_refused", || {
        let mut api = chilpa_api::Chilpa::new();
        let none = DEFAULT.split(',').map(|id| format!("-{id}")).collect::<Vec<_>>().join(",");
        assert!(!api.adjust("navigation.sidebar", &none), "at least one stays on");
        assert!(!api.adjust("navigation.sidebar", "home,library"), "every entry is sent");
        assert!(!api.adjust("navigation.sidebar", &format!("{DEFAULT},nowhere")), "no unknown entry");
        assert_eq!(row(&mut api).value, DEFAULT);
    });
}

#[test]
fn devices_turned_off_before_the_list_stays_off() {
    isolated("devices_turned_off_before_the_list_stays_off", || {
        let mut api = chilpa_api::Chilpa::new();
        // The old switch, as an earlier version kept it.
        api.remember_preference("navigation.devices", "false");
        assert_eq!(row(&mut api).value, DEFAULT.replace("deviceSync", "-deviceSync"));
        assert!(api.settings_rows("navigation", &chilpa_api::RowNotes::default()).0.iter().all(|row| row.key != "navigation.devices"),
            "the old switch has no row of its own");
    });
}

#[test]
fn with_home_hidden_and_no_tabs_kept_chilpa_opens_on_the_first_entry_shown() {
    isolated("with_home_hidden_and_no_tabs_kept_chilpa_opens_on_the_first_entry_shown", || {
        use chilpa_api::navigation::Screen;
        let mut api = chilpa_api::Chilpa::new();
        assert!(api.restored_tabs().is_none(), "Home shown: nothing changes");
        api.remember_preference("navigation.restoreTabs", "false");
        assert!(api.adjust("navigation.sidebar", &DEFAULT.replace("home,library", "-home,library")));
        let tabs = api.restored_tabs().expect("a tab on the first entry shown");
        assert_eq!(tabs.routes().len(), 1);
        assert_eq!(tabs.current().screen, Screen::Library);
    });
}

fn isolated(name: &str, run: impl FnOnce()) {
    if std::env::var("CHILPA_SIDEBAR_TEST").as_deref() == Ok(name) {
        run();
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("CHILPA_SIDEBAR_TEST", name)
        .envs(chilpa_dirs::profile_environment(profile.path()))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}
