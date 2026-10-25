use super::*;

#[test]
fn saved_expressions_survive_renaming_preferences_and_reopening() {
    let root = tempfile::tempdir().unwrap(); let file = root.path().join("settings.conf");
    let text = r#"(genre:"Jazz, Soul" | title:"OR") -label:"日本語" year:1960..1969"#;
    let saved = mutate_at(&file, |c| c.add("  Evening  ", text)).unwrap();
    assert_eq!(saved.entries[0].name, "Evening");
    assert_eq!(saved.entries[0].expression, text);
    settings::update_value_at(&file, "appearance.theme", |_| Ok(("light".into(), ()))).unwrap();
    let renamed = mutate_at(&file, |c| c.rename(&saved.entries[0].id, "Night")).unwrap();
    assert_eq!(renamed.entries[0].id, saved.entries[0].id);
    assert_eq!(renamed.revision, 2);
    let reopened = Catalog::read(settings::read_value_from(&file, settings::SAVED_QUERIES_KEY).unwrap().as_deref()).unwrap();
    assert_eq!(reopened, renamed);
    assert_eq!(settings::read_value_from(&file, "appearance.theme").unwrap().as_deref(), Some("light"));
    let removed = mutate_at(&file, |c| c.remove(&saved.entries[0].id)).unwrap();
    assert!(removed.entries.is_empty());
    assert_eq!(removed.revision, 3);
}

#[test]
fn invalid_mutations_preserve_bytes_and_revision() {
    let root = tempfile::tempdir().unwrap(); let file = root.path().join("settings.conf");
    mutate_at(&file, |c| c.add("First", "genre:unknown")).unwrap();
    let before = std::fs::read(&file).unwrap();
    for (name, text) in [("First", "genre:jazz"), (" ", "genre:jazz"), ("Next", ""), ("Next", "("),
        ("Next", "rating:0..99"), ("Next", "year:2000..1900")] {
        assert!(mutate_at(&file, |c| c.add(name, text)).is_err(), "{name}: {text}");
        assert_eq!(std::fs::read(&file).unwrap(), before);
    }
    assert!(mutate_at(&file, |c| c.rename("missing", "Other")).is_err());
    assert!(mutate_at(&file, |c| c.remove("missing")).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let catalog = mutate_at(&file, |c| c.add("Other", "genre:unknown")).unwrap();
    assert_eq!(catalog.entries.len(), 2, "identical expressions may have different names");
}

#[test]
fn limits_measure_unicode_names_and_expression_bytes() {
    let mut c = Catalog::default();
    c.add(&"界".repeat(80), "title:nevermatched").unwrap();
    assert!(c.add(&"界".repeat(81), "title:x").is_err());
    assert!(c.add("Large", &"é".repeat(MAX_EXPRESSION_BYTES / 2 + 1)).is_err());
    for n in 1..MAX_ENTRIES { c.add(&n.to_string(), "title:x").unwrap(); }
    assert!(c.add("Too many", "title:x").is_err());
    assert!(Catalog::read(Some(&" ".repeat(MAX_CATALOG_BYTES + 1))).is_err());
}

#[test]
fn damaged_or_newer_catalogs_are_never_repaired_implicitly() {
    let root = tempfile::tempdir().unwrap(); let file = root.path().join("settings.conf");
    for raw in ["broken", r#"{"version":2,"entries":[]}"#, r#"{"version":1,"revision":0,"next_id":1,"entries":[{}]}"#] {
        let original = format!("appearance.theme = dark\n{} = {raw}\n", settings::SAVED_QUERIES_KEY);
        std::fs::write(&file, &original).unwrap();
        assert!(mutate_at(&file, |c| c.add("First", "genre:jazz")).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }
}

#[test]
fn failed_save_does_not_publish_a_candidate() {
    let root = tempfile::tempdir().unwrap(); let directory = root.path().join("settings.conf");
    std::fs::create_dir(&directory).unwrap();
    assert!(mutate_at(&directory, |c| c.add("First", "genre:jazz")).is_err());
    assert!(directory.is_dir());
}

#[test]
fn checking_saving_and_applying_share_numeric_semantics() {
    for text in ["rating:0..99", "year:2000..1900", "(rating:6..8 | genre:jazz)"] {
        assert!(library_query::read(text).is_err());
        assert_eq!(library_query::check(text)["ok"], false);
        assert!(validate_expression(text).is_err());
    }
    for text in [r#"title:"NOT""#, "genre:doesnotexist", "rating:0..5", "year:1900..2000"] {
        assert!(validate_expression(text).is_ok());
    }
}

#[test]
fn concurrent_transactions_keep_every_entry_and_unrelated_preference() {
    let root = tempfile::tempdir().unwrap(); let file = root.path().join("settings.conf");
    std::thread::scope(|scope| {
        for index in 0..16 {
            let file = &file;
            scope.spawn(move || {
                mutate_at(file, |c| c.add(&format!("Query {index}"), "genre:jazz")).unwrap();
                settings::update_value_at(file, &format!("test.preference{index}"), |_| Ok(("kept".into(), ()))).unwrap();
            });
        }
    });
    let catalog = Catalog::read(settings::read_value_from(&file, settings::SAVED_QUERIES_KEY).unwrap().as_deref()).unwrap();
    assert_eq!(catalog.entries.len(), 16);
    assert_eq!(catalog.revision, 16);
    for index in 0..16 { assert_eq!(settings::read_value_from(&file, &format!("test.preference{index}")).unwrap().as_deref(), Some("kept")); }
}

#[test]
fn serialized_catalog_limit_includes_json_escaping() {
    let mut catalog = Catalog::default();
    let text = format!("title:\"{}\"", "\\".repeat(MAX_EXPRESSION_BYTES - 8));
    for n in 0..MAX_ENTRIES {
        catalog.entries.push(SavedQuery { id: format!("q{}", n + 1), name: n.to_string(), expression: text.clone() });
    }
    assert!(catalog.encode().is_err(), "escaping can exceed the catalog limit before individual expressions exceed theirs");
}

#[test]
fn facade_reopens_saved_queries_without_a_library_or_queue_changes() {
    const CHILD: &str = "CHILPA_SAVED_QUERIES_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "saved_queries::tests::facade_reopens_saved_queries_without_a_library_or_queue_changes", "--nocapture"])
            .env(CHILD, "1").envs(chilpa_dirs::profile_environment(root.path())).output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let mut first = Chilpa::new();
    let saved = first.save_query("Empty result is valid", r#"(genre:"No such genre" | title:"AND")"#).unwrap();
    first.remember_preference_checked("appearance.theme", "dark").unwrap();
    assert!(first.queue.is_empty());
    assert!(first.library.is_none(), "managing queries does not open the library");
    drop(first);
    let mut second = Chilpa::new();
    assert_eq!(second.saved_queries().unwrap(), saved);
    assert_eq!(second.saved_query(&saved.entries[0].id).unwrap(), saved.entries[0].expression);
    assert_eq!(second.preference("appearance.theme").as_deref(), Some("dark"));
    second.rename_query(&saved.entries[0].id, "Renamed").unwrap();
    second.remove_query(&saved.entries[0].id).unwrap();
    assert!(second.saved_queries().unwrap().entries.is_empty());
    assert!(second.queue.is_empty());
}
