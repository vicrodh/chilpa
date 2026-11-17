//! Channel selection persists independently of devices and keeps originals by default.
use chilpa_api::{Chilpa, RowNotes};
fn row(api: &mut Chilpa) -> chilpa_api::SettingsRow {
    api.settings_rows("audio", &RowNotes::default())
        .0
        .into_iter()
        .find(|row| row.key == "audio.channelMode")
        .expect("Audio settings must offer original channels and stereo conversion")
}
#[test]
fn original_channels_are_default_and_both_choices_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(directory.path()) {
        std::env::set_var(name, path);
    }
    let mut api = Chilpa::new();
    let initial = row(&mut api);
    assert_eq!(initial.control, "select");
    assert_eq!(initial.value, "original");
    assert_eq!(initial.values, ["original", "stereo"]);
    assert_eq!(initial.options.len(), 2);
    assert!(api.adjust("audio.channelMode", "stereo"));
    assert_eq!(row(&mut Chilpa::new()).value, "stereo");
    assert!(api.adjust("audio.channelMode", "unknown"));
    assert_eq!(
        row(&mut Chilpa::new()).value,
        "stereo",
        "invalid choices must preserve the saved mode"
    );
    assert!(api.adjust("audio.channelMode", "original"));
    assert_eq!(row(&mut Chilpa::new()).value, "original");
}
