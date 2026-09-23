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
    /// "bien", "tropezando" o "en cuarentena".
    pub state: String,
    /// Por qué está castigado, o vacío.
    pub problem: String,
}

/// Todo lo de plugins que la fachada guarda.
pub struct Plugins {
    host: Host,
    registry: Registry,
    services: Services,
    root: PathBuf,
}

impl Plugins {
    /// Arma el host. Falla solo si el motor no se pudo crear, que sería un
    /// problema de la máquina y no de un plugin.
    pub fn new(root: PathBuf) -> Option<Self> {
        let host = Host::new().ok()?;
        let services: Services = Arc::new(Mutex::new(ChilpaServices::new(root.clone())));
        Some(Self { host, registry: Registry::new(), services, root })
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
        self.registry.install(&self.host, &manifest, &wasm);
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
                        chilpa_plugin::State::Healthy => "bien".into(),
                        chilpa_plugin::State::Stumbling { .. } => "tropezando".into(),
                        chilpa_plugin::State::Quarantined { .. } => "en cuarentena".into(),
                    },
                    problem: match (&state, &i.problem) {
                        (chilpa_plugin::State::Quarantined { reason }, _) => reason.clone(),
                        (_, Some(p)) => p.clone(),
                        _ => String::new(),
                    },
                }
            })
            .collect()
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivate(&mut self, id: &str) {
        self.registry.reactivate(id);
    }

    /// Le pide algo a un plugin.
    pub fn call(&mut self, id: &str, operation: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        match self.registry.call(
            &self.host,
            &self.services,
            id,
            operation,
            input,
            Limits::default(),
        ) {
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
            self.plugins = Plugins::new(Self::plugins_dir());
        }
        match &mut self.plugins {
            Some(p) => p.load_all(),
            None => {
                self.now.error = "no se pudo arrancar el sandbox de plugins".into();
                0
            }
        }
    }

    /// Los plugins instalados, cargados o no.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins.as_ref().map(|p| p.list()).unwrap_or_default()
    }

    /// Saca un plugin de cuarentena.
    pub fn reactivate_plugin(&mut self, id: &str) {
        if let Some(p) = &mut self.plugins {
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
        match &mut self.plugins {
            Some(p) => p.call(id, operation, input),
            None => Err("el sandbox de plugins no está arrancado".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_llave_con_barras_o_puntos_dobles_no_se_vuelve_archivo() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"));
        for bad in ["../fuera", "con/barra", "..", "CON.MAYUSCULAS", "", &"x".repeat(65)] {
            assert!(s.file("com.a.b", bad).is_none(), "\"{bad}\" no debería pasar");
        }
        assert!(s.file("com.a.b", "token").is_some());
        assert!(s.file("com.a.b", "ultima-sync.json").is_some());
    }

    #[test]
    fn un_id_con_barras_tampoco() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"));
        assert!(s.file("../otro", "token").is_none());
        assert!(s.file("con/barra", "token").is_none());
    }

    #[test]
    fn el_almacen_de_un_plugin_va_y_viene() {
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
    fn el_almacen_no_es_para_guardar_un_catalogo() {
        let dir = std::env::temp_dir().join("chilpa-plugins-grande");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone());
        assert!(!s.store_set("com.a.b", "enorme", &vec![0u8; 2 * 1024 * 1024]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sin_carpeta_de_plugins_no_hay_plugins_y_no_es_un_error() {
        let dir = std::env::temp_dir().join("chilpa-plugins-que-no-existe");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Plugins::new(dir).expect("el motor arranca");
        assert_eq!(p.load_all(), 0);
        assert!(p.list().is_empty());
    }

    #[test]
    fn una_carpeta_sin_manifest_no_se_anota_como_plugin_roto() {
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
    fn un_plugin_roto_se_lista_con_su_razon() {
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
