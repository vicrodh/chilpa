//! What a manifest error says, in the window's language.
//!
//! The field is marked for the catalog where the error is made and was not
//! looked up where it is shown, so the sentence came out half in Spanish.
//!
//! A file of its own, because the language is the whole process's.

use chilpa_i18n::{t, t_args};
use chilpa_plugin::manifest::ManifestError;

#[test]
fn a_manifest_error_names_the_missing_field_in_the_window_s_language() {
    chilpa_i18n::set_language("es");
    let field = chilpa_i18n::mark("the name");
    assert_ne!(t(field), field, "the catalog has to know the field for this to say anything");
    assert_eq!(ManifestError::Empty(field).to_string(), t_args("{} is missing", &[&t(field)]));
}
