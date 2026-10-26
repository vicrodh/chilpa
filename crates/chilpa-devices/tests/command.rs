use std::process::Command;

#[test]
fn diagnostic_client_previews_then_applies_and_reconnects_without_copying_again() {
    let root = tempfile::tempdir().unwrap();
    let profile = tempfile::tempdir().unwrap();
    let source = profile.path().join("audio.flac");
    std::fs::write(&source, b"fixture bytes").unwrap();
    let request = profile.path().join("request.json");
    std::fs::write(&request, serde_json::to_vec(&serde_json::json!({"tracks":[{
        "id":"track-1","source":source,"artist":"Artist","album":"Album","title":"Song","track_number":1
    }],"playlists":[{"id":"list-1","name":"List","tracks":["track-1","track-1"]}]})).unwrap()).unwrap();
    let invoke = |apply:bool, pair:bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_chilpa-sync"));
        command.arg("--root").arg(root.path()).arg("--state").arg(profile.path().join("sync.db")).arg("--request").arg(&request);
        if pair { command.args(["--pair-name","Card"]); }
        if apply { command.arg("--apply"); }
        command.output().unwrap()
    };
    let preview = invoke(false,true);
    assert!(preview.status.success(), "{}", String::from_utf8_lossy(&preview.stderr));
    assert!(String::from_utf8_lossy(&preview.stdout).contains("actions=2 unchanged=0"));
    assert!(!root.path().join("Music").exists());
    let applied = invoke(true,false);
    assert!(applied.status.success(), "{}", String::from_utf8_lossy(&applied.stderr));
    assert!(String::from_utf8_lossy(&applied.stdout).contains("sync_complete managed_files=2"));
    let next = invoke(false,false);
    assert!(next.status.success(), "{}", String::from_utf8_lossy(&next.stderr));
    assert!(String::from_utf8_lossy(&next.stdout).contains("actions=0 unchanged=2"));
}
