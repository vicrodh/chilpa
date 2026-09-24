//! What was sealed in one run opens in the next.
//!
//! This is the test that gives the derived fallback its point: if the key
//! did not survive the process, every start would leave the previous one
//! unreadable and nobody would find out until they wanted their account.

use chilpa_secrets::SecretBox;

#[test]
fn a_secret_sealed_before_opens_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();

    // One "run": open the box, seal, and drop everything.
    let sealed = {
        let b = SecretBox::derived_only("chilpa", dir.path()).unwrap();
        b.seal(b"el token de una cuenta").unwrap()
    };

    // Another run, with the box built from scratch.
    let b = SecretBox::derived_only("chilpa", dir.path()).unwrap();
    assert_eq!(
        b.open_sealed(&sealed).unwrap(),
        b"el token de una cuenta",
        "la llave derivada es la misma en cada arranque"
    );
}
