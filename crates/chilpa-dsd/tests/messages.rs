//! What a DSD error says, in the window's language.
//!
//! The reasons are marked for the catalog where they are made and were not
//! looked up where they are shown: the sentence came out in Spanish and the
//! reason after its colon in English.
//!
//! A file of its own, because the language is the whole process's.

use chilpa_i18n::{t, t_args};

#[test]
fn a_dsd_error_gives_its_reason_in_the_window_s_language() {
    chilpa_i18n::set_language("es");
    let path = std::env::temp_dir().join("chilpa-dsd-messages-not-a-dsf.dsf");
    std::fs::write(&path, b"RIFF, which a DSF is not").unwrap();
    let refused = chilpa_dsd::DsfReader::open(&path).err().expect("it is not a DSF");
    let _ = std::fs::remove_file(&path);

    let reason = "it does not start with the DSD chunk";
    assert_ne!(t(reason), reason, "the catalog has to know the reason for this to say anything");
    assert_eq!(refused.to_string(), t_args("damaged DSD file: {}", &[&t(reason)]));
}
