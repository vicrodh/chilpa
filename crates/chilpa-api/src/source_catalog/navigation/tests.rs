use super::*;
use crate::navigation::Tabs;

fn record(screen: Screen, id: i64) -> Route {
    Route { screen, id, key: String::new() }
}

fn overflow(catalog: &Catalog, count: u64) {
    let mut inner = catalog.lock();
    let serial = inner.store.cards.keys().next().copied().unwrap_or_default();
    for number in 0..count {
        let id = inner.store.card_id("extra", &format!("album:{serial}:{number}"));
        inner.store.cards.get_mut(&id).unwrap().seen_at = number + 1;
    }
    let (tracks, cards) = inner.protection();
    inner.store.prune(&tracks, &cards);
}

#[test]
fn a_new_route_retains_its_record_before_tab_persistence() {
    let api = crate::Chilpa::new();
    let menu = api.catalog.hold([("guest".into(), "album:pending".into())], []);
    let id = api.catalog.lock().store.card_id("guest", "album:pending");
    let mut tabs = Tabs::default();
    tabs.go(record(Screen::Album, id));
    api.protect_tabs(&tabs);
    drop(menu);
    overflow(&api.catalog, 6001);
    assert!(api.catalog.card(id).is_some(), "I retain the destination before its view or persisted tabs exist");
    api.protect_tabs(&Tabs::default());
    overflow(&api.catalog, 1);
    assert!(api.catalog.card(id).is_none(), "I release the destination when its live route is replaced");
}

#[test]
fn open_tabs_take_over_menu_roots_before_mount_and_release_only_closed_tabs() {
    let catalog = Catalog::new();
    let menu = catalog.hold([("guest".into(), "album:chosen".into())], []);
    let (album, playlist, track) = {
        let mut inner = catalog.lock();
        let album = inner.store.card_id("guest", "album:chosen");
        let playlist = inner.store.card_id("guest", "playlist:chosen");
        let track = inner.store.take_item_track("guest", &serde_json::json!({"kind":"track","ref":"track:chosen","title":"Song"}), None, 1).unwrap();
        inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![track]);
        (album, playlist, track)
    };
    let mut tabs = Tabs::default();
    tabs.go(record(Screen::Album, album));
    tabs.open(record(Screen::Playlist, playlist));
    catalog.protect_navigation(&tabs.routes());
    drop(menu);
    assert!(catalog.lock().selections.is_empty(), "the menu can close before the album page mounts");
    overflow(&catalog, 6001);
    {
        let inner = catalog.lock();
        assert!(inner.store.cards.contains_key(&album), "an inactive tab retains its record independently of the active tab");
        assert!(inner.store.cards.contains_key(&playlist));
        assert!(inner.store.tracks.contains_key(&track), "the open album retains its track metadata");
    }
    tabs.close(0);
    catalog.protect_navigation(&tabs.routes());
    overflow(&catalog, 1);
    {
        let inner = catalog.lock();
        assert!(!inner.store.cards.contains_key(&album), "closing one tab releases that record");
        assert!(!inner.store.tracks.contains_key(&track));
        assert!(inner.store.cards.contains_key(&playlist), "closing another tab keeps the remaining record alive");
    }
    tabs.go(Route::of(Screen::Home));
    catalog.protect_navigation(&tabs.routes());
    overflow(&catalog, 1);
    assert!(!catalog.lock().store.cards.contains_key(&playlist), "history alone does not keep the old page alive");
}

#[test]
fn restored_native_and_card_routes_protect_cards_that_arrive_after_navigation() {
    let catalog = Catalog::new();
    let tabs = Tabs::restore("0\nsource\t0\tguest native:album:future\nsource\t0\tguest card:-1");
    catalog.protect_navigation(&tabs.routes());
    {
        let inner = catalog.lock();
        assert!(inner.store.cards.is_empty());
        assert!(inner.navigation.cards.contains(&-1));
        assert!(inner.navigation.identities.contains(&("guest".into(), "album:future".into())));
    }
    let (card_route, native_route, unrelated) = {
        let mut inner = catalog.lock();
        (inner.store.card_id("guest", "album:opaque"), inner.store.card_id("guest", "album:future"), inner.store.card_id("another", "album:future"))
    };
    assert_eq!(card_route, -1);
    overflow(&catalog, 6001);
    let inner = catalog.lock();
    assert!(inner.store.cards.contains_key(&card_route));
    assert!(inner.store.cards.contains_key(&native_route));
    assert!(!inner.store.cards.contains_key(&unrelated), "opaque identities remain scoped to their plugin");
}

#[test]
fn only_supported_remote_routes_become_catalog_roots() {
    let catalog = Catalog::new();
    catalog.protect_navigation(&[
        record(Screen::Album, 8), record(Screen::Playlist, 9), record(Screen::Home, -4),
        Route { screen: Screen::Source, id: 0, key: "guest native:".into() },
        Route { screen: Screen::Source, id: 0, key: "guest card:8".into() },
        Route { screen: Screen::Source, id: 0, key: "guest card:invalid".into() },
    ]);
    let inner = catalog.lock();
    assert!(inner.navigation.cards.is_empty());
    assert!(inner.navigation.identities.is_empty());
}

#[test]
fn disabling_tab_restore_still_protects_current_navigation_and_closing_releases_it() {
    const CHILD: &str = "CHILPA_TEST_NAVIGATION_PROTECTION";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "source_catalog::navigation::tests::disabling_tab_restore_still_protects_current_navigation_and_closing_releases_it", "--nocapture"])
            .env(CHILD, "1").envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let mut api = crate::Chilpa::new();
    api.remember_preference("navigation.restoreTabs", "false");
    api.remember_preference("ui.tabs", "sentinel");
    let mut tabs = Tabs::default();
    tabs.go(record(Screen::Album, -71));
    api.keep_tabs(&tabs);
    assert!(api.catalog.lock().protection().1.contains(&-71), "live navigation roots do not depend on session persistence");
    assert_eq!(api.preference("ui.tabs").as_deref(), Some("sentinel"), "disabled restoration does not save tabs");
    tabs.open(Route { screen: Screen::Source, id: 0, key: "guest native:album:future".into() });
    api.keep_tabs(&tabs);
    let future = api.catalog.lock().store.card_id("guest", "album:future");
    assert!(api.catalog.lock().protection().1.contains(&future));
    tabs.close(1);
    api.keep_tabs(&tabs);
    assert!(!api.catalog.lock().protection().1.contains(&future));
    assert!(api.catalog.lock().protection().1.contains(&-71));
    tabs.go(Route::of(Screen::Home));
    api.keep_tabs(&tabs);
    assert!(!api.catalog.lock().protection().1.contains(&-71));
}
