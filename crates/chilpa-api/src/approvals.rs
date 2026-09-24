//! Qué aprobó el usuario de cada plugin (ADR-008).
//!
//! Un plugin no corre hasta que alguien dijo que sí. Lo que se guarda no es
//! un "sí" a secas sino **lo que pidió cuando se le dijo que sí**: el tipo
//! de plugin y sus capacidades. Si una versión nueva pide algo más, la
//! aprobación deja de valer y se vuelve a preguntar, y se puede decir qué
//! es lo nuevo en lugar de un "cambió, vuelve a aprobar".
//!
//! Vive en un archivo de texto junto a los plugins, legible a propósito:
//! quien quiera saber qué autorizó puede leerlo sin la aplicación.

use chilpa_plugin::manifest::{Asked, Manifest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// El archivo, al lado de las carpetas de los plugins.
const FILE: &str = "approvals.json";

/// Lo aprobado, por id de plugin.
#[derive(Default)]
pub struct Approvals {
    path: PathBuf,
    approved: BTreeMap<String, Asked>,
}

impl Approvals {
    /// Lee lo que haya. Un archivo que no existe quiere decir que no se ha
    /// aprobado nada, que es lo correcto para una instalación nueva; uno
    /// ilegible quiere decir lo mismo, porque la alternativa sería asumir
    /// permisos que nadie dio.
    pub fn load(root: &Path) -> Self {
        let path = root.join(FILE);
        let approved = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, approved }
    }

    /// Cierto si lo que el manifest pide hoy cabe en lo que se aprobó.
    pub fn covers(&self, manifest: &Manifest) -> bool {
        match self.approved.get(&manifest.id) {
            Some(before) => manifest.asks_for().beyond(before).is_empty(),
            None => false,
        }
    }

    /// Lo que pide de más respecto a lo aprobado. Vacío si no hay nada
    /// nuevo; todo lo que pide si nunca se aprobó.
    pub fn beyond(&self, manifest: &Manifest) -> Vec<String> {
        // Uno nunca aprobado se compara contra "nada aprobado", y no por un
        // camino aparte: tenerlos separados hacía que a un plugin nuevo no
        // se le dijera a dónde te manda a dar permiso, porque esa línea
        // solo la ponía el otro camino.
        let nothing = Asked { kind: manifest.kind, capabilities: Vec::new(), auth: None };
        let before = self.approved.get(&manifest.id).unwrap_or(&nothing);
        manifest.asks_for().beyond(before)
    }

    /// Cierto si a ese id se le aprobó algo alguna vez, sea lo que pide hoy
    /// o no. Sirve para decir "pide algo nuevo" en lugar de "es nuevo".
    pub fn knows(&self, id: &str) -> bool {
        self.approved.contains_key(id)
    }

    /// Guarda que el usuario aprobó lo que ese plugin pide ahora.
    pub fn approve(&mut self, manifest: &Manifest) -> std::io::Result<()> {
        self.approved.insert(manifest.id.clone(), manifest.asks_for());
        self.write()
    }

    /// Quita la aprobación. El plugin deja de correr en el siguiente
    /// arranque y lo que pida vuelve a preguntarse.
    pub fn revoke(&mut self, id: &str) -> std::io::Result<()> {
        self.approved.remove(id);
        self.write()
    }

    fn write(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.approved)
            .unwrap_or_else(|_| "{}".to_string());
        std::fs::write(&self.path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, capabilities: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"{{"id":"{id}","name":"Uno","version":"1","api_version":1,
                 "kind":"source","capabilities":[{capabilities}]}}"#
        ))
        .unwrap()
    }

    fn temporal(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("chilpa-approvals-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn nothing_is_approved_until_someone_says_so() {
        let dir = temporal("nuevo");
        let a = Approvals::load(&dir);
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        assert!(!a.covers(&m), "una instalación nueva no aprueba nada sola");
        assert!(!a.knows("com.a.b"));
        assert_eq!(a.beyond(&m).len(), 1, "y se puede decir qué pide");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_was_approved_survives_a_restart() {
        let dir = temporal("vuelve");
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        Approvals::load(&dir).approve(&m).unwrap();
        assert!(Approvals::load(&dir).covers(&m), "se leyó de vuelta");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn asking_for_more_than_was_approved_asks_again() {
        let dir = temporal("mas");
        let before = manifest("com.a.b", r#"{"type":"storage"}"#);
        let mut a = Approvals::load(&dir);
        a.approve(&before).unwrap();

        // La versión nueva quiere salir a internet, y eso nadie lo aprobó.
        let now = manifest(
            "com.a.b",
            r#"{"type":"storage"},{"type":"network","domains":["api.ejemplo.com"]}"#,
        );
        assert!(!a.covers(&now));
        let added = a.beyond(&now);
        assert_eq!(added.len(), 1, "solo lo nuevo, no todo otra vez: {added:?}");
        assert!(added[0].contains("api.ejemplo.com"), "{added:?}");
        assert!(a.knows("com.a.b"), "no es un plugin nuevo: es uno que pide más");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_version_that_asks_for_the_same_keeps_its_approval() {
        let dir = temporal("igual");
        let mut a = Approvals::load(&dir);
        a.approve(&manifest("com.a.b", r#"{"type":"storage"}"#)).unwrap();
        let newer = Manifest::parse(
            r#"{"id":"com.a.b","name":"Uno","version":"2","api_version":1,
                "kind":"source","capabilities":[{"type":"storage"}]}"#,
        )
        .unwrap();
        assert!(a.covers(&newer), "subir de versión sin pedir más no vuelve a preguntar");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn revoking_asks_again_from_scratch() {
        let dir = temporal("revoca");
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        let mut a = Approvals::load(&dir);
        a.approve(&m).unwrap();
        a.revoke("com.a.b").unwrap();
        assert!(!a.covers(&m));
        assert!(!Approvals::load(&dir).covers(&m), "y quedó escrito");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreadable_file_approves_nothing() {
        // Lo contrario sería asumir permisos que nadie dio.
        let dir = temporal("roto");
        std::fs::write(dir.join(FILE), "esto no es json").unwrap();
        let a = Approvals::load(&dir);
        assert!(!a.covers(&manifest("com.a.b", "")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
