use super::*;

fn state() -> Dsp {
    Dsp { providers: vec![Provider { id: "test.dsp".into(), name: "Test".into(),
        catalog: Catalog { protocol: 1, presets: vec![Preset::default()], localizations: Default::default() } }], ..Default::default() }
}
#[test]
fn selections_are_scoped_to_output_and_disabled_providers_supply_no_parameters() {
    let mut dsp = state();
    dsp.saved.outputs.insert("output-a".into(), Selection { plugin: "test.dsp".into(), preset: "custom".into(), enabled: true, ..Selection::default() });
    assert!(dsp.preset(&dsp.selection("output-a")).is_some());
    assert!(!dsp.selection("output-b").enabled);
    dsp.providers.clear(); assert!(dsp.preset(&dsp.selection("output-a")).is_none());
}

#[test]
fn graphic_edits_keep_the_previous_draft_and_bypass_across_serialization() {
    let original = Preset { balance: 0.25, preamp_db: -8.0, ..Preset::default() };
    let mut selected = Selection { plugin: "test.dsp".into(), preset: "custom".into(),
        enabled: false, edited: Some(original.clone()), ..Selection::default() };
    selected.begin_graphic(original.clone());
    let graphic = selected.edited.as_mut().unwrap();
    graphic.filters[0].gain_db = 5.0;
    graphic.validate().unwrap();
    assert!(graphic::matches(graphic));
    assert_eq!(graphic.balance, original.balance);
    let text = serde_json::to_string(&selected).unwrap();
    let mut restored: Selection = serde_json::from_str(&text).unwrap();
    assert!(restored.graphic_origin.as_ref().unwrap().valid());
    restored.restore_graphic().unwrap();
    assert_eq!(restored.edited, Some(original));
    assert!(!restored.enabled);
    assert!(restored.graphic_origin.is_none());
}

#[test]
fn graphic_restore_keeps_an_unedited_catalog_selection_and_rejects_missing_backup() {
    let mut selected = Selection { preset: "custom".into(), ..Selection::default() };
    selected.begin_graphic(Preset::default());
    selected.begin_graphic(selected.edited.clone().unwrap());
    selected.restore_graphic().unwrap();
    assert_eq!(selected.preset, "custom");
    assert!(selected.edited.is_none());
    assert!(!selected.enabled);
    assert!(selected.restore_graphic().is_err());
}

#[test]
fn graphic_backups_reject_invalid_parameters_and_old_settings_still_load() {
    let previous: Selection = serde_json::from_str(r#"{"plugin":"test.dsp","preset":"custom","enabled":true,"edited":null}"#).unwrap();
    assert!(previous.graphic_origin.is_none());
    let mut selected = previous;
    selected.begin_graphic(Preset::default());
    assert!(selected.graphic_origin.as_ref().unwrap().valid());
    let mut invalid = Preset::default(); invalid.preamp_db = 20.0;
    selected.edited = Some(invalid);
    selected.graphic_origin = None;
    selected.begin_graphic(Preset::default());
    assert!(!selected.graphic_origin.as_ref().unwrap().valid());
}
#[test]
fn invalid_catalogs_and_duplicate_ids_are_rejected_as_a_whole() {
    let mut catalog = Catalog { protocol: 1, presets: vec![Preset::default()], localizations: Default::default() };
    catalog.validate().unwrap();
    catalog.presets.push(Preset::default()); assert!(catalog.validate().is_err());
    catalog.presets.pop(); catalog.protocol = 2; assert!(catalog.validate().is_err());
    assert!(serde_json::from_str::<Catalog>(r#"{"protocol":1,"presets":[],"script":"run"}"#).is_err());
}

#[test]
fn a_preset_of_precise_filters_shows_its_level_on_each_graphic_band() {
    use chilpa_core::dsp::{Filter, Shape};
    let desktop = Preset { filters: vec![Filter { shape: Shape::HighPass, frequency: 45.0, gain_db: 0.0, q: 0.707 }], ..Preset::default() };
    let levels = graphic::levels(&desktop, 48_000);
    assert_eq!(levels.len(), 10, "one level per band");
    assert!(levels[0] < -3.0, "a high-pass at 45 Hz reads low on the 31 Hz band: {}", levels[0]);
    assert!(levels[5].abs() < 0.1, "and flat at 1 kHz: {}", levels[5]);
    assert!(graphic::levels(&Preset::default(), 48_000).iter().all(|level| *level == 0.0), "a flat preset reads flat");
}

#[test]
fn the_poll_chooses_the_output_again_only_when_it_is_due() {
    let now = Instant::now();
    assert!(recheck_due(false, Some(now), now), "nothing applied yet");
    assert!(recheck_due(true, None, now), "never chosen");
    assert!(!recheck_due(true, Some(now), now + Duration::from_secs(1)), "a second later it stands");
    assert!(!recheck_due(true, Some(now), now + OUTPUT_RECHECK - Duration::from_millis(1)));
    assert!(recheck_due(true, Some(now), now + OUTPUT_RECHECK), "old enough to look again");
}
