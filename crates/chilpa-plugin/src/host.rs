//! El host: carga un plugin, lo llama y lo detiene si se pasa.
//!
//! Todo lo que un guest puede hacer entra por aquí, y aquí se aplica lo que
//! el manifest prometió. Un plugin que pidió red a un dominio no puede salir
//! a otro, y no porque se le pida que no: porque la llamada se rechaza antes
//! de llegar al host de verdad.
//!
//! **El plugin no recuerda nada entre llamadas.** Cada llamada arma su
//! instancia nueva, con su combustible y su memoria. Lo que quiera recordar
//! lo manda a su almacén, que es del host. Así una fuga de memoria no se
//! acumula y una llamada no puede envenenar a la siguiente.

use crate::abi;
use crate::limits::{Halt, Limits};
use crate::manifest::{Capability, Manifest, ManifestError};
use crate::services::{HostServices, Level};
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// Los servicios del host, compartidos entre llamadas y plugins.
pub type Services = Arc<Mutex<dyn HostServices>>;

/// Cada cuánto avanza el reloj de las interrupciones.
const TIC: std::time::Duration = std::time::Duration::from_millis(50);

/// Por qué no se pudo cargar o llamar a un plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PluginError {
    /// El manifest no pasó.
    Manifest(ManifestError),
    /// El módulo no es wasm válido, o el motor no lo aceptó.
    WontCompile(String),
    /// Le falta algo que todo plugin tiene que exportar.
    MissingExport(String),
    /// Dice trabajar con otra versión de la API que la del manifest.
    VersionMismatch { dice: i32, manifest: u32 },
    /// Se pasó de alguno de sus límites.
    Stopped(Halt),
    /// Tronó adentro.
    Crashed(String),
    /// Dio una dirección que no está en su memoria.
    OutOfMemory,
    /// Pidió algo que su manifest no declaró.
    NotAllowed(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Manifest(e) => write!(f, "{e}"),
            PluginError::WontCompile(e) => write!(f, "el módulo no se pudo cargar: {e}"),
            PluginError::MissingExport(n) => {
                write!(f, "no exporta \"{n}\", que todo plugin tiene que exportar")
            }
            PluginError::VersionMismatch { dice, manifest } => write!(
                f,
                "el manifest dice API {manifest} y el módulo contesta {dice}; no se carga"
            ),
            PluginError::Stopped(d) => write!(f, "{d}"),
            PluginError::Crashed(e) => write!(f, "tronó adentro: {e}"),
            PluginError::OutOfMemory => {
                write!(f, "contestó una dirección que no está en su memoria")
            }
            PluginError::NotAllowed(q) => write!(f, "pidió {q}, que su manifest no declaró"),
        }
    }
}

impl From<ManifestError> for PluginError {
    fn from(e: ManifestError) -> Self {
        PluginError::Manifest(e)
    }
}

/// Lo que vive en el store mientras corre una llamada.
struct State {
    plugin: String,
    domains: Vec<String>,
    can_store: bool,
    can_go_out: bool,
    services: Services,
    limits: StoreLimits,
}

/// El motor, compartido por todos los plugins.
pub struct Host {
    engine: Engine,
    /// El hilo que hace avanzar el reloj de las interrupciones.
    vivo: Arc<std::sync::atomic::AtomicBool>,
    clock: Option<std::thread::JoinHandle<()>>,
}

impl Host {
    pub fn new() -> Result<Self, PluginError> {
        let mut config = Config::new();
        // Combustible para el que calcula de más, reloj para el que se
        // cuelga. Hacen falta los dos.
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| PluginError::WontCompile(e.to_string()))?;

        let vivo = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag = Arc::clone(&vivo);
        let motor = engine.clone();
        let clock = std::thread::Builder::new()
            .name("chilpa-plugins-reloj".into())
            .spawn(move || {
                while flag.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(TIC);
                    motor.increment_epoch();
                }
            })
            .ok();

        Ok(Self { engine, vivo, clock })
    }

    /// Compila un plugin y comprueba que sea lo que dice ser.
    pub fn load(&self, manifest: Manifest, wasm: &[u8]) -> Result<Plugin, PluginError> {
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| PluginError::WontCompile(e.to_string()))?;

        // Tiene que exportar lo mínimo, o no se carga. Preguntarlo aquí y no
        // a media llamada es la diferencia entre "no cargó" y "se rompió
        // mientras lo usabas".
        for export in [abi::MEMORIA, abi::F_VERSION, abi::F_ALLOC] {
            if module.get_export(export).is_none() {
                return Err(PluginError::MissingExport(export.to_string()));
            }
        }

        let plugin = Plugin { manifest, module };
        plugin.check_version(self)?;
        Ok(plugin)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.vivo.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.clock.take() {
            let _ = t.join();
        }
    }
}

/// Un plugin ya compilado y listo para llamarse.
pub struct Plugin {
    manifest: Manifest,
    module: Module,
}

impl Plugin {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// Le pregunta al módulo con qué versión trabaja y la compara con la que
    /// dijo el manifest. Si no coinciden, alguien mintió y no se carga.
    fn check_version(&self, host: &Host) -> Result<(), PluginError> {
        let services: Services = Arc::new(Mutex::new(crate::services::NoServices));
        let mut store = self.build_store(host, &services, Limits::short())?;
        let linker = self.build_linker(host)?;
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| PluginError::Crashed(e.to_string()))?;
        let f = instance
            .get_typed_func::<(), i32>(&mut store, abi::F_VERSION)
            .map_err(|_| PluginError::MissingExport(abi::F_VERSION.to_string()))?;
        let dice = f.call(&mut store, ()).map_err(|e| self.translate(e, &mut store))?;
        if dice as u32 != self.manifest.api_version {
            return Err(PluginError::VersionMismatch {
                dice,
                manifest: self.manifest.api_version,
            });
        }
        Ok(())
    }

    /// Llama una operación del plugin con su presupuesto.
    pub fn call(
        &self,
        host: &Host,
        services: &Services,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<abi::Reply, PluginError> {
        let mut store = self.build_store(host, services, limits)?;
        let linker = self.build_linker(host)?;
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| PluginError::Crashed(e.to_string()))?;

        let memoria = instance
            .get_memory(&mut store, abi::MEMORIA)
            .ok_or_else(|| PluginError::MissingExport(abi::MEMORIA.to_string()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, abi::F_ALLOC)
            .map_err(|_| PluginError::MissingExport(abi::F_ALLOC.to_string()))?;
        let op = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, operation)
            .map_err(|_| PluginError::MissingExport(operation.to_string()))?;

        // Se le pide un pedazo de su memoria y se le escribe la entrada.
        let ptr = alloc
            .call(&mut store, input.len() as i32)
            .map_err(|e| self.translate(e, &mut store))?;
        write(&memoria, &mut store, ptr as u32, input)?;

        let packed = op
            .call(&mut store, (ptr, input.len() as i32))
            .map_err(|e| self.translate(e, &mut store))?;

        let Some((rptr, rlen)) = abi::unpack(packed) else {
            return Ok(abi::Reply::Error("no contestó nada".into()));
        };
        let bytes = read(&memoria, &mut store, rptr, rlen)?;
        Ok(abi::read_reply(bytes))
    }

    fn build_store(
        &self,
        host: &Host,
        services: &Services,
        limits: Limits,
    ) -> Result<Store<State>, PluginError> {
        let state = State {
            plugin: self.manifest.id.clone(),
            domains: self.manifest.allowed_domains().iter().map(|d| d.to_string()).collect(),
            can_store: self.manifest.can_store(),
            can_go_out: self.manifest.can_go_out(),
            services: Arc::clone(services),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes)
                // Una sola memoria y una sola tabla: un plugin no necesita
                // más, y cada una es memoria que se puede pedir.
                .memories(1)
                .tables(1)
                .build(),
        };
        let mut store = Store::new(&host.engine, state);
        store.limiter(|e| &mut e.limits);
        store
            .set_fuel(limits.fuel)
            .map_err(|e| PluginError::WontCompile(e.to_string()))?;
        // El reloj avanza cada TIC; el plazo son tantos tics como quepan.
        let tics = (limits.timeout.as_millis() / TIC.as_millis()).max(1) as u64;
        store.set_epoch_deadline(tics);
        Ok(store)
    }

    /// Traduce lo que wasmtime contesta a algo que el usuario pueda leer.
    ///
    /// La razón de verdad viene en la cadena de causas, no en el mensaje de
    /// arriba: el de arriba siempre dice "error while executing".
    fn translate(&self, e: wasmtime::Error, store: &mut Store<State>) -> PluginError {
        if let Some(trap) = e.downcast_ref::<wasmtime::Trap>() {
            return match trap {
                wasmtime::Trap::OutOfFuel => PluginError::Stopped(Halt::OutOfFuel),
                wasmtime::Trap::Interrupt => {
                    PluginError::Stopped(Halt::Timeout(std::time::Duration::ZERO))
                }
                other => PluginError::Crashed(other.to_string()),
            };
        }
        // Quedarse sin combustible se nota también en lo que queda.
        if store.get_fuel().map(|f| f == 0).unwrap_or(false) {
            return PluginError::Stopped(Halt::OutOfFuel);
        }
        let causa = format!("{e:#}");
        if causa.contains("memory") && (causa.contains("limit") || causa.contains("grow")) {
            return PluginError::Stopped(Halt::OutOfMemory);
        }
        PluginError::Crashed(causa)
    }

    fn build_linker(&self, host: &Host) -> Result<Linker<State>, PluginError> {
        let mut linker = Linker::new(&host.engine);
        let m = abi::MODULO_HOST;

        linker
            .func_wrap(m, "log", |mut caller: Caller<'_, State>, level: i32, ptr: i32, len: i32| {
                let Some(text) = text_of(&mut caller, ptr, len) else { return };
                let id = caller.data().plugin.clone();
                let services = Arc::clone(&caller.data().services);
                let mut save = match services.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                save.log(&id, Level::from_code(level), &text);
            })
            .and_then(|l| {
                l.func_wrap(m, "now", |caller: Caller<'_, State>| -> i64 {
                    let services = Arc::clone(&caller.data().services);
                    services.lock().map(|mut s| s.now_unix()).unwrap_or(0)
                })
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "store_get",
                    |mut caller: Caller<'_, State>,
                     kptr: i32,
                     klen: i32,
                     out: i32,
                     room: i32|
                     -> i32 {
                        if !caller.data().can_store {
                            return abi::Room::Nothing.code();
                        }
                        let Some(key) = text_of(&mut caller, kptr, klen) else {
                            return abi::Room::Nothing.code();
                        };
                        let id = caller.data().plugin.clone();
                        let services = Arc::clone(&caller.data().services);
                        let valor = services.lock().ok().and_then(|mut s| s.store_get(&id, &key));
                        deliver(&mut caller, valor.as_deref(), out, room)
                    },
                )
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "store_set",
                    |mut caller: Caller<'_, State>,
                     kptr: i32,
                     klen: i32,
                     vptr: i32,
                     vlen: i32|
                     -> i32 {
                        if !caller.data().can_store {
                            return 0;
                        }
                        let Some(key) = text_of(&mut caller, kptr, klen) else { return 0 };
                        let Some(valor) = bytes_de(&mut caller, vptr, vlen) else { return 0 };
                        let id = caller.data().plugin.clone();
                        let services = Arc::clone(&caller.data().services);
                        let ok = services
                            .lock()
                            .map(|mut s| s.store_set(&id, &key, &valor))
                            .unwrap_or(false);
                        i32::from(ok)
                    },
                )
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "http",
                    |mut caller: Caller<'_, State>,
                     rptr: i32,
                     rlen: i32,
                     out: i32,
                     room: i32|
                     -> i32 {
                        let Some(request) = bytes_de(&mut caller, rptr, rlen) else {
                            return abi::Room::Nothing.code();
                        };
                        let denied = |reason: &str| -> Vec<u8> {
                            serde_json::to_vec(&serde_json::json!({ "error": reason }))
                                .unwrap_or_default()
                        };
                        let reply = if !caller.data().can_go_out {
                            denied("este plugin no declaró permiso de red")
                        } else {
                            match domain_of(&request) {
                                None => denied("la petición no trae una URL que se entienda"),
                                Some(d) if !allowed(&d, &caller.data().domains) => denied(
                                    "ese dominio no está en los que el plugin declaró",
                                ),
                                Some(_) => {
                                    let id = caller.data().plugin.clone();
                                    let services = Arc::clone(&caller.data().services);
                                    services
                                        .lock()
                                        .map(|mut s| s.http(&id, &request))
                                        .unwrap_or_else(|_| denied("el host no pudo atender"))
                                }
                            }
                        };
                        deliver(&mut caller, Some(&reply), out, room)
                    },
                )
            })
            .map_err(|e| PluginError::WontCompile(e.to_string()))?;

        Ok(linker)
    }
}

/// El dominio al que apunta una petición, si se entiende.
fn domain_of(request: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(request).ok()?;
    let url = v.get("url")?.as_str()?;
    host_de_url(url)
}

/// Saca la máquina de una URL sin arrastrar un parser de URLs entero.
pub fn host_de_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let fin = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..fin];
    // Se tira el usuario y el puerto: lo que se compara es la máquina.
    let no_user = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = no_user.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Cierto si el dominio está en la lista, exacto o como subdominio.
///
/// Se permite el subdominio porque un servicio reparte sus respuestas entre
/// varias máquinas del mismo dominio; lo que no se permite es que la lista
/// traiga un comodín, que es donde se pierde el control.
fn allowed(host: &str, domains: &[String]) -> bool {
    domains.iter().any(|d| {
        let d = d.to_ascii_lowercase();
        host == d || host.ends_with(&format!(".{d}"))
    })
}

fn memoria_de(caller: &mut Caller<'_, State>) -> Option<wasmtime::Memory> {
    caller.get_export(abi::MEMORIA).and_then(|e| e.into_memory())
}

fn bytes_de(caller: &mut Caller<'_, State>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let mem = memoria_de(caller)?;
    let data = mem.data(&caller);
    let (from, hasta) = (ptr as usize, (ptr as usize).checked_add(len as usize)?);
    data.get(from..hasta).map(<[u8]>::to_vec)
}

fn text_of(caller: &mut Caller<'_, State>, ptr: i32, len: i32) -> Option<String> {
    let bytes = bytes_de(caller, ptr, len)?;
    String::from_utf8(bytes).ok()
}

/// Escribe la respuesta de una función del host en el búfer del guest.
fn deliver(caller: &mut Caller<'_, State>, data: Option<&[u8]>, out: i32, room: i32) -> i32 {
    let room_u = room.max(0) as u32;
    let verdict = abi::Room::para(data, room_u);
    if let (abi::Room::Fits(n), Some(d)) = (verdict, data) {
        let Some(mem) = memoria_de(caller) else { return abi::Room::Nothing.code() };
        let target = mem.data_mut(caller);
        let (from, hasta) = (out.max(0) as usize, out.max(0) as usize + n as usize);
        match target.get_mut(from..hasta) {
            Some(gap) => gap.copy_from_slice(d),
            // Un búfer fuera de su memoria se trata como que no cupo: nunca
            // se escribe a medias ni fuera.
            None => return abi::Room::TooBig(n).code(),
        }
    }
    verdict.code()
}

fn write(
    memoria: &wasmtime::Memory,
    store: &mut Store<State>,
    ptr: u32,
    data: &[u8],
) -> Result<(), PluginError> {
    let target = memoria.data_mut(store);
    let (from, hasta) = (ptr as usize, ptr as usize + data.len());
    target
        .get_mut(from..hasta)
        .ok_or(PluginError::OutOfMemory)?
        .copy_from_slice(data);
    Ok(())
}

fn read(
    memoria: &wasmtime::Memory,
    store: &mut Store<State>,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, PluginError> {
    let data = memoria.data(store);
    let (from, hasta) = (ptr as usize, (ptr as usize).saturating_add(len as usize));
    data
        .get(from..hasta)
        .map(<[u8]>::to_vec)
        .ok_or(PluginError::OutOfMemory)
}

/// Lo que un plugin tiene que declarar para que se le den servicios.
pub fn network_capability(domains: &[&str]) -> Capability {
    Capability::Network { domains: domains.iter().map(|d| d.to_string()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_yields_its_host() {
        assert_eq!(host_de_url("https://api.ejemplo.com/v1/buscar"), Some("api.ejemplo.com".into()));
        assert_eq!(host_de_url("http://API.Ejemplo.COM"), Some("api.ejemplo.com".into()));
        assert_eq!(host_de_url("https://api.ejemplo.com:8443/x"), Some("api.ejemplo.com".into()));
        assert_eq!(
            host_de_url("https://usuario:clave@api.ejemplo.com/x"),
            Some("api.ejemplo.com".into()),
            "el usuario no es la máquina"
        );
        assert_eq!(host_de_url("api.ejemplo.com/x"), None, "sin esquema no se adivina");
        assert_eq!(host_de_url("https:///x"), None);
    }

    #[test]
    fn an_allowed_domain_covers_its_subdomains_and_nothing_else() {
        let list = vec!["ejemplo.com".to_string()];
        assert!(allowed("ejemplo.com", &list));
        assert!(allowed("api.ejemplo.com", &list));
        assert!(allowed("a.b.ejemplo.com", &list));
        assert!(!allowed("ejemplo.com.malo.net", &list), "el sufijo no basta");
        assert!(!allowed("noejemplo.com", &list));
        assert!(!allowed("otro.net", &list));
        assert!(!allowed("ejemplo.com", &[]), "sin lista no se permite nada");
    }

    #[test]
    fn a_request_yields_its_domain() {
        assert_eq!(
            domain_of(br#"{"url":"https://api.ejemplo.com/buscar","metodo":"GET"}"#),
            Some("api.ejemplo.com".into())
        );
        assert_eq!(domain_of(b"{}"), None, "sin url no hay dominio");
        assert_eq!(domain_of(b"no es json"), None);
    }
}
