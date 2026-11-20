use super::*;
use crate::library_sources::{SourceIndex, SourceItem};
use std::sync::Arc;

pub(super) fn fixture() -> (Reader, Arc<SourceIndex>) {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES (1,'local','default'),(2,'server','home');
        INSERT INTO albums(id,title,album_artist) VALUES (1,'First','Player'),(2,'Second','Other');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at) VALUES
        (1,1,'/one','One','Player',1,1),(2,2,'/two','Two','Other',2,1),
        (3,2,'/three','Three','Player',1,1),(4,1,'/loose','Loose','Solo',NULL,1);").unwrap();
    for (id,tag) in [(1,"Progresive Metal"),(2,"Technical Death Metal"),(3,"Jazz"),(4,"Progresive Metal")] {
        lib.set_track_genres(id,&[tag.into()]).unwrap();
    }
    let item = |track,album,plugin:&str,tag:&str| SourceItem { track,album,plugin:plugin.into(),
        album_identity:format!("album:{album}"),album_title:format!("Remote {album}"),album_artist:format!("Artist {plugin}"),
        artists:vec![format!("Artist {plugin}")],genres:vec![tag.into()],saved_track:true,playable:true,tracks:1,..Default::default() };
    let index = SourceIndex::new(vec![item(-11,-10,"alpha","prog metal"),item(-12,-10,"alpha","heavy-metal"),
        item(-21,-20,"beta","Technical Death Metal"),item(-22,-20,"beta","My Sunday collection")],Vec::new());
    (Reader::of(lib),Arc::new(index))
}

#[test]
fn genre_sources_filter_inventory_summaries_tracks_and_saved_track_artists() {
    let (reader,index) = fixture();
    let revision = reader.lib.read_revision().unwrap();
    let selected = Selection { kind:"genre".into(),name:"Metal".into(),
        excluded_sources:vec!["server:home".into(),"plugin:beta".into()],..Default::default() };
    let options = options::Options::parse(r#"{"scope":"library"}"#).unwrap();
    let page = reader.genre_page_using(&selected,"",&options,&selected.excluded_sources,0,&index);
    assert_eq!(page["summary"]["albums"],2);
    assert_eq!(page["summary"]["looseTracks"],1);
    let originals: HashSet<_> = page["resolutions"].as_array().unwrap().iter().map(|row| row["original"].as_str().unwrap()).collect();
    assert_eq!(originals,HashSet::from(["Progresive Metal","prog metal","heavy-metal"]));
    assert!(!page["items"].as_array().unwrap().iter().any(|row|row["name"]=="Death Metal"));
    assert_eq!(page["sources"].as_array().unwrap().len(),4);
    assert_eq!(page["excluded_sources"],json!(selected.excluded_sources));
    let filter = reader.discovery_filter_using(&selected,&index);
    let (_,snapshot) = reader.refresh_using(&filter,TrackOrder::Album,||true,
        ((1,vec![("alpha".into(),false,true),("beta".into(),false,true)]),index.clone())).unwrap();
    assert_eq!(snapshot.ids.into_iter().collect::<HashSet<_>>(),HashSet::from([1,4,-11,-12]));
    let within: HashSet<_> = snapshot.album_ids.into_iter().collect();
    assert_eq!(within,HashSet::from([1,-10]));
    assert_eq!(reader.albums_view_within("title","",&within,&index).ids.into_iter().collect::<HashSet<_>>(),within);
    let artists = crate::albums_view::artists::listed_within(Vec::new(),&index,&filter,false,Some(&within),|_|String::new());
    assert_eq!(artists.len(),1);
    assert_eq!(artists[0].artist.name,"Artist alpha");
    assert_eq!(artists[0].artist.album_count,1);
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}

#[test]
fn route_scope_reaches_collections_and_explicit_options_can_restore_all_sources() {
    let (reader,_) = fixture();
    let selected = json!({"kind":"genre","name":"Metal","excluded_sources":["server:home"]}).to_string();
    let revision = reader.lib.read_revision().unwrap();
    assert_eq!(reader.discovery_snapshot(&selected).ids.into_iter().collect::<HashSet<_>>(), HashSet::from([1,4]));
    let mut ids=Vec::new();
    reader.collection_tracks("discovery",&selected,&Filter::default(),|found|ids=found.snapshot.ids);
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>(),HashSet::from([1,4]));
    assert_eq!(reader.paths(&ids).into_iter().collect::<HashSet<_>>(),HashSet::from(["/one".to_string(),"/loose".to_string()]));
    assert_eq!(reader.collection_albums("discovery","title","","",&Filter::default(),&selected).ids,[1]);
    let artists=reader.discovery_artists(&selected);
    assert_eq!(artists.len(),1);
    assert_eq!(artists[0].artist.track_count,1,"the other source shares album 1 but its track is excluded");
    let inherited=reader.discovery_page_options("genres",&selected,"",r#"{"scope":"library"}"#,0);
    assert_eq!(inherited["summary"]["albums"],1);
    assert_eq!(inherited["resolutionTotal"],1);
    let all=reader.discovery_page_options("genres",&selected,"",r#"{"excluded_sources":[]}"#,0);
    assert_eq!(all["summary"]["albums"],2);
    assert_eq!(all["excluded_sources"],json!([]));
    let blocked=reader.discovery_page_options("genres","","",r#"{"scope":"tags","excluded_sources":["local:default","server:home"]}"#,0);
    assert_eq!(blocked["items"],json!([]));
    assert!(reader.discovery_page_options("genres",&selected,"",r#"{"excluded_sources":"wrong"}"#,0)["error"].is_string());
    assert!(reader.discovery_page_options("genres","bad json","","{}",0)["error"].is_string());
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}

#[test]
fn album_genre_links_preserve_exact_edition_sources_and_original_labels() {
    let (reader,index)=fixture();
    reader.lib.set_track_genres(1,&["heavy-metal".into(),"Progresive Metal".into(),"My Sunday collection".into()]).unwrap();
    let before=reader.lib.track_genres(1).unwrap();
    let revision=reader.lib.read_revision().unwrap();
    let links=reader.album_genre_links_using(1,&index);
    let entries=links["items"].as_array().unwrap();
    assert_eq!(entries.len(),4,"all original tags of both contributing sources are linked");
    let route=|name:&str| reader.resolve_album_genre_using(Selection::parse(entries.iter().find(|row|row["name"]==name).unwrap()["key"].as_str().unwrap()).unwrap(),&index).unwrap();
    assert_eq!(route("heavy-metal").name,"Heavy Metal");
    assert_eq!(route("heavy-metal").excluded_sources,["plugin:alpha","plugin:beta"]);
    assert_eq!(route("Progresive Metal").name,"Progresive Metal","fuzzy decisions keep their original route evidence");
    let custom=route("My Sunday collection");
    assert_eq!(custom.name,"My Sunday collection");
    let custom_page=reader.discovery_page("genres",&serde_json::to_string(&custom).unwrap(),"",0);
    assert_eq!(custom_page["parents"],json!([]));
    assert_eq!(custom_page["summary"]["albums"],1);
    let remote=reader.album_genre_links_using(-10,&index);
    assert_eq!(remote["items"].as_array().unwrap().len(),2);
    let remote_key=reader.resolve_album_genre_using(Selection::parse(remote["items"][0]["key"].as_str().unwrap()).unwrap(),&index).unwrap();
    assert_eq!(remote_key.excluded_sources,vec!["local:default","plugin:beta","server:home"]);
    let second=reader.album_genre_links_using(2,&index);
    let second_key=reader.resolve_album_genre_using(Selection::parse(second["items"][0]["key"].as_str().unwrap()).unwrap(),&index).unwrap();
    assert_eq!(second_key.excluded_sources,["local:default","plugin:alpha","plugin:beta"]);
    let page=reader.album_page(1,true).unwrap();
    assert!(!page.heading.meta.contains("heavy-metal"));
    assert_eq!(serde_json::from_str::<Value>(&page.genre_links).unwrap()["items"].as_array().unwrap().len(),4);
    assert_eq!(reader.lib.track_genres(1).unwrap(),before);
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}

#[test]
fn album_genre_links_merge_only_strong_identities_and_preserve_every_spelling() {
    let (reader, _) = fixture();
    let tags: Vec<String> = ["Heavy Metal", "heavy-metal", "heavy metal", "dnb", "Drum and Bass",
        "Progresive Metal", "Progressive Metal", "Personal-Shelf", "personal shelf", "Personal Corner", "Wardcore", "wardcore"]
        .map(String::from).into();
    reader.lib.set_track_genres(1, &tags).unwrap();
    let before = reader.lib.track_genres(1).unwrap();
    let revision = reader.lib.read_revision().unwrap();
    let remote = |genres| SourceItem { track:-11, album:-10, plugin:"alpha".into(), genres, ..Default::default() };
    let index = SourceIndex::new(vec![remote(tags.clone()), remote(vec!["Heavy Metal".into()])], Vec::new());
    let mut reversed = tags.clone();
    reversed.reverse();
    let reordered = SourceIndex::new(vec![remote(reversed)], Vec::new());
    assert_eq!(reader.album_genre_links_using(-10, &index), reader.album_genre_links_using(-10, &reordered));
    for album in [1, -10] {
        let links = reader.album_genre_links_using(album, &index);
        let entries = links["items"].as_array().unwrap();
        let route = |row: &Value| reader.resolve_album_genre_using(Selection::parse(row["key"].as_str().unwrap()).unwrap(),&index).unwrap().name;
        let heavy: Vec<_> = entries.iter().filter(|row| route(row) == "Heavy Metal").collect();
        assert_eq!(heavy.len(), 1);
        assert_eq!(heavy[0]["name"], "Heavy Metal");
        let heavy_originals = if album > 0 { json!(["Heavy Metal", "heavy-metal"]) }
            else { json!(["Heavy Metal", "heavy metal", "heavy-metal"]) };
        assert_eq!(heavy[0]["originalsCount"], heavy_originals.as_array().unwrap().len());
        let aliases: Vec<_> = entries.iter().filter(|row| route(row) == "Drum and Bass").collect();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0]["name"], "dnb");
        assert_eq!(aliases[0]["originalsCount"], 2);
        for name in ["Progresive Metal", "Progressive Metal", "Personal Corner"] {
            let row = entries.iter().find(|row| row["name"] == name).unwrap();
            assert_eq!(route(row), name);
            assert_eq!(row["originalsCount"], 1);
        }
        let originals: u64 = entries.iter().filter(|row|row["name"] != "Jazz")
            .map(|row|row["originalsCount"].as_u64().unwrap()).sum();
        // SQLite already retains one spelling for case-only duplicates.
        let expected = if album > 0 { &before } else { &tags };
        assert_eq!(originals as usize,expected.len());
        assert_eq!(entries.len(), if album > 0 { 8 } else { 7 });
        let custom = entries.iter().find(|row| row["name"] == "personal shelf").unwrap();
        assert_eq!(custom["originalsCount"], 2);
        assert_eq!(route(custom), "personal shelf");
        assert!(genres::parents(&route(custom)).is_empty());
        let uncertain = entries.iter().find(|row| row["name"] == "Wardcore").unwrap();
        assert_eq!(route(uncertain), "Wardcore");
        assert_eq!(uncertain["originalsCount"], if album > 0 { 1 } else { 2 });
        assert!(genres::parents(&route(uncertain)).is_empty());
    }
    assert_eq!(reader.lib.track_genres(1).unwrap(), before);
    assert_eq!(reader.lib.read_revision().unwrap(), revision);
}

#[test]
fn excluded_preferred_editions_do_not_hide_eligible_genre_albums() {
    let (reader,_) = fixture();
    reader.lib.connection().execute_batch("UPDATE albums SET title='First (Remastered)',album_artist='Player' WHERE id=2;").unwrap();
    reader.lib.set_track_genres(2,&["Progresive Metal".into()]).unwrap();
    assert!(reader.lib.select_album_version(1,2).unwrap());
    let key=json!({"kind":"genre","name":"Metal","excluded_sources":["server:home"]}).to_string();
    assert_eq!(reader.discovery_albums(&key,"").ids,[1]);
    assert_eq!(reader.discovery_page("genres",&key,"",0)["summary"]["albums"],1);
}

#[test]
fn source_route_limits_accept_many_real_keys_and_reject_oversized_filters() {
    let keys: Vec<String> = (0..64).map(|at| format!("plugin:{at}:{}","x".repeat(192))).collect();
    let key=json!({"kind":"genre","name":"Metal","excluded_sources":keys}).to_string();
    assert!(key.len()>8192 && key.len()<65536);
    assert_eq!(Selection::parse(&key).unwrap().excluded_sources.len(),64);
    assert!(options::Options::parse(&json!({"excluded_sources":vec!["x";257]}).to_string()).is_none());
    assert!(Selection::parse(&json!({"excluded_sources":["x".repeat(513)]}).to_string()).is_none());
    assert!(options::Options::parse(&" ".repeat(65537)).is_none());
}

#[test]
fn column_selections_narrow_a_genre_and_its_choices_stay_within_it() {
    let (reader,index) = fixture();
    let sheet = |selection: &str| ["", "", "", "", selection].join("\u{1f}");
    let key = r#"{"kind":"genre","name":"Metal","excluded_sources":["server:home","plugin:beta"]}"#;
    let stamp = (1,vec![("alpha".into(),false,true),("beta".into(),false,true)]);
    let narrowed = reader.sheet_filter(&format!("{key}{SHEET}{}", sheet(r#"{"artist":["Player"]}"#)),&Filter::default(),&index).unwrap();
    let (_,snapshot) = reader.refresh_using(&narrowed,TrackOrder::Album,||true,(stamp.clone(),index.clone())).unwrap();
    assert_eq!(snapshot.ids,vec![1],"only the selected artist's tracks of the genre");
    let whole = reader.sheet_filter(&format!("{key}{SHEET}{}", sheet("")),&Filter::default(),&index).unwrap();
    let (_,snapshot) = reader.refresh_using(&whole,TrackOrder::Album,||true,(stamp,index.clone())).unwrap();
    assert_eq!(snapshot.ids.into_iter().collect::<HashSet<_>>(),HashSet::from([1,4,-11,-12]),"no selection keeps the genre whole");
    let choices = |query: String| reader.track_column(&query,&Filter::default(),"artist","","")["items"].as_array().unwrap()
        .iter().map(|item| item["key"].as_str().unwrap().to_string()).collect::<HashSet<_>>();
    assert_eq!(choices(format!(r#"{{"kind":"genre","name":"Jazz"}}{SHEET}{}"#, sheet(""))),HashSet::from(["Player".to_string()]));
    assert!(choices(sheet("")).len() > 1,"the whole library offers more artists than the genre");
    assert!(reader.sheet_filter(&format!(r#"{{"kind":"genre","name":""}}{SHEET}{}"#, sheet("")),&Filter::default(),&index).is_none());
    assert!(reader.discovery_snapshot_with(&format!(r#"{{"kind":"genre","name":"Jazz"}}{SHEET}{}"#, sheet(r#"{"artist":["Nobody"]}"#)),&Filter::default()).ids.is_empty());
}
