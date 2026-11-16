use super::super::Broker;
use super::*;
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

fn document() -> Value {
    json!({"layout":"article","title":"An article","article":{"id":"article:1","authors":["Writer"],
        "published_at":"2026-11-16","summary":"Summary","image":"https://example.invalid/cover.jpg",
        "blocks":[{"type":"heading","text":"Heading","level":2},
            {"type":"paragraph","text":"Plain text"},{"type":"quote","text":"Quoted","attribution":"Writer"},
            {"type":"list","ordered":false,"items":["First","Second"]},
            {"type":"image","url":"https://example.invalid/inside.jpg","alt":"Description","caption":"Caption"}]}})
}
fn route() -> String {
    crate::native_page_request::route("fixture", "article:1", json!({}), "article")
}

#[test]
fn article_documents_validate_plain_blocks_and_all_resource_boundaries() {
    assert_eq!(document::validate(&document()).unwrap().len(), 2);
    let mut value = document();
    value["article"]["blocks"] = json!([]);
    value["article"]["summary"] = "".into();
    let fixed = [
        value["title"].as_str().unwrap(),
        value["article"]["id"].as_str().unwrap(),
        value["article"]["published_at"].as_str().unwrap(),
        value["article"]["authors"][0].as_str().unwrap(),
    ]
    .iter()
    .map(|text| text.len())
    .sum::<usize>();
    value["article"]["summary"] = "x".repeat(512 * 1024 - fixed).into();
    assert!(document::validate(&value).is_ok());
    value["article"]["summary"] = "x".repeat(512 * 1024 - fixed + 1).into();
    assert!(document::validate(&value).unwrap_err().contains("512 KiB"));
    value["article"]["summary"] = "\n".repeat(512 * 1024 - fixed).into();
    assert!(
        document::validate(&value).unwrap_err().contains("1 MiB"),
        "escaped JSON has its own encoded limit"
    );
    let mut value = document();
    value["article"]["blocks"] = json!(vec![json!({"type":"paragraph","text":"x"}); 256]);
    assert!(document::validate(&value).is_ok());
    value["article"]["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"paragraph","text":"x"}));
    assert!(document::validate(&value).is_err());
    value["article"]["blocks"] = json!([{"type":"list","ordered":true,"items":vec!["x";256]}]);
    assert!(document::validate(&value).is_ok());
    value["article"]["blocks"][0]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!("x"));
    assert!(document::validate(&value).is_err());
    value["article"]["blocks"] = json!(
        (0..7)
            .map(|n| json!({"type":"image","url":format!("https://example.invalid/{n}.jpg")}))
            .collect::<Vec<_>>()
    );
    assert_eq!(document::validate(&value).unwrap().len(), 8);
    value["article"]["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"image","url":"https://example.invalid/cover.jpg"}));
    assert!(
        document::validate(&value).is_err(),
        "the cover and repeated images count toward eight placements"
    );
}

#[test]
fn article_documents_reject_nested_content_unknown_fields_and_unsafe_addresses() {
    for block in [
        json!({"type":"html","text":"<p>no</p>"}),
        json!({"type":"paragraph","text":7}),
        json!({"type":"heading","text":"x","level":7}),
        json!({"type":"list","ordered":true,"items":[{"text":"nested"}]}),
        json!({"type":"paragraph","text":"x","style":{"font":"ignored"}}),
    ] {
        let mut value = document();
        value["article"]["blocks"] = json!([block]);
        assert!(document::validate(&value).is_err());
    }
    for url in [
        "data:image/png;base64,eA==",
        "file:///private/image.jpg",
        "https://user:password@example.invalid/x",
        "https://example.invalid/x#fragment",
        "https://example.invalid/has space",
    ] {
        let mut value = document();
        value["article"]["image"] = url.into();
        assert!(document::validate(&value).is_err());
    }
    let mut value = document();
    value["article"]["image"] = format!("https://example.invalid/{}", "x".repeat(4096)).into();
    assert!(document::validate(&value).is_err());
    let mut value = document();
    value["article"]["authors"] = json!(vec!["Writer"; 33]);
    assert!(document::validate(&value).is_err());
    let mut value = document();
    value["article"]["summary"] = "text\u{0000}".into();
    assert!(document::validate(&value).is_err());
}

struct Source {
    generation: AtomicU64,
    settings_epoch: AtomicU64,
    document: Mutex<Value>,
    blocked: Mutex<bool>,
    wake: Condvar,
    pages: AtomicUsize,
    images: Mutex<Vec<String>>,
    in_image: AtomicUsize,
}
impl Source {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            generation: AtomicU64::new(1),
            settings_epoch: AtomicU64::new(0),
            document: Mutex::new(document()),
            blocked: Mutex::new(false),
            wake: Condvar::new(),
            pages: AtomicUsize::new(0),
            images: Mutex::new(Vec::new()),
            in_image: AtomicUsize::new(0),
        })
    }
    fn wait(&self) {
        let mut blocked = self.blocked.lock().unwrap();
        while *blocked {
            blocked = self.wake.wait(blocked).unwrap();
        }
    }
    fn release(&self) {
        *self.blocked.lock().unwrap() = false;
        self.wake.notify_all();
    }
}
impl PageSource for Source {
    fn article_stamp(&self, _: &str) -> (u64, u64) {
        (
            self.generation.load(Ordering::SeqCst),
            self.settings_epoch.load(Ordering::SeqCst),
        )
    }
    fn generation(&self, _: &str) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn article_pages(&self, _: &str) -> bool {
        true
    }
    fn page(&self, _: &str, params: Value) -> Result<Value, Failure> {
        self.pages.fetch_add(1, Ordering::SeqCst);
        self.wait();
        let reference = params["ref"].as_str().unwrap();
        if reference == "listing" || reference.starts_with("listing:") {
            let page = reference
                .strip_prefix("listing:")
                .map(|number| number.parse::<usize>().unwrap());
            let article = page.unwrap_or(1);
            Ok(
                json!({"layout":"collection","items":[{"kind":"article","ref":format!("article:{article}"),"title":"An article"}],
                "next":page.filter(|page| *page<11).map(|page|format!("listing:{}",page+1))}),
            )
        } else {
            Ok(self.document.lock().unwrap().clone())
        }
    }
    fn image(&self, _: &str, url: &str) -> Result<String, Failure> {
        self.images.lock().unwrap().push(url.into());
        self.in_image.fetch_add(1, Ordering::SeqCst);
        self.wait();
        Ok("/owned-fixture/cached-picture.jpg".into())
    }
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "article operation timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn fixture() -> (
    tempfile::TempDir,
    NativeArticleWindow,
    Arc<Source>,
    Arc<Catalog>,
) {
    let folder = tempfile::tempdir().unwrap();
    let source = Source::new();
    let catalog = crate::source_catalog::collection_window::ready_test_catalog();
    let broker = Broker::with_source(
        source.clone(),
        catalog.clone(),
        folder.path().join("collections.sqlite"),
    );
    (
        folder,
        NativeArticleWindow {
            window: NativeCollectionWindow { broker, session: 0 },
        },
        source,
        catalog,
    )
}
fn answer(window: &mut NativeArticleWindow) -> Value {
    let mut answer = None;
    until(|| {
        answer = window.poll();
        answer.is_some()
    });
    answer.unwrap()
}

#[test]
fn article_transfer_and_images_leave_no_disk_document_or_musical_publication() {
    let (folder, mut window, source, catalog) = fixture();
    let unchanged = crate::source_catalog::collection_window::article_test_probe(&catalog);
    let session = window.open(&route(), 1);
    let value = answer(&mut window);
    assert_eq!(value["document"], document());
    assert_eq!(value["sessionId"], session);
    assert!(!folder.path().join("collections.sqlite").exists());
    assert!(window.poll().is_none());
    assert!(!window.request_image(session, "https://example.invalid/undeclared.jpg", 2));
    assert!(window.request_image(session, "https://example.invalid/inside.jpg", 3));
    let value = answer(&mut window);
    assert_eq!(value["kind"], "image");
    assert_eq!(value["requestId"], 3);
    assert_eq!(value["path"], "/owned-fixture/cached-picture.jpg");
    until(|| !window.busy());
    assert_eq!(source.pages.load(Ordering::SeqCst), 1);
    assert_eq!(
        window
            .window
            .broker
            .shared
            .lock()
            .article
            .as_ref()
            .unwrap()
            .images
            .len(),
        2
    );
    window.close(session);
    assert!(window.window.broker.shared.lock().article.is_none());
    assert!(window.poll().is_none());
    unchanged();
}

#[test]
fn article_close_discards_late_rpc_and_old_owners_cannot_retire_the_next_view() {
    let (_folder, mut first, source, _catalog) = fixture();
    *source.blocked.lock().unwrap() = true;
    let old = first.open(&route(), 1);
    until(|| source.pages.load(Ordering::SeqCst) == 1);
    first.close(old);
    let mut next = NativeArticleWindow {
        window: NativeCollectionWindow {
            broker: first.window.broker.clone(),
            session: 0,
        },
    };
    let session = next.open(&route(), 2);
    first.close(old);
    source.release();
    assert_eq!(answer(&mut next)["sessionId"], session);
    assert!(first.poll().is_none());
    drop(first);
    assert!(next.request_image(session, "https://example.invalid/cover.jpg", 3));
    assert_eq!(answer(&mut next)["kind"], "image");
}

#[test]
fn article_image_queue_is_bounded_and_close_releases_pending_demands() {
    let (_folder, mut window, source, _) = fixture();
    source.document.lock().unwrap()["article"]["blocks"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"type":"image","url":"https://example.invalid/third.jpg"}),
            json!({"type":"image","url":"https://example.invalid/fourth.jpg"}),
        ]);
    let session = window.open(&route(), 1);
    answer(&mut window);
    *source.blocked.lock().unwrap() = true;
    assert!(window.request_image(session, "https://example.invalid/cover.jpg", 2));
    until(|| source.in_image.load(Ordering::SeqCst) == 1);
    assert!(window.request_image(session, "https://example.invalid/inside.jpg", 3));
    assert!(window.request_image(session, "https://example.invalid/third.jpg", 4));
    assert!(!window.request_image(session, "https://example.invalid/fourth.jpg", 5));
    window.close(session);
    source.release();
    until(|| !window.window.broker.shared.lock().working);
    assert_eq!(source.images.lock().unwrap().len(), 1);
    assert!(window.poll().is_none());
    assert!(window.window.broker.shared.lock().pending.is_empty());
}

#[test]
fn article_runtime_changes_reject_late_publications_but_music_changes_do_not_reload() {
    let (_folder, mut window, source, catalog) = fixture();
    *source.blocked.lock().unwrap() = true;
    window.open(&route(), 1);
    until(|| source.pages.load(Ordering::SeqCst) == 1);
    source.generation.store(2, Ordering::SeqCst);
    source.release();
    let value = answer(&mut window);
    assert!(value["document"].is_null());
    assert_ne!(value["error"], "");
    window.open(&route(), 2);
    answer(&mut window);
    until(|| !window.busy());
    catalog.expire_personal_pages("fixture");
    catalog.mark_native_favorite("other", "album:1", true);
    assert!(window.poll().is_none());
    assert_eq!(source.pages.load(Ordering::SeqCst), 2);
    window.open(&route(), 3);
    until(|| window.window.broker.shared.lock().mailbox.is_some());
    source.generation.store(3, Ordering::SeqCst);
    let value = answer(&mut window);
    assert!(value["document"].is_null());
    assert_ne!(value["error"], "");
}

#[test]
fn article_row_actions_preserve_the_page_token_without_adopting_a_music_shelf() {
    let (_folder, article, source, catalog) = fixture();
    let unchanged = crate::source_catalog::collection_window::article_test_probe(&catalog);
    let mut window = NativeCollectionWindow {
        broker: article.window.broker.clone(),
        session: 0,
    };
    let session = window.open("fixture native:listing", 1);
    let mut value = None;
    until(|| {
        value = window.poll();
        value.is_some()
    });
    assert!(window.seek(session, 0, 100, 2));
    until(|| {
        value = window.poll();
        value.is_some()
    });
    assert_eq!(value.as_ref().unwrap()["error"], "");
    assert_eq!(value.as_ref().unwrap()["requestId"], 2);
    let row = value.take().unwrap()["page"]["rows"][0]["rowToken"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(window.prepare_action(session, &row, 3));
    until(|| {
        value = window.poll();
        value.is_some()
    });
    let action = &value.as_ref().unwrap()["action"];
    assert_eq!(action["kind"], "article");
    assert_eq!(action["lease"], 0);
    assert_eq!(action["navigationKey"], route());
    assert!(window.broker.shared.lock().action.is_none());
    assert!(source.pages.load(Ordering::SeqCst) > 0);
    unchanged();
}

#[test]
fn article_released_image_cannot_publish_into_a_new_demand_for_the_same_url() {
    let (_folder, mut window, source, _) = fixture();
    let session = window.open(&route(), 1);
    answer(&mut window);
    *source.blocked.lock().unwrap() = true;
    let url = "https://example.invalid/cover.jpg";
    assert!(window.request_image(session, url, 2));
    until(|| source.in_image.load(Ordering::SeqCst) == 1);
    window.release_image(session, url);
    assert!(window.request_image(session, url, 3));
    source.release();
    let value = answer(&mut window);
    assert_eq!(value["requestId"], 3);
    assert_eq!(source.images.lock().unwrap().len(), 2);
    window.release_image(session, url);
    assert!(window.poll().is_none());
    assert!(
        window
            .window
            .broker
            .shared
            .lock()
            .article
            .as_ref()
            .unwrap()
            .demands
            .is_empty()
    );
}

#[test]
fn article_dimensions_are_optional_and_validated_without_changing_image_limits() {
    let mut value = document();
    value["article"]["image_width"] = 9033.into();
    value["article"]["image_height"] = 600.into();
    value["article"]["blocks"][4]["width"] = 4096.into();
    value["article"]["blocks"][4]["height"] = 1.into();
    assert!(document::validate(&value).is_ok());
    for invalid in [json!(0), json!(65536), json!(-1), json!("1600"), json!(1.5)] {
        value["article"]["image_width"] = invalid;
        assert!(document::validate(&value).is_err());
    }
}

#[test]
fn article_action_after_projection_eviction_uses_its_original_page() {
    let (_folder, article, source, catalog) = fixture();
    let unchanged = crate::source_catalog::collection_window::article_test_probe(&catalog);
    let mut window = NativeCollectionWindow {
        broker: article.window.broker.clone(),
        session: 0,
    };
    let session = window.open("fixture native:listing:0", 1);
    let mut answer = None;
    until(|| {
        answer = window.poll();
        answer.is_some()
    });
    assert!(window.seek(session, 0, 1, 2));
    until(|| {
        answer = window.poll();
        answer.is_some()
    });
    assert_eq!(answer.as_ref().unwrap()["error"], "");
    assert_eq!(answer.as_ref().unwrap()["requestId"], 2);
    let token = answer.take().unwrap()["page"]["rows"][0]["rowToken"]
        .as_str()
        .unwrap()
        .to_string();
    for index in 1..12 {
        assert!(window.seek(session, index, 1, index as u64 + 2));
        until(|| {
            answer = window.poll();
            answer.is_some()
        });
        assert_eq!(answer.as_ref().unwrap()["error"], "");
        assert_eq!(answer.as_ref().unwrap()["requestId"], index as u64 + 2);
        assert_eq!(answer.as_ref().unwrap()["page"]["first"], index);
    }
    let calls_before_action = source.pages.load(Ordering::SeqCst);
    assert!(window.prepare_action(session, &token, 20));
    until(|| {
        answer = window.poll();
        answer.is_some()
    });
    assert_eq!(answer.as_ref().unwrap()["error"], "");
    assert_eq!(answer.as_ref().unwrap()["requestId"], 20);
    assert_eq!(
        source.pages.load(Ordering::SeqCst),
        calls_before_action + 1,
        "the evicted original page is fetched before resolving its article action"
    );
    assert_eq!(
        answer.unwrap()["action"]["navigationKey"],
        crate::native_page_request::route("fixture", "article:0", json!({}), "article")
    );
    unchanged();
}

#[test]
fn article_native_images_obey_the_sources_domains_before_touching_the_cache() {
    let natives = Arc::new(crate::natives::Natives::default());
    let manifest: chilpa_plugin::Manifest = serde_json::from_value(json!({"id":"fixture","name":"Fixture","version":"1.0.0",
        "api_version":1,"kind":"source","runtime":"process","capabilities":[{"type":"network","domains":["approved.invalid"]}]})).unwrap();
    let (client, _ends) = crate::natives::tests::channel();
    crate::natives::tests::started(
        &natives,
        manifest.clone(),
        std::path::Path::new(""),
        &client,
    );
    crate::natives::tests::remember_source(
        &natives,
        manifest,
        json!({"pages":true,"article_pages":1}),
    );
    let source = super::super::worker::NativeSource(natives);
    assert!(source.article_pages("fixture"));
    assert!(
        source
            .image("fixture", "https://outside.invalid/picture.jpg")
            .is_err()
    );
    assert!(
        source
            .image("unknown", "https://approved.invalid/picture.jpg")
            .is_err()
    );
}

#[test]
fn article_settings_epoch_retires_visible_body_without_changing_runtime_or_reloading() {
    let (_folder, mut window, source, catalog) = fixture();
    window.open(&route(), 1);
    answer(&mut window);
    until(|| !window.busy());
    let unchanged = crate::source_catalog::collection_window::article_test_probe(&catalog);
    source.settings_epoch.fetch_add(1, Ordering::SeqCst);
    let result = window.poll().unwrap();
    assert_eq!(result["requestId"], 0);
    assert!(result["document"].is_null());
    assert_eq!(source.generation.load(Ordering::SeqCst), 1);
    assert_eq!(source.pages.load(Ordering::SeqCst), 1);
    assert!(window.poll().is_none());
    unchanged();
}

#[test]
fn article_cache_flags_and_notes_are_typed_and_share_the_text_budget() {
    let mut value = document();
    value["offline"] = false.into();
    value["stale"] = false.into();
    value["error"] = Value::Null;
    assert!(document::validate(&value).is_ok());
    value["error"] = "é".repeat(1024).into();
    assert!(
        document::validate(&value).is_ok(),
        "2048 UTF-8 bytes are accepted"
    );
    value["error"] = format!("{}x", "é".repeat(1024)).into();
    assert!(document::validate(&value).is_err());
    for flag in ["offline", "stale"] {
        for invalid in [Value::Null, json!(0), json!("false"), json!([]), json!({})] {
            let mut value = document();
            value[flag] = invalid;
            assert!(
                document::validate(&value).is_err(),
                "{flag} must be a boolean when present"
            );
        }
    }
    for invalid in [json!(false), json!(1), json!([]), json!({})] {
        let mut value = document();
        value["error"] = invalid;
        assert!(document::validate(&value).is_err());
    }
    let mut full = json!({"layout":"article","title":"","error":"",
        "article":{"id":"x","authors":[],"summary":"x".repeat(512*1024-1),"blocks":[]}});
    assert!(document::validate(&full).is_ok());
    full["error"] = "x".into();
    assert!(document::validate(&full).unwrap_err().contains("512 KiB"));
    let mut value = document();
    value["cache_key"] = "private-provider-field".into();
    assert!(
        document::validate(&value).is_err(),
        "only the three declared cache fields are allowed"
    );
}

#[test]
fn article_optional_nulls_and_omissions_preserve_required_block_validation() {
    for omit in [false, true] {
        let mut value = document();
        for field in ["published_at", "image", "image_width", "image_height"] {
            if omit {
                value["article"].as_object_mut().unwrap().remove(field);
            } else {
                value["article"][field] = Value::Null;
            }
        }
        for field in ["alt", "caption", "width", "height"] {
            if omit {
                value["article"]["blocks"][4]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
            } else {
                value["article"]["blocks"][4][field] = Value::Null;
            }
        }
        if omit {
            value["article"]["blocks"][2]
                .as_object_mut()
                .unwrap()
                .remove("attribution");
        } else {
            value["article"]["blocks"][2]["attribution"] = Value::Null;
        }
        assert_eq!(
            document::validate(&value).unwrap(),
            vec!["https://example.invalid/inside.jpg"]
        );
        value["article"]["blocks"][4]["url"] = Value::Null;
        assert!(
            document::validate(&value).is_err(),
            "the image block itself still needs a URL"
        );
    }
    for field in ["published_at", "image", "image_width", "image_height"] {
        let mut value = document();
        value["article"][field] = json!([]);
        assert!(document::validate(&value).is_err());
    }
}

#[test]
fn article_reopening_reads_cached_offline_and_stale_bodies_without_rpc_errors() {
    let (folder, mut window, source, catalog) = fixture();
    let unchanged = crate::source_catalog::collection_window::article_test_probe(&catalog);
    for (request, flags) in [
        json!({"offline":false,"stale":false}),
        json!({"offline":true,"error":null}),
        json!({"stale":true,"error":"Showing the saved article."}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut cached = document();
        for (key, value) in flags.as_object().unwrap() {
            cached[key] = value.clone();
        }
        *source.document.lock().unwrap() = cached.clone();
        let session = window.open(&route(), request as u64 + 1);
        let reply = answer(&mut window);
        assert_eq!(reply["sessionId"], session);
        assert_eq!(reply["requestId"], request + 1);
        assert_eq!(
            reply["error"], "",
            "cache notices remain inside the accepted document"
        );
        assert_eq!(reply["document"], cached);
        window.close(session);
        assert!(window.poll().is_none());
    }
    assert_eq!(source.pages.load(Ordering::SeqCst), 3);
    assert!(!folder.path().join("collections.sqlite").exists());
    unchanged();
}
