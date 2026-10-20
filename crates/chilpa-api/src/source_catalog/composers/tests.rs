use super::*;
use crate::library_sources::{SourceIndex, Stars};
use std::sync::{atomic::{AtomicUsize, Ordering}, Mutex};

const RECORDING: &str = "00000000-0000-4000-8000-000000000001";
const WORK: &str = "00000000-0000-4000-8000-000000000002";
const ISRC: &str = "USABC1200001";

struct Fixture {
    calls: AtomicUsize,
    replies: Mutex<std::collections::VecDeque<Value>>,
}
impl Fixture {
    fn new(replies: Vec<Value>) -> Self { Self { calls: AtomicUsize::new(0), replies: Mutex::new(replies.into()) } }
}
impl Http for Fixture {
    fn get(&self, _: &str, _: &str, timeout: Duration, bytes: usize) -> Result<Reply, String> {
        assert_eq!(timeout, Duration::from_secs(6));
        assert_eq!(bytes, 4 * 1024 * 1024);
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Reply { status: 200, headers: Default::default(),
            body: serde_json::to_vec(&self.replies.lock().unwrap().pop_front().unwrap_or(json!({"recordings":[]}))).unwrap() })
    }
}

fn answers() -> Vec<Value> {
    vec![
        json!({"recordings":[{"id":RECORDING}]}),
        json!({"relations":[{"type":"performance","work":{"id":WORK}}]}),
        json!({"relations":[{"type":"composer","artist":{"name":"Composer"}}]}),
    ]
}

fn track(catalog: &Catalog, reference: &str, isrc: &str, composers: &[&str]) -> i64 {
    let mut inner = catalog.lock();
    let id = inner.store.track_id(&format!("plugin:fixture:{reference}"));
    let row = inner.store.tracks.get_mut(&id).unwrap();
    row.plugin = "fixture".into();
    row.title = reference.into();
    row.playable = true;
    row.metadata = json!({"kind":"track","ref":reference,"isrc":isrc,"composers":composers});
    id
}

fn pending(catalog: &Catalog) -> Vec<(String, String)> {
    let inner = catalog.lock();
    inner.composers.pending(&inner.store, super::super::now())
}

#[test]
fn opt_out_hidden_windows_and_empty_queues_start_no_worker_or_io() {
    let mut app = crate::Chilpa::new();
    let catalog = app.catalog.clone();
    track(&catalog, "track:1", ISRC, &[]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("composers.json");
    let http = Fixture::new(vec![]);
    let requested = [("fixture".into(), ISRC.into())];
    catalog.advance_composers();
    assert!(!catalog.lock().composers.running);
    assert!(pending(&catalog).is_empty());
    catalog.composer_pass(&requested, &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0);
    assert!(!path.exists());
    catalog.composer_source("fixture", true);
    app.set_window_state(false, false);
    catalog.advance_composers();
    catalog.composer_pass(&requested, &http, &path, false);
    assert!(!catalog.lock().composers.running);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0);
    assert!(!path.exists());
    app.set_window_state(true, true);
    catalog.lock().store.tracks.retain(|_, _| false);
    for _ in 0..100 { catalog.advance_composers(); }
    assert!(!catalog.lock().composers.running);
}

#[test]
fn a_pass_has_sixty_tracks_and_skips_duplicates_invalid_isrcs_and_source_credits() {
    let catalog = Catalog::new();
    catalog.composer_source("fixture", true);
    for n in 0..65 { track(&catalog, &format!("track:{n}"), &format!("USABC{n:07}"), &[]); }
    track(&catalog, "track:duplicate", "usabc0000000", &[]);
    track(&catalog, "track:own", ISRC, &["Source composer"]);
    track(&catalog, "track:invalid", "invalid", &[]);
    let first = pending(&catalog);
    assert_eq!(first.len(), PASS);
    let http = Fixture::new(vec![]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("composers.json");
    let oversized: Vec<_> = (0..65).map(|n| ("fixture".into(), format!("USABC{n:07}"))).collect();
    catalog.composer_pass(&oversized, &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), PASS);
    assert!(!catalog.lock().composers.running);
    catalog.composer_pass(&oversized, &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), PASS);
    let remaining = pending(&catalog);
    assert_eq!(remaining.len(), 5);
    catalog.composer_pass(&remaining, &http, &path, false);
    assert!(pending(&catalog).is_empty());
    let revision = catalog.revisions();
    for _ in 0..100 { catalog.advance_composers(); }
    assert_eq!(http.calls.load(Ordering::Relaxed), 65);
    assert!(!catalog.lock().composers.running);
    assert_eq!(catalog.revisions(), revision);
}

#[test]
fn hiding_or_opting_out_stops_each_stage_and_discards_in_flight_results() {
    struct Stop<'a> { catalog: &'a Catalog, http: Fixture, after: usize, disable: bool }
    impl Http for Stop<'_> {
        fn get(&self, url: &str, accept: &str, timeout: Duration, bytes: usize) -> Result<Reply, String> {
            let reply = self.http.get(url, accept, timeout, bytes);
            if self.http.calls.load(Ordering::Relaxed) == self.after {
                if self.disable { self.catalog.composer_source("fixture", false); }
                else { self.catalog.composer_window(false); }
            }
            reply
        }
    }
    for disable in [false, true] {
        for after in 1..=3 {
            let catalog = Catalog::new();
            catalog.composer_source("fixture", true);
            let http = Stop { catalog: &catalog, http: Fixture::new(answers()), after, disable };
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("composers.json");
            catalog.composer_pass(&[("fixture".into(), ISRC.into())], &http, &path, false);
            assert_eq!(http.http.calls.load(Ordering::Relaxed), after);
            let inner = catalog.lock();
            assert!(!inner.composers.running);
            assert!(inner.composers.cache.as_ref().is_none_or(|c| c.get(ISRC, super::super::now()).is_none()));
            assert!(!path.exists());
        }
    }
}

#[test]
fn the_worker_shares_work_reads_persists_results_and_leaves_other_sources_alone() {
    let catalog = Catalog::new();
    let id = track(&catalog, "track:1", ISRC, &[]);
    catalog.composer_source("fixture", true);
    let mut replies = answers();
    replies.extend(answers().into_iter().take(2));
    let http = Fixture::new(replies);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("composers.json");
    let revision = catalog.revisions();
    catalog.composer_pass(&[("fixture".into(), ISRC.into()), ("fixture".into(), "USABC1200002".into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 5);
    assert_ne!(catalog.revisions(), revision);
    assert_eq!(catalog.tracks_by_ids(&[id])[0].0.metadata["composers"], json!(["Composer"]));
    assert_eq!(Cache::read(&path, super::super::now()).get(ISRC, super::super::now()).unwrap(), ["Composer"]);
    catalog.lock().store.tracks.get_mut(&id).unwrap().plugin = "other".into();
    assert_eq!(catalog.tracks_by_ids(&[id])[0].0.metadata["composers"], json!([]));
    let revision = catalog.revisions();
    catalog.composer_pass(&[("fixture".into(), "USABC1200003".into())], &http, &path, false);
    assert_eq!(catalog.revisions(), revision, "an absent credit does not redraw the library");
    catalog.composer_source("fixture", false);
    assert!(catalog.lock().composers.cache.is_none());
}

#[test]
fn errors_are_not_negative_cache_entries_and_back_off_before_retrying() {
    let catalog = Catalog::new();
    catalog.composer_source("fixture", true);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("composers.json");
    catalog.composer_pass(&[("fixture".into(), ISRC.into())], &Fixture::new(vec![json!({})]), &path, false);
    let inner = catalog.lock();
    assert!(inner.composers.retry_at >= super::super::now() + RETRY - 1);
    assert!(inner.composers.cache.as_ref().unwrap().get(ISRC, super::super::now()).is_none());
    assert!(!inner.composers.running);
    assert!(!path.exists());
}

#[test]
fn cached_composers_reach_rows_facets_queries_and_both_groupings_with_source_credits_first() {
    let catalog = Catalog::new();
    let missing = track(&catalog, "track:1", ISRC, &[]);
    let second = track(&catalog, "track:2", ISRC, &[]);
    let own = track(&catalog, "track:3", ISRC, &["Source composer"]);
    let minority = track(&catalog, "track:4", ISRC, &["Minority composer"]);
    let (album, own_album) = {
        let mut inner = catalog.lock();
        let album = inner.store.take_item("fixture", &json!({"kind":"album","ref":"album:1","title":"Record"}), 1).unwrap();
        let own_album = inner.store.take_item("fixture", &json!({"kind":"album","ref":"album:2","title":"Other"}), 1).unwrap();
        inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![minority, missing, second]);
        inner.store.cards.get_mut(&own_album).unwrap().tracks = Some(vec![own]);
        for id in [missing, second, minority] { inner.store.tracks.get_mut(&id).unwrap().album_id = album; }
        inner.store.tracks.get_mut(&own).unwrap().album_id = own_album;
        inner.store.shelves.insert("fixture section:albums".into(), super::super::store::Shelf {
            cards: vec![album, own_album], ..Default::default()
        });
        inner.store.shelves.insert("fixture section:tracks".into(), super::super::store::Shelf {
            tracks: vec![missing, second, own, minority], ..Default::default()
        });
        (album, own_album)
    };
    catalog.composer_source("fixture", true);
    let before = [album, own_album].map(|id| catalog.entity_revision(id));
    let signature = catalog.integrated_signature(&[("fixture".into(), true, true)]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("composers.json");
    let mut cache = Cache::default();
    cache.insert(ISRC, vec!["Composer".into(), "Second composer".into()], super::super::now());
    cache.write(&path).unwrap();
    let http = Fixture::new(vec![]);
    catalog.composer_pass(&[("fixture".into(), ISRC.into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0, "I reuse the disk cache before any request");
    assert_ne!(signature, catalog.integrated_signature(&[("fixture".into(), true, true)]));
    let after = [album, own_album].map(|id| catalog.entity_revision(id));
    assert_ne!(before[0], after[0]);
    assert_eq!(before[1], after[1], "a source credit keeps its page unchanged");
    let index = SourceIndex::new(catalog.integrated_items(&["fixture".into()], &["fixture".into()]), vec![]);
    let filter = crate::Filter { composers: vec!["Composer".into()], ..Default::default() };
    assert_eq!(index.items.iter().filter(|i| crate::library_sources::passes(i, &filter, &Stars::default())).count(), 2);
    let mut facets = crate::Facets::default();
    crate::library_sources::merge_facets(&mut facets, &crate::Filter::default(), &index, &Stars::default(), None);
    assert!(facets.composers.iter().any(|c| c.label == "Composer" && c.albums == 1));
    assert!(facets.composers.iter().any(|c| c.label == "Source composer" && c.albums == 1));
    use chilpa_library::store::query::{Field, Node, Values};
    let query = crate::Filter { query: Some(Node::Is(Field::Composer, Values::Any(vec!["Second composer".into()]))), ..Default::default() };
    assert_eq!(index.items.iter().filter(|i| crate::library_sources::passes(i, &query, &Stars::default())).count(), 2);
    let reader = crate::Reader::of(chilpa_library::Library::in_memory().unwrap());
    let grouped = reader.album_groups(vec![album, own_album], crate::library_sources::AlbumGroup::Composer, false, false, &index);
    assert_eq!(grouped.groups, [(0, "Composer".into()), (1, "Source composer".into())]);
    let tracks = reader.tracks_using("title\u{1f}composer\u{1f}", &crate::Filter::default(), &index);
    assert_eq!(tracks.snapshot.ids, [missing, second, minority, own]);
    assert_eq!(tracks.sections, [(0, "Composer".into()), (2, "Minority composer".into()), (3, "Source composer".into())]);
    for (item, key) in index.items.iter().zip(&index.keys) {
        assert_eq!(key.composer, if item.track == own { "Source composer" } else if item.track == minority { "Minority composer" } else { "Composer" });
    }
    let displayed = catalog.tracks_by_ids(&[missing, own]);
    assert_eq!(super::super::view::row_of(&displayed[0].0, String::new()).composer, "Composer, Second composer");
    assert_eq!(super::super::view::row_of(&displayed[1].0, String::new()).composer, "Source composer");
    assert_eq!(catalog.tracks(album).unwrap().iter().find(|t| t.id == missing).unwrap().metadata["composers"], json!(["Composer", "Second composer"]));
    assert_eq!(catalog.track_by_path(&displayed[0].0.path).unwrap().0.metadata["composers"], json!(["Composer", "Second composer"]));
    assert_eq!(catalog.lock().store.tracks[&missing].metadata["composers"], json!([]));
    for value in [json!({"isrc":ISRC,"composer":"Single"}), json!({"isrc":ISRC,"composer":"Single","composers":[]})] {
        catalog.lock().store.tracks.get_mut(&own).unwrap().metadata = value.clone();
        assert_eq!(catalog.tracks_by_ids(&[own])[0].0.metadata, value);
        assert_eq!(catalog.integrated_items(&[], &["fixture".into()]).iter().find(|i| i.track == own).unwrap().composers, ["Single"]);
    }
    catalog.composer_source("fixture", false);
    assert_ne!(catalog.entity_revision(album), after[0]);
    assert_eq!(catalog.tracks_by_ids(&[missing])[0].0.metadata["composers"], json!([]));
}
