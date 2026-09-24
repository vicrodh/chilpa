//! The plugins: loading them, approving them and connecting their account.
//!
//! Nobody runs without the user approving what it asks for (ADR-008), and
//! connecting an account opens the browser and waits on another thread.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;
use super::open_in_browser;

impl Chilpa {

    pub fn load_plugins(mut self: Pin<&mut Self>) {
        let count = self.as_mut().rust_mut().api.load_plugins();
        self.as_mut().refresh_plugins();
        let total = self.as_mut().rust_mut().api.plugins().len();
        if total > 0 {
            let loaded = self.api.t_args(
                "{} of {} plugins loaded",
                &[&count.to_string(), &total.to_string()],
            );
            self.as_mut().set_library_status(QString::from(&loaded));
        }
    }

    pub fn reactivate_plugin(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        self.as_mut().rust_mut().api.reactivate_plugin(&name);
        self.as_mut().refresh_plugins();
    }

    pub fn approve_plugin(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        let count = self.as_mut().rust_mut().api.approve_plugin(&name);
        self.as_mut().refresh_plugins();
        let running = self.api.tf(
            "{} plugin running",
            "{} plugins running",
            i64::try_from(count).unwrap_or_default(),
            &[&count.to_string()],
        );
        self.as_mut().set_library_status(QString::from(&running));
    }

    pub fn connect_account(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        match self.as_mut().rust_mut().api.connect_account(&name) {
            Ok(url) => {
                // The system browser is opened. If there is none, it is said
                // and the URL is left to the view: copying it by hand is ugly
                // but it works, and keeping quiet does not.
                match open_in_browser(&url) {
                    Ok(()) => {
                        let waiting =
                            self.api.t("Your browser is open to grant permission. Waiting here.");
                        self.as_mut().set_library_status(QString::from(&waiting));
                    }
                    Err(e) => {
                        self.as_mut().rust_mut().api.cancel_connect();
                        let failed =
                            self.api.t_args("could not open the browser: {}", &[&e.to_string()]);
                        self.as_mut().set_error(QString::from(&failed));
                    }
                }
            }
            Err(e) => self.as_mut().set_error(QString::from(&e)),
        }
        self.as_mut().refresh_plugins();
    }

    pub fn cancel_connect(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.cancel_connect();
        self.as_mut().set_library_status(QString::from(""));
    }

    pub fn disconnect_account(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        self.as_mut().rust_mut().api.disconnect_plugin(&name);
        self.as_mut().refresh_plugins();
    }

    pub fn revoke_plugin(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        let count = self.as_mut().rust_mut().api.revoke_plugin(&name);
        self.as_mut().refresh_plugins();
        let running = self.api.tf(
            "{} plugin running",
            "{} plugins running",
            i64::try_from(count).unwrap_or_default(),
            &[&count.to_string()],
        );
        self.as_mut().set_library_status(QString::from(&running));
    }

    pub(super) fn refresh_plugins(mut self: Pin<&mut Self>) {
        let list = self.as_mut().rust_mut().api.plugins();
        let mut l = QList::<QString>::default();
        for p in &list {
            l.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                p.id,
                p.name,
                p.state,
                p.problem,
                // What is left to approve, which is what has to be read before
                // saying yes. With " - " because a line carries no tabs.
                p.pending.join(" · "),
                if p.wants_more { "more" } else { "" },
                // How its account stands: no account, connected, or expired
                // with no way to renew itself.
                if p.account_expired {
                    "expired"
                } else if p.connected {
                    "connected"
                } else if p.account_host.is_empty() {
                    ""
                } else {
                    "missing"
                },
                p.account_host
            )));
        }
        self.as_mut().set_plugins(QStringList::from(&l));
    }
}
