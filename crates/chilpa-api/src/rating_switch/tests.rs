use super::*;
use chilpa_library::store::ratings::RatingTarget;
use serde_json::json;

/// True in the child that runs the test in a profile of its own: the
/// switch is a preference, and another test must not read it turned off.
fn isolated(name: &str) -> bool {
    const CHILD: &str = "CHILPA_RATING_SWITCH_TEST";
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

fn query(text: &str) -> Option<chilpa_library::store::query::Node> {
    crate::library_query::read(text).unwrap()
}

/// Two records and three tracks: the first track has five stars, the
/// second record four.
fn rated_library() -> chilpa_library::Library {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let local = lib.source_id("local", "default").unwrap();
    lib.connection().execute_batch(&format!(
        "INSERT INTO albums(id, title, album_artist, version_key) VALUES (1, 'Set', 'Band', '1:first'), (2, 'Other', 'Band', '2:first');
         INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES
             (7, {local}, '/a.flac', 'One', 1, 0), (8, {local}, '/b.flac', 'Two', 2, 0), (9, {local}, '/c.flac', 'Three', 2, 0);"
    )).unwrap();
    lib.rate(RatingTarget::Track(7), 5).unwrap();
    lib.rate(RatingTarget::Album(2), 4).unwrap();
    lib
}

fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

#[test]
fn off_every_rating_condition_is_left_out_and_nothing_else() {
    let filter = Filter {
        ratings: vec![5], track_ratings: vec![4, 0], genres: vec!["Jazz".into()],
        query: query("genre:jazz rating:4.. (year:1960 | -rating:0)"), ..Filter::default()
    };
    let unrated = applied(&filter, false);
    assert!(unrated.ratings.is_empty() && unrated.track_ratings.is_empty());
    assert_eq!(unrated.genres, filter.genres);
    let text = unrated.query.as_ref().unwrap().to_string();
    assert!(!text.contains("rating"), "{text}");
    assert!(text.contains("genre:") && text.contains("year:1960"), "{text}");
    assert!(matches!(applied(&filter, true), Cow::Borrowed(_)), "on, the filter is used as it is");
    let plain = Filter { genres: vec!["Jazz".into()], query: query("year:1960"), ..Filter::default() };
    assert!(matches!(applied(&plain, false), Cow::Borrowed(_)), "a filter without stars is used as it is");
    let only = Filter { query: query("rating:5"), ..Filter::default() };
    assert_eq!(applied(&only, false).query, None, "stars alone keep nothing out");
}

#[test]
fn off_refine_has_no_rating_row_and_the_other_rows_keep_their_places() {
    let rows = vec![
        json!({"id":"0","field":"genre"}),
        json!({"id":"1","field":"rating","kind":"rating"}),
        json!({"id":"2","group":true,"rows":[{"id":"2.0","field":"rating"},{"id":"2.1","field":"year"}]}),
        json!({"id":"3","group":true,"rows":[{"id":"3.0","field":"rating"}]}),
    ];
    assert_eq!(super::rows(rows.clone(), true), rows);
    assert_eq!(super::rows(rows, false), [json!({"id":"0","field":"genre"}), json!({"id":"2","group":true,"rows":[{"id":"2.1","field":"year"}]})]);
}

#[test]
fn off_help_filter_has_no_example_about_stars() {
    assert_eq!(examples(true).len(), crate::library_query::EXAMPLES.len());
    let off = examples(false);
    assert_eq!(off.len(), crate::library_query::EXAMPLES.len() - 1);
    assert!(off.iter().all(|(example, _)| !example.contains("rating")));
}

#[test]
fn settings_search_finds_the_switch_under_library() {
    let api = Chilpa::new();
    for words in ["rating", "stars", "star ratings"] {
        assert!(api.settings_find(words, 40).iter().any(|found| found.key == ENABLED && found.section == "library"), "{words}");
    }
}

#[test]
fn the_switch_is_on_by_default_and_remembers_the_opt_out() {
    if !isolated("rating_switch::tests::the_switch_is_on_by_default_and_remembers_the_opt_out") {
        return;
    }
    let value = |api: &mut Chilpa| api.settings_rows("library", &crate::RowNotes::default()).0.into_iter()
        .find(|row| row.key == ENABLED).map(|row| (row.control, row.value));
    let mut api = Chilpa::new();
    assert!(api.ratings_shown());
    assert_eq!(value(&mut api), Some(("toggle".to_string(), "true".to_string())));
    assert!(api.adjust(ENABLED, "false"));
    drop(api);
    let mut reopened = Chilpa::new();
    assert!(!reopened.ratings_shown());
    assert_eq!(value(&mut reopened), Some(("toggle".to_string(), "false".to_string())));
    assert!(!reopened.adjust(ENABLED, "maybe"), "only a boolean is taken");
    assert!(!reopened.ratings_shown());
    assert!(reopened.adjust(ENABLED, "true"));
    assert_eq!(value(&mut reopened), Some(("toggle".to_string(), "true".to_string())));
}

#[test]
fn off_the_stars_given_stay_and_filter_nothing_until_turned_on_again() {
    if !isolated("rating_switch::tests::off_the_stars_given_stay_and_filter_nothing_until_turned_on_again") {
        return;
    }
    let reader = crate::Reader::of(rated_library());
    let five_star_tracks = |reader: &crate::Reader| sorted(reader.tracks("title\u{1f}off\u{1f}\u{1f}5", &Filter::default()).snapshot.ids);
    let four_star_records = Filter { query: query("rating:4.."), ..Filter::default() };
    let library = |reader: &crate::Reader| sorted(reader.refresh(&four_star_records, crate::TrackOrder::Title, || true).unwrap().1.ids);
    let refine_fields = |text: &str| crate::library_query::refine(text)["rows"].as_array().unwrap().iter()
        .map(|row| row["field"].as_str().unwrap_or_default().to_string()).collect::<Vec<_>>();
    let help_examples = |api: &mut Chilpa| serde_json::from_str::<Value>(&api.command_line_json(":help filter")).unwrap()["lines"]
        .as_array().unwrap().iter().filter(|line| line["example"] == true).count();

    let mut api = Chilpa::new();
    api.library = Some(rated_library());
    let given = api.ratings().unwrap();
    assert_eq!((given.tracks.len(), given.albums.len()), (1, 1));
    assert_eq!(five_star_tracks(&reader), [7]);
    assert_eq!(library(&reader), [8, 9]);
    assert_eq!(refine_fields("genre:jazz rating:4.."), ["genre", "rating"]);
    let every_example = help_examples(&mut api);

    assert!(api.adjust(ENABLED, "false"));
    assert_eq!(api.ratings().unwrap(), given, "turning ratings off keeps every star");
    assert_eq!(five_star_tracks(&reader), [7, 8, 9], "the Tracks screen's stars keep nothing out");
    assert_eq!(library(&reader), [7, 8, 9], "a rating condition keeps nothing out");
    assert_eq!(refine_fields("genre:jazz rating:4.."), ["genre"]);
    assert_eq!(help_examples(&mut api), every_example - 1);

    assert!(api.adjust(ENABLED, "true"));
    assert_eq!(api.ratings().unwrap(), given);
    assert_eq!(five_star_tracks(&reader), [7]);
    assert_eq!(library(&reader), [8, 9]);
    assert_eq!(refine_fields("genre:jazz rating:4.."), ["genre", "rating"]);
}
