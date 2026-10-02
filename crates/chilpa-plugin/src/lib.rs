//! Chilpa's plugin host.
//!
//! A plugin runs in a WebAssembly sandbox, inside the process, under limits
//! the engine enforces and nobody's good will (ADR-007). What it can do is
//! lent to it by the host, function by function; what is not on that list
//! does not exist in its world.
//!
//! The two rules of ADR-003, one line each:
//!
//! 1. **A native plugin never sees a token.** The host adds its secret to
//!    requests. A separately approved compatibility worker can handle its
//!    own credentials through installation-scoped encrypted storage.
//! 2. **A broken plugin does not break Chilpa.** If it goes over fuel,
//!    memory or the clock, that call is stopped. If it blows up three times
//!    in a row it goes to quarantine, and only the user takes it out.
//!
//! This crate knows neither the core, nor the library, nor the network. What
//! it can lend arrives as a `HostServices` implemented by whoever uses it.
//! That way the sandbox drags in no network stack, and the tests can look at
//! exactly what a plugin tried to do.

pub mod abi;
pub mod bridge;
pub mod bridge_plugin;
pub mod catalog;
pub mod extension;
pub mod presentation;
pub mod compatibility;
pub mod host;
pub mod limits;
pub mod manifest;
pub mod messages;
pub mod pipe_transport;
pub mod registry;
pub mod runtime;
pub mod services;
pub mod settings;
pub mod watchdog;
pub mod worker_client;

pub use abi::Reply;
pub use host::{Host, Plugin, PluginError, Services};
pub use limits::{Halt, Limits};
pub use manifest::{Capability, Kind, Manifest, ManifestError, Runtime, API_VERSION};
pub use messages::{
    op, RemoteAlbum, Quality, Query, RemotePlaylist, Results, Stream, RemoteTrack,
};
pub use registry::{Installed, Registry};
pub use runtime::RuntimePlugin;
pub use services::{Recorded, TestHost, HostServices, Level, AuthMap};
pub use watchdog::{State, Watchdog};
