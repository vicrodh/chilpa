use super::*;

struct Fixture {
    _temporary: tempfile::TempDir,
    settings: PathBuf,
    images: PathBuf,
    first: PathBuf,
    second: PathBuf,
    identity: String,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first.png");
        let second = temporary.path().join("second.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30])).save(&first).unwrap();
        image::RgbImage::from_pixel(2, 2, image::Rgb([40, 50, 60])).save(&second).unwrap();
        Self { settings:temporary.path().join("settings.conf"), images:temporary.path().join("images"),
            first, second, identity:presentation::token("Heavy Metal"), _temporary:temporary }
    }
    fn choose(&self, role: Role, image: Option<&Path>) -> bool {
        choose(&self.settings, &self.images, &self.identity, role, image).unwrap()
    }
    fn choices(&self) -> Choices { read_choices(&self.settings).unwrap() }
    fn files(&self) -> usize { std::fs::read_dir(&self.images).map(|files| files.count()).unwrap_or(0) }
    fn seed_legacy(&self) -> String {
        let path = keep_image(&self.first, &self.images).unwrap();
        let file = Path::new(&path).file_name().unwrap().to_str().unwrap().to_owned();
        let legacy = BTreeMap::from([(self.identity.clone(), file.clone())]);
        chilpa_core::settings::update_value_at(&self.settings, LEGACY_KEY,
            |_| Ok((serde_json::to_string(&legacy).unwrap(), ()))).unwrap();
        file
    }
}

#[test]
fn card_and_header_choices_are_independent_and_share_strong_aliases() {
    let fixture = Fixture::new();
    assert!(fixture.choose(Role::Card, Some(&fixture.first)));
    assert!(fixture.choices()[&fixture.identity].header.is_none());
    let card = fixture.choices()[&fixture.identity].card.clone().unwrap();
    assert!(fixture.choose(Role::Header, Some(&fixture.second)));
    let header = fixture.choices()[&fixture.identity].header.clone().unwrap();
    assert_ne!(card, header);
    assert_eq!(fixture.files(), 2);
    assert_eq!(fixture.choices()[&fixture.identity].card.as_deref(), Some(card.as_str()));
    let alias = presentation::token("heavy-metal");
    assert_eq!(alias, fixture.identity);
    assert!(choose(&fixture.settings, &fixture.images, &alias, Role::Card, None).unwrap());
    assert!(!fixture.images.join(card).exists());
    assert_eq!(fixture.choices()[&fixture.identity].header.as_deref(), Some(header.as_str()));
    assert_eq!(fixture.files(), 1);
    assert!(fixture.choose(Role::Header, None));
    assert!(fixture.choices().is_empty());
    assert_eq!(fixture.files(), 0);
}

#[test]
fn legacy_choices_migrate_both_roles_atomically_and_resets_never_revive_them() {
    let fixture = Fixture::new();
    let legacy_file = fixture.seed_legacy();
    let another = presentation::token("Personal-Shelf");
    chilpa_core::settings::update_value_at(&fixture.settings, LEGACY_KEY, |text| {
        let mut map: BTreeMap<String, String> = serde_json::from_str(text.unwrap()).unwrap();
        map.insert(another.clone(), legacy_file.clone());
        Ok((serde_json::to_string(&map).unwrap(), ()))
    }).unwrap();
    let stale = chilpa_core::settings::load_from(&fixture.settings);
    assert_eq!(fixture.choices()[&fixture.identity].header.as_deref(), Some(legacy_file.as_str()));
    assert!(fixture.choose(Role::Header, Some(&fixture.second)));
    let migrated = fixture.choices();
    assert_eq!(migrated[&fixture.identity].card.as_deref(), Some(legacy_file.as_str()));
    assert_ne!(migrated[&fixture.identity].header.as_deref(), Some(legacy_file.as_str()));
    assert_eq!(migrated[&another].card, migrated[&another].header);
    assert_eq!(migrated[&another].card.as_deref(), Some(legacy_file.as_str()));
    assert!(fixture.choose(Role::Card, None));
    assert!(fixture.images.join(&legacy_file).exists(), "another genre still owns the file");
    assert!(fixture.choose(Role::Header, None));
    for role in [Role::Card, Role::Header] {
        assert!(choose(&fixture.settings, &fixture.images, &another, role, None).unwrap());
    }
    assert_eq!(fixture.files(), 0);
    chilpa_core::settings::save_to(&fixture.settings, &stale).unwrap();
    assert!(fixture.choices().is_empty());
    assert!(chilpa_core::settings::read_value_from(&fixture.settings, LEGACY_KEY).unwrap().is_some());
    assert_eq!(chilpa_core::settings::read_value_from(&fixture.settings, SETTINGS_KEY).unwrap().as_deref(), Some("{}"));
}

#[test]
fn identical_content_is_deduplicated_until_the_last_genre_and_role_reference() {
    let fixture = Fixture::new();
    assert!(fixture.choose(Role::Card, Some(&fixture.first)));
    assert!(fixture.choose(Role::Header, Some(&fixture.first)));
    let other = presentation::token("Jazz");
    assert!(choose(&fixture.settings, &fixture.images, &other, Role::Header, Some(&fixture.first)).unwrap());
    assert_eq!(fixture.files(), 1);
    let saved = std::fs::read(&fixture.settings).unwrap();
    assert!(!fixture.choose(Role::Header, Some(&fixture.first)));
    assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
    assert!(fixture.choose(Role::Card, None));
    assert!(fixture.choose(Role::Header, None));
    assert_eq!(fixture.files(), 1);
    assert!(choose(&fixture.settings, &fixture.images, &other, Role::Header, None).unwrap());
    assert_eq!(fixture.files(), 0);
    let saved = std::fs::read(&fixture.settings).unwrap();
    assert!(!fixture.choose(Role::Header, None));
    assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
}

#[test]
fn empty_resets_and_unreadable_or_invalid_images_leave_preferences_and_files_alone() {
    let fixture = Fixture::new();
    assert!(!fixture.choose(Role::Header, None));
    assert!(!fixture.settings.exists());
    assert!(fixture.choose(Role::Card, Some(&fixture.first)));
    let saved = std::fs::read(&fixture.settings).unwrap();
    let malformed = fixture.first.with_file_name("invalid.png");
    std::fs::write(&malformed, b"not an image").unwrap();
    for image in [&malformed, &fixture.first.with_file_name("missing.png")] {
        assert!(choose(&fixture.settings, &fixture.images, &fixture.identity, Role::Header, Some(image)).is_err());
        assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
        assert_eq!(fixture.files(), 1);
    }
    let large = fixture.first.with_file_name("large.png");
    std::fs::File::create(&large).unwrap().set_len(MAX_IMAGE + 1).unwrap();
    assert!(choose(&fixture.settings, &fixture.images, &fixture.identity, Role::Header, Some(&large)).is_err());
    assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
    assert_eq!(fixture.files(), 1);
}

#[test]
fn a_current_role_edit_survives_a_choice_staged_from_older_preferences() {
    let fixture = Fixture::new();
    assert!(fixture.choose(Role::Card, Some(&fixture.first)));
    let first = fixture.choices()[&fixture.identity].card.clone();
    let other = presentation::token("Custom Shelf");
    assert!(choose_using(&fixture.settings, &fixture.images, &fixture.identity, Role::Header, Some(&fixture.second), |picked, into| {
        let image = keep_image(picked, into)?;
        chilpa_core::settings::update_value_at(&fixture.settings, SETTINGS_KEY, |text| {
            let mut choices = decode(text)?;
            choices.insert(other.clone(), Roles { card:first.clone(), header:None });
            choices.get_mut(&fixture.identity).unwrap().card = None;
            choices.get_mut(&fixture.identity).unwrap().header = first.clone();
            Ok((serde_json::to_string(&choices).unwrap(), ()))
        })?;
        Ok(image)
    }).unwrap());
    let choices = fixture.choices();
    assert!(choices[&fixture.identity].card.is_none());
    assert_eq!(choices[&other].card, first);
    assert_ne!(choices[&fixture.identity].header, first);
    assert_eq!(fixture.files(), 2);
}

#[test]
fn a_failed_commit_removes_only_new_unreferenced_content() {
    let fixture = Fixture::new();
    assert!(fixture.choose(Role::Card, Some(&fixture.first)));
    let first = fixture.choices()[&fixture.identity].card.clone().unwrap();
    let new_identity = presentation::token("New genre");
    let result = choose_using(&fixture.settings, &fixture.images, &new_identity, Role::Header, Some(&fixture.second), |picked, into| {
        let image = keep_image(picked, into)?;
        let choices: Choices = (0..MAX_CHOICES).map(|at|
            (format!("{at:064x}"), Roles { card:Some(first.clone()), header:Some(first.clone()) })).collect();
        chilpa_core::settings::update_value_at(&fixture.settings, SETTINGS_KEY,
            |_| Ok((serde_json::to_string(&choices).unwrap(), ())))?;
        Ok(image)
    });
    assert!(result.is_err());
    assert_eq!(fixture.choices().len(), MAX_CHOICES);
    assert_eq!(fixture.files(), 1);
    assert!(fixture.images.join(first).is_file());
}

#[test]
fn two_roles_fit_at_the_genre_limit_and_damaged_v2_never_falls_back_to_legacy() {
    let fixture = Fixture::new();
    let file = fixture.seed_legacy();
    let choices: Choices = (0..MAX_CHOICES).map(|at|
        (format!("{at:064x}"), Roles { card:Some(file.clone()), header:None })).collect();
    let encoded = serde_json::to_string(&choices).unwrap();
    chilpa_core::settings::update_value_at(&fixture.settings, SETTINGS_KEY, |_| Ok((encoded, ()))).unwrap();
    let identity = format!("{:064x}", 0);
    assert!(choose(&fixture.settings, &fixture.images, &identity, Role::Header, Some(&fixture.second)).unwrap());
    assert_eq!(fixture.choices().len(), MAX_CHOICES);
    let saved = std::fs::read(&fixture.settings).unwrap();
    assert!(choose(&fixture.settings, &fixture.images, &fixture.identity, Role::Header, Some(&fixture.second)).is_err());
    assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
    for invalid in ["{broken".to_string(), format!("{{\"{}\":{{\"third\":\"{}\"}}}}", identity, file),
        format!("{{\"{}\":{{\"header\":\"../escape.png\"}}}}", identity), " ".repeat(MAX_JSON + 1)] {
        chilpa_core::settings::update_value_at(&fixture.settings, SETTINGS_KEY, |_| Ok((invalid.clone(), ()))).unwrap();
        assert!(read_choices(&fixture.settings).is_err());
        let saved = std::fs::read(&fixture.settings).unwrap();
        assert!(choose(&fixture.settings, &fixture.images, &identity, Role::Card, None).is_err());
        assert_eq!(std::fs::read(&fixture.settings).unwrap(), saved);
        assert_eq!(fixture.files(), 2);
    }
}

#[test]
fn choosing_the_legacy_image_again_migrates_without_a_visible_change() {
    let fixture = Fixture::new();
    let file = fixture.seed_legacy();
    assert!(!fixture.choose(Role::Card, Some(&fixture.first)));
    assert!(chilpa_core::settings::read_value_from(&fixture.settings, SETTINGS_KEY).unwrap().is_some());
    let roles = &fixture.choices()[&fixture.identity];
    assert_eq!(roles.card.as_deref(), Some(file.as_str()));
    assert_eq!(roles.header.as_deref(), Some(file.as_str()));
    assert_eq!(fixture.files(), 1);
    assert!(fixture.choose(Role::Header, None));
    assert!(fixture.images.join(file).exists());
    assert!(fixture.choices()[&fixture.identity].header.is_none());
}

#[test]
fn concurrent_role_choices_merge_into_one_genre() {
    let fixture = Fixture::new();
    let start = std::sync::Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| {
        let card_start = start.clone();
        let borrowed = &fixture;
        let card = scope.spawn(move || {
            card_start.wait();
            borrowed.choose(Role::Card, Some(&borrowed.first))
        });
        start.wait();
        assert!(fixture.choose(Role::Header, Some(&fixture.second)));
        assert!(card.join().unwrap());
    });
    let choices = fixture.choices();
    assert_eq!(choices.len(), 1);
    assert_ne!(choices[&fixture.identity].card, choices[&fixture.identity].header);
    assert_eq!(fixture.files(), 2);
}

#[test]
fn malformed_legacy_preferences_are_reported_before_any_image_is_copied() {
    let fixture = Fixture::new();
    for invalid in ["{broken".to_string(), format!("{{\"{}\":\"../escape.png\"}}", fixture.identity)] {
        chilpa_core::settings::update_value_at(&fixture.settings, LEGACY_KEY, |_| Ok((invalid, ()))).unwrap();
        let before = std::fs::read(&fixture.settings).unwrap();
        assert!(choose(&fixture.settings, &fixture.images, &fixture.identity, Role::Header, Some(&fixture.first)).is_err());
        assert_eq!(std::fs::read(&fixture.settings).unwrap(), before);
        assert_eq!(fixture.files(), 0);
        assert!(chilpa_core::settings::read_value_from(&fixture.settings, SETTINGS_KEY).unwrap().is_none());
    }
}
