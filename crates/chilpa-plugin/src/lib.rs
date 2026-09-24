//! El host de plugins de Chilpa.
//!
//! A plugin runs in a WebAssembly sandbox, inside the process, under limits
//! the engine enforces and nobody's good will (ADR-007). What it can do is
//! lent to it by the host, function by function; what is not on that list
//! does not exist in its world.
//!
//! The two rules of ADR-003, one line each:
//!
//! 1. **A plugin never sees a token.** It keeps no credentials because it
//!    never receives them: the host makes the request and the secret is
//!    added on this side.
//! 2. **A broken plugin does not break Chilpa.** If it goes over fuel,
//!    memory or the clock, that call is stopped. If it blows up three times
//!    in a row it goes to quarantine, and only the user takes it out.
//!
//! This crate knows neither the core, nor the library, nor the network. What
//! it can lend arrives as a `HostServices` implemented by whoever uses it.
//! That way the sandbox drags in no network stack, and the tests can look at
//! exactly what a plugin tried to do.

pub mod abi;
pub mod host;
pub mod limits;
pub mod manifest;
pub mod messages;
pub mod registry;
pub mod services;
pub mod watchdog;

pub use abi::Reply;
pub use host::{Host, Plugin, PluginError, Services};
pub use limits::{Halt, Limits};
pub use manifest::{Capability, Kind, Manifest, ManifestError, Runtime, API_VERSION};
pub use messages::{
    op, RemoteAlbum, Quality, Query, RemotePlaylist, Results, Stream, RemoteTrack,
};
pub use registry::{Installed, Registry};
pub use services::{Recorded, TestHost, HostServices, Level, AuthMap};
pub use watchdog::{State, Watchdog};
