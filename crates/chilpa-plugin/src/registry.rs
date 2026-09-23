//! El registro: qué plugins hay, cuáles sirven y cuáles están castigados.
//!
//! Es la pieza que junta el manifest, el host y el vigilante, y la que
//! cumple la promesa del ADR-003: *antes de romper Chilpa, no carga.* Un
//! plugin que no pasa se anota con su razón y se sigue con los demás; nunca
//! se aborta el arranque por culpa de uno.

use crate::host::{Host, Plugin, PluginError, Servicios};
use crate::limits::Limits;
use crate::manifest::Manifest;
use crate::watchdog::{Estado, Watchdog};
use crate::Respuesta;
use std::collections::BTreeMap;

/// Un plugin que se intentó cargar.
pub struct Instalado {
    pub id: String,
    pub nombre: String,
    /// El plugin listo, o `None` si no cargó.
    pub plugin: Option<Plugin>,
    /// Por qué no cargó, si no cargó.
    pub problema: Option<String>,
}

impl Instalado {
    pub fn cargado(&self) -> bool {
        self.plugin.is_some()
    }
}

/// Todos los plugins de esta sesión.
#[derive(Default)]
pub struct Registry {
    plugins: BTreeMap<String, Instalado>,
    watchdog: Watchdog,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intenta cargar un plugin. Nunca falla hacia afuera: lo que no se pudo
    /// se anota y se sigue.
    ///
    /// Devuelve cierto si quedó utilizable.
    pub fn instalar(&mut self, host: &Host, manifest_json: &str, wasm: &[u8]) -> bool {
        let manifest = match Manifest::parse(manifest_json) {
            Ok(m) => m,
            Err(e) => {
                // El manifest no pasó, pero puede que su JSON sí se lea: se
                // rescata el id y el nombre para que en la lista aparezca
                // como lo que es y no como una cadena de puntos.
                let (id, nombre) = a_ciegas(manifest_json);
                self.watchdog.cuarentena(&id, &e.to_string());
                self.plugins.insert(
                    id.clone(),
                    Instalado { id, nombre, plugin: None, problema: Some(e.to_string()) },
                );
                return false;
            }
        };
        let (id, nombre) = (manifest.id.clone(), manifest.name.clone());
        match host.cargar(manifest, wasm) {
            Ok(p) => {
                self.plugins
                    .insert(id.clone(), Instalado { id, nombre, plugin: Some(p), problema: None });
                true
            }
            Err(e) => {
                let razon = e.to_string();
                self.watchdog.cuarentena(&id, &razon);
                self.plugins.insert(
                    id.clone(),
                    Instalado { id, nombre, plugin: None, problema: Some(razon) },
                );
                false
            }
        }
    }

    /// Los plugins instalados, cargados o no.
    pub fn instalados(&self) -> impl Iterator<Item = &Instalado> {
        self.plugins.values()
    }

    /// Los que se pueden usar ahora mismo.
    pub fn utilizables(&self) -> Vec<&str> {
        self.plugins
            .values()
            .filter(|i| i.cargado() && self.watchdog.usable(&i.id))
            .map(|i| i.id.as_str())
            .collect()
    }

    pub fn estado(&self, id: &str) -> Estado {
        if !self.plugins.contains_key(id) {
            return Estado::Cuarentena { razon: "no está instalado".into() };
        }
        self.watchdog.estado(id)
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivar(&mut self, id: &str) {
        self.watchdog.reactivar(id);
    }

    pub fn en_cuarentena(&self) -> Vec<(String, String)> {
        self.watchdog.en_cuarentena()
    }

    /// Le pide algo a un plugin y lleva la cuenta de cómo le fue.
    ///
    /// Un error del plugin (un JSON con `error`) **no** cuenta como falla:
    /// "no encontré esa canción" es una respuesta, no una rotura. Lo que
    /// cuenta es tronar, pasarse de los límites o no contestar.
    pub fn llamar(
        &mut self,
        host: &Host,
        servicios: &Servicios,
        id: &str,
        operacion: &str,
        entrada: &[u8],
        limites: Limits,
    ) -> Result<Respuesta, PluginError> {
        if !self.watchdog.usable(id) {
            return Err(PluginError::SinPermiso(format!(
                "llamar a \"{id}\", que está en cuarentena"
            )));
        }
        let Some(inst) = self.plugins.get(id) else {
            return Err(PluginError::SinPermiso(format!("llamar a \"{id}\", que no está instalado")));
        };
        let Some(plugin) = &inst.plugin else {
            return Err(PluginError::SinPermiso(format!("llamar a \"{id}\", que no cargó")));
        };

        match plugin.llamar(host, servicios, operacion, entrada, limites) {
            Ok(r) => {
                self.watchdog.exito(id);
                Ok(r)
            }
            Err(e) => {
                self.watchdog.falla(id, &e.to_string());
                Err(e)
            }
        }
    }
}

/// Lo que se pueda rescatar de un manifest que no pasó.
fn a_ciegas(json: &str) -> (String, String) {
    let v = serde_json::from_str::<serde_json::Value>(json).ok();
    let sacar = |campo: &str| {
        v.as_ref().and_then(|v| v.get(campo)?.as_str().map(str::to_string)).unwrap_or_default()
    };
    let id = sacar("id");
    let id = if id.is_empty() { "(manifest ilegible)".to_string() } else { id };
    (id, sacar("name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_manifest_ilegible_se_anota_y_no_tumba_el_registro() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        assert!(!r.instalar(&host, "esto no es json", b""));
        assert_eq!(r.instalados().count(), 1);
        assert!(!r.utilizables().is_empty() == false);
        let inst = r.instalados().next().unwrap();
        assert!(!inst.cargado());
        assert!(inst.problema.is_some());
    }

    #[test]
    fn un_manifest_con_id_pero_roto_se_anota_con_su_id() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // Tiene id pero le falta todo lo demás.
        assert!(!r.instalar(&host, r#"{"id":"com.a.b"}"#, b""));
        assert_eq!(r.instalados().next().unwrap().id, "com.a.b");
        assert!(matches!(r.estado("com.a.b"), Estado::Cuarentena { .. }));
    }

    #[test]
    fn uno_que_no_paso_la_validacion_conserva_su_nombre() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // El JSON se lee; lo que no pasa es la versión de la API.
        let json = r#"{"id":"com.a.b","name":"Se llama así","version":"1",
                       "api_version":42,"kind":"source"}"#;
        assert!(!r.instalar(&host, json, b""));
        let inst = r.instalados().next().unwrap();
        assert_eq!(inst.nombre, "Se llama así", "en la lista sale con su nombre");
        assert!(inst.problema.as_ref().unwrap().contains("versión"));
    }

    #[test]
    fn a_uno_que_no_esta_no_se_le_puede_pedir_nada() {
        let r = Registry::new();
        assert!(matches!(r.estado("com.no.existe"), Estado::Cuarentena { .. }));
    }
}
