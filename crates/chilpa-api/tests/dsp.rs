//! I load the exact package in an isolated profile, then exercise the facade.
use chilpa_api::Chilpa;
use serde_json::{Value, json};
use std::{path::PathBuf, time::{Duration, Instant}};

fn wait(api: &mut Chilpa, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state: Value = serde_json::from_str(&api.dsp_json()).unwrap();
        if condition(&state) { return state; }
        assert!(Instant::now() < deadline, "DSP did not settle: {state}");
        std::thread::yield_now();
    }
}
#[test]
fn packaged_dsp_loads_with_approval_and_keeps_presets_favorites_and_output_state() {
    let Some(package) = std::env::var_os("CHILPA_DSP_PACKAGE").map(PathBuf::from) else {
        eprintln!("CHILPA_DSP_PACKAGE is unset; the packaged DSP smoke was not run."); return;
    };
    let profile = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", profile.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", profile.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", profile.path().join("cache"));
    let manifest: Value = serde_json::from_slice(&std::fs::read(package.join("manifest.json")).unwrap()).unwrap();
    let id = manifest["id"].as_str().unwrap();
    let installed = Chilpa::plugins_dir().join(id); std::fs::create_dir_all(&installed).unwrap();
    for file in ["manifest.json", "plugin.wasm"] { std::fs::copy(package.join(file), installed.join(file)).unwrap(); }
    let mut api = Chilpa::new(); api.remember("file:capture", false); api.load_plugins();
    assert!(!serde_json::from_str::<Value>(&api.dsp_json()).unwrap()["available"].as_bool().unwrap());
    api.approve_plugin(id);
    let state = wait(&mut api, |s| s["available"] == true);
    assert!(api.call_plugin(id, "chilpa_dsp_presets", br#"{"protocol":999}"#).is_err());
    assert!(api.call_plugin(id, "chilpa_dsp_presets", b"not JSON").is_err());
    let presets = state["presets"].as_array().unwrap(); assert!(presets.len() >= 50);
    for p in presets {
        let preset: chilpa_core::dsp::Preset = serde_json::from_value(p["preset"].clone()).unwrap();
        preset.validate().unwrap();
        for rate in [8000, 44_100, 48_000, 96_000, 192_000, 384_000] {
            let gain = preset.preamp_for_rate(rate); assert!(gain.is_finite() && gain <= 0.0, "{} at {rate}", preset.id);
            assert!(preset.curve(rate).iter().all(|v| v.is_finite()));
        }
    }
    for preset in ["hd650", "edition-xs", "pxc550", "pxc550-ii", "soundbar-21"] {
        api.dsp_action("choose", &json!({"plugin":id,"preset":preset}).to_string());
        let state: Value = serde_json::from_str(&api.dsp_json()).unwrap(); assert_eq!(state["preset"]["id"], preset);
        assert_eq!(state["selection"]["enabled"], true); assert_eq!(state["active"], false, "no audio is playing");
    }
    let key = format!("{id}/hd650");
    api.dsp_action("favorite", &json!({"key":key}).to_string());
    api.dsp_action("save", r#"{"name":"My soundbar"}"#);
    api.dsp_action("enable", r#"{"enabled":false}"#);
    let state: Value = serde_json::from_str(&api.dsp_json()).unwrap();
    assert!(state["favorites"].as_array().unwrap().contains(&json!(key)));
    assert_eq!(state["selection"]["enabled"], false);
    api.remember("file:dsp-secondary", false);
    let other: Value = serde_json::from_str(&api.dsp_json()).unwrap();
    assert_eq!(other["selection"]["enabled"], false);
    api.dsp_action("choose", &json!({"plugin":id,"preset":"hd650"}).to_string());
    api.remember("file:capture", false);
    let restored: Value = serde_json::from_str(&api.dsp_json()).unwrap();
    assert_eq!(restored["preset"]["name"], "My soundbar");
    assert_eq!(restored["selection"]["enabled"], false);
    api.dsp_action("import", r#"{"text":"Filter 1: ON AP Fc 100 Hz Q 1"}"#);
    assert!(!serde_json::from_str::<Value>(&api.dsp_json()).unwrap()["error"].as_str().unwrap().is_empty());
    let mut restarted = Chilpa::new(); restarted.load_plugins();
    let state = wait(&mut restarted, |s| s["available"] == true);
    assert!(state["favorites"].as_array().unwrap().contains(&json!(key)));
    assert_eq!(state["preset"]["name"], "My soundbar");
    assert_eq!(state["selection"]["enabled"], false);
    restarted.revoke_plugin(id);
    let state = wait(&mut restarted, |s| s["available"] == false);
    assert_eq!(state["active"], false);
}
