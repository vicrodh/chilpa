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
use crate::limits::{Detencion, Limits};
use crate::manifest::{Capability, Manifest, ManifestError};
use crate::services::{HostServices, Nivel};
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// Los servicios del host, compartidos entre llamadas y plugins.
pub type Servicios = Arc<Mutex<dyn HostServices>>;

/// Cada cuánto avanza el reloj de las interrupciones.
const TIC: std::time::Duration = std::time::Duration::from_millis(50);

/// Por qué no se pudo cargar o llamar a un plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PluginError {
    /// El manifest no pasó.
    Manifest(ManifestError),
    /// El módulo no es wasm válido, o el motor no lo aceptó.
    NoCompila(String),
    /// Le falta algo que todo plugin tiene que exportar.
    FaltaExport(String),
    /// Dice trabajar con otra versión de la API que la del manifest.
    VersionInconsistente { dice: i32, manifest: u32 },
    /// Se pasó de alguno de sus límites.
    Detenido(Detencion),
    /// Tronó adentro.
    Trono(String),
    /// Dio una dirección que no está en su memoria.
    MemoriaFuera,
    /// Pidió algo que su manifest no declaró.
    SinPermiso(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Manifest(e) => write!(f, "{e}"),
            PluginError::NoCompila(e) => write!(f, "el módulo no se pudo cargar: {e}"),
            PluginError::FaltaExport(n) => {
                write!(f, "no exporta \"{n}\", que todo plugin tiene que exportar")
            }
            PluginError::VersionInconsistente { dice, manifest } => write!(
                f,
                "el manifest dice API {manifest} y el módulo contesta {dice}; no se carga"
            ),
            PluginError::Detenido(d) => write!(f, "{d}"),
            PluginError::Trono(e) => write!(f, "tronó adentro: {e}"),
            PluginError::MemoriaFuera => {
                write!(f, "contestó una dirección que no está en su memoria")
            }
            PluginError::SinPermiso(q) => write!(f, "pidió {q}, que su manifest no declaró"),
        }
    }
}

impl From<ManifestError> for PluginError {
    fn from(e: ManifestError) -> Self {
        PluginError::Manifest(e)
    }
}

/// Lo que vive en el store mientras corre una llamada.
struct Estado {
    plugin: String,
    dominios: Vec<String>,
    puede_guardar: bool,
    puede_salir: bool,
    servicios: Servicios,
    limites: StoreLimits,
}

/// El motor, compartido por todos los plugins.
pub struct Host {
    engine: Engine,
    /// El hilo que hace avanzar el reloj de las interrupciones.
    vivo: Arc<std::sync::atomic::AtomicBool>,
    reloj: Option<std::thread::JoinHandle<()>>,
}

impl Host {
    pub fn new() -> Result<Self, PluginError> {
        let mut config = Config::new();
        // Combustible para el que calcula de más, reloj para el que se
        // cuelga. Hacen falta los dos.
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| PluginError::NoCompila(e.to_string()))?;

        let vivo = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let bandera = Arc::clone(&vivo);
        let motor = engine.clone();
        let reloj = std::thread::Builder::new()
            .name("chilpa-plugins-reloj".into())
            .spawn(move || {
                while bandera.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(TIC);
                    motor.increment_epoch();
                }
            })
            .ok();

        Ok(Self { engine, vivo, reloj })
    }

    /// Compila un plugin y comprueba que sea lo que dice ser.
    pub fn cargar(&self, manifest: Manifest, wasm: &[u8]) -> Result<Plugin, PluginError> {
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| PluginError::NoCompila(e.to_string()))?;

        // Tiene que exportar lo mínimo, o no se carga. Preguntarlo aquí y no
        // a media llamada es la diferencia entre "no cargó" y "se rompió
        // mientras lo usabas".
        for export in [abi::MEMORIA, abi::F_VERSION, abi::F_ALLOC] {
            if module.get_export(export).is_none() {
                return Err(PluginError::FaltaExport(export.to_string()));
            }
        }

        let plugin = Plugin { manifest, module };
        plugin.comprobar_version(self)?;
        Ok(plugin)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.vivo.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.reloj.take() {
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
    fn comprobar_version(&self, host: &Host) -> Result<(), PluginError> {
        let servicios: Servicios = Arc::new(Mutex::new(crate::services::SinServicios));
        let mut store = self.armar_store(host, &servicios, Limits::cortos())?;
        let linker = self.armar_linker(host)?;
        let instancia = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| PluginError::Trono(e.to_string()))?;
        let f = instancia
            .get_typed_func::<(), i32>(&mut store, abi::F_VERSION)
            .map_err(|_| PluginError::FaltaExport(abi::F_VERSION.to_string()))?;
        let dice = f.call(&mut store, ()).map_err(|e| self.traducir(e, &mut store))?;
        if dice as u32 != self.manifest.api_version {
            return Err(PluginError::VersionInconsistente {
                dice,
                manifest: self.manifest.api_version,
            });
        }
        Ok(())
    }

    /// Llama una operación del plugin con su presupuesto.
    pub fn llamar(
        &self,
        host: &Host,
        servicios: &Servicios,
        operacion: &str,
        entrada: &[u8],
        limites: Limits,
    ) -> Result<abi::Respuesta, PluginError> {
        let mut store = self.armar_store(host, servicios, limites)?;
        let linker = self.armar_linker(host)?;
        let instancia = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| PluginError::Trono(e.to_string()))?;

        let memoria = instancia
            .get_memory(&mut store, abi::MEMORIA)
            .ok_or_else(|| PluginError::FaltaExport(abi::MEMORIA.to_string()))?;
        let alloc = instancia
            .get_typed_func::<i32, i32>(&mut store, abi::F_ALLOC)
            .map_err(|_| PluginError::FaltaExport(abi::F_ALLOC.to_string()))?;
        let op = instancia
            .get_typed_func::<(i32, i32), i64>(&mut store, operacion)
            .map_err(|_| PluginError::FaltaExport(operacion.to_string()))?;

        // Se le pide un pedazo de su memoria y se le escribe la entrada.
        let ptr = alloc
            .call(&mut store, entrada.len() as i32)
            .map_err(|e| self.traducir(e, &mut store))?;
        escribir(&memoria, &mut store, ptr as u32, entrada)?;

        let empacado = op
            .call(&mut store, (ptr, entrada.len() as i32))
            .map_err(|e| self.traducir(e, &mut store))?;

        let Some((rptr, rlen)) = abi::desempacar(empacado) else {
            return Ok(abi::Respuesta::Error("no contestó nada".into()));
        };
        let bytes = leer(&memoria, &mut store, rptr, rlen)?;
        Ok(abi::leer_respuesta(bytes))
    }

    fn armar_store(
        &self,
        host: &Host,
        servicios: &Servicios,
        limites: Limits,
    ) -> Result<Store<Estado>, PluginError> {
        let estado = Estado {
            plugin: self.manifest.id.clone(),
            dominios: self.manifest.allowed_domains().iter().map(|d| d.to_string()).collect(),
            puede_guardar: self.manifest.puede_guardar(),
            puede_salir: self.manifest.puede_salir(),
            servicios: Arc::clone(servicios),
            limites: StoreLimitsBuilder::new()
                .memory_size(limites.memory_bytes)
                // Una sola memoria y una sola tabla: un plugin no necesita
                // más, y cada una es memoria que se puede pedir.
                .memories(1)
                .tables(1)
                .build(),
        };
        let mut store = Store::new(&host.engine, estado);
        store.limiter(|e| &mut e.limites);
        store
            .set_fuel(limites.fuel)
            .map_err(|e| PluginError::NoCompila(e.to_string()))?;
        // El reloj avanza cada TIC; el plazo son tantos tics como quepan.
        let tics = (limites.timeout.as_millis() / TIC.as_millis()).max(1) as u64;
        store.set_epoch_deadline(tics);
        Ok(store)
    }

    /// Traduce lo que wasmtime contesta a algo que el usuario pueda leer.
    ///
    /// La razón de verdad viene en la cadena de causas, no en el mensaje de
    /// arriba: el de arriba siempre dice "error while executing".
    fn traducir(&self, e: wasmtime::Error, store: &mut Store<Estado>) -> PluginError {
        if let Some(trap) = e.downcast_ref::<wasmtime::Trap>() {
            return match trap {
                wasmtime::Trap::OutOfFuel => PluginError::Detenido(Detencion::SinCombustible),
                wasmtime::Trap::Interrupt => {
                    PluginError::Detenido(Detencion::Timeout(std::time::Duration::ZERO))
                }
                otro => PluginError::Trono(otro.to_string()),
            };
        }
        // Quedarse sin combustible se nota también en lo que queda.
        if store.get_fuel().map(|f| f == 0).unwrap_or(false) {
            return PluginError::Detenido(Detencion::SinCombustible);
        }
        let causa = format!("{e:#}");
        if causa.contains("memory") && (causa.contains("limit") || causa.contains("grow")) {
            return PluginError::Detenido(Detencion::SinMemoria);
        }
        PluginError::Trono(causa)
    }

    fn armar_linker(&self, host: &Host) -> Result<Linker<Estado>, PluginError> {
        let mut linker = Linker::new(&host.engine);
        let m = abi::MODULO_HOST;

        linker
            .func_wrap(m, "log", |mut caller: Caller<'_, Estado>, nivel: i32, ptr: i32, len: i32| {
                let Some(texto) = texto_de(&mut caller, ptr, len) else { return };
                let id = caller.data().plugin.clone();
                let servicios = Arc::clone(&caller.data().servicios);
                let mut guarda = match servicios.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                guarda.log(&id, Nivel::de_codigo(nivel), &texto);
            })
            .and_then(|l| {
                l.func_wrap(m, "now", |caller: Caller<'_, Estado>| -> i64 {
                    let servicios = Arc::clone(&caller.data().servicios);
                    servicios.lock().map(|mut s| s.now_unix()).unwrap_or(0)
                })
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "store_get",
                    |mut caller: Caller<'_, Estado>,
                     kptr: i32,
                     klen: i32,
                     out: i32,
                     cupo: i32|
                     -> i32 {
                        if !caller.data().puede_guardar {
                            return abi::Cupo::Nada.codigo();
                        }
                        let Some(llave) = texto_de(&mut caller, kptr, klen) else {
                            return abi::Cupo::Nada.codigo();
                        };
                        let id = caller.data().plugin.clone();
                        let servicios = Arc::clone(&caller.data().servicios);
                        let valor = servicios.lock().ok().and_then(|mut s| s.store_get(&id, &llave));
                        entregar(&mut caller, valor.as_deref(), out, cupo)
                    },
                )
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "store_set",
                    |mut caller: Caller<'_, Estado>,
                     kptr: i32,
                     klen: i32,
                     vptr: i32,
                     vlen: i32|
                     -> i32 {
                        if !caller.data().puede_guardar {
                            return 0;
                        }
                        let Some(llave) = texto_de(&mut caller, kptr, klen) else { return 0 };
                        let Some(valor) = bytes_de(&mut caller, vptr, vlen) else { return 0 };
                        let id = caller.data().plugin.clone();
                        let servicios = Arc::clone(&caller.data().servicios);
                        let ok = servicios
                            .lock()
                            .map(|mut s| s.store_set(&id, &llave, &valor))
                            .unwrap_or(false);
                        i32::from(ok)
                    },
                )
            })
            .and_then(|l| {
                l.func_wrap(
                    m,
                    "http",
                    |mut caller: Caller<'_, Estado>,
                     rptr: i32,
                     rlen: i32,
                     out: i32,
                     cupo: i32|
                     -> i32 {
                        let Some(peticion) = bytes_de(&mut caller, rptr, rlen) else {
                            return abi::Cupo::Nada.codigo();
                        };
                        let negado = |motivo: &str| -> Vec<u8> {
                            serde_json::to_vec(&serde_json::json!({ "error": motivo }))
                                .unwrap_or_default()
                        };
                        let respuesta = if !caller.data().puede_salir {
                            negado("este plugin no declaró permiso de red")
                        } else {
                            match dominio_de(&peticion) {
                                None => negado("la petición no trae una URL que se entienda"),
                                Some(d) if !permitido(&d, &caller.data().dominios) => negado(
                                    "ese dominio no está en los que el plugin declaró",
                                ),
                                Some(_) => {
                                    let id = caller.data().plugin.clone();
                                    let servicios = Arc::clone(&caller.data().servicios);
                                    servicios
                                        .lock()
                                        .map(|mut s| s.http(&id, &peticion))
                                        .unwrap_or_else(|_| negado("el host no pudo atender"))
                                }
                            }
                        };
                        entregar(&mut caller, Some(&respuesta), out, cupo)
                    },
                )
            })
            .map_err(|e| PluginError::NoCompila(e.to_string()))?;

        Ok(linker)
    }
}

/// El dominio al que apunta una petición, si se entiende.
fn dominio_de(peticion: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(peticion).ok()?;
    let url = v.get("url")?.as_str()?;
    host_de_url(url)
}

/// Saca la máquina de una URL sin arrastrar un parser de URLs entero.
pub fn host_de_url(url: &str) -> Option<String> {
    let resto = url.split_once("://")?.1;
    let fin = resto.find(['/', '?', '#']).unwrap_or(resto.len());
    let autoridad = &resto[..fin];
    // Se tira el usuario y el puerto: lo que se compara es la máquina.
    let sin_usuario = autoridad.rsplit_once('@').map(|(_, h)| h).unwrap_or(autoridad);
    let host = sin_usuario.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Cierto si el dominio está en la lista, exacto o como subdominio.
///
/// Se permite el subdominio porque un servicio reparte sus respuestas entre
/// varias máquinas del mismo dominio; lo que no se permite es que la lista
/// traiga un comodín, que es donde se pierde el control.
fn permitido(host: &str, dominios: &[String]) -> bool {
    dominios.iter().any(|d| {
        let d = d.to_ascii_lowercase();
        host == d || host.ends_with(&format!(".{d}"))
    })
}

fn memoria_de(caller: &mut Caller<'_, Estado>) -> Option<wasmtime::Memory> {
    caller.get_export(abi::MEMORIA).and_then(|e| e.into_memory())
}

fn bytes_de(caller: &mut Caller<'_, Estado>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let mem = memoria_de(caller)?;
    let datos = mem.data(&caller);
    let (desde, hasta) = (ptr as usize, (ptr as usize).checked_add(len as usize)?);
    datos.get(desde..hasta).map(<[u8]>::to_vec)
}

fn texto_de(caller: &mut Caller<'_, Estado>, ptr: i32, len: i32) -> Option<String> {
    let bytes = bytes_de(caller, ptr, len)?;
    String::from_utf8(bytes).ok()
}

/// Escribe la respuesta de una función del host en el búfer del guest.
fn entregar(caller: &mut Caller<'_, Estado>, datos: Option<&[u8]>, out: i32, cupo: i32) -> i32 {
    let cupo_u = cupo.max(0) as u32;
    let veredicto = abi::Cupo::para(datos, cupo_u);
    if let (abi::Cupo::Cabe(n), Some(d)) = (veredicto, datos) {
        let Some(mem) = memoria_de(caller) else { return abi::Cupo::Nada.codigo() };
        let destino = mem.data_mut(caller);
        let (desde, hasta) = (out.max(0) as usize, out.max(0) as usize + n as usize);
        match destino.get_mut(desde..hasta) {
            Some(hueco) => hueco.copy_from_slice(d),
            // Un búfer fuera de su memoria se trata como que no cupo: nunca
            // se escribe a medias ni fuera.
            None => return abi::Cupo::NoCabe(n).codigo(),
        }
    }
    veredicto.codigo()
}

fn escribir(
    memoria: &wasmtime::Memory,
    store: &mut Store<Estado>,
    ptr: u32,
    datos: &[u8],
) -> Result<(), PluginError> {
    let destino = memoria.data_mut(store);
    let (desde, hasta) = (ptr as usize, ptr as usize + datos.len());
    destino
        .get_mut(desde..hasta)
        .ok_or(PluginError::MemoriaFuera)?
        .copy_from_slice(datos);
    Ok(())
}

fn leer(
    memoria: &wasmtime::Memory,
    store: &mut Store<Estado>,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, PluginError> {
    let datos = memoria.data(store);
    let (desde, hasta) = (ptr as usize, (ptr as usize).saturating_add(len as usize));
    datos
        .get(desde..hasta)
        .map(<[u8]>::to_vec)
        .ok_or(PluginError::MemoriaFuera)
}

/// Lo que un plugin tiene que declarar para que se le den servicios.
pub fn capacidad_de_red(dominios: &[&str]) -> Capability {
    Capability::Network { domains: dominios.iter().map(|d| d.to_string()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_una_url_sale_la_maquina() {
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
    fn un_dominio_permitido_cubre_sus_subdominios_y_nada_mas() {
        let lista = vec!["ejemplo.com".to_string()];
        assert!(permitido("ejemplo.com", &lista));
        assert!(permitido("api.ejemplo.com", &lista));
        assert!(permitido("a.b.ejemplo.com", &lista));
        assert!(!permitido("ejemplo.com.malo.net", &lista), "el sufijo no basta");
        assert!(!permitido("noejemplo.com", &lista));
        assert!(!permitido("otro.net", &lista));
        assert!(!permitido("ejemplo.com", &[]), "sin lista no se permite nada");
    }

    #[test]
    fn de_una_peticion_sale_su_dominio() {
        assert_eq!(
            dominio_de(br#"{"url":"https://api.ejemplo.com/buscar","metodo":"GET"}"#),
            Some("api.ejemplo.com".into())
        );
        assert_eq!(dominio_de(b"{}"), None, "sin url no hay dominio");
        assert_eq!(dominio_de(b"no es json"), None);
    }
}
