//! The host: it loads a plugin, calls it and stops it if it goes over.
//!
//! Everything a guest can do comes through here, and here is where what the
//! manifest promised is enforced. A plugin that asked for network to one
//! domain cannot reach another, and not because it is asked not to: because
//! the call is refused before it reaches the real host.
//!
//! **A plugin remembers nothing between calls.** Every call builds its own
//! fresh instance, with its own fuel and its own memory. Whatever it wants
//! to remember it sends to its store, which belongs to the host. That way a
//! memory leak does not pile up and one call cannot poison the next.

use crate::abi;
use crate::limits::{Halt, Limits};
use crate::manifest::{Capability, Manifest, ManifestError};
use crate::services::{HostServices, Level};
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// The host's services, shared between calls and plugins.
pub type Services = Arc<Mutex<dyn HostServices>>;

/// How often the interruption clock ticks.
const TIC: std::time::Duration = std::time::Duration::from_millis(50);

/// Why a plugin could not be loaded or called.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PluginError {
    /// The manifest did not pass.
    Manifest(ManifestError),
    /// The module is not valid wasm, or the engine did not accept it.
    WontCompile(String),
    /// It is missing something every plugin has to export.
    MissingExport(String),
    /// It claims a different API version than the manifest does.
    VersionMismatch { dice: i32, manifest: u32 },
    /// It went over one of its limits.
    Stopped(Halt),
    /// It blew up inside.
    Crashed(String),
    /// It gave an address that is not in its memory.
    OutOfMemory,
    /// It asked for something its manifest did not declare.
    NotAllowed(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Manifest(e) => write!(f, "{e}"),
            PluginError::WontCompile(e) => {
                write!(f, "{}", chilpa_i18n::t_args("the module could not be loaded: {}", &[e]))
            }
            PluginError::MissingExport(n) => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "it does not export \"{}\", which every plugin has to export",
                    &[n],
                )
            ),
            PluginError::VersionMismatch { dice, manifest } => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "the manifest says API {} and the module answers {}; it does not load",
                    &[&manifest.to_string(), &dice.to_string()],
                )
            ),
            PluginError::Stopped(d) => write!(f, "{d}"),
            PluginError::Crashed(e) => {
                write!(f, "{}", chilpa_i18n::t_args("it blew up inside: {}", &[e]))
            }
            PluginError::OutOfMemory => {
                write!(f, "{}", chilpa_i18n::t("it answered an address not in its memory"))
            }
            PluginError::NotAllowed(q) => write!(
                f,
                "{}",
                chilpa_i18n::t_args("it asked for {}, which its manifest did not declare", &[q])
            ),
        }
    }
}

impl From<ManifestError> for PluginError {
    fn from(e: ManifestError) -> Self {
        PluginError::Manifest(e)
    }
}

/// What lives in the store while a call runs.
struct State {
    plugin: String,
    domains: Vec<String>,
    can_store: bool,
    can_go_out: bool,
    services: Services,
    limits: StoreLimits,
}

/// The engine, shared by every plugin.
pub struct Host {
    engine: Engine,
    /// The thread that advances the interruption clock.
    vivo: Arc<std::sync::atomic::AtomicBool>,
    clock: Option<std::thread::JoinHandle<()>>,
}

impl Host {
    pub fn new() -> Result<Self, PluginError> {
        let mut config = Config::new();
        // Fuel for the one that computes too much, a clock for the one that
        // hangs. Both are needed.
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| PluginError::WontCompile(e.to_string()))?;

        let vivo = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag = Arc::clone(&vivo);
        let motor = engine.clone();
        let clock = std::thread::Builder::new()
            .name("chilpa-plugins-clock".into())
            .spawn(move || {
                while flag.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(TIC);
                    motor.increment_epoch();
                }
            })
            .ok();

        Ok(Self { engine, vivo, clock })
    }

    /// Compiles a plugin and checks it is what it says it is.
    pub fn load(&self, manifest: Manifest, wasm: &[u8]) -> Result<Plugin, PluginError> {
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| PluginError::WontCompile(e.to_string()))?;

        // It has to export the minimum, or it does not load. Asking here and
        // not mid call is the difference between "it did not load" and "it
        // broke while you were using it".
        for export in [abi::MEMORY, abi::F_VERSION, abi::F_ALLOC] {
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

/// A plugin already compiled and ready to be called.
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

    /// Asks the module which version it works with and compares it to the one
    /// the manifest stated. If they disagree, someone lied and it does not load.
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

    /// Calls one of the plugin's operations with its budget.
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
            .get_memory(&mut store, abi::MEMORY)
            .ok_or_else(|| PluginError::MissingExport(abi::MEMORY.to_string()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, abi::F_ALLOC)
            .map_err(|_| PluginError::MissingExport(abi::F_ALLOC.to_string()))?;
        let op = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, operation)
            .map_err(|_| PluginError::MissingExport(operation.to_string()))?;

        // A piece of its memory is asked for and the input is written there.
        let ptr = alloc
            .call(&mut store, input.len() as i32)
            .map_err(|e| self.translate(e, &mut store))?;
        write(&memoria, &mut store, ptr as u32, input)?;

        let packed = op
            .call(&mut store, (ptr, input.len() as i32))
            .map_err(|e| self.translate(e, &mut store))?;

        let Some((rptr, rlen)) = abi::unpack(packed) else {
            return Ok(abi::Reply::Error(chilpa_i18n::t("it answered nothing").into()));
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
                // One memory and one table: a plugin needs no more, and
                // each one is memory that can be asked for.
                .memories(1)
                .tables(1)
                .build(),
        };
        let mut store = Store::new(&host.engine, state);
        store.limiter(|e| &mut e.limits);
        store
            .set_fuel(limits.fuel)
            .map_err(|e| PluginError::WontCompile(e.to_string()))?;
        // The clock advances every TICK; the deadline is as many ticks as fit.
        let tics = (limits.timeout.as_millis() / TIC.as_millis()).max(1) as u64;
        store.set_epoch_deadline(tics);
        Ok(store)
    }

    /// Translates what wasmtime answers into something the user can read.
    ///
    /// The real reason comes in the chain of causes, not in the top message: the
    /// top one always says "error while executing".
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
        // Running out of fuel also shows in what is left.
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
        let m = abi::HOST_MODULE;

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
                        let value = services.lock().ok().and_then(|mut s| s.store_get(&id, &key));
                        deliver(&mut caller, value.as_deref(), out, room)
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
                        let Some(value) = bytes_at(&mut caller, vptr, vlen) else { return 0 };
                        let id = caller.data().plugin.clone();
                        let services = Arc::clone(&caller.data().services);
                        let ok = services
                            .lock()
                            .map(|mut s| s.store_set(&id, &key, &value))
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
                        let Some(request) = bytes_at(&mut caller, rptr, rlen) else {
                            return abi::Room::Nothing.code();
                        };
                        let denied = |reason: &str| -> Vec<u8> {
                            serde_json::to_vec(&serde_json::json!({ "error": reason }))
                                .unwrap_or_default()
                        };
                        let reply = if !caller.data().can_go_out {
                            denied(&chilpa_i18n::t("this plugin declared no network permission"))
                        } else {
                            match domain_of(&request) {
                                None => denied(&chilpa_i18n::t("the request carries no URL that can be understood")),
                                Some(d) if !allowed(&d, &caller.data().domains) => denied(
                                    &chilpa_i18n::t(
                                        "that domain is not among the ones the plugin declared",
                                    ),
                                ),
                                Some(_) => {
                                    let id = caller.data().plugin.clone();
                                    let services = Arc::clone(&caller.data().services);
                                    services
                                        .lock()
                                        .map(|mut s| s.http(&id, &request))
                                        .unwrap_or_else(|_| denied(&chilpa_i18n::t("the host could not serve it")))
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

/// The domain a request points at, if it can be understood.
fn domain_of(request: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(request).ok()?;
    let url = v.get("url")?.as_str()?;
    host_of_url(url)
}

/// Pulls the host out of a URL without dragging in a whole URL parser.
pub fn host_of_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let fin = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..fin];
    // The userinfo and the port are thrown away: what is compared is the host.
    let no_user = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = no_user.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// True if the domain is on the list, exactly or as a subdomain.
///
/// Subdomains are allowed because a service spreads its replies across
/// several hosts of the same domain; what is not allowed is a wildcard on
/// the list, which is where control is lost.
fn allowed(host: &str, domains: &[String]) -> bool {
    domains.iter().any(|d| {
        let d = d.to_ascii_lowercase();
        host == d || host.ends_with(&format!(".{d}"))
    })
}

fn memory_of(caller: &mut Caller<'_, State>) -> Option<wasmtime::Memory> {
    caller.get_export(abi::MEMORY).and_then(|e| e.into_memory())
}

fn bytes_at(caller: &mut Caller<'_, State>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let mem = memory_of(caller)?;
    let data = mem.data(&caller);
    let (from, hasta) = (ptr as usize, (ptr as usize).checked_add(len as usize)?);
    data.get(from..hasta).map(<[u8]>::to_vec)
}

fn text_of(caller: &mut Caller<'_, State>, ptr: i32, len: i32) -> Option<String> {
    let bytes = bytes_at(caller, ptr, len)?;
    String::from_utf8(bytes).ok()
}

/// Writes a host function's reply into the guest's buffer.
fn deliver(caller: &mut Caller<'_, State>, data: Option<&[u8]>, out: i32, room: i32) -> i32 {
    let room_u = room.max(0) as u32;
    let verdict = abi::Room::holding(data, room_u);
    if let (abi::Room::Fits(n), Some(d)) = (verdict, data) {
        let Some(mem) = memory_of(caller) else { return abi::Room::Nothing.code() };
        let target = mem.data_mut(caller);
        let (from, hasta) = (out.max(0) as usize, out.max(0) as usize + n as usize);
        match target.get_mut(from..hasta) {
            Some(gap) => gap.copy_from_slice(d),
            // A buffer outside its memory is treated as not fitting: there is
            // never a half write and never one outside.
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

/// What a plugin has to declare to be given services.
pub fn network_capability(domains: &[&str]) -> Capability {
    Capability::Network { domains: domains.iter().map(|d| d.to_string()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the sandbox offers the guest, and nothing more.
    ///
    /// This test is ADR-008's lock. If anyone adds an import to the linker,
    /// it fails until they write it into `abi::HOST_IMPORTS`, which is where
    /// it is stated that one returning a secret is **never** added.
    ///
    /// It does not check one list against another hand written list: it asks
    /// the linker what it has. A test that repeated the list would see nothing.
    #[test]
    fn the_sandbox_offers_exactly_what_is_declared_and_nothing_more() {
        let host = Host::new().unwrap();
        let manifest = crate::manifest::Manifest::parse(
            r#"{"id":"com.a.b","name":"One","version":"1","api_version":1,"kind":"source"}"#,
        )
        .unwrap();
        // A guest that imports nothing: enough to build the linker.
        let wasm = wat::parse_str(
            r#"(module (memory (export "memory") 1)
                 (func (export "chilpa_api_version") (result i32) (i32.const 1))
                 (func (export "chilpa_alloc") (param i32) (result i32) (i32.const 0)))"#,
        )
        .unwrap();
        let plugin = host.load(manifest, &wasm).unwrap();
        let services: Services = Arc::new(Mutex::new(crate::services::NoServices));
        let mut store = plugin.build_store(&host, &services, Limits::default()).unwrap();
        let linker = plugin.build_linker(&host).unwrap();

        let mut offered: Vec<String> = linker
            .iter(&mut store)
            .map(|(module, name, _)| format!("{module}.{name}"))
            .collect();
        offered.sort();

        let mut declared: Vec<String> = abi::HOST_IMPORTS
            .iter()
            .map(|n| format!("{}.{n}", abi::HOST_MODULE))
            .collect();
        declared.sort();

        assert_eq!(
            offered, declared,
            "the sandbox offers something not in abi::HOST_IMPORTS, or the other way round"
        );
    }

    #[test]
    fn nothing_the_sandbox_offers_hands_over_a_secret() {
        // This is not a behaviour test: it is a reminder with teeth. If
        // anyone names a new import "credential", this fails before it
        // reaches a review.
        for name in abi::HOST_IMPORTS {
            for forbidden in ["credential", "secret", "token", "password", "keyring"] {
                assert!(
                    !name.contains(forbidden),
                    "\"{name}\" sounds like handing a secret to the plugin (ADR-008)"
                );
            }
        }
    }

    #[test]
    fn a_url_yields_its_host() {
        assert_eq!(host_of_url("https://api.example.com/v1/search"), Some("api.example.com".into()));
        assert_eq!(host_of_url("http://API.Example.COM"), Some("api.example.com".into()));
        assert_eq!(host_of_url("https://api.example.com:8443/x"), Some("api.example.com".into()));
        assert_eq!(
            host_of_url("https://user:secret@api.example.com/x"),
            Some("api.example.com".into()),
            "the userinfo is not the host"
        );
        assert_eq!(host_of_url("api.example.com/x"), None, "with no scheme nothing is guessed");
        assert_eq!(host_of_url("https:///x"), None);
    }

    #[test]
    fn an_allowed_domain_covers_its_subdomains_and_nothing_else() {
        let list = vec!["example.com".to_string()];
        assert!(allowed("example.com", &list));
        assert!(allowed("api.example.com", &list));
        assert!(allowed("a.b.example.com", &list));
        assert!(!allowed("example.com.evil.net", &list), "a suffix is not enough");
        assert!(!allowed("noexample.com", &list));
        assert!(!allowed("other.net", &list));
        assert!(!allowed("example.com", &[]), "with no list nothing is allowed");
    }

    #[test]
    fn a_request_yields_its_domain() {
        assert_eq!(
            domain_of(br#"{"url":"https://api.example.com/buscar","metodo":"GET"}"#),
            Some("api.example.com".into())
        );
        assert_eq!(domain_of(b"{}"), None, "with no url there is no domain");
        assert_eq!(domain_of(b"no es json"), None);
    }
}
