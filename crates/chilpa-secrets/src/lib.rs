//! Dónde se guarda un secreto para que no quede en claro.
//!
//! Lo usa Chilpa para las credenciales de los plugins (ADR-008). Un token en
//! texto plano lo lee cualquier proceso que corra como el usuario, y eso no
//! se puede sostener en un programa que promete que un plugin no ve tus
//! secretos.
//!
//! # Dos caminos, una sola API
//!
//! 1. **El llavero del sistema** (lo preferido): una llave maestra AES-256
//!    vive adentro del almacén del sistema —libsecret en Linux, Keychain en
//!    macOS, DPAPI en Windows— y con ella se cifra todo lo demás.
//! 2. **Derivado del equipo** (cuando no hay llavero): la llave sale de
//!    HKDF-SHA256 sobre el `machine-id` más un UUID de instalación. Es más
//!    débil, porque quien tenga acceso al sistema de archivos puede
//!    rehacerla, pero sirve en una caja headless y **sigue atada a ese
//!    equipo**.
//!
//! Los dos producen el mismo sobre, así que quien llama no tiene que saber
//! cuál se usó. Lo que sí puede saber, y decirle al usuario, es
//! [`SecretBox::backend`].
//!
//! # Contra qué protege
//!
//! - Copiar el directorio a otra máquina. Allá no está la entrada del
//!   llavero, o el `machine-id` es otro, y lo cifrado no abre.
//! - Mirar los archivos. Un token no se saca leyendo el blob.
//!
//! # Contra qué no
//!
//! - Alguien con shell en la misma máquina que pueda correr Chilpa: siempre
//!   puede pedirle a Chilpa que abra, y eso está bien, es el mismo modelo
//!   que cualquier cifrado local.
//! - Recompilar Chilpa para que copie lo descifrado. Eso es un ataque al
//!   código fuente, no al cifrado.
//!
//! **Nada se guarda en claro, nunca, en ninguno de los dos caminos.**

mod backend;
mod cipher;
mod envelope;
mod error;
mod install_id;

pub use backend::BackendKind;
pub use error::SecretError;

use std::path::Path;

/// Cierra y abre secretos pequeños.
pub struct SecretBox {
    backend: backend::Backend,
}

impl SecretBox {
    /// Abre la caja. `service_name` es cómo se llama la entrada del llavero;
    /// `storage_dir` es dónde vive el id de instalación.
    pub fn open(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        Ok(Self { backend: backend::Backend::new(service_name, storage_dir)? })
    }

    /// Solo el camino derivado, sin tocar el llavero. Para pruebas.
    pub fn derived_only(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        Ok(Self { backend: backend::Backend::derived_only(service_name, storage_dir)? })
    }

    /// Qué camino quedó activo, para poder decirlo.
    pub fn backend(&self) -> BackendKind {
        self.backend.kind()
    }

    /// Cifra. Lo que sale se puede escribir a disco tal cual.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
        self.backend.seal(plaintext)
    }

    /// Descifra. Falla si lo tocaron, si lo cifró el otro backend, o si la
    /// llave maestra ya no es la misma.
    pub fn open_sealed(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        self.backend.open(sealed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_in(dir: &Path) -> SecretBox {
        SecretBox::derived_only("chilpa-test", dir).unwrap()
    }

    #[test]
    fn what_goes_in_comes_back_out() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        let sealed = b.seal(b"un token que nadie debe leer").unwrap();
        assert_ne!(sealed, b"un token que nadie debe leer", "no quedó en claro");
        assert_eq!(b.open_sealed(&sealed).unwrap(), b"un token que nadie debe leer");
    }

    #[test]
    fn the_same_secret_twice_does_not_look_the_same() {
        // Un nonce por cifrado: si no, dos cuentas con el mismo token se
        // verían iguales en disco, y eso ya dice algo.
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        assert_ne!(b.seal(b"igual").unwrap(), b.seal(b"igual").unwrap());
    }

    #[test]
    fn a_byte_changed_is_a_secret_refused() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        let mut sealed = b.seal(b"token").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(b.open_sealed(&sealed).is_err(), "GCM autentica, no solo cifra");
    }

    #[test]
    fn what_another_install_sealed_does_not_open_here() {
        // Es la promesa de fondo: copiar los archivos a otro lado no
        // alcanza, porque allá el id de instalación es otro.
        let one = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let sealed = box_in(one.path()).seal(b"token").unwrap();
        assert!(box_in(other.path()).open_sealed(&sealed).is_err());
    }

    #[test]
    fn a_sealed_blob_that_is_too_short_says_so_instead_of_crashing() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        assert!(matches!(b.open_sealed(b"").unwrap_err(), SecretError::Malformed(_)));
        assert!(matches!(b.open_sealed(b"\x01\x01corto").unwrap_err(), SecretError::Malformed(_)));
    }

    #[test]
    fn what_one_backend_sealed_the_other_refuses_by_name() {
        // Sin el marcador, esto fallaría con un error de cripto que no dice
        // nada. Con él se puede decir qué pasó.
        let dir = tempfile::tempdir().unwrap();
        let mut sealed = box_in(dir.path()).seal(b"token").unwrap();
        sealed[1] = 0; // como si lo hubiera cerrado el llavero
        assert!(matches!(
            box_in(dir.path()).open_sealed(&sealed).unwrap_err(),
            SecretError::BackendMismatch
        ));
    }
}
