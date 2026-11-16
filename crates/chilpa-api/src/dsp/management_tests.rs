use super::*;
use chilpa_core::dsp::{Channel, Filter, GraphicLayout, Shape};

fn api() -> Chilpa {
    let mut api = Chilpa::new();
    api.dsp = Dsp { initialized: true, providers: vec![Provider { id: "test.dsp".into(), name: "Test".into(), paged: false,
        catalog: Catalog { protocol: 1, presets: vec![Preset::default()], localizations: Default::default() } }], ..Dsp::default() };
    api.remember("file:capture", false);
    api.dsp_action("choose", r#"{"plugin":"test.dsp","preset":"custom"}"#);
    assert!(api.dsp.error.is_empty());
    api
}
fn action(api: &mut Chilpa, name: &str, value: Value) {
    api.dsp_action(name, &value.to_string());
    assert!(api.dsp.error.is_empty(), "{name}: {}", api.dsp.error);
}
fn current(api: &Chilpa) -> Preset { api.dsp.preset(&api.dsp.selection(&api.chosen_output().0)).unwrap() }

#[test]
fn replacing_renaming_and_autosave_keep_one_personal_identity() {
    let mut api = api();
    action(&mut api, "save", json!({"name":"My correction"}));
    let id = api.dsp.selection(&api.chosen_output().0).preset;
    action(&mut api, "favorite", json!({"key":format!("test.dsp/{id}")}));
    let mut preset = current(&api); preset.tone.bass_db = 4.0;
    action(&mut api, "edit", json!(preset));
    assert_eq!(api.dsp.saved.custom[0].tone.bass_db, 0.0);
    action(&mut api, "replace", json!({}));
    action(&mut api, "rename", json!({"name":"Renamed correction"}));
    assert_eq!(api.dsp.saved.custom.len(), 1);
    assert_eq!(api.dsp.saved.custom[0].id, id);
    assert_eq!(api.dsp.saved.custom[0].tone.bass_db, 4.0);
    action(&mut api, "autosave", json!({"enabled":true}));
    let mut preset = current(&api); preset.tone.bass_db = 6.0;
    action(&mut api, "edit", json!(preset));
    assert_eq!(api.dsp.saved.custom[0].tone.bass_db, 6.0);
    assert_eq!(api.dsp.saved.custom[0].name, "Renamed correction");
    assert!(api.dsp.selection(&api.chosen_output().0).edited.is_none());
    assert!(api.dsp.saved.favorites.contains(&format!("test.dsp/{id}")));
    let mut restored = Dsp::default(); restored.load_preferences();
    assert!(restored.saved.autosave);
    assert_eq!(restored.saved.custom[0], api.dsp.saved.custom[0]);
}

#[test]
fn output_rules_prefer_device_then_explicit_class_then_general_default() {
    let mut api = api();
    action(&mut api, "assign", json!({"scope":"default"}));
    api.remember("file:second", false);
    assert_eq!(api.dsp.selection_scope("file:second"), "default");
    assert!(api.dsp.selection("file:second").enabled);
    action(&mut api, "classify_output", json!({"class":"headphones"}));
    action(&mut api, "assign", json!({"scope":"headphones"}));
    assert_eq!(api.dsp.selection_scope("file:second"), "headphones");
    action(&mut api, "enable", json!({"enabled":false}));
    assert_eq!(api.dsp.selection_scope("file:second"), "device");
    assert!(!api.dsp.selection("file:second").enabled);
    action(&mut api, "clear_assignment", json!({}));
    assert!(api.dsp.selection("file:second").enabled);
    action(&mut api, "clear_rule", json!({"scope":"headphones"}));
    assert_eq!(api.dsp.selection_scope("file:second"), "default");
    api.dsp_action("assign", r#"{"scope":"unknown"}"#);
    assert!(!api.dsp.error.is_empty());
    assert_eq!(api.dsp.saved.rules.len(), 1);
}

#[test]
fn batch_import_and_delete_are_atomic_and_export_roundtrips() {
    let mut api = api();
    let mut first = Preset::default(); first.name = "First".into(); first.tone.treble_db = 2.0;
    let mut second = first.clone(); second.name = "Second".into(); second.tone.bass_db = 3.0;
    let text = json!([first, second]).to_string();
    action(&mut api, "import_many", json!({"text":text}));
    assert_eq!(api.dsp.saved.custom.len(), 2);
    let exported = api.dsp_export(r#"["user-1","user-2"]"#);
    let imported = exchange::parse_many(&exported, "Imported").unwrap();
    assert_eq!(imported, api.dsp.saved.custom);
    api.dsp_action("import_many", &json!({"text":r#"[{"name":"Good"},{"name":"Invalid","tone":{"bass_db":90}}]"#}).to_string());
    assert!(!api.dsp.error.is_empty()); assert_eq!(api.dsp.saved.custom.len(), 2);
    api.dsp_action("delete_many", r#"{"ids":["user-1","custom"]}"#);
    assert!(!api.dsp.error.is_empty()); assert_eq!(api.dsp.saved.custom.len(), 2);
    action(&mut api, "delete_many", json!({"ids":["user-1","user-2"]}));
    assert!(api.dsp.saved.custom.is_empty());
}

#[test]
fn configurable_graphic_eq_keeps_tone_and_the_original_correction() {
    let mut api = api();
    let mut original = current(&api);
    original.filters = (0..10).map(|index| Filter { frequency: 100.0 + index as f64 * 200.0, gain_db: -2.0, ..Filter::default() }).collect();
    original.tone.bass_db = 3.0; original.preamp_db = -4.0;
    action(&mut api, "edit", json!(original));
    action(&mut api, "graphic_configure", json!(GraphicLayout {bands:32, minimum_hz:20.0, maximum_hz:20_000.0}));
    assert_eq!(current(&api).filters.len(), 32);
    assert!((current(&api).filters[31].frequency - 20_000.0).abs() < 1e-8);
    action(&mut api, "graphic_band", json!({"index":31,"gain":6}));
    assert_eq!(current(&api).filters[31].gain_db, 6.0);
    assert_eq!(current(&api).tone.bass_db, 3.0);
    assert_eq!(current(&api).preamp_db, -4.0);
    action(&mut api, "graphic_restore", json!({}));
    assert_eq!(current(&api), original);
}

#[test]
fn locked_filters_reject_edits_and_can_be_reordered_and_unlocked() {
    let mut api = api();
    let mut preset = current(&api);
    preset.filters = vec![Filter {locked:true, channel:Channel::Left, ..Filter::default()}, Filter::default()];
    action(&mut api, "edit", json!(preset));
    api.dsp_action("filter_edit", r#"{"index":0,"field":"frequency","value":1800}"#);
    assert!(!api.dsp.error.is_empty());
    assert_eq!(current(&api).filters[0].frequency, 1000.0);
    action(&mut api, "filter_move", json!({"index":0,"target":1}));
    assert!(current(&api).filters[1].locked);
    action(&mut api, "filter_edit", json!({"index":1,"field":"locked","value":false}));
    action(&mut api, "filter_edit", json!({"index":1,"field":"shape","value":"band_pass"}));
    action(&mut api, "filter_edit", json!({"index":1,"field":"channel","value":"right"}));
    assert_eq!(current(&api).filters[1].shape, Shape::BandPass);
    assert_eq!(current(&api).filters[1].channel, Channel::Right);
}

#[test]
fn invalid_graphic_origins_in_rules_are_sanitized_and_restore_revalidates() {
    let mut api = api();
    let poison = json!({"preset":"custom","edited":{"graphic_layout":{"bands":usize::MAX,"minimum_hz":20,"maximum_hz":20000}},"enabled":true});
    let mut selection = api.dsp.selection(&api.chosen_output().0);
    selection.graphic_origin = Some(serde_json::from_value(poison).unwrap());
    assert!(selection.restore_graphic().is_err());
    selection.graphic_origin = Some(serde_json::from_value(json!({"preset":"custom","edited":{"graphic_layout":{"bands":100000000,"minimum_hz":20,"maximum_hz":20000}},"enabled":true})).unwrap());
    api.dsp.saved.rules.insert("default".into(), selection);
    api.dsp.persist().unwrap();
    let mut restored = Dsp::default(); restored.load_preferences();
    assert!(restored.saved.rules["default"].graphic_origin.is_none());
    let invalid = GraphicLayout {bands:usize::MAX, minimum_hz:20.0, maximum_hz:20000.0};
    assert!(invalid.frequencies().is_empty());
}
