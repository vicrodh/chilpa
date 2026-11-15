use super::*;

fn reader() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id,provider,instance,enabled) VALUES
        (1,'local','default',1),(2,'server','home',1),(3,'server','disabled',0);
        INSERT INTO albums(id,title,album_artist) VALUES
        (1,'First','First artist'),(2,'Second','Second artist'),(3,'Third','Third artist'),
        (4,'Fourth','Fourth artist'),(5,'Disabled','Hidden artist');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at) VALUES
        (1,1,'/first','One','First artist',1,1),(2,1,'/second','Two','First artist',1,1),
        (3,2,'/remote','Three','Second artist',2,1),(4,1,'/anison','Four','Third artist',3,1),
        (5,1,'/rock','Five','Fourth artist',4,1),(6,1,'/loose','Loose','Soloist',NULL,1),
        (7,3,'/hidden','Hidden','Hidden artist',5,1);").unwrap();
    for (id, tags) in [(1, vec!["J-Pop", "j_pop", "J POP", "j-rock"]), (2, vec!["jpop"]),
        (3, vec!["J-Pop"]), (4, vec!["Anison"]), (5, vec!["japanese rock", "JROCK"]),
        (6, vec!["Enka"]), (7, vec!["K-Pop"])] {
        lib.set_track_genres(id, &tags.into_iter().map(str::to_string).collect::<Vec<_>>()).unwrap();
    }
    Reader::of(lib)
}

#[test]
fn asian_cards_header_shelves_and_play_share_descendants_aliases_and_source_scope() {
    let reader = reader();
    let revision = reader.lib.read_revision().unwrap();
    let original = reader.lib.track_genres(1).unwrap();
    for (excluded, albums, tracks) in [(vec![], vec![1, 2, 3], vec![1, 2, 3, 4, 6]),
        (vec!["server:home"], vec![1, 3], vec![1, 2, 4, 6]),
        (vec!["local:default"], vec![2], vec![3])] {
        let key = json!({"kind":"genre", "name":"Asian", "excluded_sources":excluded}).to_string();
        let options = json!({"excluded_sources":excluded}).to_string();
        let root = reader.discovery_page_options("genres", "", "", &options, 0);
        let card = root["items"].as_array().unwrap().iter().find(|row| row["name"] == "Asian").unwrap();
        assert_eq!(card["albums"], albums.len());
        let page = reader.discovery_page_options("genres", &key, "", &options, 0);
        assert_eq!(page["summary"]["albums"], card["albums"]);
        let shelf = reader.collection_albums("discovery", "title", "", "", &Filter::default(), &key);
        assert_eq!(shelf.ids.into_iter().collect::<HashSet<_>>(), albums.into_iter().collect());
        let mut played = Vec::new();
        reader.collection_tracks("discovery", &key, &Filter::default(), |rows| played = rows.snapshot.ids);
        assert_eq!(played.into_iter().collect::<HashSet<_>>(), tracks.into_iter().collect());
        assert_eq!(reader.discovery_artists(&key).len(), card["albums"].as_u64().unwrap() as usize);
    }
    let header = reader.album_genre_links(1);
    let pop = header["items"].as_array().unwrap().iter().find(|row| Selection::parse(row["key"].as_str().unwrap()).unwrap().genre_token == presentation::token("J-Pop")).unwrap();
    assert_eq!(pop["originalsCount"], 4);
    assert_eq!(pop["name"], "J POP");
    let key = pop["key"].as_str().unwrap();
    assert_eq!(reader.discovery_page("genres", key, "", 0)["summary"]["albums"], 1);
    assert_eq!(reader.discovery_snapshot(key).ids, [1, 2]);
    let rock = json!({"kind":"genre","name":"japanese rock"}).to_string();
    assert_eq!(reader.discovery_page("genres", &rock, "", 0)["summary"]["albums"], 2);
    assert_eq!(reader.discovery_snapshot(&rock).ids.into_iter().collect::<HashSet<_>>(), HashSet::from([1, 5]));
    assert_eq!(reader.lib.track_genres(1).unwrap(), original);
    assert_eq!(reader.lib.read_revision().unwrap(), revision);
}

#[test]
fn empty_and_albumless_genres_are_removed_before_search_totals_and_pagination() {
    let reader = reader();
    for scope in ["", "all", "library", "tags"] {
        let options = json!({"scope":scope}).to_string();
        for search in ["enka", "k-pop", "classical"] {
            let page = reader.discovery_page_options("genres", "", search, &options, 0);
            assert_eq!(page["total"], 0, "{scope} {search}");
            assert_eq!(page["items"], json!([]));
        }
    }
    reader.lib.set_track_genres(1, &(0..125).map(|id| format!("Custom populated {id:03}")).collect::<Vec<_>>()).unwrap();
    reader.lib.set_track_genres(6, &(0..125).map(|id| format!("Custom absent {id:03}")).collect::<Vec<_>>()).unwrap();
    let mut seen = HashSet::new();
    for page in 0..3 {
        let result = reader.discovery_page_options("genres", "", "Custom", r#"{"scope":"tags"}"#, page);
        assert_eq!(result["total"], 125);
        assert_eq!(result["items"].as_array().unwrap().len(), if page < 2 { 60 } else { 5 });
        for item in result["items"].as_array().unwrap() {
            assert!(seen.insert(item["name"].as_str().unwrap().to_string()));
            assert_eq!(item["albums"], 1);
        }
    }
}

#[test]
fn a_failed_genre_inventory_reports_an_error_instead_of_zero_albums() {
    let reader = reader();
    reader.lib.connection().execute_batch("ALTER TABLE genres RENAME TO unavailable_genres").unwrap();
    let page = reader.discovery_page("genres", r#"{"kind":"genre","name":"Asian"}"#, "", 0);
    assert!(page["error"].as_str().is_some_and(|message| message.contains("no such table") && message.contains("genres")));
    assert!(page.get("summary").is_none());
}
