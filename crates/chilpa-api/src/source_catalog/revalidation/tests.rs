use super::*;
use super::super::{fetch, native};
use serde_json::{Value, json};

fn document(kind: &str) -> Value {
    let items: Vec<_> = (1..=2).map(|id| json!({"kind":if kind == "tracks" { "track" } else if kind == "albums" { "album" } else { "playlist" },
        "ref":format!("{kind}:{id}"),"title":format!("Item {id}"),"album":"Album","album_ref":"album:parent",
        "tracks":2,"editable":true,"updated_at":7})).collect();
    json!({"total":items.len(),"items":items,"folders":[{"ref":"folder:1","name":"Folder"}]})
}

fn read(catalog: &Catalog, kind: &str, answer: &Value, refresh: bool) -> Result<(), String> {
    let key = format!("fixture section:{kind}");
    native::read_library(catalog, "fixture", &key, kind, |request| {
        assert_eq!(request["kind"], kind);
        assert_eq!(request["offset"], 0);
        assert_eq!(request["refresh"], refresh);
        Ok(answer.clone())
    }, || true)
}

fn seed(kind: &str) -> std::sync::Arc<Catalog> {
    let catalog = Catalog::new();
    catalog.lock().loaded = true;
    read(&catalog, kind, &document(kind), false).unwrap();
    catalog.publish(false);
    fetch::persist(&catalog, |_| Ok(()));
    assert!(!catalog.lock().dirty);
    catalog
}

fn periodic(catalog: &Catalog, key: &str) -> Job {
    let mut inner = catalog.lock();
    super::queue(catalog, &mut inner, key.into(), false);
    let job = Job::Shelf(key.into());
    assert!(inner.queue.queued.contains(&job));
    inner.queue.queued.remove(&job);
    inner.queue.later.retain(|queued| queued != &job);
    inner.queue.urgent.retain(|queued| queued != &job);
    inner.running.insert(job.clone());
    job
}

#[test]
fn identical_personal_revalidation_keeps_revisions_and_disk_quiet() {
    for kind in ["albums", "tracks", "playlists"] {
        let catalog = seed(kind);
        let key = format!("fixture section:{kind}");
        let previous = super::super::now() - 1;
        catalog.lock().store.shelves.get_mut(&key).unwrap().read_at = previous;
        let before = catalog.revisions();
        let job = periodic(&catalog, &key);
        assert!(!catalog.loading(&key));
        assert!(!catalog.shelf_snapshot(&key, "").2, "cached rows never enter foreground loading");
        read(&catalog, kind, &document(kind), false).unwrap();
        fetch::finish(&catalog, &job, Ok(()));
        assert_eq!(catalog.revisions(), before);
        assert!(!catalog.loading(&key));
        assert!(catalog.stored_shelf(&key).unwrap().read_at > previous);
        fetch::persist(&catalog, |_| panic!("identical revalidation must not replace the catalog"));
    }
}

#[test]
fn changed_library_rows_folders_order_and_membership_publish_and_persist() {
    for change in ["metadata", "folders", "order", "membership"] {
        let catalog = seed("playlists");
        let key = "fixture section:playlists";
        let mut answer = document("playlists");
        match change {
            "metadata" => answer["items"][0]["description"] = json!("Changed description"),
            "folders" => answer["folders"][0]["name"] = json!("Renamed folder"),
            "order" => answer["items"].as_array_mut().unwrap().reverse(),
            _ => { answer["items"].as_array_mut().unwrap().pop(); }
        }
        let before = catalog.revisions();
        let job = periodic(&catalog, key);
        read(&catalog, "playlists", &answer, false).unwrap();
        fetch::finish(&catalog, &job, Ok(()));
        assert_ne!(catalog.revisions().0, before.0, "changed library content must reach the UI");
        let mut writes = 0;
        fetch::persist(&catalog, |_| { writes += 1; Ok(()) });
        assert_eq!(writes, 1);
        fetch::persist(&catalog, |_| panic!("one content change needs one durable replacement"));
    }
}

#[test]
fn failed_revalidation_and_identical_recovery_both_publish_status() {
    let catalog = seed("playlists");
    let key = "fixture section:playlists";
    let before = catalog.revisions();
    let job = periodic(&catalog, key);
    fetch::finish(&catalog, &job, Err("Source failed".into()));
    assert_eq!(catalog.shelf_error(key).as_deref(), Some("Source failed"));
    assert_ne!(catalog.revisions(), before);
    let before = catalog.revisions();
    let job = periodic(&catalog, key);
    assert!(catalog.loading(key), "recovery from a visible error remains foreground work");
    read(&catalog, "playlists", &document("playlists"), false).unwrap();
    fetch::finish(&catalog, &job, Ok(()));
    assert!(catalog.shelf_error(key).is_none());
    assert!(!catalog.loading(key));
    assert_ne!(catalog.revisions(), before);
    fetch::persist(&catalog, |_| panic!("an error recovery with identical rows changes no durable content"));
}

#[test]
fn manual_retry_promotes_running_revalidation_and_requests_provider_refresh() {
    let catalog = seed("playlists");
    let key = "fixture section:playlists";
    let job = periodic(&catalog, key);
    assert!(!catalog.loading(key));
    catalog.retry(key);
    assert!(catalog.loading(key));
    assert!(!catalog.lock().revalidating.contains(&job));
    let before = catalog.revisions();
    read(&catalog, "playlists", &document("playlists"), true).unwrap();
    fetch::finish(&catalog, &job, Ok(()));
    assert!(!catalog.loading(key));
    assert_ne!(catalog.revisions(), before);
}

#[test]
fn foreground_empty_results_and_existing_work_keep_their_loading_contract() {
    let catalog = Catalog::new();
    catalog.lock().loaded = true;
    let key = "fixture section:albums";
    let job = periodic(&catalog, key);
    assert!(catalog.loading(key));
    read(&catalog, "albums", &json!({"items":[],"total":0}), false).unwrap();
    let before = catalog.revisions();
    fetch::finish(&catalog, &job, Ok(()));
    assert!(!catalog.loading(key));
    assert_ne!(catalog.revisions(), before);
    {
        let mut inner = catalog.lock();
        catalog.queue(&mut inner, job.clone(), true);
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(!inner.revalidating.contains(&job));
        inner.queue.queued.remove(&job);
        inner.queue.urgent.clear();
        inner.running.insert(job.clone());
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(!inner.revalidating.contains(&job));
    }
    assert!(catalog.loading(key));
    fetch::finish(&catalog, &job, Ok(()));
    let job = periodic(&catalog, key);
    assert!(!catalog.loading(key), "a complete cached empty shelf can be checked quietly");
    catalog.lock().running.insert(Job::Append(key.into()));
    assert!(catalog.loading(key), "append loading remains visible beside quiet revalidation");
    fetch::finish(&catalog, &job, Ok(()));
}

#[test]
fn identical_ingest_preserves_unrelated_dirty_work_and_disabled_sources_stay_blocked() {
    let catalog = seed("playlists");
    let key = "fixture section:playlists";
    catalog.lock().dirty = true;
    read(&catalog, "playlists", &document("playlists"), false).unwrap();
    assert!(catalog.lock().dirty);
    let mut writes = 0;
    fetch::persist(&catalog, |_| { writes += 1; Ok(()) });
    assert_eq!(writes, 1);
    {
        let mut inner = catalog.lock();
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(!inner.revalidating.is_empty());
    }
    catalog.set_source_states([("fixture".into(), false)]);
    assert!(catalog.lock().revalidating.is_empty());
    {
        let mut inner = catalog.lock();
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(inner.queue.queued.is_empty());
        assert!(inner.revalidating.is_empty());
    }
}

#[test]
fn navigation_cancellation_and_plugin_removal_release_quiet_job_markers() {
    let catalog = seed("playlists");
    let key = "fixture section:playlists";
    let job = Job::Shelf(key.into());
    catalog.protect_navigation(&[crate::navigation::Route {
        screen:crate::navigation::Screen::Source, id:0, key:key.into(),
    }]);
    {
        let mut inner = catalog.lock();
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(inner.revalidating.contains(&job));
    }
    catalog.protect_navigation(&[]);
    assert!(!catalog.lock().queue.queued.contains(&job));
    assert!(!catalog.lock().revalidating.contains(&job));
    {
        let mut inner = catalog.lock();
        super::queue(&catalog, &mut inner, key.into(), false);
        assert!(inner.revalidating.contains(&job));
    }
    catalog.forget_plugin("fixture");
    assert!(!catalog.lock().queue.queued.contains(&job));
    assert!(!catalog.lock().revalidating.contains(&job));
}
