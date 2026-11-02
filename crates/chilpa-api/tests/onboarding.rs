//! The Welcome set aside for a card's button, against real settings.
//!
//! One test on purpose: the onboarding state and the settings directory
//! belong to the whole process.

use chilpa_api::Chilpa;

fn doc(chilpa: &Chilpa) -> serde_json::Value {
    let (_, text) = chilpa.onboarding_doc(0).expect("a document");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_welcome_set_aside_is_remembered_until_it_closes() {
    let dir = std::env::temp_dir().join("chilpa-api-onboarding");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let mut chilpa = Chilpa::new();
    chilpa.onboarding_start();
    assert_eq!(doc(&chilpa)["open"], true, "a new install is welcomed");
    assert_eq!(doc(&chilpa)["kind"], "welcome");

    chilpa.onboarding_command("aside:2");
    let aside = doc(&chilpa);
    assert_eq!((aside["open"].clone(), aside["aside"].clone(), aside["reached"].clone()), (false.into(), true.into(), 2.into()));
    assert_eq!(chilpa.preference("onboarding.welcomeAside").as_deref(), Some("2"));
    assert_eq!(chilpa.preference("onboarding.seenVersion"), None, "set aside is not seen");

    chilpa.onboarding_command("whatsNew");
    chilpa.onboarding_command("close");
    let after_news = doc(&chilpa);
    assert_eq!((after_news["aside"].clone(), after_news["reached"].clone()), (true.into(), 2.into()), "What's new leaves the Welcome aside");
    assert_eq!(after_news["welcomeCards"], 5);
    assert_eq!(chilpa.preference("onboarding.welcomeAside").as_deref(), Some("2"));
    chilpa.onboarding_command("whatsNew");
    chilpa.onboarding_command("close");
    chilpa.onboarding_command("dismiss");
    assert_eq!(doc(&chilpa)["aside"], false, "the row's X ends it even after What's new");
    assert_eq!(chilpa.preference("onboarding.welcomeAside").as_deref(), Some(""));

    chilpa.onboarding_command("welcome");
    chilpa.onboarding_command("aside:2");
    chilpa.onboarding_command("welcome");
    let resumed = doc(&chilpa);
    assert_eq!((resumed["open"].clone(), resumed["reached"].clone()), (true.into(), 2.into()));

    chilpa.onboarding_command("close");
    let closed = doc(&chilpa);
    assert_eq!((closed["aside"].clone(), closed["reached"].clone()), (false.into(), 0.into()));
    assert_eq!(chilpa.preference("onboarding.welcomeAside").as_deref(), Some(""));
    assert_eq!(chilpa.preference("onboarding.seenVersion").as_deref(), Some(env!("CARGO_PKG_VERSION")));

    chilpa.onboarding_command("aside:3");
    assert_eq!(doc(&chilpa)["aside"], false, "only an open Welcome steps aside");
    assert_eq!(chilpa.preference("onboarding.welcomeAside").as_deref(), Some(""));
}
