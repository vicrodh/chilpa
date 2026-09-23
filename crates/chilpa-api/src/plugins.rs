//! Los plugins, vistos desde la fachada.
//!
//! Aquí se juntan las dos mitades: el sandbox, que vive en `chilpa-plugin` y
//! no sabe de red ni de disco, y lo que Chilpa le presta. Las credenciales
//! están de este lado y nunca cruzan (ADR-007).
//!
//! Los plugins se leen de `~/.local/share/chilpa/plugins/<id>/`, con su
//! `manifest.json` y su `plugin.wasm`. Uno que no cargue se anota con su
//! razón y no impide que los demás carguen.

use super::*;
use crate::approvals::Approvals;
use chilpa_plugin::{Host, HostServices, Limits, Level, Registry, Services};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Lo que Chilpa le presta a un plugin.
///
/// Lo que no está aquí, no existe para el guest.
pub struct ChilpaServices {
    root: PathBuf,
}

impl ChilpaServices {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// El archivo donde vive una llave del almacén de un plugin.
    ///
    /// La llave se revisa antes de volverse un nombre de archivo: una que
    /// traiga barras o puntos dobles saldría de la carpeta del plugin, que
    /// es justo lo que el sandbox está para impedir.
    fn file(&self, plugin: &str, key: &str) -> Option<PathBuf> {
        if !valid_key(key) || !valid_id(plugin) {
            return None;
        }
        Some(self.root.join(plugin).join("store").join(key))
    }

    /// La credencial guardada para un plugin, si hay.
    ///
    /// Vive en la carpeta del plugin pero **fuera de su almacén**, y el
    /// guest no tiene forma de leerla: no hay una llave con la que pedirla.
    fn credential(&self, plugin: &str) -> Option<String> {
        if !valid_id(plugin) {
            return None;
        }
        let path = self.root.join(plugin).join("credencial");
        std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

impl HostServices for ChilpaServices {
    fn log(&mut self, plugin: &str, level: Level, message: &str) {
        // Con el id por delante: un renglón de log sin dueño no sirve para
        // saber a quién reclamarle.
        eprintln!("[plugin {plugin}] {level:?}: {message}");
    }

    fn now_unix(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(plugin, key)?).ok()
    }

    fn store_set(&mut self, plugin: &str, key: &str, valor: &[u8]) -> bool {
        let Some(path) = self.file(plugin, key) else { return false };
        // Un plugin no puede llenar el disco de a poquito: el almacén es
        // para ajustes y marcas, no para una copia del catálogo.
        if valor.len() > 1024 * 1024 {
            return false;
        }
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, valor).is_ok()
    }

    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8> {
        // La credencial se agrega aquí. El plugin dijo a dónde; con qué
        // permiso lo decide Chilpa.
        let credential = self.credential(plugin);
        chilpa_core::send_request(request, credential.as_deref())
    }
}

/// Una llave del almacén sirve si se puede volver un nombre de archivo sin
/// salirse de su carpeta.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && !key.contains("..")
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
}

/// Lo mismo para el id, que también es una carpeta.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && !id.contains('/')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

/// Un plugin, como lo ve la vista.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    /// "pendiente de aprobación", "bien", "tropezando" o "en cuarentena".
    pub state: String,
    /// Por qué está castigado, o vacío.
    pub problem: String,
    /// Lo que pide, en palabras, para poder aprobarlo o negarlo.
    pub asks: Vec<String>,
    /// De lo que pide, lo que todavía no se ha aprobado. Vacío en uno que
    /// ya está aprobado; todo lo que pide en uno nuevo.
    pub pending: Vec<String>,
    /// Cierto si ya se le había aprobado algo y ahora pide más. Es distinto
    /// de uno nuevo, y se dice distinto.
    pub wants_more: bool,
}

/// Todo lo de plugins que la fachada guarda.
pub struct Plugins {
    host: Host,
    registry: Registry,
    services: Services,
    approvals: Approvals,
    root: PathBuf,
}

impl Plugins {
    /// Arma el host. Falla solo si el motor no se pudo crear, que sería un
    /// problema de la máquina y no de un plugin.
    pub fn new(root: PathBuf) -> Option<Self> {
        let host = Host::new().ok()?;
        let services: Services = Arc::new(Mutex::new(ChilpaServices::new(root.clone())));
        let approvals = Approvals::load(&root);
        Some(Self { host, registry: Registry::new(), services, approvals, root })
    }

    /// Lee la carpeta de plugins y carga lo que encuentre.
    ///
    /// Devuelve cuántos quedaron utilizables. Una carpeta que no existe no
    /// es un error: quiere decir que no hay plugins.
    pub fn load_all(&mut self) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return 0 };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            self.load_one(&dir);
        }
        self.registry.usable().len()
    }

    fn load_one(&mut self, dir: &Path) {
        let manifest = match std::fs::read_to_string(dir.join("manifest.json")) {
            Ok(m) => m,
            // Una carpeta sin manifest no es un plugin roto: no es un
            // plugin. Se pasa de largo sin anotar nada.
            Err(_) => return,
        };
        let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap_or_default();
        // Un plugin que nadie aprobó no se compila siquiera: el cierre
        // contesta que no y su wasm nunca se toca (ADR-008).
        let approvals = &self.approvals;
        self.registry.install(&self.host, &manifest, &wasm, |m| approvals.covers(m));
    }

    /// Lo que hay, para mostrarlo.
    pub fn list(&self) -> Vec<PluginInfo> {
        self.registry
            .installed()
            .map(|i| {
                let state = self.registry.state(&i.id);
                PluginInfo {
                    id: i.id.clone(),
                    name: if i.name.is_empty() { i.id.clone() } else { i.name.clone() },
                    state: match &state {
                        chilpa_plugin::State::PendingApproval => "pendiente de aprobación".into(),
                        chilpa_plugin::State::Healthy => "bien".into(),
                        chilpa_plugin::State::Stumbling { .. } => "tropezando".into(),
                        chilpa_plugin::State::Quarantined { .. } => "en cuarentena".into(),
                    },
                    problem: match (&state, &i.problem) {
                        (chilpa_plugin::State::Quarantined { reason }, _) => reason.clone(),
                        (_, Some(p)) => p.clone(),
                        _ => String::new(),
                    },
                    asks: i.asks(),
                    pending: i
                        .manifest
                        .as_ref()
                        .map(|m| self.approvals.beyond(m))
                        .unwrap_or_default(),
                    wants_more: self.approvals.knows(&i.id),
                }
            })
            .collect()
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivate(&mut self, id: &str) {
        self.registry.reactivate(id);
    }

    /// Olvida todo lo cargado. Se usa al revocar: un plugin que dejó de
    /// estar aprobado tiene que dejar de existir en esta sesión, no
    /// quedarse instanciado hasta el siguiente arranque.
    pub fn forget(&mut self) {
        self.registry = Registry::new();
    }

    /// El usuario aprueba lo que ese plugin pide hoy.
    ///
    /// No lo carga: cargar es releer la carpeta, y eso lo hace quien llama,
    /// para que aprobar dos seguidos no recompile todo dos veces.
    pub fn approve(&mut self, id: &str) -> Result<(), String> {
        let manifest = self
            .registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.clone())
            .ok_or_else(|| format!("no hay un plugin \"{id}\" con manifest que aprobar"))?;
        self.approvals.approve(&manifest).map_err(|e| e.to_string())
    }

    /// Quita la aprobación. El plugin deja de cargar y lo que pida se
    /// vuelve a preguntar.
    pub fn revoke(&mut self, id: &str) -> Result<(), String> {
        self.approvals.revoke(id).map_err(|e| e.to_string())
    }

    /// Los plugins de catálogo que sirven ahora, con su nombre.
    pub fn searchers(&self) -> Vec<(String, String)> {
        self.registry.usable_of_kind(chilpa_plugin::manifest::Kind::Source)
    }

    /// Le pide algo a un plugin.
    pub fn call(&mut self, id: &str, operation: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        self.call_with(id, operation, input, Limits::default())
    }

    /// Lo mismo, con los límites cortos: es lo que se usa cuando alguien
    /// está esperando frente a la ventana, como al buscar.
    pub fn call_short(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.call_with(id, operation, input, Limits::short())
    }

    fn call_with(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Vec<u8>, String> {
        match self.registry.call(&self.host, &self.services, id, operation, input, limits) {
            Ok(chilpa_plugin::Reply::Ok(bytes)) => Ok(bytes),
            Ok(chilpa_plugin::Reply::Error(e)) => Err(e),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Chilpa {
    /// Dónde viven los plugins.
    pub fn plugins_dir() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
            .unwrap_or_else(std::env::temp_dir);
        base.join("chilpa").join("plugins")
    }

    /// Carga los plugins que haya. Se llama una vez al arrancar.
    ///
    /// Devuelve cuántos quedaron utilizables. Que no haya ninguno es lo
    /// normal hoy y no es un problema.
    pub fn load_plugins(&mut self) -> usize {
        if self.plugins.is_none() {
            self.plugins = Plugins::new(Self::plugins_dir()).map(|p| Arc::new(Mutex::new(p)));
        }
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.load_all(),
            None => {
                self.now.error = "no se pudo arrancar el sandbox de plugins".into();
                0
            }
        }
    }

    /// Los plugins instalados, cargados o no.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .as_ref()
            .and_then(|p| p.lock().ok())
            .map(|p| p.list())
            .unwrap_or_default()
    }

    /// El usuario aprueba lo que un plugin pide, y se vuelve a leer la
    /// carpeta para que entre a correr sin reiniciar.
    ///
    /// Devuelve cuántos quedaron utilizables, igual que cargar.
    pub fn approve_plugin(&mut self, id: &str) -> usize {
        let Some(p) = self.plugins.clone() else {
            self.now.error = "el sandbox de plugins no está arrancado".into();
            return 0;
        };
        let Ok(mut p) = p.lock() else { return 0 };
        if let Err(e) = p.approve(id) {
            self.now.error = format!("no se pudo guardar la aprobación: {e}");
            return 0;
        }
        p.load_all()
    }

    /// Quita la aprobación de un plugin: deja de correr ahora mismo y lo
    /// que pida se volverá a preguntar.
    pub fn revoke_plugin(&mut self, id: &str) -> usize {
        let Some(p) = self.plugins.clone() else { return 0 };
        let Ok(mut p) = p.lock() else { return 0 };
        if let Err(e) = p.revoke(id) {
            self.now.error = format!("no se pudo quitar la aprobación: {e}");
            return 0;
        }
        p.forget();
        p.load_all()
    }

    /// Saca un plugin de cuarentena.
    pub fn reactivate_plugin(&mut self, id: &str) {
        if let Some(mut p) = self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            p.reactivate(id);
        }
    }

    /// Le pide una operación a un plugin.
    ///
    /// El error que regresa ya viene redactado: puede ser del plugin ("no
    /// encontré esa pista") o de Chilpa ("se pasó del cómputo permitido").
    pub fn call_plugin(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.call(id, operation, input),
            None => Err("el sandbox de plugins no está arrancado".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_with_slashes_or_dot_dots_never_becomes_a_file() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"));
        for bad in ["../fuera", "con/barra", "..", "CON.MAYUSCULAS", "", &"x".repeat(65)] {
            assert!(s.file("com.a.b", bad).is_none(), "\"{bad}\" no debería pasar");
        }
        assert!(s.file("com.a.b", "token").is_some());
        assert!(s.file("com.a.b", "ultima-sync.json").is_some());
    }

    #[test]
    fn nor_does_an_id_with_slashes() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"));
        assert!(s.file("../otro", "token").is_none());
        assert!(s.file("con/barra", "token").is_none());
    }

    #[test]
    fn a_plugin_store_round_trips() {
        let dir = std::env::temp_dir().join("chilpa-plugins-almacen");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone());
        assert!(s.store_set("com.a.b", "token", b"abc"));
        assert_eq!(s.store_get("com.a.b", "token").as_deref(), Some(&b"abc"[..]));
        assert_eq!(s.store_get("com.a.b", "otra"), None);
        // Y cada plugin tiene el suyo.
        assert_eq!(s.store_get("com.otro.plugin", "token"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_store_is_not_for_keeping_a_catalogue() {
        let dir = std::env::temp_dir().join("chilpa-plugins-grande");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone());
        assert!(!s.store_set("com.a.b", "enorme", &vec![0u8; 2 * 1024 * 1024]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_plugin_folder_means_no_plugins_and_is_not_an_error() {
        let dir = std::env::temp_dir().join("chilpa-plugins-que-no-existe");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Plugins::new(dir).expect("el motor arranca");
        assert_eq!(p.load_all(), 0);
        assert!(p.list().is_empty());
    }

    #[test]
    fn a_folder_without_a_manifest_is_not_listed_as_a_broken_plugin() {
        let dir = std::env::temp_dir().join("chilpa-plugins-sin-manifest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("basura")).unwrap();
        std::fs::write(dir.join("basura").join("leeme.txt"), b"hola").unwrap();
        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.load_all(), 0);
        assert!(p.list().is_empty(), "no es un plugin roto: no es un plugin");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_reason() {
        let dir = std::env::temp_dir().join("chilpa-plugins-roto");
        let _ = std::fs::remove_dir_all(&dir);
        let own = dir.join("com.roto.plugin");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(
            own.join("manifest.json"),
            r#"{"id":"com.roto.plugin","name":"Roto","version":"1","api_version":99,"kind":"source"}"#,
        )
        .unwrap();
        std::fs::write(own.join("plugin.wasm"), b"").unwrap();

        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.load_all(), 0, "no quedó utilizable");
        let list = p.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, "en cuarentena");
        assert!(list[0].problem.contains("versión"), "{}", list[0].problem);
        let _ = std::fs::remove_dir_all(dir);
    }
}
