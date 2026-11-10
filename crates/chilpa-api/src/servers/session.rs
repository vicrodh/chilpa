//! Jobs retain their account and connection revision until they finish.

use chilpa_library::StoredServer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    source: i64,
    provider: String,
    instance: String,
    account: String,
    identity: String,
    revision: i64,
}

impl Scope {
    pub(crate) fn of(server: &StoredServer) -> Self {
        Self { source:server.source_id, provider:server.provider.clone(), instance:server.instance.clone(),
            account:server.account.clone(), identity:server.identity.clone(), revision:server.session_revision }
    }
    pub(crate) fn matches(&self, server: &StoredServer) -> bool { self == &Self::of(server) }
}

pub(crate) fn changed() -> String {
    chilpa_i18n::t("The server connection or account changed. Review the action and try again.")
}

pub(crate) fn confirmation(server: &StoredServer, reference: &str) -> String {
    format!("{reference}:{}:{}", server.instance, server.session_revision)
}
