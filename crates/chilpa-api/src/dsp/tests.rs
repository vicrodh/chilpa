use super::*;

fn state() -> Dsp {
    Dsp { providers: vec![Provider { id: "test.dsp".into(), name: "Test".into(),
        catalog: Catalog { protocol: 1, presets: vec![Preset::default()] } }], ..Default::default() }
}
#[test]
fn selections_are_scoped_to_output_and_disabled_providers_supply_no_parameters() {
    let mut dsp = state();
    dsp.saved.outputs.insert("output-a".into(), Selection { plugin: "test.dsp".into(), preset: "custom".into(), enabled: true, edited: None });
    assert!(dsp.preset(&dsp.selection("output-a")).is_some());
    assert!(!dsp.selection("output-b").enabled);
    dsp.providers.clear(); assert!(dsp.preset(&dsp.selection("output-a")).is_none());
}
#[test]
fn invalid_catalogs_and_duplicate_ids_are_rejected_as_a_whole() {
    let mut catalog = Catalog { protocol: 1, presets: vec![Preset::default()] };
    catalog.validate().unwrap();
    catalog.presets.push(Preset::default()); assert!(catalog.validate().is_err());
    catalog.presets.pop(); catalog.protocol = 2; assert!(catalog.validate().is_err());
    assert!(serde_json::from_str::<Catalog>(r#"{"protocol":1,"presets":[],"script":"run"}"#).is_err());
}
