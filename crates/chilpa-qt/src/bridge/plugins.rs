//! Los plugins: cargarlos, aprobarlos y conectar su cuenta.
//!
//! Nadie corre sin que el usuario apruebe lo que pide (ADR-008), y conectar
//! una cuenta abre el navegador y espera en otro hilo.

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
            self.as_mut().set_library_status(QString::from(&format!(
                "{count} de {total} plugins cargados"
            )));
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
        self.as_mut()
            .set_library_status(QString::from(&format!("{count} plugins corriendo")));
    }

    pub fn connect_account(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        match self.as_mut().rust_mut().api.connect_account(&name) {
            Ok(url) => {
                // Se abre el navegador del sistema. Si no hay ninguno, se
                // dice y se deja la URL a la vista: copiarla a mano es feo
                // pero funciona, y quedarse callado no.
                match open_in_browser(&url) {
                    Ok(()) => self.as_mut().set_library_status(QString::from(
                        "Te abrí el navegador para dar permiso. Aquí te espero.",
                    )),
                    Err(e) => {
                        self.as_mut().rust_mut().api.cancel_connect();
                        self.as_mut()
                            .set_error(QString::from(&format!("no se pudo abrir el navegador: {e}")));
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
        self.as_mut()
            .set_library_status(QString::from(&format!("{count} plugins corriendo")));
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
                // Lo que falta aprobar, que es lo que hay que leer antes de
                // decir que sí. Con " · " porque un renglón no lleva tabs.
                p.pending.join(" · "),
                if p.wants_more { "mas" } else { "" },
                // Cómo está su cuenta: sin cuenta, conectada, o vencida sin
                // cómo renovarse.
                if p.account_expired {
                    "vencida"
                } else if p.connected {
                    "conectada"
                } else if p.account_host.is_empty() {
                    ""
                } else {
                    "falta"
                },
                p.account_host
            )));
        }
        self.as_mut().set_plugins(QStringList::from(&l));
    }
}
