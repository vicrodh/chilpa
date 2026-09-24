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
use crate::accounts::Account;
use crate::approvals::Approvals;
use chilpa_plugin::{Host, HostServices, Limits, Level, Registry, Services};
use chilpa_auth::Auth;
use chilpa_secrets::{BackendKind, SecretBox};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Lo que Chilpa le presta a un plugin.
///
/// Lo que no está aquí, no existe para el guest.
pub struct ChilpaServices {
    root: PathBuf,
    /// Cómo conecta su cuenta cada plugin, sacado de su manifest al
    /// cargarlo. Hace falta aquí para poder renovar un acceso vencido sin
    /// que el plugin participe.
    auth: Arc<Mutex<BTreeMap<String, Auth>>>,
    /// Donde se abren las credenciales. Compartida con `Plugins`, que es
    /// quien las guarda: una sola caja para que las dos orillas usen la
    /// misma llave maestra.
    secrets: Option<Arc<SecretBox>>,
}

impl ChilpaServices {
    pub fn new(root: PathBuf, secrets: Option<Arc<SecretBox>>) -> Self {
        Self { root, secrets, auth: Arc::new(Mutex::new(BTreeMap::new())) }
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
    /// Vive cifrada en la carpeta del plugin pero **fuera de su almacén**, y
    /// el guest no tiene forma de pedirla: no hay una importación que la
    /// devuelva, y su `store_get` no alcanza este archivo (ADR-008).
    ///
    /// Que no se pueda abrir no se rescata de ninguna forma: se contesta que
    /// no hay credencial, y la petición sale sin `Authorization`. Inventar
    /// algo aquí sería mandar basura al servicio del usuario.
    fn credential(&self, plugin: &str) -> Option<String> {
        let account = self.account(plugin)?;
        let now = now_unix();

        // Si el acceso venció y hay con qué renovarlo, se renueva aquí, se
        // guarda cifrado, y la petición sale con el nuevo. El plugin no se
        // entera de que hubo una renovación, que es justo el punto.
        if account.stale(now) {
            let auth = self.auth.lock().ok()?.get(plugin).cloned()?;
            match account.renew(&auth, now, &post) {
                Ok(fresh) => {
                    let _ = self.save(plugin, &fresh);
                    return fresh.header();
                }
                // Que falle renovar no se rescata inventando algo: la
                // petición sale sin credencial y el servicio dirá que no,
                // que es más honesto que mandar un token vencido.
                Err(_) => return None,
            }
        }
        account.header()
    }

    /// Lo guardado para ese plugin, descifrado.
    fn account(&self, plugin: &str) -> Option<Account> {
        open_account(&self.root, self.secrets.as_deref(), plugin)
    }

    /// Guarda, cifrado.
    fn save(&self, plugin: &str, account: &Account) -> Result<(), String> {
        seal_account(&self.root, self.secrets.as_deref(), plugin, account)
    }
}

/// Lee la cuenta cifrada de un plugin, si hay.
fn open_account(root: &Path, secrets: Option<&SecretBox>, plugin: &str) -> Option<Account> {
    let path = credential_file(root, plugin)?;
    let sealed = std::fs::read(path).ok()?;
    let plain = secrets?.open_sealed(&sealed).ok()?;
    serde_json::from_slice(&plain).ok()
}

/// Guarda una cuenta cifrada en la carpeta de su plugin.
///
/// Vive suelta porque la usan los dos lados: quien conecta la cuenta y
/// quien la renueva a media petición, y uno llega por un `dyn HostServices`
/// que no sabe de esto.
fn seal_account(
    root: &Path,
    secrets: Option<&SecretBox>,
    plugin: &str,
    account: &Account,
) -> Result<(), String> {
    let secrets = secrets.ok_or("no hay dónde guardar un secreto en este equipo")?;
    let path = credential_file(root, plugin).ok_or("ese id no sirve de carpeta")?;
    let plain = serde_json::to_vec(account).map_err(|e| e.to_string())?;
    let sealed = secrets.seal(&plain).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// La petición que `chilpa-auth` no trae adentro: ese crate no conoce la
/// red, y aquí se le presta la misma que usa todo lo demás.
fn post(url: &str, body: &str) -> Result<(u16, String), String> {
    let request = serde_json::json!({
        "url": url,
        "method": "POST",
        "headers": {
            "Content-Type": "application/x-www-form-urlencoded",
            "Accept": "application/json",
        },
        "body": body,
    });
    let raw = chilpa_core::send_request(&serde_json::to_vec(&request).unwrap(), None);
    let reply: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if let Some(e) = reply.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    Ok((
        reply.get("status").and_then(|s| s.as_u64()).unwrap_or(0) as u16,
        reply.get("body").and_then(|b| b.as_str()).unwrap_or_default().to_string(),
    ))
}

/// Dónde vive la credencial cifrada de un plugin.
///
/// El id se revisa antes de volverse una ruta, igual que una llave del
/// almacén: uno con barras saldría de su carpeta.
fn credential_file(root: &Path, plugin: &str) -> Option<PathBuf> {
    valid_id(plugin).then(|| root.join(plugin).join("credential.sealed"))
}

impl ChilpaServices {
    fn auth_map_inner(&self) -> Arc<Mutex<chilpa_plugin::AuthMap>> {
        Arc::clone(&self.auth)
    }
}

impl HostServices for ChilpaServices {
    fn auth_map(&self) -> Option<Arc<Mutex<chilpa_plugin::AuthMap>>> {
        Some(self.auth_map_inner())
    }

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

    fn store_set(&mut self, plugin: &str, key: &str, value: &[u8]) -> bool {
        let Some(path) = self.file(plugin, key) else { return false };
        // Un plugin no puede llenar el disco de a poquito: el almacén es
        // para ajustes y marcas, no para una copia del catálogo.
        if value.len() > 1024 * 1024 {
            return false;
        }
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, value).is_ok()
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
    /// Cierto si tiene una credencial guardada. No dice cuál, ni la fachada
    /// la muestra nunca: solo si hay cuenta conectada o no.
    pub connected: bool,
    /// Cierto si la cuenta venció y no hay con qué renovarla sola: el
    /// usuario tiene que volver a conectarla. Se dice en lugar de dejar que
    /// todo empiece a fallar sin explicación.
    pub account_expired: bool,
    /// A dónde se manda al usuario a dar permiso, vacío si no usa OAuth o
    /// no necesita cuenta.
    pub account_host: String,
}

/// Una conexión de cuenta a medias: ya se mandó al usuario al navegador y
/// se está esperando a que regrese.
///
/// La espera corre en su propio hilo porque el usuario puede tardar lo que
/// quiera —hasta cinco minutos— y la ventana no puede quedarse tiesa
/// mientras tanto. Lo que contesta llega por `poll`, igual que la búsqueda.
pub struct Connecting {
    pub id: String,
    pub answer: std::sync::mpsc::Receiver<Result<chilpa_auth::Tokens, String>>,
    pub thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Connecting {
    fn drop(&mut self) {
        // No se espera al hilo: si el usuario abandonó el navegador, seguir
        // esperándolo dejaría la aplicación sin cerrar. El hilo se acaba
        // solo cuando se le agota la paciencia.
        self.thread.take();
    }
}

/// Todo lo de plugins que la fachada guarda.
pub struct Plugins {
    host: Host,
    registry: Registry,
    services: Services,
    approvals: Approvals,
    /// La caja de secretos, o `None` si no se pudo abrir ninguna. Sin ella
    /// no se guarda nada: antes que dejar un token en claro, no se guarda.
    secrets: Option<Arc<SecretBox>>,
    root: PathBuf,
}

impl Plugins {
    /// Arma el host. Falla solo si el motor no se pudo crear, que sería un
    /// problema de la máquina y no de un plugin.
    pub fn new(root: PathBuf) -> Option<Self> {
        let host = Host::new().ok()?;
        let secrets = SecretBox::open("chilpa", &root).ok().map(Arc::new);
        let services: Services =
            Arc::new(Mutex::new(ChilpaServices::new(root.clone(), secrets.clone())));
        let approvals = Approvals::load(&root);
        Some(Self { host, registry: Registry::new(), services, approvals, secrets, root })
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
        self.remember_auth();
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

    /// Le pasa a los servicios cómo conecta su cuenta cada plugin.
    ///
    /// Es lo único que hace falta para renovar un acceso vencido a media
    /// petición, y es lo que evita tener que preguntarle al registro desde
    /// adentro de una llamada que ya tiene el candado tomado.
    fn remember_auth(&mut self) {
        let known: BTreeMap<String, Auth> = self
            .registry
            .installed()
            .filter_map(|i| Some((i.id.clone(), i.manifest.as_ref()?.auth.clone()?)))
            .collect();
        if let Ok(services) = self.services.lock() {
            if let Some(map) = services.auth_map() {
                if let Ok(mut m) = map.lock() {
                    *m = known;
                }
            }
        }
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
                    connected: self.has_credential(&i.id),
                    account_expired: self.expired(&i.id),
                    account_host: i
                        .manifest
                        .as_ref()
                        .and_then(|m| m.auth.as_ref())
                        .map(|a| a.host())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivate(&mut self, id: &str) {
        self.registry.reactivate(id);
    }

    /// Guarda la credencial de un plugin, cifrada.
    ///
    /// El plugin nunca la ve: lo único que hace con ella es que Chilpa la
    /// ponga en sus peticiones (ADR-008).
    pub fn set_credential(&mut self, id: &str, value: &str) -> Result<(), String> {
        self.save_account(id, &Account::Token { header: value.to_string() })
    }

    /// Guarda una cuenta ya conectada, sea fija o de OAuth.
    pub fn save_account(&mut self, id: &str, account: &Account) -> Result<(), String> {
        seal_account(&self.root, self.secrets.as_deref(), id, account)
    }

    /// Borra la credencial de un plugin. Revocarlo la borra también.
    pub fn forget_credential(&mut self, id: &str) {
        if let Some(path) = credential_file(&self.root, id) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Cierto si la cuenta de ese plugin venció sin cómo renovarse.
    fn expired(&self, id: &str) -> bool {
        open_account(&self.root, self.secrets.as_deref(), id)
            .is_some_and(|a| a.dead(now_unix()))
    }

    /// Cierto si ese plugin tiene una credencial guardada.
    pub fn has_credential(&self, id: &str) -> bool {
        credential_file(&self.root, id).is_some_and(|p| p.exists())
    }

    /// Dónde se están guardando los secretos, para poder decirlo.
    pub fn secrets_backend(&self) -> Option<BackendKind> {
        self.secrets.as_ref().map(|s| s.backend())
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

    /// Cómo conecta su cuenta ese plugin, según su manifest.
    pub fn auth_of(&self, id: &str) -> Option<chilpa_auth::Auth> {
        self.registry.installed().find(|i| i.id == id)?.manifest.as_ref()?.auth.clone()
    }

    /// Se lleva la carpeta del plugin. Devuelve cierto si ya no está.
    pub fn uninstall(&mut self, id: &str) -> bool {
        let Some(dir) = valid_id(id).then(|| self.root.join(id)) else { return false };
        if !dir.exists() {
            return true;
        }
        std::fs::remove_dir_all(&dir).is_ok()
    }

    /// Cierto si ese plugin dijo que maneja su propia cuenta.
    pub fn manages_own_account(&self, id: &str) -> bool {
        self.registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.as_ref())
            .is_some_and(|m| {
                m.capabilities
                    .iter()
                    .any(|c| {
                        matches!(c, chilpa_plugin::manifest::Capability::OwnAccount { .. })
                    })
            })
    }

    /// Cierto si ese source dice que puede entregar audio.
    pub fn delivers_audio(&self, id: &str) -> bool {
        self.registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.as_ref())
            .is_none_or(|m| m.delivers_audio)
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
        // Quitar el permiso borra el secreto: dejarlo guardado para un
        // plugin que ya no corre es guardar un token por nada.
        //
        // Quitar el permiso no desinstala: el plugin sigue en su carpeta y
        // lo suyo también. Lo que lo borra todo es desinstalarlo, que le
        // pide primero que limpie.
        p.forget_credential(id);
        p.forget();
        p.load_all()
    }

    /// Le pide una operación a un plugin con los límites cortos.
    ///
    /// Es lo que se usa cuando alguien está esperando frente a la ventana,
    /// como al resolver una pista para ponerla a sonar.
    pub fn call_plugin_short(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.call_short(id, operation, input),
            None => Err("el sandbox de plugins no está arrancado".into()),
        }
    }

    /// Arranca la conexión de una cuenta y contesta a dónde hay que mandar
    /// al usuario.
    ///
    /// No abre el navegador: cómo se abre uno es cosa del sistema, y quien
    /// llama sabe si hay ventana o no. A partir de aquí se espera en otro
    /// hilo, y el resultado llega por [`Chilpa::poll`].
    pub fn connect_account(&mut self, id: &str) -> Result<String, String> {
        let auth = self
            .auth_of(id)
            .ok_or_else(|| format!("\"{id}\" no dice cómo se conecta una cuenta"))?;
        let back = chilpa_auth::Loopback::bind().map_err(|e| e.to_string())?;
        let pkce = chilpa_auth::Pkce::new();
        let url =
            chilpa_auth::authorize_url(&auth, &pkce, &back).ok_or("ese source no usa OAuth")?;

        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("chilpa-connect".into())
            .spawn(move || {
                let answer = chilpa_auth::wait_for_code(&back)
                    .map_err(|e| e.to_string())
                    .and_then(|code| {
                        chilpa_auth::exchange(&auth, &code, &pkce, &back, now_unix(), &post)
                            .map_err(|e| e.to_string())
                    });
                let _ = tx.send(answer);
            })
            .ok();

        self.connecting = Some(Connecting { id: id.to_string(), answer: rx, thread });
        Ok(url)
    }

    /// Cierto mientras se espera a que el usuario termine en el navegador.
    pub fn connecting(&self) -> bool {
        self.connecting.is_some()
    }

    /// Deja de esperar. El usuario cerró el navegador o cambió de opinión.
    pub fn cancel_connect(&mut self) {
        self.connecting = None;
    }

    /// Recoge lo que haya contestado el navegador. Lo llama `poll`.
    ///
    /// Devuelve el id del plugin que quedó conectado, si alguno.
    pub(crate) fn advance_connect(&mut self) -> Option<String> {
        let answer = self.connecting.as_ref()?.answer.try_recv().ok()?;
        let c = self.connecting.take()?;
        match answer {
            Ok(tokens) => {
                let saved = self
                    .plugins
                    .clone()
                    .and_then(|p| p.lock().ok().map(|mut p| p.save_account(&c.id, &Account::OAuth { tokens })));
                match saved {
                    Some(Ok(())) => Some(c.id.clone()),
                    Some(Err(e)) => {
                        self.now.error = format!("la cuenta se conectó pero no se pudo guardar: {e}");
                        None
                    }
                    None => None,
                }
            }
            Err(e) => {
                self.now.error = format!("no se conectó la cuenta: {e}");
                None
            }
        }
    }

    /// Cómo conecta su cuenta ese plugin, según su manifest.
    fn auth_of(&self, id: &str) -> Option<chilpa_auth::Auth> {
        let p = self.plugins.as_ref()?.lock().ok()?;
        p.auth_of(id)
    }

    /// Guarda la credencial de un plugin, cifrada, y dice dónde quedó.
    ///
    /// El plugin no la recibe nunca: lo único que cambia para él es que sus
    /// peticiones salen autenticadas (ADR-008).
    pub fn connect_plugin(&mut self, id: &str, credential: &str) -> bool {
        let Some(p) = self.plugins.clone() else {
            self.now.error = "el sandbox de plugins no está arrancado".into();
            return false;
        };
        let Ok(mut p) = p.lock() else { return false };
        match p.set_credential(id, credential) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = format!("no se pudo guardar la credencial: {e}");
                false
            }
        }
    }

    /// Borra la credencial de un plugin. El plugin sigue instalado.
    pub fn disconnect_plugin(&mut self, id: &str) {
        if let Some(mut p) = self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            p.forget_credential(id);
        }
    }

    /// Dónde se están guardando los secretos en este equipo, en palabras.
    /// Vacío si no hay dónde.
    pub fn secrets_location(&self) -> String {
        self.plugins
            .as_ref()
            .and_then(|p| p.lock().ok())
            .and_then(|p| p.secrets_backend())
            .map(|b| b.describe().to_string())
            .unwrap_or_default()
    }

    /// Lo desinstala: le pide que borre lo suyo, le quita el permiso y se
    /// lleva su carpeta.
    ///
    /// El orden importa. Primero se le avisa, **mientras todavía puede
    /// correr**: un plugin que maneja su propia cuenta guardó su credencial
    /// donde Chilpa no llega, y la única forma de que se vaya es que él la
    /// borre. Quitarle el permiso antes lo dejaría sin poder hacerlo.
    ///
    /// Que no conteste no detiene nada: se desinstala igual y se dice qué
    /// quedó sin limpiar, porque callarlo sería dejar al usuario creyendo
    /// que se llevó todo.
    pub fn uninstall_plugin(&mut self, id: &str) -> bool {
        let Some(shared) = self.plugins.clone() else { return false };
        let Ok(mut p) = shared.lock() else { return false };

        let own = p.manages_own_account(id);
        let told = p
            .call_short(id, chilpa_plugin::messages::op::REVOKE, b"{}")
            .is_ok();
        p.forget_credential(id);
        let _ = p.revoke(id);
        p.forget();
        let gone = p.uninstall(id);
        p.load_all();
        drop(p);

        if !gone {
            self.now.error = format!("no se pudo borrar la carpeta de \"{id}\"");
            return false;
        }
        if own && !told {
            self.now.error = "lo desinstalé, pero manejaba su cuenta por su lado y no \
                 contestó cuando le pedí borrarla: puede que tu credencial siga guardada \
                 donde él la haya puesto"
                .into();
        }
        true
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
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"), None);
        for bad in ["../fuera", "con/barra", "..", "CON.MAYUSCULAS", "", &"x".repeat(65)] {
            assert!(s.file("com.a.b", bad).is_none(), "\"{bad}\" no debería pasar");
        }
        assert!(s.file("com.a.b", "token").is_some());
        assert!(s.file("com.a.b", "ultima-sync.json").is_some());
    }

    #[test]
    fn nor_does_an_id_with_slashes() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"), None);
        assert!(s.file("../otro", "token").is_none());
        assert!(s.file("con/barra", "token").is_none());
    }

    #[test]
    fn a_plugin_store_round_trips() {
        let dir = std::env::temp_dir().join("chilpa-plugins-almacen");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone(), None);
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
        let mut s = ChilpaServices::new(dir.clone(), None);
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
