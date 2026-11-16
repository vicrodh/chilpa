use super::*;
use std::io::{BufRead, BufReader, Write};

#[test]
fn successful_settings_submit_retires_only_articles_and_failed_or_read_only_calls_do_not() {
    let natives = Arc::new(Natives::default());
    let manifest: Manifest =
        serde_json::from_value(json!({"id":"fixture","name":"Fixture","version":"1.0.0",
        "api_version":1,"kind":"source","runtime":"process"}))
        .unwrap();
    let (client, (input, mut output)) = tests::channel();
    tests::started(&natives, manifest, Path::new(""), &client);
    let catalog = crate::source_catalog::collection_window::ready_test_catalog();
    let musical = catalog.collection_epochs("fixture");
    let runtime = natives.collection_generation("fixture");
    let initial = natives.article_stamp("fixture");
    let responder = std::thread::spawn(move || {
        let mut reader = BufReader::new(input);
        for _ in 0..4 {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let answer = if request["params"]["fail"] == true {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":"Rejected"}})
            } else {
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"fields":[]}})
            };
            writeln!(output, "{answer}").unwrap();
            output.flush().unwrap();
        }
        // Keep the channel alive until the assertions have inspected its state.
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
    });
    natives
        .call(
            "fixture",
            "settings.open",
            json!({}),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(natives.article_stamp("fixture"), initial);
    assert!(
        natives
            .call(
                "fixture",
                "settings.submit",
                json!({"fail":true}),
                Duration::from_secs(2)
            )
            .is_err()
    );
    assert_eq!(natives.article_stamp("fixture"), initial);
    natives
        .call(
            "fixture",
            "settings.submit",
            json!({}),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(natives.article_stamp("fixture"), (runtime, initial.1 + 1));
    natives
        .call_raw("fixture", "auth.status", json!({}), Duration::from_secs(2))
        .unwrap();
    assert_eq!(natives.article_stamp("fixture"), (runtime, initial.1 + 1));
    assert_eq!(natives.collection_generation("fixture"), runtime);
    assert_eq!(catalog.collection_epochs("fixture"), musical);
    // Finish our synthetic pipe peer without involving a process or network.
    client.notify("test.finished", json!({}));
    responder.join().unwrap();
}

#[test]
fn an_old_settings_reply_cannot_invalidate_the_replacement_runtime() {
    let natives = Arc::new(Natives::default());
    let manifest: Manifest =
        serde_json::from_value(json!({"id":"fixture","name":"Fixture","version":"1.0.0",
        "api_version":1,"kind":"source","runtime":"process"}))
        .unwrap();
    let (old, _old_ends) = tests::channel();
    let (new, _new_ends) = tests::channel();
    tests::started(&natives, manifest.clone(), Path::new(""), &old);
    tests::started(&natives, manifest, Path::new(""), &new);
    let before = natives.article_stamp("fixture");
    natives.article_settings_result("fixture", &old, "settings.submit", true);
    assert_eq!(natives.article_stamp("fixture"), before);
    natives.article_settings_result("fixture", &new, "settings.submit", true);
    assert_eq!(natives.article_stamp("fixture"), (before.0, before.1 + 1));
}

#[test]
fn greeting_advertises_native_articles_and_collections_without_embedded_web() {
    let (client, _ends) = tests::channel();
    let launched = Launched {
        client,
        data: PathBuf::from("fixture-data"),
        cache: PathBuf::from("fixture-cache"),
        export: None,
    };
    for network in [false, true] {
        let greeting = greeting(&launched, network);
        assert_eq!(greeting["protocol"], PROTOCOL);
        assert_eq!(greeting["features"], json!({
            "source_navigation": 1, "collection_pages": 1, "article_pages": 1
        }));
        assert!(greeting["features"].get("web_pages").is_none());
        assert!(greeting.get("web_pages").is_none());
        assert_eq!(greeting["network"]["enabled"], network);
        assert!(greeting.get("export_dir").is_none());
    }
}
