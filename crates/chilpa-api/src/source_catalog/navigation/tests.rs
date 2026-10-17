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

#[test]
fn restored_tabs_protect_the_disk_catalog_before_workers_or_views_start() {
    const CHILD: &str = "CHILPA_TEST_STARTUP_NAVIGATION_ROOTS";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "source_catalog::navigation::tests::restored_tabs_protect_the_disk_catalog_before_workers_or_views_start", "--nocapture"])
            .env(CHILD, "1").envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let mut seed = crate::Chilpa::new();
    let (album, native, unrelated, bytes) = {
        let mut inner = seed.catalog.lock();
        let album = inner.store.card_id("guest", "album:restored");
        let native = inner.store.card_id("guest", "album:native");
        let unrelated = inner.store.card_id("guest", "album:unrelated");
        inner.store.shelves.insert("guest native:album:native".into(), super::super::store::Shelf { cards:vec![native], ..Default::default() });
        for number in 0..6001 {
            let id = inner.store.card_id("extra", &format!("album:{number}"));
            inner.store.cards.get_mut(&id).unwrap().seen_at = number + 1;
        }
        (album, native, unrelated, serde_json::to_vec(&inner.store).unwrap())
    };
    let mut tabs = Tabs::default();
    tabs.go(record(Screen::Album, album));
    tabs.open(Route { screen: Screen::Source, id:0, key:"guest native:album:native".into() });
    seed.keep_tabs(&tabs);
    let saved_tabs = seed.preference("ui.tabs");
    drop(seed);

    let mut restarted = crate::Chilpa::new();
    {
        let mut inner = restarted.catalog.lock();
        assert!(!inner.loaded, "I install restored roots before a worker can load the catalog");
        assert!(inner.navigation.cards.contains(&album));
        assert!(inner.navigation.identities.contains(&("guest".into(), "album:native".into())));
        inner.store = serde_json::from_slice(&bytes).unwrap();
        inner.store.index();
        let (tracks, cards) = inner.protection();
        let shelves = inner.protected_shelves();
        inner.store.prune_with_shelves(&tracks, &cards, &shelves);
        assert!(inner.store.cards.contains_key(&album), "startup pruning retains the restored numeric route");
        assert!(inner.store.cards.contains_key(&native), "startup pruning retains the restored native identity");
        assert!(!inner.store.cards.contains_key(&unrelated), "an unrooted old card remains evictable");
        assert!(inner.store.shelves.contains_key("guest native:album:native"));
    }
    restarted.remember_preference("navigation.restoreTabs", "false");
    let disabled = crate::Chilpa::new();
    assert!(disabled.catalog.lock().navigation.cards.is_empty());
    assert!(disabled.catalog.lock().navigation.identities.is_empty());
    assert_eq!(disabled.preference("ui.tabs"), saved_tabs, "disabling restore leaves saved tabs intact");
    restarted.remember_preference("navigation.restoreTabs", "true");
    crate::startup_chain::arm();
    crate::startup_chain::arm();
    assert_eq!(crate::startup_chain::level(), 2);
    let bypassed = crate::Chilpa::new();
    assert!(bypassed.catalog.lock().navigation.cards.is_empty());
    assert!(bypassed.catalog.lock().navigation.identities.is_empty());
    assert_eq!(bypassed.preference("ui.tabs"), saved_tabs, "the crash chain bypass leaves saved tabs intact");
}
