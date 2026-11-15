use super::*;
use std::{
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct Source {
    calls: Mutex<Vec<Value>>,
    revision: AtomicU64,
    generation: AtomicU64,
    offline: AtomicBool,
    updating: AtomicBool,
    personal: AtomicBool,
    repeat: AtomicBool,
    blocked: Mutex<bool>,
    wake: Condvar,
    active: AtomicUsize,
    maximum: AtomicUsize,
    pages: usize,
}
impl Source {
    fn new(pages: usize) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            revision: AtomicU64::new(1),
            generation: AtomicU64::new(1),
            offline: AtomicBool::new(false),
            updating: AtomicBool::new(false),
            personal: AtomicBool::new(false),
            repeat: AtomicBool::new(false),
            blocked: Mutex::new(false),
            wake: Condvar::new(),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            pages,
        })
    }
    fn release(&self) {
        *self.blocked.lock().unwrap() = false;
        self.wake.notify_all();
    }
}
impl worker::PageSource for Source {
    fn generation(&self, _: &str) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn page(&self, _: &str, params: Value) -> Result<Value, worker::Failure> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        self.calls.lock().unwrap().push(params.clone());
        let mut blocked = self.blocked.lock().unwrap();
        while *blocked {
            blocked = self.wake.wait(blocked).unwrap();
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        if self.offline.load(Ordering::SeqCst) {
            return Err(worker::Failure::Offline("offline".into()));
        }
        let reference = params["ref"].as_str().unwrap();
        let page = if reference == "base" {
            0
        } else {
            reference
                .split(':')
                .nth(1)
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };
        if page > 0 {
            assert!(
                params.get("filters").is_none() && params.get("order").is_none(),
                "opaque continuations keep their own query"
            );
        }
        let count = if page == 0 { 37 } else { 100 };
        let rows = (0..count)
            .map(|row| {
                json!({"kind":"track","ref":format!("track:{page}:{row}"),"title":"Same title",
            "artist":"Fixture","duration_ms":1000,"favorite":false,"available":true})
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"layout":"collection","title":"Fixture","items":rows,"total":37+100*(self.pages-1),
            "personal":self.personal.load(Ordering::SeqCst),"revision":self.revision.load(Ordering::SeqCst),"next":(page+1<self.pages).then(||if self.repeat.load(Ordering::SeqCst) { reference.to_string() } else { cursor(page+1) }),
            "updating":self.updating.load(Ordering::SeqCst),"refresh_after_ms":1000,"request_pending":false}),
        )
    }
}
fn cursor(page: usize) -> String {
    let prefix = format!("cursor:{page:05}:");
    format!("{prefix}{}", "x".repeat(4096 - prefix.len()))
}
fn window(source: Arc<Source>) -> (tempfile::TempDir, NativeCollectionWindow, Arc<Catalog>) {
    let directory = tempfile::tempdir().unwrap();
    let catalog = crate::source_catalog::collection_window::ready_test_catalog();
    let broker = Broker::with_source(
        source,
        catalog.clone(),
        directory.path().join("collections.sqlite"),
    );
    (
        directory,
        NativeCollectionWindow { broker, session: 0 },
        catalog,
    )
}
fn until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < end, "collection operation timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn answer(window: &mut NativeCollectionWindow, request: u64) -> Value {
    let mut found = None;
    until(|| {
        found = window.poll();
        found.is_some()
    });
    let value = found.unwrap();
    assert_eq!(value["requestId"], request);
    assert!(value.to_string().len() <= CHUNK_BYTES);
    value
}
fn query() -> store::Query {
    store::Query {
        id: 0,
        generation: 1,
        plugin: "fixture".into(),
        params: json!({"ref":"base"}),
        header: json!({}),
        revision: Some("version".into()),
        host: "host".into(),
        personal: false,
        known: 0,
        pages: 0,
        next: None,
        total: None,
        anchor: 0,
        restart: None,
    }
}

#[test]
fn byte_short_chunks_keep_absolute_offsets_and_every_row() {
    let query = query();
    let job = Job {
        session: 1,
        epoch: 1,
        request: 1,
        command: Command::Seek(0, 100),
    };
    let rows=(0..100).map(|at|json!({"kind":"track","ref":format!("track:{at:03}:{}","x".repeat(4086)),"title":"界".repeat(256),
        "artist":"界".repeat(256),"subtitle":"界".repeat(256),"label":"界".repeat(256),"image":"x".repeat(4096),"private":"x".repeat(10000)})).collect::<Vec<_>>();
    let wire = json!({"layout":"collection","items":rows});
    let page = store::Page {
        ordinal: 4,
        first: 237,
        count: 100,
        reference: None,
        hash: projection::content(&wire),
        next_hash: None,
    };
    let mut first = 237;
    let mut tokens = std::collections::HashSet::new();
    while first < 337 {
        let value = projection::chunk(&query, &job, &page, &wire, first, 100).unwrap();
        let count = value["page"]["count"].as_u64().unwrap() as usize;
        assert!(count > 0 && count < 100);
        assert_eq!(value["page"]["first"], first);
        assert_eq!(value["page"]["nextOffset"], first + count);
        assert!(value.to_string().len() <= CHUNK_BYTES);
        for row in value["page"]["rows"].as_array().unwrap() {
            assert!(row.get("private").is_none() && row.get("ref").is_none());
            let token = row["rowToken"].as_str().unwrap();
            assert!(token.len() <= 512 && tokens.insert(token.to_string()));
        }
        first += count;
    }
    assert_eq!(tokens.len(), 100);
}

#[test]
fn eviction_replay_and_actions_keep_the_original_page_and_close_releases_it() {
    let source = Source::new(12);
    let (_directory, mut window, catalog) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    let initial = answer(&mut window, 1);
    assert_eq!(initial["knownCount"], 37);
    let original = initial["page"]["rows"][0]["rowToken"]
        .as_str()
        .unwrap()
        .to_string();
    let mut known = 37;
    for request in 2..=12 {
        assert!(window.seek(session, known, 100, request));
        let value = answer(&mut window, request);
        assert_eq!(value["page"]["first"], known);
        known += 100;
        assert_eq!(value["knownCount"], known);
    }
    let before = source.calls.lock().unwrap().len();
    assert!(window.seek(session, 0, 100, 13));
    let first = answer(&mut window, 13);
    assert_eq!(first["page"]["rows"][0]["rowToken"], original);
    assert_eq!(
        source.calls.lock().unwrap().len(),
        before + 1,
        "evicted first page is fetched from its checkpoint"
    );
    assert!(window.prepare_action(session, &original, 14));
    let action = answer(&mut window, 14);
    assert_eq!(action["action"]["trackRow"], 0);
    assert_eq!(action["action"]["ref"], "track:0:0");
    let key = action["action"]["key"].as_str().unwrap();
    assert_eq!(
        catalog.shelf_tracks(key)[0].0.path,
        crate::source_opener::entry_of("fixture", "track:0:0")
    );
    assert!(window.broker.shared.lock().action.is_some());
    window.close(session);
    assert!(catalog.stored_shelf(key).is_none());
    assert!(window.broker.shared.lock().action.is_none());
    until(|| {
        catalog
            .track_by_path(&crate::source_opener::entry_of("fixture", "track:0:0"))
            .is_none()
    });
    assert!(!window.busy());
    assert_eq!(source.maximum.load(Ordering::SeqCst), 1);
}

#[test]
fn reopen_offline_and_heartbeat_keep_rows_but_changed_revisions_retire_tokens() {
    let source = Source::new(3);
    let (_directory, mut window, _) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    let initial = answer(&mut window, 1);
    assert!(window.seek(session, 37, 100, 2));
    answer(&mut window, 2);
    source.updating.store(true, Ordering::SeqCst);
    assert!(window.refresh(session, false, 3));
    let heartbeat = answer(&mut window, 3);
    assert_eq!(heartbeat["generation"], initial["generation"]);
    assert_eq!(heartbeat["knownCount"], 137);
    assert_eq!(heartbeat["header"]["updating"], true);
    assert_eq!(heartbeat["anchorIndex"], 37);
    window.close(session);
    source.offline.store(true, Ordering::SeqCst);
    let reopened = window.open("fixture native:base", 4);
    let offline = answer(&mut window, 4);
    assert_eq!(offline["offline"], true);
    assert_eq!(offline["knownCount"], 137);
    assert_eq!(offline["page"]["first"], 37);
    assert!(!offline["page"]["rows"].as_array().unwrap().is_empty());
    source.offline.store(false, Ordering::SeqCst);
    source.revision.store(2, Ordering::SeqCst);
    assert!(window.refresh(reopened, true, 5));
    let changed = answer(&mut window, 5);
    assert_ne!(changed["generation"], initial["generation"]);
    assert_eq!(changed["knownCount"], 37);
    assert!(window.prepare_action(
        reopened,
        initial["page"]["rows"][0]["rowToken"].as_str().unwrap(),
        6
    ));
    assert!(
        answer(&mut window, 6)["error"]
            .as_str()
            .unwrap()
            .contains("earlier")
    );
}

#[test]
fn source_events_and_public_history_preserve_the_large_visible_window() {
    let source = Source::new(14);
    let (_directory, mut window, catalog) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    let mut visible = answer(&mut window, 1);
    for request in 2..=12 {
        let first = visible["knownCount"].as_u64().unwrap() as usize;
        assert!(window.seek(session, first, 100, request));
        visible = answer(&mut window, request);
    }
    assert_eq!(visible["knownCount"], 1137);
    assert_eq!(visible["anchorIndex"], 1037);
    catalog.collection_epochs("other");
    let events: [fn(&Catalog); 6] = [
        |catalog| catalog.expire_personal_pages("other"),
        |catalog| catalog.expire_native_pages("other"),
        |catalog| catalog.mark_native_favorite("other", "track:0:0", true),
        |catalog| catalog.set_source_states([("other".into(), false)]),
        |catalog| catalog.forget_plugin("other"),
        |catalog| catalog.expire_personal_pages("fixture"),
    ];
    for (at, event) in events.into_iter().enumerate() {
        event(&catalog);
        let request = 20 + at as u64;
        assert!(window.refresh(session, false, request));
        let after = answer(&mut window, request);
        for field in [
            "knownCount",
            "anchorIndex",
            "generation",
            "dataRevision",
            "page",
        ] {
            assert_eq!(
                after[field], visible[field],
                "unrelated event {at} preserves {field}"
            );
        }
        assert_eq!(after["stale"], false);
    }
    // The same scope check applies to history arriving during a continuation.
    *source.blocked.lock().unwrap() = true;
    assert!(window.seek(session, 1137, 100, 30));
    until(|| source.active.load(Ordering::SeqCst) == 1);
    catalog.expire_personal_pages("fixture");
    catalog.expire_native_pages("other");
    source.release();
    let continued = answer(&mut window, 30);
    assert_eq!(continued["knownCount"], 1237);
    assert_eq!(continued["anchorIndex"], 1137);
    assert_eq!(continued["generation"], visible["generation"]);
    assert_eq!(continued["stale"], false);
}

#[test]
fn personal_history_and_same_source_mutations_retire_their_own_tokens() {
    let source = Source::new(3);
    source.personal.store(true, Ordering::SeqCst);
    let (_directory, mut window, catalog) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    let mut current = answer(&mut window, 1);
    let events: [fn(&Catalog); 5] = [
        |catalog| catalog.expire_personal_pages("fixture"),
        |catalog| catalog.expire_native_pages("fixture"),
        |catalog| catalog.mark_native_favorite("fixture", "track:0:0", true),
        |catalog| catalog.set_source_states([("fixture".into(), false)]),
        |catalog| catalog.forget_plugin("fixture"),
    ];
    for (at, event) in events.into_iter().enumerate() {
        let request = 10 + at as u64 * 3;
        assert!(window.seek(session, 37, 100, request));
        assert_eq!(answer(&mut window, request)["knownCount"], 137);
        event(&catalog);
        assert!(window.prepare_action(
            session,
            current["page"]["rows"][0]["rowToken"].as_str().unwrap(),
            request + 1
        ));
        let retired = answer(&mut window, request + 1);
        assert_eq!(retired["stale"], true);
        assert!(retired.get("action").is_none());
        catalog.set_source_states([("fixture".into(), true)]);
        assert!(window.refresh(session, false, request + 2));
        let refreshed = answer(&mut window, request + 2);
        assert_ne!(refreshed["generation"], current["generation"]);
        assert_eq!(refreshed["knownCount"], 37);
        current = refreshed;
    }
    // The persisted query still scopes history after reopening its window.
    window.close(session);
    let reopened = window.open("fixture native:base", 40);
    current = answer(&mut window, 40);
    catalog.expire_personal_pages("fixture");
    assert!(window.seek(reopened, 0, 100, 41));
    assert_eq!(answer(&mut window, 41)["stale"], true);
    assert!(window.refresh(reopened, false, 42));
    assert_ne!(answer(&mut window, 42)["generation"], current["generation"]);
}

#[test]
fn initial_personal_response_checks_history_changes_during_its_rpc() {
    let source = Source::new(2);
    source.personal.store(true, Ordering::SeqCst);
    *source.blocked.lock().unwrap() = true;
    let (_directory, mut window, catalog) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    until(|| source.active.load(Ordering::SeqCst) == 1);
    catalog.expire_personal_pages("fixture");
    source.release();
    let stale = answer(&mut window, 1);
    assert_eq!(stale["stale"], true);
    assert_eq!(stale["knownCount"], 0);
    assert!(stale.get("page").is_none());
    assert!(window.refresh(session, false, 2));
    assert_eq!(answer(&mut window, 2)["knownCount"], 37);
}

#[test]
fn close_cancels_two_coalesced_requests_without_polling_or_an_extra_rpc() {
    let source = Source::new(4);
    *source.blocked.lock().unwrap() = true;
    let (_directory, mut window, _) = window(source.clone());
    let session = window.open("fixture native:base", 1);
    until(|| source.active.load(Ordering::SeqCst) == 1);
    for request in 2..100 {
        assert!(window.seek(session, 0, 100, request));
    }
    assert!(window.refresh(session, false, 100));
    assert!(!window.prepare_action(session, "invalid", 101));
    assert_eq!(window.broker.shared.lock().pending.len(), 2);
    window.close(session);
    source.release();
    until(|| !window.broker.shared.lock().working);
    assert!(window.poll().is_none());
    assert_eq!(source.calls.lock().unwrap().len(), 1);
    assert!(window.broker.shared.lock().pending.is_empty());
}

#[test]
fn runtime_changes_during_rpc_and_old_handles_cannot_publish_into_new_sessions() {
    let source = Source::new(2);
    let (_directory, mut old, _) = window(source.clone());
    let first_session = old.open("fixture native:base", 1);
    answer(&mut old, 1);
    *source.blocked.lock().unwrap() = true;
    assert!(old.seek(first_session, 37, 100, 2));
    until(|| source.active.load(Ordering::SeqCst) == 1);
    source.generation.store(2, Ordering::SeqCst);
    source.release();
    assert_eq!(answer(&mut old, 2)["stale"], true);
    let mut next = NativeCollectionWindow {
        broker: old.broker.clone(),
        session: 0,
    };
    let new_session = next.open("fixture native:base", 3);
    old.close(first_session);
    let value = answer(&mut next, 3);
    assert_eq!(value["sessionId"], new_session);
    assert!(old.poll().is_none());
}

#[test]
fn ten_thousand_four_kib_cursors_stay_on_disk_with_bounded_checkpoints_and_journal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collections.sqlite");
    let store = store::Store::open(&path).unwrap();
    // This storage-layout test measures pages and journals, not fsync latency.
    store.db.pragma_update(None, "synchronous", "OFF").unwrap();
    let mut query = store.open_query("large", query()).unwrap();
    for ordinal in 0..10_000 {
        let reference = cursor(ordinal);
        let next = (ordinal + 1 < 10_000).then(|| cursor(ordinal + 1));
        let page = store::Page {
            ordinal,
            first: ordinal * 100,
            count: 100,
            reference: None,
            hash: projection::hash(format!("page {ordinal}").as_bytes()),
            next_hash: next.as_ref().map(|next| projection::hash(next.as_bytes())),
        };
        query.known += 100;
        query.pages += 1;
        query.next = next;
        store.append(&query, &reference, &page).unwrap();
    }
    let (count, bytes): (i64, i64) = store
        .db
        .query_row(
            "SELECT count(request_ref),sum(length(request_ref)) FROM pages",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((count, bytes), (1250, 1250 * 4096));
    assert_eq!(query.known, 1_000_000);
    for ordinal in [0, 1000, 9999] {
        let page = store
            .containing(&query, ordinal * 100 + 99)
            .unwrap()
            .unwrap();
        assert_eq!(page.ordinal, ordinal);
        let checkpoint = store.checkpoint(&query, ordinal).unwrap();
        assert!(ordinal - checkpoint.ordinal < store::CHECKPOINT);
        assert_eq!(checkpoint.reference.unwrap(), cursor(ordinal / 8 * 8));
    }
    assert_eq!(
        store
            .db
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        -512
    );
    assert!(std::fs::metadata(&path).unwrap().len() <= 31 * 1024 * 1024);
    let tx = store.db.unchecked_transaction().unwrap();
    tx.execute("UPDATE pages SET content_hash=upper(content_hash)", [])
        .unwrap();
    let journal = path.with_file_name("collections.sqlite-journal");
    let journal_bytes = std::fs::metadata(journal).map_or(0, |meta| meta.len());
    assert!(std::fs::metadata(&path).unwrap().len() + journal_bytes <= 64 * 1024 * 1024);
    tx.rollback().unwrap();
}

#[test]
fn full_cache_evicts_closed_queries_before_rejecting_the_active_window() {
    let directory = tempfile::tempdir().unwrap();
    let store = store::Store::open(&directory.path().join("collections.sqlite")).unwrap();
    store.db.pragma_update(None, "max_page_count", 128).unwrap();
    let old = store.open_query("old", query()).unwrap();
    for first in 0..6 {
        store
            .cache(
                &old,
                &json!({"first":first,"count":1,"rows":[{"title":"x".repeat(60_000)}]}),
            )
            .unwrap();
    }
    let active = store.open_query("active", query()).unwrap();
    store
        .cache(
            &active,
            &json!({"first":0,"count":1,"rows":[{"title":"y".repeat(400_000)}]}),
        )
        .unwrap();
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT count(*) FROM queries WHERE id=?1",
                [old.id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        store.cached(&active, 0).unwrap().unwrap()["rows"][0]["title"]
            .as_str()
            .unwrap()
            .len(),
        400_000
    );
    let (count, bytes): (i64, i64) = store
        .db
        .query_row("SELECT count(*),sum(bytes) FROM projections", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert!(count <= 8 && bytes <= CACHE_BYTES as i64);
}

#[test]
fn query_filters_have_separate_tokens_and_cursor_cycles_preserve_the_known_prefix() {
    let source = Source::new(3);
    let (_directory, mut window, _) = window(source.clone());
    let key = crate::native_page_request::route(
        "fixture",
        "base",
        json!({"filters":{"genre":"Metal"},"order":"title","limit":37}),
        "collection",
    );
    window.open(&key, 1);
    let first = answer(&mut window, 1);
    assert_eq!(
        first["header"]["base_key"],
        crate::native_page_request::route("fixture", "base", json!({}), "collection")
    );
    assert_eq!(source.calls.lock().unwrap()[0]["filters"]["genre"], "Metal");
    assert_eq!(source.calls.lock().unwrap()[0]["limit"], 37);
    let different = crate::native_page_request::route(
        "fixture",
        "base",
        json!({"filters":{"genre":"Jazz"},"order":"title"}),
        "collection",
    );
    let session = window.open(&different, 2);
    let second = answer(&mut window, 2);
    assert_eq!(second["header"]["base_key"], first["header"]["base_key"]);
    assert_ne!(
        first["page"]["rows"][0]["rowToken"],
        second["page"]["rows"][0]["rowToken"]
    );
    source.repeat.store(true, Ordering::SeqCst);
    assert!(window.seek(session, 37, 100, 3));
    let failed = answer(&mut window, 3);
    assert_eq!(failed["knownCount"], 37);
    assert!(failed["error"].as_str().unwrap().contains("repeated"));
}

#[test]
fn cached_projections_are_lru_and_error_envelopes_keep_the_same_byte_bound() {
    let directory = tempfile::tempdir().unwrap();
    let store = store::Store::open(&directory.path().join("collections.sqlite")).unwrap();
    let query = store.open_query("cache", query()).unwrap();
    for first in 0..8 {
        store
            .cache(
                &query,
                &json!({"first":first,"count":1,"rows":[{"title":"x".repeat(1000)}]}),
            )
            .unwrap();
    }
    assert!(store.cached(&query, 0).unwrap().is_some());
    store
        .cache(&query, &json!({"first":8,"count":1,"rows":[{}]}))
        .unwrap();
    assert!(store.cached(&query, 0).unwrap().is_some());
    assert!(store.cached(&query, 1).unwrap().is_none());
    let count: i64 = store
        .db
        .query_row("SELECT count(*) FROM projections", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 8);
    let chunk = json!({"pageIdentity":"test","first":0,"count":100,"rows":vec![json!({"title":"x".repeat(6000)});100]});
    let document =
        json!({"sessionId":1,"requestId":1,"error":"界".repeat(512),"header":{"title":"Cached"}});
    let projected = projection::cached_document(document, &chunk, 0, 100).unwrap();
    assert!(projected.to_string().len() <= CHUNK_BYTES);
    assert!(projected["page"]["count"].as_u64().unwrap() < 100);
    assert_eq!(projected["page"]["nextOffset"], projected["page"]["count"]);
}
