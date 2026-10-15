use super::*;
use crate::Chilpa;

fn protected(api: &Chilpa) -> HashSet<String> {
    api.catalog.lock().protected_tracks.clone()
}

#[test]
fn queue_protection_follows_edits_disabled_persistence_and_both_restore_formats() {
    const CHILD: &str = "CHILPA_TEST_QUEUE_PROTECTION";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "source_catalog::queue_retention_tests::queue_protection_follows_edits_disabled_persistence_and_both_restore_formats", "--nocapture"])
            .env(CHILD, "1").envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let path = |id| crate::source_opener::entry_of("guest", id);
    let mut api = Chilpa::new();
    api.set_keep_queue(false);
    api.replace_queue(vec![path("active")]);
    assert_eq!(protected(&api), [path("active")].into(), "disabled persistence still protects live metadata");
    api.now.playing = true;
    api.now.paused = true;
    api.enqueue(&[path("appended")], "file:capture", false);
    assert_eq!(protected(&api), [path("active"), path("appended")].into(), "append updates protection before the next tick");
    api.purge_queue(|entry| entry == path("appended"));
    assert_eq!(protected(&api), [path("active")].into(), "purging an entry releases only that entry");
    let other = api.listen_create("Other", &[path("inactive")]);
    assert_eq!(protected(&api), [path("active"), path("inactive")].into());
    api.listen_replace(other, &[path("edited")]);
    assert_eq!(protected(&api), [path("active"), path("edited")].into());
    api.listen_undo();
    assert_eq!(protected(&api), [path("active"), path("inactive")].into(), "undo protects the restored buffer entries");
    api.clear_queue();
    assert_eq!(protected(&api), [path("inactive")].into(), "clearing the active queue preserves inactive buffers");
    api.replace_queue(vec![path("replacement")]);
    api.listen_close(other);
    assert_eq!(protected(&api), [path("replacement")].into(), "closing a buffer releases its entries");
    api.set_keep_queue(true);
    drop(api);
    let mut restored = Chilpa::new();
    assert_eq!(protected(&restored), [path("replacement")].into(), "normal startup protects the restored queue");
    restored.listen_create("Inactive", &[path("inactive-restored")]);
    restored.listen_close(restored.listen.active);
    assert_eq!(restored.listen.active, 0);
    drop(restored);
    let mut inactive = Chilpa::new();
    assert_eq!(protected(&inactive), [path("inactive-restored")].into(), "startup protects buffers even with no active queue");
    inactive.set_keep_queue(false);
    drop(inactive);
    std::fs::write(chilpa_dirs::data().join("queue.json"), serde_json::to_vec(&serde_json::json!({
        "entries":[{"path":path("legacy"),"placement":"original","group":null}], "seconds":12.0
    })).unwrap()).unwrap();
    let mut legacy = Chilpa::new();
    legacy.set_keep_queue(true);
    legacy.restore_queue_session();
    assert_eq!(protected(&legacy), [path("legacy")].into(), "legacy restoration installs the same protection");
}
