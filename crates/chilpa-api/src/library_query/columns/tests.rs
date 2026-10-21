use super::*;
use crate::{Reader, TrackOrder};

fn rows(values: &[&str]) -> Vec<FacetEntry> {
    values.iter().map(|v| FacetEntry { key: (*v).into(), label: (*v).into(), albums: 1, year: None, icon: String::new() }).collect()
}
fn text(filter: &Filter) -> String { filter.query.as_ref().map(ToString::to_string).unwrap_or_default() }
fn filter(text: &str) -> Filter { Filter { query: read(text).unwrap(), ..Default::default() } }

#[test]
fn clicks_edit_the_query_and_both_roles_for_every_column() {
    for (field, value) in [("year","1960"),("genre","Jazz"),("label","Label"),("composer","Writer"),
        ("rating","4"),("artist","Artist"),("album","Blue Record")] {
        let mut rows = rows(&[value]);
        if field == "album" { rows[0].key = "42".into(); }
        let mut f = Filter::default();
        let anchor = choose(&mut f,field,&rows,None,1,0);
        let expected = refine::refine_edit("", &json!({"op":"value","field":field,"value":value,"state":"include"}).to_string());
        assert!(!expected.is_empty(), "{field}");
        assert_eq!(text(&f),expected);
        assert_eq!(selection(&f,field,&rows).included,[rows[0].key.clone()]);
        assert!(selection(&f,field,&rows).excluded.is_empty());
        choose(&mut f,field,&rows,anchor,1,0);
        assert_eq!(text(&f),"");
        assert!(!selection(&f,field,&rows).active);
        choose(&mut f,field,&rows,anchor,1,4);
        assert!(text(&f).starts_with(&format!("-{field}:")));
        assert_eq!(selection(&f,field,&rows).excluded,[rows[0].key.clone()]);
        assert!(selection(&f,field,&rows).included.is_empty());
        choose(&mut f,field,&rows,anchor,1,4);
        assert_eq!(text(&f),"");
        choose(&mut f,field,&rows,anchor,1,4);
        choose(&mut f,field,&rows,anchor,1,0);
        assert_eq!(text(&f),expected,"including moves the excluded value");
        choose(&mut f,field,&rows,anchor,0,0);
        assert_eq!(text(&f),"");
        assert_eq!(f,Filter::default(),"I keep no second column state");
    }
}

#[test]
fn all_clears_only_its_field_and_alt_all_does_nothing() {
    let rows = rows(&["Jazz","Pop"]);
    let mut f=filter("genre:Jazz -genre:Pop year:1960 (genre:Soul | label:Label)");
    choose(&mut f,"genre",&rows,None,0,4);
    assert!(text(&f).contains("genre:"));
    choose(&mut f,"genre",&rows,None,0,0);
    assert_eq!(text(&f),"year:1960 (label:Label)");
    assert!(!selection(&f,"genre",&rows).active);
    assert!(selection(&f,"year",&[]).active);
}

#[test]
fn clicks_and_modifier_ranges_preserve_the_other_fields() {
    let rows=rows(&["Blues","Jazz","Pop","Soul"]);
    let mut f=filter("year:1960 -genre:Pop");
    let anchor=choose(&mut f,"genre",&rows,None,2,0);
    choose(&mut f,"genre",&rows,anchor,1,0);
    assert_eq!(text(&f),"year:1960 -genre:Pop genre:Jazz,Blues");
    choose(&mut f,"genre",&rows,anchor,1,1);
    assert_eq!(text(&f),"year:1960 -genre:Pop genre:Jazz");
    let anchor=choose(&mut f,"genre",&rows,anchor,4,2);
    assert_eq!(selection(&f,"genre",&rows).included,["Jazz","Pop","Soul"]);
    assert!(selection(&f,"genre",&rows).excluded.is_empty());
    choose(&mut f,"genre",&rows,anchor,1,3);
    assert_eq!(selection(&f,"genre",&rows).included,["Blues","Jazz","Pop","Soul"]);
    assert!(text(&f).starts_with("year:1960"));
    assert_eq!(choose(&mut f,"genre",&rows,Some(99),4,2),None);
}

fn reader() -> Reader {
    let lib=chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES (1,'local','default');
        INSERT INTO albums(id,title,album_artist,year) VALUES (1,'Blue Record','Artist',1960),(2,'Gold Record','Other',1980);
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,label,added_at)
        VALUES (1,1,'/one.flac','One','Artist',1,'Label',1),(2,1,'/two.flac','Two','Artist',1,'Label',1),
        (3,1,'/three.flac','Three','Other',2,'Other Label',1);").unwrap();
    for (id,genre) in [(1,"Jazz"),(2,"Jazz"),(3,"Pop")] { lib.set_track_genres(id,&[genre.into()]).unwrap(); }
    Reader::of(lib)
}
fn counts(rows: &[FacetEntry]) -> Vec<(&str,usize)> { rows.iter().map(|e| (e.key.as_str(),e.albums)).collect() }

#[test]
fn each_column_leaves_only_its_own_query_field_out() {
    let reader=reader();
    let f=filter("genre:Jazz year:1980");
    let (facets,snapshot)=reader.refresh(&f,TrackOrder::Artist,||true).unwrap();
    assert!(snapshot.ids.is_empty());
    assert_eq!(counts(&facets.genres),[("Pop",1)]);
    assert_eq!(counts(&facets.years),[("1960",1)]);
    assert!(facets.artists.is_empty());
    let f=filter("-genre:Jazz year:1960");
    let facets=reader.refresh(&f,TrackOrder::Artist,||true).unwrap().0;
    assert_eq!(counts(&facets.genres),[("Jazz",1)]);
    assert_eq!(counts(&facets.years),[("1980",1)]);
    let f=filter("(genre:Jazz | artist:Other) year:1960");
    assert_eq!(counts(&reader.facets(&f).genres),[("Jazz",1)]);
}

#[test]
fn library_queries_leave_the_independent_albums_and_tracks_views_unchanged() {
    let reader=reader();
    let mut f=Filter::default();
    let rows=reader.facets(&f).genres;
    choose(&mut f,"genre",&rows,None,1,0);
    let snapshot=reader.refresh(&f,TrackOrder::Artist,||true).unwrap().1;
    assert_eq!(snapshot.ids,[1,2]);
    assert_eq!(snapshot.album_ids,[1]);
    assert_eq!(reader.albums_view("artist","off","",&f).ids,[1,2]);
    assert_eq!(reader.tracks_using("title\u{1f}off\u{1f}",&f,&crate::library_sources::SourceIndex::default()).snapshot.ids,[1,3,2]);
    let revision=reader.lib.read_revision().unwrap();
    assert_eq!(reader.refresh(&f,TrackOrder::Artist,||true).unwrap().1,snapshot);
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}

#[test]
fn console_ranges_and_case_folded_values_supply_column_roles() {
    for (field,query,values,wanted) in [("year","year:1959..1961",vec!["1950","1960"],"1960"),
        ("rating","rating:3..5",vec!["2","4"],"4"),("genre","genre:jazz",vec!["Jazz","Pop"],"Jazz")] {
        assert_eq!(selection(&filter(query),field,&rows(&values)).included,[wanted]);
    }
}

#[test]
fn clicks_remove_values_from_ranges_and_groups_without_losing_other_conditions() {
    let years=rows(&["1960","1965","1970"]);
    let mut f=filter("year:1960..1970 genre:Jazz");
    choose(&mut f,"year",&years,None,2,0);
    assert_eq!(selection(&f,"year",&years).included,["1960","1970"]);
    assert!(selection(&f,"year",&years).excluded.is_empty());
    assert!(text(&f).contains("genre:Jazz"));
    choose(&mut f,"year",&years,None,3,4);
    assert_eq!(selection(&f,"year",&years).excluded,["1970"]);
    choose(&mut f,"year",&years,None,3,0);
    assert_eq!(selection(&f,"year",&years).included,["1960","1970"]);
    let genres=rows(&["Jazz"]);
    let mut f=filter("(genre:Jazz | artist:Other) year:1960");
    choose(&mut f,"genre",&genres,None,1,0);
    assert_eq!(text(&f),"(artist:Other) year:1960");
}

#[test]
fn extending_a_range_uses_alternatives_and_shift_replaces_hidden_picks() {
    let reader=reader();
    let years=rows(&["1960","1980"]);
    let mut f=filter("year:1960..1980");
    choose(&mut f,"year",&years,None,1,0);
    assert_eq!(reader.refresh(&f,TrackOrder::Artist,||true).unwrap().1.ids,[3]);
    choose(&mut f,"year",&years,None,1,0);
    assert_eq!(reader.refresh(&f,TrackOrder::Artist,||true).unwrap().1.ids,[1,2,3]);
    let genres=rows(&["Jazz","Pop"]);
    let mut f=filter("genre:Hidden -genre:Soul");
    choose(&mut f,"genre",&genres,Some(0),2,2);
    assert_eq!(text(&f),"-genre:Soul genre:Jazz,Pop");
}
