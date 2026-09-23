//! Lo cifrado en una corrida se abre en la siguiente.
//!
//! Es la prueba que le da sentido al respaldo derivado: si la llave no
//! sobreviviera al proceso, cada arranque dejaría ilegible lo del anterior y
//! nadie se enteraría hasta querer usar su cuenta.

use chilpa_secrets::SecretBox;

#[test]
fn a_secret_sealed_before_opens_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();

    // Una "corrida": se abre la caja, se cifra, y se suelta todo.
    let sealed = {
        let b = SecretBox::derived_only("chilpa", dir.path()).unwrap();
        b.seal(b"el token de una cuenta").unwrap()
    };

    // Otra corrida, con la caja armada desde cero.
    let b = SecretBox::derived_only("chilpa", dir.path()).unwrap();
    assert_eq!(
        b.open_sealed(&sealed).unwrap(),
        b"el token de una cuenta",
        "la llave derivada es la misma en cada arranque"
    );
}
