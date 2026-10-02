//! The registry dispatches protocol messages without knowing a guest's ABI.

use crate::{Host, Limits, Manifest, Plugin, PluginError, Reply, Services};

/// Implemented by host-owned adapters, never by repository metadata.
///
/// An adapter translates the foreign ABI into the shared message protocol.
/// It must enforce the approved capabilities and limits before exposing a
/// guest. Stateful runtimes receive exclusive access for each call.
pub trait RuntimePlugin: Send {
    fn manifest(&self) -> &Manifest;

    fn call(
        &mut self,
        host: &Host,
        services: &Services,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Reply, PluginError>;

    /// False once the runtime behind it can no longer answer, as a worker
    /// whose session was revoked. A sandbox that is rebuilt per call is
    /// always alive.
    fn alive(&self) -> bool {
        true
    }
}

impl RuntimePlugin for Plugin {
    fn manifest(&self) -> &Manifest {
        Plugin::manifest(self)
    }

    fn call(
        &mut self,
        host: &Host,
        services: &Services,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Reply, PluginError> {
        Plugin::call(self, host, services, operation, input, limits)
    }
}
