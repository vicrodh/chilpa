use super::*;
use crate::Filter;
use serde_json::json;
use std::time::Instant;

const LEGACY_RELEASES: &str = "SELECT DISTINCT t.album_id IS NULL, coalesce(t.album_id,t.id),
    g.name, coalesce(a.album_artist,t.artist), coalesce(
        (SELECT aw.path FROM album_artwork aw WHERE aw.album_id=a.id), a.artwork_path,t.release_artwork)
    FROM tracks t JOIN sources s ON s.id=t.source_id AND s.enabled=1
    LEFT JOIN albums a ON a.id=t.album_id
    JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id
    WHERE t.present=1 ORDER BY 1,2,3";

type Association = (bool, i64, String, String);
fn associations(lib: &Library) -> Vec<Association> {
    let mut rows = Vec::new();
    lib.visit_genre_releases(|loose, id, tag, artist| rows.push((loose, id, tag.into(), artist.into()))).unwrap();
    rows
}

fn legacy(lib: &Library) -> Vec<Association> {
    lib.db.prepare(LEGACY_RELEASES).unwrap().query_map([], |row|
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

#[test]
fn discovery_names_and_associations_keep_original_tags_and_visibility() {
    let lib = Library::in_memory().unwrap();
    lib.db.execute_batch("INSERT INTO sources(id,provider,instance,enabled) VALUES (1,'local','on',1),(2,'local','off',0);
        INSERT INTO albums(id,title,album_artist,artwork_path) VALUES (1,'Album','Ensemble','/cover');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,present,added_at,release_artwork) VALUES
        (1,1,'/a','First','Player',1,1,1,'/disc1'),(2,1,'/b','Second','Player',1,1,1,'/disc2'),
        (3,1,'/c','Single','Soloist',NULL,1,1,'/single'),(4,1,'/d','Absent','Missing',NULL,0,1,NULL),
        (5,2,'/e','Disabled','Hidden',NULL,1,1,NULL);
        INSERT INTO genres(id,name) VALUES (1,'Électro custom'),(2,'électro custom'),(3,'Gone'),(4,'Hidden'),(5,'Unassigned');
        INSERT INTO track_genres(track_id,genre_id) VALUES (1,1),(2,1),(2,2),(3,1),(4,3),(5,4);").unwrap();
    let expected = lib.facet_genres(&Filter::default()).unwrap().into_iter().map(|entry| entry.label).collect::<Vec<_>>();
    let revision = lib.read_revision().unwrap();
    lib.db.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(lib.genre_names().unwrap(), expected);
    assert_eq!(expected.len(), 2);
    let rows = associations(&lib);
    assert_eq!(rows, vec![(false,1,"Électro custom".into(),"Ensemble".into()),
        (false,1,"électro custom".into(),"Ensemble".into()),(true,3,"Électro custom".into(),"Soloist".into())]);
    assert_eq!(rows, legacy(&lib));
    assert_eq!(lib.read_revision().unwrap(), revision);
}

fn fixture() -> Library {
    let lib = Library::in_memory().unwrap();
    lib.db.execute_batch("BEGIN;
        INSERT INTO sources(id,provider,instance) VALUES (1,'local','default');
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<2000)
        INSERT INTO albums(id,title,album_artist,artwork_path)
        SELECT i,printf('Record %04d',i),printf('Artist %04d',i),printf('/art/%d',i) FROM n;
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<20000)
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at,bit_depth,sample_rate_hz)
        SELECT i,1,printf('/track/%05d.flac',i),'Song','Player',(i-1)/10+1,1,16,44100 FROM n;
        INSERT INTO genres(id,name) VALUES (1,'Jazz'),(2,'Technical Death Metal');
        INSERT INTO track_genres(track_id,genre_id) SELECT id,(album_id%2)+1 FROM tracks;
        COMMIT; ANALYZE;
        WITH RECURSIVE n(i) AS (SELECT 3 UNION ALL SELECT i+1 FROM n WHERE i<502)
        INSERT INTO genres(id,name) SELECT i,printf('Unused %03d',i) FROM n;").unwrap();
    lib
}

#[test]
fn twenty_thousand_tracks_keep_release_counts_with_read_only_queries() {
    let mut samples = Vec::new();
    for round in 0..3 {
        let lib = fixture();
        let revision = lib.read_revision().unwrap();
        let mut old = Vec::new();
        let mut current = Vec::new();
        for candidate in if round % 2 == 0 { [false,true] } else { [true,false] } {
            let start = Instant::now();
            let rows = if candidate { associations(&lib) } else { legacy(&lib) };
            samples.push(json!({"round":round,"stage":if candidate {"associations"} else {"legacy_artwork_associations"},"ms":start.elapsed().as_secs_f64()*1000.0,"rows":rows.len()}));
            if candidate { current = rows; } else { old = rows; }
        }
        assert_eq!(current, old);
        assert_eq!(current.len(), 2000);
        for names_only in if round % 2 == 0 { [false,true] } else { [true,false] } {
            let start = Instant::now();
            let names = if names_only { lib.genre_names().unwrap() } else {
                lib.facet_genres(&Filter::default()).unwrap().into_iter().map(|entry| entry.label).collect()
            };
            samples.push(json!({"round":round,"stage":if names_only {"names"} else {"facets"},"ms":start.elapsed().as_secs_f64()*1000.0}));
            assert_eq!(names, ["Jazz","Technical Death Metal"]);
        }
        let mut names = lib.db.prepare(GENRE_NAMES).unwrap();
        assert_eq!(names.query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap().len(), 2);
        let steps = names.get_status(rusqlite::StatementStatus::VmStep);
        assert!(steps < 20_000, "unassigned genres must not each scan 20,000 tracks: {steps} VM steps");
        samples.push(json!({"round":round,"stage":"names_work","ms":0,"vm_steps":steps,"unassigned_genres":500}));
        let ids = (1..=2000).collect::<Vec<_>>();
        for warm in [false,true] {
            let start = Instant::now();
            assert_eq!(lib.grouped_album_ids(&ids).unwrap(), ids);
            samples.push(json!({"round":round,"stage":"grouped_album_ids","warm":warm,"ms":start.elapsed().as_secs_f64()*1000.0}));
        }
        assert_eq!(lib.read_revision().unwrap(), revision);
    }
    if let Some(directory) = std::env::var_os("CHILPA_DISCOVERY_MEASUREMENT_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("library.json"),
            serde_json::to_vec_pretty(&json!({"tracks":20000,"albums":2000,"unassigned_genres":500,"mode":"test-profile, in-memory SQLite, first and repeated connection reads","samples":samples})).unwrap()).unwrap();
    }
}

#[test]
fn source_scope_precedes_name_resolution_and_release_deduplication() {
    let lib = Library::in_memory().unwrap();
    lib.db.execute_batch("INSERT INTO sources(id,provider,instance,enabled) VALUES (1,'local','default',1),(2,'server','home',1),(3,'server','off',0);
        INSERT INTO albums(id,title,album_artist) VALUES (1,'Shared','Player'),(2,'Outside','Other');
        INSERT INTO tracks(id,source_id,external_id,title,album_id,present,added_at) VALUES
        (1,1,'/one','One',1,1,1),(2,2,'/two','Two',1,1,1),(3,2,'/three','Three',2,1,1),
        (4,1,'/four','Loose',NULL,1,1),(5,3,'/five','Hidden',1,1,1);").unwrap();
    for (id, tag) in [(1,"Progresive Metal"),(2,"Jazz"),(3,"Technical Death Metal"),(4,"Progresive Metal"),(5,"Hidden")] {
        lib.set_track_genres(id, &[tag.into()]).unwrap();
    }
    let revision = lib.read_revision().unwrap();
    lib.db.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(lib.genre_names_excluding(&["server:home".into()]).unwrap(), ["Progresive Metal"]);
    assert_eq!(lib.genre_names_excluding(&["local:default".into()]).unwrap(), ["Jazz","Technical Death Metal"]);
    let mut scoped = Vec::new();
    lib.visit_genre_releases_excluding(&["server:home".into()], |loose,id,tag,_| scoped.push((loose,id,tag.to_string()))).unwrap();
    assert_eq!(scoped, [(false,1,"Progresive Metal".into()),(true,4,"Progresive Metal".into())]);
    assert_eq!(lib.album_genre_sources(1).unwrap(), (vec!["Jazz".into(),"Progresive Metal".into()],vec!["local:default".into(),"server:home".into()]));
    assert_eq!(lib.album_genre_sources(999).unwrap(), (Vec::new(),Vec::new()));
    assert_eq!(lib.genre_names_excluding(&["server:home' OR 1=1 --".into()]).unwrap(),lib.genre_names().unwrap());
    assert_eq!(lib.read_revision().unwrap(),revision);
}
