//! Cuál de los dos caminos se usa, y por qué.
//!
//! Al abrir se intenta el keyring del sistema. Si se pudo, ahí vive una
//! llave maestra de 32 bytes y con ella se cifra todo. Si no se pudo, por la
//! razón que sea, la llave se deriva con HKDF de identificadores del equipo.
//!
//! **Solo Linux, macOS y Windows consultan el keyring.** Es un requisito de
//! corrección, no una preferencia: sin la feature nativa, `keyring` 3.x
//! resuelve a un `mock` en memoria **cuyas escrituras tienen éxito**. Esa
//! llamada no falla hacia el respaldo; entrega una llave que muere con el
//! proceso, y todo lo cifrado en una corrida queda ilegible en la siguiente
//! mientras el backend se reporta como "keyring". El camino derivado, en
//! cambio, sale del mismo id de instalación en cada arranque.
//!
//! El marcador del backend va dentro de cada sobre, para que lo que cifró
//! uno no lo descifre el otro en silencio, y para que si alguien consigue
//! keyring después, sus datos viejos no se ignoren sin avisar.

use std::path::Path;

use hkdf::Hkdf;
// Solo el camino del keyring inventa una llave; el otro la deriva.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use rand::RngExt;
use sha2::Sha256;

use crate::cipher::{open_with_key, seal_with_key};
use crate::error::SecretError;
use crate::install_id;

const MASTER_KEY_LEN: usize = 32;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const KEYRING_ENTRY: &str = "master-key-v1";
const HKDF_INFO: &[u8] = b"chilpa-secrets master-key derivation v1";

const MARKER_KEYRING: u8 = 0;
const MARKER_DERIVED: u8 = 1;

/// Cuál camino quedó activo. Sirve para decírselo al usuario: no es lo
/// mismo tener el keyring del sistema que no tenerlo, y esconderlo sería
/// prometer una seguridad que no está.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// La llave maestra vive en el almacén del sistema. Copiar los archivos
    /// a otra máquina los deja ilegibles.
    Keyring,
    /// La llave se deriva de `machine-id` más el id de instalación. Sigue
    /// atada al equipo, pero quien tenga acceso al sistema de archivos y al
    /// `machine-id` puede rehacerla.
    Derived,
}

impl BackendKind {
    /// Cómo se le dice al usuario, que no tiene por qué saber qué es HKDF.
    pub fn describe(&self) -> &'static str {
        match self {
            BackendKind::Keyring => "en el llavero del sistema",
            BackendKind::Derived => "cifrado y atado a este equipo (no hay llavero del sistema)",
        }
    }
}

pub struct Backend {
    kind: BackendKind,
    master_key: [u8; MASTER_KEY_LEN],
}

impl Backend {
    pub fn new(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Ok(master_key) = try_keyring(service_name) {
            return Ok(Self { kind: BackendKind::Keyring, master_key });
        }

        let master_key = derive_key(service_name, storage_dir)?;
        Ok(Self { kind: BackendKind::Derived, master_key })
    }

    /// Solo el camino derivado, sin tocar el keyring.
    ///
    /// Existe para las pruebas. Llegar al Keychain de verdad desde una
    /// prueba no es más fuerte, es más frágil: macOS ata el permiso de una
    /// entrada al binario exacto que la creó, así que el **segundo** `cargo
    /// test` después de editar código es un binario nuevo contra una entrada
    /// vieja, y el sistema bloquea el proceso con un diálogo que una corrida
    /// headless no puede contestar. Lo que se prueba —ida y vuelta, y
    /// detectar manoseo— es idéntico en los dos caminos.
    pub fn derived_only(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        let master_key = derive_key(service_name, storage_dir)?;
        Ok(Self { kind: BackendKind::Derived, master_key })
    }

    pub fn kind(&self) -> BackendKind {
        self.kind
    }

    fn marker(&self) -> u8 {
        match self.kind {
            BackendKind::Keyring => MARKER_KEYRING,
            BackendKind::Derived => MARKER_DERIVED,
        }
    }

    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
        seal_with_key(&self.master_key, self.marker(), plaintext)
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        open_with_key(&self.master_key, self.marker(), sealed)
    }
}

/// Lee, o crea y guarda, la llave maestra en el almacén del sistema.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn try_keyring(service_name: &str) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use keyring::credential::CredentialPersistence;
    use keyring::Entry;

    // Un build sin la feature de su plataforma exporta el mock en memoria,
    // cuyas escrituras tienen éxito y cuyo contenido muere con el proceso.
    // Eso es peor que el respaldo derivado, que al menos es estable, y no se
    // nota porque nada falla. Se rechaza cualquier backend que no escriba a
    // disco: los reales heredan `UntilDelete` y el mock lo cambia a
    // `EntryOnly`.
    //
    // Alcance: lee el builder de compilación, que es el que usa `Entry::new`
    // aquí. No vería a alguien que lo cambiara en caliente con
    // `keyring::set_default_credential_builder`; nadie lo hace, y contra lo
    // que esto protege es contra un error de configuración del build.
    let persistence = keyring::default::default_credential_builder().persistence();
    if !matches!(persistence, CredentialPersistence::UntilDelete) {
        return Err(SecretError::Keyring(
            "el backend del keyring no escribe a disco (¿mock?); no se usa".into(),
        ));
    }

    let entry = Entry::new(service_name, KEYRING_ENTRY)
        .map_err(|e| SecretError::Keyring(format!("Entry::new: {e}")))?;

    match entry.get_password() {
        Ok(existing) => {
            let bytes = B64
                .decode(existing.trim())
                .map_err(|e| SecretError::Keyring(format!("base64: {e}")))?;
            if bytes.len() != MASTER_KEY_LEN {
                return Err(SecretError::Keyring(format!(
                    "la entrada mide {} bytes y deberían ser {MASTER_KEY_LEN}",
                    bytes.len()
                )));
            }
            let mut out = [0u8; MASTER_KEY_LEN];
            out.copy_from_slice(&bytes);
            Ok(out)
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; MASTER_KEY_LEN];
            rand::rng().fill(&mut key);
            entry
                .set_password(&B64.encode(key))
                .map_err(|e| SecretError::Keyring(format!("set_password: {e}")))?;
            Ok(key)
        }
        Err(e) => Err(SecretError::Keyring(format!("get_password: {e}"))),
    }
}

fn derive_key(
    service_name: &str,
    storage_dir: &Path,
) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    let install_uuid = install_id::load_or_create(storage_dir).map_err(SecretError::Io)?;
    let machine = install_id::machine_id().unwrap_or_default();

    // Ninguno de los tres es secreto. La fuerza está en que los tres tienen
    // que estar en el mismo sistema de archivos para rehacer la llave.
    let mut ikm: Vec<u8> = Vec::with_capacity(service_name.len() + machine.len() + 64);
    ikm.extend_from_slice(service_name.as_bytes());
    ikm.push(0);
    ikm.extend_from_slice(machine.as_bytes());
    ikm.push(0);
    ikm.extend_from_slice(install_uuid.as_bytes());

    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut out = [0u8; MASTER_KEY_LEN];
    hk.expand(HKDF_INFO, &mut out).map_err(|e| SecretError::Other(format!("HKDF: {e}")))?;
    Ok(out)
}
