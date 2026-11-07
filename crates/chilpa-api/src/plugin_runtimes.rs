//! Which offered and installed plugins need the process runtime, and why
//! this system cannot give it to them. The plugin center lists them anyway,
//! with the reason, and never offers them to install or switch on.

use crate::plugins::Plugins;
use chilpa_plugin::{catalog::CatalogEntry, Runtime};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

type Worker = Option<std::sync::Arc<dyn chilpa_plugin::bridge::WorkerLauncher>>;

/// The compatibility worker, or why there is none in the user's language.
#[cfg(target_os = "linux")]
pub(crate) fn compatibility(root: &std::path::Path) -> (Worker, Option<String>) {
    use chilpa_bwrap::Unavailable;
    match chilpa_bwrap::compatibility(root) {
        Ok(worker) => (Some(worker), None),
        Err(reason) => (None, Some(match reason {
            Unavailable::Portal => chilpa_i18n::t("Compatible plugins need the Flatpak portal, which is not available."),
            Unavailable::MissingProgram(program) => chilpa_i18n::t_args(
                "Compatible plugins need {}, which is not installed.", &[&program.display().to_string()]),
            Unavailable::Privileged => chilpa_i18n::t("Compatible plugins do not run while Chilpa runs as root."),
        })),
    }
}

/// Other systems have no compatibility worker yet, and nothing to say about it.
#[cfg(not(target_os = "linux"))]
pub(crate) fn compatibility(_: &std::path::Path) -> (Worker, Option<String>) {
    (None, None)
}

/// Why process plugins cannot start here, in the user's language, asked
/// without holding the host's lock: in Flatpak the first answer opens the
/// portal's connection.
pub(crate) fn unavailable(plugins: &Mutex<Plugins>) -> Result<Option<String>, &'static str> {
    let natives = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.natives.clone();
    Ok(natives.availability().err())
}

/// What the plugin center knows about process plugins on this system.
pub(crate) struct ProcessPlugins {
    /// Why none can start here, in the user's language; `None` where they can.
    unavailable: Option<String>,
    /// Each installed plugin's runtime.
    runtimes: HashMap<String, Runtime>,
}

impl ProcessPlugins {
    /// `unavailable` comes from [`unavailable`], asked before `plugins` was locked.
    pub(crate) fn of(unavailable: Option<String>, plugins: &Plugins) -> Self {
        Self { unavailable, runtimes: plugins.runtimes() }
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
