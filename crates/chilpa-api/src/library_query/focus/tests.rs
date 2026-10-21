use super::*;
use chilpa_library::store::query::parse;

fn round_trip(text: &str) {
    let tree = parse(text).unwrap();
    assert_eq!(parse(&tree.to_string()).unwrap(), tree, "{text}");
}

#[test]
fn values_move_between_include_exclude_and_none() {
    let text = with_value("", "genre", "Jazz", ValueState::Include);
    assert_eq!(text, "genre:Jazz");
    let text = with_value(&text, "genre", "Blues", ValueState::Include);
    assert_eq!(text, "genre:Jazz,Blues");
    assert_eq!(with_value(&text, "genre", "Jazz", ValueState::Include), text);
    let text = with_value(&text, "genre", "Jazz", ValueState::Exclude);
    assert_eq!(text, "genre:Blues -genre:Jazz");
    assert_eq!(with_value(&text, "genre", "Jazz", ValueState::None), "genre:Blues");
    assert_eq!(with_value(&text, "genre", "Blues", ValueState::Exclude), "-genre:Jazz,Blues");
    let text = with_value(&text, "genre", "jazz", ValueState::Include);
    assert_eq!(text, "genre:Blues,jazz");
    assert_eq!(with_value("-genre:Jazz", "genre", "Jazz", ValueState::None), "");
    assert_eq!(with_value("genre:Jazz", "genre", "Jazz", ValueState::Include), "genre:Jazz");
    assert_eq!(with_value("genre:Jazz -genre:Jazz", "genre", "Jazz", ValueState::Include), "genre:Jazz");
    round_trip(&text);
}

#[test]
fn each_field_stays_one_row_and_preserves_top_level_join() {
    let text = "genre:Jazz,Blues -genre:Pop year:1960..1979 added:30d rating:3.. composer:none";
    let rows = focus_rows(text);
    assert!(!rows.advanced);
    assert_eq!(rows.rows.len(), 5);
    assert_eq!(rows.rows[0].include, ["Jazz", "Blues"]);
    assert_eq!(rows.rows[0].exclude, ["Pop"]);
    assert_eq!(rows.rows[1].range, Some((Some(1960), Some(1979))));
    assert_eq!(rows.rows[2].within, Some(30));
    assert_eq!(rows.rows[3].minimum, Some(3));
    assert!(rows.rows[4].missing);
    let any = crate::library_query::query_joined(text, true);
    assert!(focus_rows(&any).any);
    assert_eq!(with_value("genre:Jazz | label:Verve", "genre", "Blues", ValueState::Include), "genre:Jazz,Blues | label:Verve");
    round_trip(&any);
}

#[test]
fn range_span_missing_and_minimum_replace_and_remove_their_condition() {
    let text = with_range("", "year", Some(1960), Some(1979));
    assert_eq!(text, "year:1960..1979");
    assert_eq!(with_range(&text, "year", None, Some(1980)), "year:..1980");
    assert_eq!(with_range(&text, "year", Some(1980), None), "year:1980..");
    assert_eq!(with_range(&text, "year", None, None), "");
    let text = with_within(&text, "added", 30);
    assert_eq!(text, "year:1960..1979 added:1m");
    assert_eq!(with_within("added:7d", "added", 365), "added:1y");
    assert_eq!(with_within("added:7d", "added", 0), "");
    let text = with_missing(&text, "composer", true);
    assert_eq!(with_missing(&text, "composer", true), text);
    assert_eq!(with_missing("composer:none", "composer", false), "");
    let text = with_minimum(&text, "rating", Some(4));
    assert_eq!(focus_rows(&text).rows.last().unwrap().minimum, Some(4));
    assert_eq!(with_minimum("rating:4..", "rating", Some(0)), "rating:0..");
    assert_eq!(with_minimum("rating:4..", "rating", None), "");
    round_trip(&text);
}

#[test]
fn groups_and_other_conditions_stay_advanced() {
    for text in ["(genre:Jazz | artist:Monk) year:1960..", "-(genre:Jazz label:Verve)",
        "genre:Jazz genre:Blues", "-genre:Jazz | -genre:Blues", "rating:2..4", "words", "-year:1960..", "added:1..2"] {
        let rows = focus_rows(text);
        assert!(rows.advanced, "{text}");
        assert!(rows.rows.is_empty());
        assert_eq!(rows.text, parse(text).unwrap().to_string());
        assert_eq!(with_value(text, "genre", "Pop", ValueState::Include), text);
    }
    assert!(!focus_rows("genre:Jazz | label:Verve").advanced);
    assert_eq!(focus_rows("").rows.len(), 0);
}

#[test]
fn invalid_edits_never_erase_the_previous_query() {
    let text = "genre:Jazz";
    assert_eq!(with_range(text, "year", Some(2000), Some(1900)), text);
    assert_eq!(with_range(text, "genre", Some(1), None), text);
    assert_eq!(with_within(text, "added", i64::MAX), text);
    assert_eq!(with_within(text, "year", 30), text);
    assert_eq!(with_missing(text, "artist", true), text);
    assert_eq!(with_minimum(text, "rating", Some(6)), text);
    assert_eq!(with_minimum(text, "genre", Some(3)), text);
    assert_eq!(with_value(text, "unknown", "a", ValueState::Include), text);
    assert_eq!(with_value(text, "year", "oops", ValueState::Include), text);
    assert_eq!(with_value("genre:(", "genre", "a", ValueState::Include), "genre:(");
}

#[test]
fn facet_names_round_trip_without_splitting_or_losing_quotes() {
    for field in ["genre", "label", "composer", "artist", "album"] {
        for value in ["None", "Davis, Miles", "The \"Band\"", "C:\\Music", "A | B", "Kind of Blue"] {
            let text = with_value("", field, value, ValueState::Include);
            assert_eq!(focus_rows(&text).rows[0].include, [value], "{field}: {value}");
            assert_eq!(with_value(&text, field, value, ValueState::None), "");
            round_trip(&text);
        }
    }
}

#[test]
fn play_counts_release_types_and_quality_stay_as_advanced_text() {
    for text in ["plays:0", "-plays:1..", "type:single,ep", "-type:live", "quality:cd,hires", "genre:jazz q:dsd"] {
        let rows = focus_rows(text);
        assert!(rows.advanced, "{text}");
        assert!(rows.rows.is_empty(), "{text}");
        assert_eq!(rows.text, parse(text).unwrap().to_string());
    }
}
