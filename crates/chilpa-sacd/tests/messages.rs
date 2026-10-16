//! What a SACD error says, in the window's language.
//!
//! The reasons are marked for the catalog where they are made and were not
//! looked up where they are shown: the sentence came out in Spanish and the
//! reason after its colon in English.
//!
//! A file of its own, because the language is the whole process's and the
//! other tests read the English messages.

use chilpa_i18n::{t, t_args};
use chilpa_sacd::SacdError;

/// The reason in the window's language; the catalog has to know it, or a
/// test comparing English with English would pass without saying anything.
fn translated(reason: &str) -> String {
    let said = t(reason);
    assert_ne!(said, reason, "the catalog has no translation for {reason:?}");
    said
}

#[test]
fn a_sacd_error_gives_its_reason_in_the_window_s_language() {
    chilpa_i18n::set_language("es");

    let path = std::env::temp_dir().join("chilpa-sacd-messages-not-a-sacd.iso");
    std::fs::write(&path, vec![0u8; 600 * chilpa_sacd::SECTOR]).unwrap();
    let refused = chilpa_sacd::open(&path).err().expect("an image of zeros is not a SACD");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        refused.to_string(),
        t_args("invalid table of contents: {}", &[&translated("SACDMTOC was not found")])
    );

    let bad = SacdError::BadSector { lsn: 7, why: chilpa_i18n::mark("impossible packet counts").into() };
    assert_eq!(bad.to_string(), t_args("sector {} is invalid: {}", &["7", &translated("impossible packet counts")]));

    // A reason already put into words, with what it carries, stays as it is.
    let why = t_args("a DST frame does not decode: {}", &["x"]);
    let decoded = SacdError::BadSector { lsn: 7, why: why.clone() };
    assert_eq!(decoded.to_string(), t_args("sector {} is invalid: {}", &["7", &why]));
}
