//! El host de plugins de Chilpa.
//!
//! Un plugin corre en un sandbox de WebAssembly, adentro del proceso, con
//! límites que aplica el motor y no la buena voluntad de nadie (ADR-007).
//! Lo que puede hacer se lo presta el host, función por función; lo que no
//! está en esa lista, no existe en su mundo.
//!
//! Las dos reglas del ADR-003, aquí en una línea cada una:
//!
//! 1. **Un plugin nunca ve un token.** No guarda credenciales porque no las
//!    recibe: la petición la hace el host y el secreto se agrega de este
//!    lado.
//! 2. **Un plugin roto no rompe Chilpa.** Si se pasa de combustible, de
//!    memoria o de reloj, se detiene esa llamada. Si truena tres veces
//!    seguidas, se va a cuarentena y de ahí solo lo saca el usuario.
//!
//! Este crate no conoce al core, ni a la biblioteca, ni a la red. Recibe lo
//! que puede prestar como un `HostServices` que implementa quien lo usa. Así
//! el sandbox no arrastra una pila de red, y las pruebas pueden mirar
//! exactamente qué intentó un plugin.

pub mod abi;
pub mod host;
pub mod limits;
pub mod manifest;
pub mod registry;
pub mod services;
pub mod watchdog;

pub use abi::Respuesta;
pub use host::{Host, Plugin, PluginError, Servicios};
pub use limits::{Detencion, Limits};
pub use manifest::{Capability, Kind, Manifest, ManifestError, Runtime, API_VERSION};
pub use registry::{Instalado, Registry};
pub use services::{Anotado, HostDePrueba, HostServices, Nivel};
pub use watchdog::{Estado, Watchdog};
