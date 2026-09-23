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
use chilpa_plugin::{Host, HostServices, Limits, Nivel, Registry, Servicios};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Lo que Chilpa le presta a un plugin.
///
/// Lo que no está aquí, no existe para el guest.
pub struct ServiciosDeChilpa {
    raiz: PathBuf,
}

impl ServiciosDeChilpa {
    pub fn new(raiz: PathBuf) -> Self {
        Self { raiz }
    }

    /// El archivo donde vive una llave del almacén de un plugin.
    ///
    /// La llave se revisa antes de volverse un nombre de archivo: una que
    /// traiga barras o puntos dobles saldría de la carpeta del plugin, que
    /// es justo lo que el sandbox está para impedir.
    fn archivo(&self, plugin: &str, llave: &str) -> Option<PathBuf> {
        if !llave_valida(llave) || !id_valido(plugin) {
            return None;
        }
        Some(self.raiz.join(plugin).join("almacen").join(llave))
    }

    /// La credencial guardada para un plugin, si hay.
    ///
    /// Vive en la carpeta del plugin pero **fuera de su almacén**, y el
    /// guest no tiene forma de leerla: no hay una llave con la que pedirla.
    fn credencial(&self, plugin: &str) -> Option<String> {
        if !id_valido(plugin) {
            return None;
        }
        let ruta = self.raiz.join(plugin).join("credencial");
        std::fs::read_to_string(ruta).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

impl HostServices for ServiciosDeChilpa {
    fn log(&mut self, plugin: &str, nivel: Nivel, mensaje: &str) {
        // Con el id por delante: un renglón de log sin dueño no sirve para
        // saber a quién reclamarle.
        eprintln!("[plugin {plugin}] {nivel:?}: {mensaje}");
    }

    fn now_unix(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn store_get(&mut self, plugin: &str, llave: &str) -> Option<Vec<u8>> {
        std::fs::read(self.archivo(plugin, llave)?).ok()
    }

    fn store_set(&mut self, plugin: &str, llave: &str, valor: &[u8]) -> bool {
        let Some(ruta) = self.archivo(plugin, llave) else { return false };
        // Un plugin no puede llenar el disco de a poquito: el almacén es
        // para ajustes y marcas, no para una copia del catálogo.
        if valor.len() > 1024 * 1024 {
            return false;
        }
        if let Some(dir) = ruta.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(ruta, valor).is_ok()
    }

    fn http(&mut self, plugin: &str, peticion: &[u8]) -> Vec<u8> {
        // La credencial se agrega aquí. El plugin dijo a dónde; con qué
        // permiso lo decide Chilpa.
        let credencial = self.credencial(plugin);
        chilpa_core::hacer_peticion(peticion, credencial.as_deref())
    }
}

/// Una llave del almacén sirve si se puede volver un nombre de archivo sin
/// salirse de su carpeta.
fn llave_valida(llave: &str) -> bool {
    !llave.is_empty()
        && llave.len() <= 64
        && !llave.contains("..")
        && llave.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
}

/// Lo mismo para el id, que también es una carpeta.
fn id_valido(id: &str) -> bool {
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
    servicios: Servicios,
    raiz: PathBuf,
}

impl Plugins {
    /// Arma el host. Falla solo si el motor no se pudo crear, que sería un
    /// problema de la máquina y no de un plugin.
    pub fn new(raiz: PathBuf) -> Option<Self> {
        let host = Host::new().ok()?;
        let servicios: Servicios = Arc::new(Mutex::new(ServiciosDeChilpa::new(raiz.clone())));
        Some(Self { host, registry: Registry::new(), servicios, raiz })
    }

    /// Lee la carpeta de plugins y carga lo que encuentre.
    ///
    /// Devuelve cuántos quedaron utilizables. Una carpeta que no existe no
    /// es un error: quiere decir que no hay plugins.
    pub fn cargar_todos(&mut self) -> usize {
        let Ok(entradas) = std::fs::read_dir(&self.raiz) else { return 0 };
        for entrada in entradas.flatten() {
            let dir = entrada.path();
            if !dir.is_dir() {
                continue;
            }
            self.cargar_uno(&dir);
        }
        self.registry.utilizables().len()
    }

    fn cargar_uno(&mut self, dir: &Path) {
        let manifest = match std::fs::read_to_string(dir.join("manifest.json")) {
            Ok(m) => m,
            // Una carpeta sin manifest no es un plugin roto: no es un
            // plugin. Se pasa de largo sin anotar nada.
            Err(_) => return,
        };
        let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap_or_default();
        self.registry.instalar(&self.host, &manifest, &wasm);
    }

    /// Lo que hay, para mostrarlo.
    pub fn listar(&self) -> Vec<PluginInfo> {
        self.registry
            .instalados()
            .map(|i| {
                let estado = self.registry.estado(&i.id);
                PluginInfo {
                    id: i.id.clone(),
                    name: if i.nombre.is_empty() { i.id.clone() } else { i.nombre.clone() },
                    state: match &estado {
                        chilpa_plugin::Estado::Bien => "bien".into(),
                        chilpa_plugin::Estado::Tropezando { .. } => "tropezando".into(),
                        chilpa_plugin::Estado::Cuarentena { .. } => "en cuarentena".into(),
                    },
                    problem: match (&estado, &i.problema) {
                        (chilpa_plugin::Estado::Cuarentena { razon }, _) => razon.clone(),
                        (_, Some(p)) => p.clone(),
                        _ => String::new(),
                    },
                }
            })
            .collect()
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivar(&mut self, id: &str) {
        self.registry.reactivar(id);
    }

    /// Le pide algo a un plugin.
    pub fn llamar(&mut self, id: &str, operacion: &str, entrada: &[u8]) -> Result<Vec<u8>, String> {
        match self.registry.llamar(
            &self.host,
            &self.servicios,
            id,
            operacion,
            entrada,
            Limits::default(),
        ) {
            Ok(chilpa_plugin::Respuesta::Ok(bytes)) => Ok(bytes),
            Ok(chilpa_plugin::Respuesta::Error(e)) => Err(e),
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
            Some(p) => p.cargar_todos(),
            None => {
                self.now.error = "no se pudo arrancar el sandbox de plugins".into();
                0
            }
        }
    }

    /// Los plugins instalados, cargados o no.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins.as_ref().map(|p| p.listar()).unwrap_or_default()
    }

    /// Saca un plugin de cuarentena.
    pub fn reactivate_plugin(&mut self, id: &str) {
        if let Some(p) = &mut self.plugins {
            p.reactivar(id);
        }
    }

    /// Le pide una operación a un plugin.
    ///
    /// El error que regresa ya viene redactado: puede ser del plugin ("no
    /// encontré esa pista") o de Chilpa ("se pasó del cómputo permitido").
    pub fn call_plugin(
        &mut self,
        id: &str,
        operacion: &str,
        entrada: &[u8],
    ) -> Result<Vec<u8>, String> {
        match &mut self.plugins {
            Some(p) => p.llamar(id, operacion, entrada),
            None => Err("el sandbox de plugins no está arrancado".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_llave_con_barras_o_puntos_dobles_no_se_vuelve_archivo() {
        let s = ServiciosDeChilpa::new(PathBuf::from("/tmp/chilpa-plugins"));
        for mala in ["../fuera", "con/barra", "..", "CON.MAYUSCULAS", "", &"x".repeat(65)] {
            assert!(s.archivo("com.a.b", mala).is_none(), "\"{mala}\" no debería pasar");
        }
        assert!(s.archivo("com.a.b", "token").is_some());
        assert!(s.archivo("com.a.b", "ultima-sync.json").is_some());
    }

    #[test]
    fn un_id_con_barras_tampoco() {
        let s = ServiciosDeChilpa::new(PathBuf::from("/tmp/chilpa-plugins"));
        assert!(s.archivo("../otro", "token").is_none());
        assert!(s.archivo("con/barra", "token").is_none());
    }

    #[test]
    fn el_almacen_de_un_plugin_va_y_viene() {
        let dir = std::env::temp_dir().join("chilpa-plugins-almacen");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ServiciosDeChilpa::new(dir.clone());
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
        let mut s = ServiciosDeChilpa::new(dir.clone());
        assert!(!s.store_set("com.a.b", "enorme", &vec![0u8; 2 * 1024 * 1024]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sin_carpeta_de_plugins_no_hay_plugins_y_no_es_un_error() {
        let dir = std::env::temp_dir().join("chilpa-plugins-que-no-existe");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Plugins::new(dir).expect("el motor arranca");
        assert_eq!(p.cargar_todos(), 0);
        assert!(p.listar().is_empty());
    }

    #[test]
    fn una_carpeta_sin_manifest_no_se_anota_como_plugin_roto() {
        let dir = std::env::temp_dir().join("chilpa-plugins-sin-manifest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("basura")).unwrap();
        std::fs::write(dir.join("basura").join("leeme.txt"), b"hola").unwrap();
        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.cargar_todos(), 0);
        assert!(p.listar().is_empty(), "no es un plugin roto: no es un plugin");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn un_plugin_roto_se_lista_con_su_razon() {
        let dir = std::env::temp_dir().join("chilpa-plugins-roto");
        let _ = std::fs::remove_dir_all(&dir);
        let suyo = dir.join("com.roto.plugin");
        std::fs::create_dir_all(&suyo).unwrap();
        std::fs::write(
            suyo.join("manifest.json"),
            r#"{"id":"com.roto.plugin","name":"Roto","version":"1","api_version":99,"kind":"source"}"#,
        )
        .unwrap();
        std::fs::write(suyo.join("plugin.wasm"), b"").unwrap();

        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.cargar_todos(), 0, "no quedó utilizable");
        let lista = p.listar();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].state, "en cuarentena");
        assert!(lista[0].problem.contains("versión"), "{}", lista[0].problem);
        let _ = std::fs::remove_dir_all(dir);
    }
}
