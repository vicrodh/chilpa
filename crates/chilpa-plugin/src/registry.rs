//! El registro: qué plugins hay, cuáles sirven y cuáles están castigados.
//!
//! Es la pieza que junta el manifest, el host y el vigilante, y la que
//! cumple la promesa del ADR-003: *antes de romper Chilpa, no carga.* Un
//! plugin que no pasa se anota con su razón y se sigue con los demás; nunca
//! se aborta el arranque por culpa de uno.

use crate::host::{Host, Plugin, PluginError, Services};
use crate::limits::Limits;
use crate::manifest::{Kind, Manifest};
use crate::watchdog::{State, Watchdog};
use crate::Reply;
use std::collections::BTreeMap;

/// Un plugin que se intentó cargar.
pub struct Installed {
    pub id: String,
    pub name: String,
    /// El plugin listo, o `None` si no cargó.
    pub plugin: Option<Plugin>,
    /// Por qué no cargó, si no cargó.
    pub problem: Option<String>,
}

impl Installed {
    pub fn loaded(&self) -> bool {
        self.plugin.is_some()
    }
}

/// Todos los plugins de esta sesión.
#[derive(Default)]
pub struct Registry {
    plugins: BTreeMap<String, Installed>,
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
    pub fn install(&mut self, host: &Host, manifest_json: &str, wasm: &[u8]) -> bool {
        let manifest = match Manifest::parse(manifest_json) {
            Ok(m) => m,
            Err(e) => {
                // El manifest no pasó, pero puede que su JSON sí se lea: se
                // rescata el id y el nombre para que en la lista aparezca
                // como lo que es y no como una cadena de puntos.
                let (id, name) = salvage(manifest_json);
                self.watchdog.quarantine(&id, &e.to_string());
                self.plugins.insert(
                    id.clone(),
                    Installed { id, name, plugin: None, problem: Some(e.to_string()) },
                );
                return false;
            }
        };
        let (id, name) = (manifest.id.clone(), manifest.name.clone());
        match host.load(manifest, wasm) {
            Ok(p) => {
                self.plugins
                    .insert(id.clone(), Installed { id, name, plugin: Some(p), problem: None });
                true
            }
            Err(e) => {
                let reason = e.to_string();
                self.watchdog.quarantine(&id, &reason);
                self.plugins.insert(
                    id.clone(),
                    Installed { id, name, plugin: None, problem: Some(reason) },
                );
                false
            }
        }
    }

    /// Los plugins instalados, cargados o no.
    pub fn installed(&self) -> impl Iterator<Item = &Installed> {
        self.plugins.values()
    }

    /// Los que se pueden usar ahora mismo.
    pub fn usable(&self) -> Vec<&str> {
        self.plugins
            .values()
            .filter(|i| i.loaded() && self.watchdog.usable(&i.id))
            .map(|i| i.id.as_str())
            .collect()
    }

    /// Los que sirven ahora y son de ese tipo, con su nombre, en el orden
    /// en que se instalaron.
    ///
    /// El orden importa: un plugin no puede colarse antes que otro, y el
    /// mapa está ordenado por id, así que la lista es siempre la misma.
    pub fn usable_of_kind(&self, kind: Kind) -> Vec<(String, String)> {
        self.plugins
            .values()
            .filter(|i| self.watchdog.usable(&i.id))
            .filter_map(|i| {
                let p = i.plugin.as_ref()?;
                (p.manifest().kind == kind).then(|| {
                    let name = if i.name.is_empty() { i.id.clone() } else { i.name.clone() };
                    (i.id.clone(), name)
                })
            })
            .collect()
    }

    pub fn state(&self, id: &str) -> State {
        if !self.plugins.contains_key(id) {
            return State::Quarantined { reason: "no está instalado".into() };
        }
        self.watchdog.state(id)
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivate(&mut self, id: &str) {
        self.watchdog.reactivate(id);
    }

    pub fn quarantined(&self) -> Vec<(String, String)> {
        self.watchdog.quarantined()
    }

    /// Le pide algo a un plugin y lleva la cuenta de cómo le fue.
    ///
    /// Un error del plugin (un JSON con `error`) **no** cuenta como falla:
    /// "no encontré esa canción" es una respuesta, no una rotura. Lo que
    /// cuenta es tronar, pasarse de los límites o no contestar.
    pub fn call(
        &mut self,
        host: &Host,
        services: &Services,
        id: &str,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Reply, PluginError> {
        if !self.watchdog.usable(id) {
            return Err(PluginError::NotAllowed(format!(
                "llamar a \"{id}\", que está en cuarentena"
            )));
        }
        let Some(inst) = self.plugins.get(id) else {
            return Err(PluginError::NotAllowed(format!("llamar a \"{id}\", que no está instalado")));
        };
        let Some(plugin) = &inst.plugin else {
            return Err(PluginError::NotAllowed(format!("llamar a \"{id}\", que no cargó")));
        };

        match plugin.call(host, services, operation, input, limits) {
            Ok(r) => {
                self.watchdog.success(id);
                Ok(r)
            }
            Err(e) => {
                self.watchdog.failure(id, &e.to_string());
                Err(e)
            }
        }
    }
}

/// Lo que se pueda rescatar de un manifest que no pasó.
fn salvage(json: &str) -> (String, String) {
    let v = serde_json::from_str::<serde_json::Value>(json).ok();
    let take = |field: &str| {
        v.as_ref().and_then(|v| v.get(field)?.as_str().map(str::to_string)).unwrap_or_default()
    };
    let id = take("id");
    let id = if id.is_empty() { "(manifest ilegible)".to_string() } else { id };
    (id, take("name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_manifest_is_recorded_and_does_not_bring_the_registry_down() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        assert!(!r.install(&host, "esto no es json", b""));
        assert_eq!(r.installed().count(), 1);
        assert!(!r.usable().is_empty() == false);
        let inst = r.installed().next().unwrap();
        assert!(!inst.loaded());
        assert!(inst.problem.is_some());
    }

    #[test]
    fn a_broken_manifest_that_has_an_id_is_recorded_under_it() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // Tiene id pero le falta todo lo demás.
        assert!(!r.install(&host, r#"{"id":"com.a.b"}"#, b""));
        assert_eq!(r.installed().next().unwrap().id, "com.a.b");
        assert!(matches!(r.state("com.a.b"), State::Quarantined { .. }));
    }

    #[test]
    fn one_that_failed_validation_keeps_its_name() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // El JSON se lee; lo que no pasa es la versión de la API.
        let json = r#"{"id":"com.a.b","name":"Se llama así","version":"1",
                       "api_version":42,"kind":"source"}"#;
        assert!(!r.install(&host, json, b""));
        let inst = r.installed().next().unwrap();
        assert_eq!(inst.name, "Se llama así", "en la lista sale con su nombre");
        assert!(inst.problem.as_ref().unwrap().contains("versión"));
    }

    #[test]
    fn nothing_can_be_asked_of_one_that_is_not_there() {
        let r = Registry::new();
        assert!(matches!(r.state("com.no.existe"), State::Quarantined { .. }));
    }
}
