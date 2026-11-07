//! Which offered and installed plugins need the process runtime, and why
//! this system cannot give it to them. The plugin center lists them anyway,
//! with the reason, and never offers them to install or switch on.

use crate::plugins::Plugins;
use chilpa_plugin::{catalog::CatalogEntry, Runtime};
use serde_json::{json, Value};
use std::collections::HashMap;

/// What the plugin center knows about process plugins on this system.
pub(crate) struct ProcessPlugins {
    /// Why none can start here, in the user's language; `None` where they can.
    unavailable: Option<String>,
    /// Each installed plugin's runtime.
    runtimes: HashMap<String, Runtime>,
}

impl ProcessPlugins {
    pub(crate) fn of(plugins: &Plugins) -> Self {
        Self { unavailable: plugins.natives.availability().err(), runtimes: plugins.runtimes() }
    }

    /// The plugin center's `process_plugins`.
    pub(crate) fn state(&self) -> Value {
        json!({"available": self.unavailable.is_none(), "reason": self.unavailable.clone().unwrap_or_default()})
    }

    /// Why an offer cannot be installed here, or empty. A build for this
    /// platform is a process plugin's; an update says its runtime through
    /// the version already installed.
    pub(crate) fn offer(&self, entry: &CatalogEntry) -> String {
        let process = entry.platform_build || self.runtimes.get(&entry.id) == Some(&Runtime::Process);
        if process { self.unavailable.clone().unwrap_or_default() } else { String::new() }
    }

    /// Why an installed plugin cannot run here, or empty.
    pub(crate) fn installed(&self, id: &str) -> String {
        if self.runtimes.get(id) == Some(&Runtime::Process) { self.unavailable.clone().unwrap_or_default() } else { String::new() }
    }
}
