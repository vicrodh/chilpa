mod fields;

use super::*;
use crate::Library;
use rusqlite::params;

/// Six tracks on four records, with what each condition looks at.
fn library() -> Library {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let db = lib.connection();
    for (id, title, artist, year) in [(1, "Kind of Blue", "Miles Davis", Some(1959)), (2, "Monk's Dream", "Thelonious Monk", Some(1963)),
        (3, "Blue Train", "John Coltrane", Some(1957)), (4, "Untitled", "Nobody", None)] {
        db.execute("INSERT INTO albums(id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)", params![id, title, artist, year]).unwrap();
    }
    for (id, album, title, artist, label, added) in [(1, 1, "So What", "Miles Davis", Some("Columbia"), 100), (2, 1, "Blue in Green", "Miles Davis", Some("Columbia"), 100),
        (3, 2, "Bye-Ya", "Thelonious Monk", Some("Columbia"), 5_000), (4, 3, "Blue Train", "John Coltrane", Some("Blue Note"), 9_000),
        (5, 3, "Moment's Notice", "John Coltrane", Some("Blue Note"), 9_000), (6, 4, "Hum", "Nobody", None, 9_500)] {
        db.execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, label, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, source, format!("/{id}.flac"), title, artist, album, label, added]).unwrap();
    }
    for (id, name) in [(1, "Jazz"), (2, "Modal"), (3, "Hard Bop")] {
        db.execute("INSERT INTO genres(id, name) VALUES (?1, ?2)", params![id, name]).unwrap();
    }
    for (track, genre) in [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1), (4, 1), (4, 3), (5, 3)] {
        db.execute("INSERT INTO track_genres(track_id, genre_id) VALUES (?1, ?2)", params![track, genre]).unwrap();
    }
    db.execute("INSERT INTO composers(id, name) VALUES (1, 'Thelonious Monk'), (2, 'Miles Davis')", []).unwrap();
    db.execute("INSERT INTO track_composers(track_id, composer_id) VALUES (3, 1), (1, 2)", []).unwrap();
    db.execute("INSERT INTO album_ratings(album_id, stars, revision, updated_at) VALUES (1, 5, 1, 1), (3, 4, 1, 1)", []).unwrap();
    lib
}

fn found(lib: &Library, query: &str) -> Vec<i64> {
    let node = parse(query).unwrap_or_else(|e| panic!("{query}: {e}"));
    let (sql, args) = node.compile(10_000);
    let mut stmt = lib.connection().prepare(&format!("SELECT t.id FROM tracks t JOIN albums a ON a.id = t.album_id WHERE {sql} ORDER BY t.id")).unwrap();
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn conditions_stack_with_and_or_and_not() {
    let lib = library();
    assert_eq!(found(&lib, ""), [1, 2, 3, 4, 5, 6], "no condition keeps everything");
    assert_eq!(found(&lib, "genre:jazz"), [1, 2, 3, 4], "a name matches without regard to case");
    assert_eq!(found(&lib, "genre:modal,\"hard bop\""), [1, 2, 4, 5], "either value");
    assert_eq!(found(&lib, "genre:jazz -genre:modal"), [3, 4]);
    assert_eq!(found(&lib, "label:columbia | composer:\"John Coltrane\",monk"), [1, 2, 3], "an unknown composer holds nothing");
    assert_eq!(found(&lib, "(genre:modal | label:\"blue note\") -year:1957"), [1, 2]);
    assert_eq!(found(&lib, "NOT (genre:jazz OR genre:\"hard bop\")"), [6]);
    assert_eq!(found(&lib, "year:1958..1965"), [1, 2, 3]);
    assert_eq!(found(&lib, "year:..1958"), [4, 5]);
    assert_eq!(found(&lib, "year:none"), [6]);
    assert_eq!(found(&lib, "genre:none"), [6]);
    assert_eq!(found(&lib, "label:none"), [6]);
    assert_eq!(found(&lib, "composer:none"), [2, 4, 5, 6]);
    assert_eq!(found(&lib, "rating:4..5"), [1, 2, 4, 5], "a record's stars");
    assert_eq!(found(&lib, "rating:0"), [3, 6], "no stars is zero");
    assert_eq!(found(&lib, "artist:\"miles davis\",monk"), [1, 2], "the whole credit, not part of it");
    assert_eq!(found(&lib, "album:blue"), [1, 2, 4, 5], "words of the record's title");
    assert_eq!(found(&lib, "title:blue"), [2, 4]);
    assert_eq!(found(&lib, "blue coltrane"), [4, 5], "bare words, each anywhere");
    assert_eq!(found(&lib, "monk"), [3], "a word starts a word: the artist and the record");
    assert_eq!(found(&lib, "onk"), Vec::<i64>::new(), "not the middle of one");
    assert_eq!(found(&lib, "title:\"blue in\""), [2], "every word of it");
    assert_eq!(found(&lib, "added:1y"), [1, 2, 3, 4, 5, 6]);
    assert_eq!(found(&lib, "added:5000..9000"), [3, 4, 5]);
}

#[test]
fn every_value_is_a_parameter_and_never_part_of_the_statement() {
    let lib = library();
    let hostile = "x'); DROP TABLE tracks; --";
    let node = Node::All(vec![Node::Is(Field::Genre, Values::Any(vec![hostile.into()])), Node::Is(Field::Text, Values::Any(vec!["100%_\"*".into()]))]);
    let (sql, args) = node.compile(0);
    assert!(!sql.contains("DROP") && !sql.contains("100%"), "the statement holds only marks: {sql}");
    assert_eq!(args.len(), 2, "one name, and the words once");
    assert_eq!(found(&lib, &node.to_string()), Vec::<i64>::new(), "a quote or a star in a word is part of the word");
    let tracks: i64 = lib.connection().query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
    assert_eq!(tracks, 6);
}

#[test]
fn a_query_reads_back_as_the_same_tree() {
    for text in ["genre:jazz,\"hard bop\" -label:\"blue note\"", "(composer:monk | artist:\"bill evans\") year:1955..1965",
        "-(genre:modal | genre:none) added:30d", "rating:4,5 album:\"kind of\" title:blue", "y:..1960 l:none c:monk", "blue \"so what\""] {
        let tree = parse(text).unwrap();
        let written = tree.to_string();
        assert_eq!(parse(&written).unwrap(), tree, "{text} -> {written}");
    }
    assert_eq!(parse("genre:jazz AND genre:soul").unwrap(), parse("genre:jazz genre:soul").unwrap());
    assert_eq!(parse("--genre:jazz").unwrap(), parse("genre:jazz").unwrap(), "two NOTs cancel");
}

#[test]
fn a_query_that_cannot_be_read_says_where() {
    let fails = |text: &str| parse(text).err().unwrap_or_else(|| panic!("{text} should not read"));
    assert_eq!(fails("genre:\"jazz").what, "a quote is not closed");
    assert_eq!(fails("(genre:jazz").what, "a parenthesis is not closed");
    assert_eq!(fails("genre:jazz)").at, 10);
    assert_eq!(fails("| genre:jazz").what, "OR needs a condition on each side");
    assert_eq!(fails("genre:").what, "a field needs a value");
    assert_eq!(fails("rating:7").what, "a rating goes from 0 to 5");
    assert_eq!(fails("year:soon").what, "a year or a rating is a number");
    assert_eq!(fails("added:soon").what, "added takes a span like 30d, 6w, 3m or 1y");
    assert_eq!(fails("artist:none").what, "that field is never empty");
    assert_eq!(fails(&vec!["w"; MOST_CONDITIONS + 1].join(" ")).what, "the query has too many conditions");
    assert_eq!(fails(&format!("{}x{}", "(".repeat(DEEPEST + 1), ")".repeat(DEEPEST + 1))).what, "the groups go too deep");
}

#[test]
fn a_name_condition_reads_its_index_instead_of_every_track() {
    let lib = library();
    let (sql, args) = parse("genre:jazz | composer:monk").unwrap().compile(0);
    let mut stmt = lib.connection().prepare(&format!("EXPLAIN QUERY PLAN SELECT t.id FROM tracks t JOIN albums a ON a.id = t.album_id WHERE {sql}")).unwrap();
    let plan: Vec<String> = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(3)).unwrap().map(Result::unwrap).collect();
    let plan = plan.join("\n");
    assert!(plan.contains("UNION"), "either set, read once each:\n{plan}");
    assert!(!plan.contains("SCAN tg") && !plan.contains("SCAN tc"), "no pass over every genre or composer row:\n{plan}");
}

#[test]
fn quoted_values_and_album_alternatives_keep_their_meaning() {
    for field in [Field::Genre, Field::Artist, Field::Album] {
        let tree = Node::Is(field, Values::Any(vec!["None".into(), "Davis, Miles".into(), "The \"Band\"".into(), "C:\\Music".into()]));
        assert_eq!(parse(&tree.to_string()).unwrap(), tree);
    }
    let lib = library();
    assert_eq!(found(&lib, "album:\"Kind of Blue\",\"Monk's Dream\""), [1, 2, 3]);
    assert!(parse("added:9223372036854775807y").is_err());
}

#[test]
fn literal_fields_execute_like_the_original_fields() {
    let lib=library();
    for (plain,literal) in [("blue", "words:blue"),("genre:Jazz","genre=:Jazz"),("-genre:Jazz","-genre=:Jazz")] {
        assert_eq!(found(&lib,plain),found(&lib,literal));
        let tree=parse(literal).unwrap(); assert_eq!(parse(&tree.to_string()).unwrap(),tree);
    }
    for text in ["(genre:Jazz)","(genre:Jazz genre:Modal)","(genre:Jazz | genre:Modal) year:1959"] {
        let grouped=parse_grouped(text).unwrap(); assert_eq!(grouped.to_string(),text);
        assert_eq!(grouped.ungrouped(),parse(text).unwrap());
        let (sql,args)=grouped.compile(10_000); let (plain,plain_args)=parse(text).unwrap().compile(10_000);
        assert_eq!(sql,plain); assert_eq!(args.len(),plain_args.len());
    }
}
