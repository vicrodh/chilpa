use super::*;
use crate::dsp::{Catalog, Loaded, Preset, Provider, Selection};
use std::{collections::BTreeMap, sync::mpsc, time::{Duration, Instant}};

fn state() -> Dsp {
    Dsp {
        initialized: true,
        providers: vec![Provider { id: "test.dsp".into(), name: "Test".into(), paged: true,
            catalog: Catalog { protocol: 1, presets: vec![
                Preset { id: "flat".into(), name: "Flat".into(), ..Default::default() },
                Preset { id: "chosen".into(), name: "Chosen".into(), ..Default::default() },
            ], localizations: BTreeMap::new() } }],
        next_scan: Some(Instant::now() + Duration::from_secs(60)),
        applied: Some("null".into()), synced_at: Some(Instant::now()),
        ..Default::default()
    }
}

fn selection(id: &str) -> Selection {
    Selection { plugin: "test.dsp".into(), preset: id.into(), enabled: true, ..Default::default() }
}

fn old_name(dsp: &Dsp, output: &str, language: &str) -> Option<String> {
    let selected = dsp.selection(output);
    let preset = dsp.preset(&selected)?;
    Some(dsp.providers.iter().find(|provider| provider.id == selected.plugin).filter(|_| selected.edited.is_none())
        .map_or(preset.name.clone(), |provider| provider.catalog.display(&preset, language).name))
}

fn assert_name(dsp: &Dsp, output: &str, language: &str, expected: &str) {
    assert_eq!(dsp.status_name(output, language), Some(expected.into()));
    assert_eq!(dsp.status_name(output, language), old_name(dsp, output, language));
}

#[test]
fn status_name_preserves_device_class_default_and_unavailable_provider_precedence() {
    let mut dsp = state();
    assert_name(&dsp, "output", "en", "Flat");
    dsp.saved.rules.insert("default".into(), selection("chosen"));
    assert_name(&dsp, "output", "en", "Chosen");
    dsp.saved.output_classes.insert("output".into(), "headphones".into());
    dsp.saved.rules.insert("headphones".into(), selection("flat"));
    assert_name(&dsp, "output", "en", "Flat");
    dsp.saved.outputs.insert("output".into(), selection("chosen"));
    assert_name(&dsp, "output", "en", "Chosen");
    dsp.saved.outputs.get_mut("output").unwrap().plugin = "missing".into();
    assert_name(&dsp, "output", "en", "Flat");
    dsp.providers.clear();
    assert_eq!(dsp.status_name("output", "en"), None);
    assert_eq!(dsp.status_name("output", "en"), old_name(&dsp, "output", "en"));
}

#[test]
fn status_name_matches_localization_edited_personal_and_resolved_presets() {
    let mut dsp = state();
    dsp.saved.outputs.insert("output".into(), selection("chosen"));
    let labels = chilpa_core::dsp::Labels { name: "Localized".into(), ..Default::default() };
    dsp.providers[0].catalog.localizations.insert("en".into(), BTreeMap::from([("chosen".into(), labels)]));
    assert_name(&dsp, "output", "en-US", "Localized");
    dsp.providers[0].catalog.localizations.insert("en-US".into(), BTreeMap::new());
    assert_name(&dsp, "output", "en-US", "Chosen");
    dsp.saved.custom.push(Preset { id: "chosen".into(), name: "Personal".into(), ..Default::default() });
    assert_name(&dsp, "output", "fr", "Personal");
    dsp.saved.outputs.get_mut("output").unwrap().edited = Some(Preset { id: "chosen".into(), name: "Draft".into(), ..Default::default() });
    assert_name(&dsp, "output", "en", "Draft");
    let mut resolved = selection("remote");
    resolved.resolved = Some(Preset { id: "remote".into(), name: "Resolved".into(), ..Default::default() });
    dsp.saved.outputs.insert("output".into(), resolved);
    assert_name(&dsp, "output", "en", "Resolved");
    dsp.saved.outputs.get_mut("output").unwrap().resolved.as_mut().unwrap().id = "stale".into();
    assert_eq!(dsp.status_name("output", "en"), None);
    assert_eq!(dsp.status_name("output", "en"), old_name(&dsp, "output", "en"));
}

fn api() -> Chilpa {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.dsp = state();
    api
}

fn assert_document_status(api: &mut Chilpa) {
    let status = api.dsp_status();
    assert!(api.dsp.analysis.is_none(), "status must leave the editor analysis unallocated");
    let value: serde_json::Value = serde_json::from_str(&api.dsp_json()).unwrap();
    assert_eq!(status, (value["available"].as_bool().unwrap(), value["active"].as_bool().unwrap(),
        value["preset"]["name"].as_str().unwrap_or("").to_owned()));
    api.dsp.analysis = None;
}

#[test]
fn status_matches_the_full_document_without_constructing_editor_analysis() {
    let mut api = api();
    for (rate, playing, passthrough) in [(0, false, false), (48_000, true, false),
        (1_536_001, false, false), (48_000, true, true), (48_000, false, true)] {
        api.sample_rate = rate;
        api.now.playing = playing;
        api.dsp.passthrough = passthrough;
        assert_document_status(&mut api);
    }
    for _ in 0..100 { assert!(api.dsp_status().0); }
    assert!(api.dsp.analysis.is_none());
    api.dsp.providers.clear();
    assert_document_status(&mut api);
}

#[test]
fn status_consumes_provider_completion_without_opening_the_editor() {
    let mut api = api();
    let providers = std::mem::take(&mut api.dsp.providers);
    assert_eq!(api.dsp_status(), (false, false, String::new()));
    let (tx, rx) = mpsc::channel();
    api.dsp.pending = Some(rx);
    assert!(tx.send(Loaded { signature: api.dsp.signature.clone(), providers, error: String::new() }).is_ok());
    assert_eq!(api.dsp_status(), (true, false, "Flat".into()));
    assert!(api.dsp.pending.is_none());
    assert!(api.dsp.analysis.is_none());
    api.dsp.providers.clear();
    assert_eq!(api.dsp_status(), (false, false, String::new()));
}

#[test]
fn status_bypass_matches_cast_high_rate_and_playing_native_dsd() {
    assert!(unblocked(0, false, false, false));
    assert!(unblocked(1_536_000, false, true, false));
    assert!(!unblocked(1_536_001, false, false, false));
    assert!(!unblocked(48_000, true, false, false));
    assert!(!unblocked(48_000, false, true, true));
    assert!(unblocked(48_000, false, false, true));
}
