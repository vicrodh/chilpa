//! Missing album metadata must not hide a track from any track surface.
use super::{Filter, Library, TrackOrder};
use super::query::{Field, Node, Values};
use rusqlite::params;

fn library() -> Library {
    let lib = Library::in_memory().unwrap();
    let local = lib.source_id("local", "default").unwrap();
    let other = lib.source_id("server", "hidden").unwrap();
    lib.connection().execute("UPDATE sources SET enabled=0 WHERE id=?1", [other]).unwrap();
    lib.connection().execute("INSERT INTO albums(id,title,album_artist,year) VALUES(1,'Record','Band',2001)", []).unwrap();
    for (id, album, artist, year, present, reachable, source) in [
        (1, Some(1), "Band", 2001, 1, 1, local),
        (2, None, "Solo", 1997, 1, 1, local),
        (3, None, "Solo", 1997, 1, 0, local),
        (4, None, "Gone", 1996, 0, 1, local),
        (5, None, "Hidden", 1998, 1, 1, other),
    ] {
        lib.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,release_year,present,reachable,duration_ms,added_at,bit_depth,sample_rate_hz)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1000,1,16,44100)",
            params![id, source, format!("/music/{id}.flac"), format!("Song {id}"), artist, album, year, present, reachable]).unwrap();
        lib.set_track_genres(id, &["Jazz".into()]).unwrap();
        lib.set_track_composers(id, &["Writer".into()]).unwrap();
    }
    lib
}

#[test]
fn albumless_tracks_keep_ids_rows_paths_availability_and_real_album_totals() {
    let lib = library();
    for grouped in [false, true] {
        lib.set_group_album_versions(grouped).unwrap();
        let all = lib.grouped_snapshot(&Filter::default(), TrackOrder::Title).unwrap();
        assert_eq!(all.ids, [1, 2, 3]);
        assert_eq!(all.album_ids, [1]);
        assert_eq!((all.totals.tracks, all.totals.albums, all.totals.duration_ms), (3, 1, 3000));
        let loose = lib.grouped_snapshot(&Filter { albumless: true, ..Default::default() }, TrackOrder::Title).unwrap();
        assert_eq!(loose.ids, [2, 3]);
        assert_eq!(loose.totals.albums, 0);
        assert!(loose.album_ids.is_empty());
        let rows = lib.tracks_by_ids(&loose.ids).unwrap();
        assert!(rows.iter().all(|row| row.album_id.is_none() && row.album.is_empty()));
        assert_eq!(rows.iter().map(|row| row.available).collect::<Vec<_>>(), [true, false]);
        let paths: Vec<_> = rows.iter().map(|row| row.external_id.clone()).collect();
        assert_eq!(lib.tracks_by_paths(&paths).unwrap(), rows.into_iter().map(Some).collect::<Vec<_>>());
    }
    assert_eq!(lib.folder_children("/music").unwrap().1.iter().map(|row| row.id).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(lib.find("Song", 20).unwrap().tracks.len(), 3);
    assert_eq!(lib.find("Writer", 20).unwrap().tracks.len(), 3);
}

#[test]
fn loose_tracks_contribute_to_columns_refine_and_split_artist_credits() {
    let lib = library();
    let loose = Filter { albumless: true, ..Default::default() };
    assert_eq!(lib.facet_years_matching(&loose).unwrap().iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["1997"]);
    assert_eq!(lib.facet_artists(&loose).unwrap().iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["Solo"]);
    assert!(lib.facet_albums(&loose).unwrap().is_empty());
    for filter in [
        Filter { artist: Some("Solo".into()), year: Some(1997), ..loose.clone() },
        Filter { queries: ["1997".into(), "Jazz".into(), "".into(), "Writer".into(), "".into(), "Solo".into(), "".into()], ..loose.clone() },
        Filter { formats: vec!["flac".into()], qualities: vec!["lossless".into()], ..loose.clone() },
    ] {
        assert_eq!(lib.browse_ids(&filter, TrackOrder::Title).unwrap().ids, [2, 3]);
        assert_eq!(lib.query_count(&filter).unwrap(), 2);
    }
    for text in ["artist:Solo year:1997", "title:Song", "quality:cd", "genre:Jazz composer:Writer", "-album:Record"] {
        let query = super::query::parse(text).unwrap();
        let filter = Filter { query: Some(query), ..loose.clone() };
        assert_eq!(lib.query_count(&filter).unwrap(), 2, "{text}");
        assert_eq!(lib.browse_ids(&filter, TrackOrder::Title).unwrap().ids, [2, 3], "{text}");
        assert_eq!(lib.query_counts(&[&filter]).unwrap(), [2]);
    }
    let (baseline, artists) = lib.query_facet_counts(&loose, None, Field::Artist, "so").unwrap();
    assert_eq!(baseline, 0);
    assert_eq!(artists, [("Solo".into(), 2)]);
    assert!(lib.query_vocabulary().unwrap().iter().find(|(f, _)| *f == Field::Artist).unwrap().1.contains(&"Solo".into()));
    lib.connection().execute("UPDATE tracks SET artist='Alpha & Beta' WHERE id=2", []).unwrap();
    lib.state_credit("Alpha & Beta", &["Alpha".into(), "Beta".into()]).unwrap();
    let filter = Filter { query: Some(Node::Is(Field::Artist, Values::Any(vec!["Beta".into()]))), ..loose };
    assert_eq!(lib.browse_ids(&filter, TrackOrder::Title).unwrap().ids, [2]);
    assert_eq!(lib.query_facet_counts(&filter, None, Field::Artist, "beta").unwrap().1, [("Beta".into(), 1)]);
}

#[test]
fn keyed_order_matches_sql_with_missing_records_and_each_group() {
    let lib = library();
    for group in ["", "album", "artist", "year", "decade", "genre", "composer", "name"] {
        for order in [TrackOrder::Artist, TrackOrder::ArtistDescending, TrackOrder::Year, TrackOrder::YearDescending, TrackOrder::Album, TrackOrder::Title] {
            let filter = Filter { grouped_by: group.into(), ..Default::default() };
            let (snapshot, keys) = lib.keyed_snapshot(&filter, order).unwrap();
            assert_eq!(snapshot.ids.len(), 3);
            assert!(keys.windows(2).all(|pair| !order.compare(group, &pair[0], &pair[1]).is_gt()), "{group} {order:?}");
        }
    }
}
