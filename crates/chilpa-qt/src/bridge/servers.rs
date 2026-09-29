//! Adding and removing my servers (ADR-017). Every step is the facade's;
//! this carries it to the dialog as JSON and back.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Where adding a server stands, as `Chilpa::server_setup_json`.
        #[qproperty(QString, setup)]
        type Servers = super::ServersRust;
        #[qinvokable]
        fn begin(self: Pin<&mut Servers>, kind: &QString);
        #[qinvokable]
        fn address(self: Pin<&mut Servers>, text: &QString);
        #[qinvokable]
        fn sign_in(self: Pin<&mut Servers>, user: &QString, secret: &QString, as_key: bool);
        #[qinvokable]
        fn quick_connect(self: Pin<&mut Servers>);
        #[qinvokable]
        fn pick(self: Pin<&mut Servers>, id: &QString);
        /// The chosen libraries' ids, as JSON.
        #[qinvokable]
        fn finish(self: Pin<&mut Servers>, chosen: &QString) -> bool;
        #[qinvokable]
        fn cancel(self: Pin<&mut Servers>);
        /// Reads the setup again; the dialog calls it while it is open.
        #[qinvokable]
        fn refresh(self: Pin<&mut Servers>);
        /// What removing it takes, as the confirmation says it.
        #[qinvokable]
        fn removal(self: &Servers, id: i64) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut Servers>, id: i64) -> bool;
    }
}

use crate::{app, registry};
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct ServersRust {
    setup: QString,
}

impl qobject::Servers {
    pub fn begin(self: Pin<&mut Self>, kind: &QString) {
        app::with(|api| api.setup_server(&kind.to_string()));
        self.refresh();
    }
    pub fn address(self: Pin<&mut Self>, text: &QString) {
        app::with(|api| api.setup_address(&text.to_string()));
        self.refresh();
    }
    pub fn sign_in(self: Pin<&mut Self>, user: &QString, secret: &QString, as_key: bool) {
        app::with(|api| api.setup_sign_in(&user.to_string(), &secret.to_string(), as_key));
        self.refresh();
    }
    pub fn quick_connect(self: Pin<&mut Self>) {
        app::with(|api| api.setup_quick_connect());
        self.refresh();
    }
    pub fn pick(self: Pin<&mut Self>, id: &QString) {
        app::with(|api| api.setup_pick_server(&id.to_string()));
        self.refresh();
    }
    pub fn finish(self: Pin<&mut Self>, chosen: &QString) -> bool {
        let chosen: Vec<String> = serde_json::from_str(&chosen.to_string()).unwrap_or_default();
        let added = app::with(|api| api.setup_finish(&chosen));
        self.refresh();
        added
    }
    pub fn cancel(self: Pin<&mut Self>) {
        app::with(|api| api.setup_cancel());
        self.refresh();
    }
    pub fn refresh(self: Pin<&mut Self>) {
        let json = app::with(|api| {
            api.poll();
            api.server_setup_json()
        });
        self.set_setup(QString::from(&json));
    }
    pub fn removal(&self, id: i64) -> QString {
        QString::from(&app::with(|api| api.server_removal(id)))
    }
    pub fn remove(self: Pin<&mut Self>, id: i64) -> bool {
        let removed = app::with(|api| api.remove_server(id));
        registry::library(|l| l.content_changed());
        removed
    }
}
