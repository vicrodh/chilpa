//! The chosen services in Settings: what their rows say, what typing into
//! one does and what their buttons do. The keys are this module's own; the
//! window's bridge hands them over without knowing them (frontend ADR-0015).
//!
//! A service that is not connected shows what it needs to be: an address, a
//! key, a button. Connected, it shows whose account it is with the way out,
//! and its switch.

use super::{Chosen, ScrobbleStatus};

/// One row of a chosen service, from its key: `integrations.<id>.<part>`.
fn parse(key: &str) -> Option<(Chosen, &str)> {
    let (id, part) = key.strip_prefix("integrations.")?.split_once('.')?;
    let which = Chosen::ALL.into_iter().find(|which| which.id() == id)?;
    matches!(part, "url" | "key" | "account" | "enabled").then_some((which, part))
}

impl crate::Chilpa {
    /// Where a chosen service stands. One nobody connected has no thread,
    /// and stands where everything starts: no account, nothing waiting.
    pub(crate) fn chosen_scrobbler(&self, which: Chosen) -> ScrobbleStatus {
        self.scrobbler.as_ref().and_then(|s| s.running(which)).map(|handle| handle.status()).unwrap_or_default()
    }

    /// What was typed for a chosen service: where it is and how Chilpa is
    /// known to it are kept as written; its account's row starts connecting
    /// with all of it.
    pub(crate) fn scrobble_setting(&mut self, key: &str, value: &str) -> bool {
        let Some((which, part)) = parse(key) else { return false };
        match part {
            "url" | "key" => self.remember_preference(key, value.trim()),
            "enabled" => {
                if let Some(handle) = self.scrobbler.as_ref().and_then(|s| s.running(which)) {
                    handle.enable(value == "true");
                }
            }
            _ => {
                let typed = |part: &str| self.preference(&format!("integrations.{}.{part}", which.id())).unwrap_or_default();
                let with = match which {
                    Chosen::Koito => super::server::pack(&[&typed("url"), value.trim()]),
                    Chosen::Gnufm => super::server::pack(&[&typed("url"), &typed("key"), value.trim()]),
                    Chosen::Librefm => String::new(),
                };
                self.connect_chosen(which, with);
            }
        }
        true
    }

    fn connect_chosen(&mut self, which: Chosen, with: String) {
        if self.offline() {
            return;
        }
        self.start_scrobbling();
        if let Some(scrobbler) = self.scrobbler.as_mut() {
            scrobbler.chosen(which).connect(with);
        }
    }

    /// An account's button: the way out once connected, and for a service
    /// with nothing to type, the way in.
    pub(crate) fn scrobble_maintenance(&mut self, key: &str) -> Option<String> {
        let (which, "account") = parse(key)? else { return None };
        let status = self.chosen_scrobbler(which);
        if !status.user.is_empty() {
            if let Some(handle) = self.scrobbler.as_ref().and_then(|s| s.running(which)) {
                handle.disconnect();
            }
        } else if which == Chosen::Librefm && !status.checking {
            self.connect_chosen(which, String::new());
        }
        Some(String::new())
    }

    /// An account being connected or forgotten: its rows are read again
    /// until it is done.
    pub(crate) fn scrobble_working(&self) -> bool {
        Chosen::ALL.into_iter().any(|which| self.chosen_scrobbler(which).checking)
    }

    pub(crate) fn scrobble_row(&mut self, row: &mut crate::SettingsRow) -> bool {
        let Some((which, part)) = parse(&row.key) else { return false };
        let status = self.chosen_scrobbler(which);
        let connected = !status.user.is_empty();
        match part {
            // Asked for only while there is no account to show instead.
            "url" | "key" if connected || status.checking => row.control.clear(),
            "url" | "key" => row.value = self.preference(&row.key).unwrap_or_default(),
            "enabled" if connected => row.value = status.enabled.to_string(),
            "enabled" => row.control.clear(),
            _ if status.checking && !status.link.is_empty() => {
                // Waiting for the person to allow it on the server's page:
                // it opens, and can be opened again from here.
                row.description = chilpa_i18n::t("Allow Chilpa on the page that opened; this updates by itself.");
                row.control = "external".into();
                row.action = chilpa_i18n::t("Open the page");
                row.value = status.link.clone();
            }
            _ => crate::settings_rows::account(row, &status),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SettingsRow;

    fn row(api: &mut crate::Chilpa, key: &str, control: &str) -> SettingsRow {
        let mut row = SettingsRow { kind: 1, key: key.into(), control: control.into(), ..SettingsRow::default() };
        assert!(api.scrobble_row(&mut row), "{key} is this module's");
        row
    }

    #[test]
    fn only_the_chosen_services_rows_are_taken() {
        assert_eq!(parse("integrations.koito.url"), Some((Chosen::Koito, "url")));
        assert_eq!(parse("integrations.gnufm.account"), Some((Chosen::Gnufm, "account")));
        assert_eq!(parse("integrations.librefm.enabled"), Some((Chosen::Librefm, "enabled")));
        assert_eq!(parse("integrations.lastfm.account"), None, "the window's bridge still has it");
        assert_eq!(parse("integrations.koito.other"), None);
        let mut api = crate::Chilpa::new();
        assert!(!api.scrobble_setting("integrations.discord.enabled", "true"));
        assert_eq!(api.scrobble_maintenance("integrations.koito.url"), None, "a field is no button");
    }

    #[test]
    fn a_service_with_no_account_asks_for_one_and_hides_its_switch() {
        let mut api = crate::Chilpa::new();
        assert_eq!(row(&mut api, "integrations.koito.url", "text").control, "text");
        assert_eq!(row(&mut api, "integrations.koito.account", "secret").control, "secret");
        assert_eq!(row(&mut api, "integrations.koito.enabled", "toggle").control, "", "left out until there is an account");
        assert_eq!(row(&mut api, "integrations.librefm.account", "button").control, "button");
        assert_eq!(row(&mut api, "integrations.gnufm.key", "text").control, "text");
        assert!(!api.scrobble_working());
    }
}
