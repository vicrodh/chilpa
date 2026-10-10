//! The link modal's state, as the facade resolves the link off the window's
//! thread and the bridge asks how it goes until it is done.
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
        /// The modal is shown.
        #[qproperty(bool, open)]
        /// The link is being resolved.
        #[qproperty(bool, resolving)]
        /// What went wrong, said to the user; empty when nothing did.
        #[qproperty(QString, error)]
        /// The link is a playlist of another service: the importer can take it.
        #[qproperty(bool, playlist)]
        type Link = super::LinkRust;

        /// Shows the modal, empty.
        #[qinvokable]
        fn show(self: Pin<&mut Link>);
        /// Hides the modal and forgets the link.
        #[qinvokable]
        fn close(self: Pin<&mut Link>);
        /// Resolves a link.
        #[qinvokable]
        fn submit(self: Pin<&mut Link>, url: &QString);
        /// Hands the playlist to the importer, which starts reading it.
        #[qinvokable]
        fn open_importer(self: Pin<&mut Link>) -> bool;

        /// The link resolved to a screen: by id, or by key when the key is
        /// not empty.
        #[qsignal]
        fn opened(self: Pin<&mut Link>, screen: QString, id: i64, key: QString);
    }

    impl cxx_qt::Threading for Link {}
}

use chilpa_api::Leads;
use chilpa_qt_kit::app;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

/// The submit the poller belongs to: a newer submit or a close ends it.
static ROUND: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct LinkRust {
    open: bool,
    resolving: bool,
    error: QString,
    playlist: bool,
}

impl qobject::Link {
    pub fn show(mut self: Pin<&mut Self>) {
        ROUND.fetch_add(1, Ordering::SeqCst);
        app::with(|api| api.close_link());
        self.as_mut().set_resolving(false);
        self.as_mut().set_error(QString::default());
        self.as_mut().set_playlist(false);
        self.as_mut().set_open(true);
    }

    pub fn close(mut self: Pin<&mut Self>) {
        ROUND.fetch_add(1, Ordering::SeqCst);
        app::with(|api| api.close_link());
        self.as_mut().set_resolving(false);
        self.as_mut().set_open(false);
    }

    pub fn submit(mut self: Pin<&mut Self>, url: &QString) {
        let round = ROUND.fetch_add(1, Ordering::SeqCst) + 1;
        app::with(|api| api.open_link(&url.to_string()));
        self.as_mut().set_error(QString::default());
        self.as_mut().set_playlist(false);
        self.as_mut().set_resolving(true);
        self.as_mut().read();
        let thread = self.qt_thread();
        let _ = std::thread::Builder::new().name("chilpa-link-watch".into()).spawn(move || {
            while ROUND.load(Ordering::SeqCst) == round {
                std::thread::sleep(std::time::Duration::from_millis(120));
                if thread.queue(|link| link.read()).is_err() {
                    return;
                }
            }
        });
    }

    pub fn open_importer(mut self: Pin<&mut Self>) -> bool {
        let handed = app::with(|api| api.open_link_in_importer());
        if handed {
            self.as_mut().close();
        }
        handed
    }

    /// Reads how the link goes and, when it is done, where it leads.
    fn read(mut self: Pin<&mut Self>) {
        if !*self.open() {
            return;
        }
        let (state, lead) = app::with(|api| (api.open_link_state(), api.take_open_link()));
        let state: serde_json::Value = serde_json::from_str(&state).unwrap_or_default();
        let resolving = state["resolving"].as_bool().unwrap_or(false);
        self.as_mut().set_resolving(resolving);
        self.as_mut().set_error(QString::from(state["error"].as_str().unwrap_or_default()));
        self.as_mut().set_playlist(state["playlist"].as_bool().unwrap_or(false));
        if !resolving {
            ROUND.fetch_add(1, Ordering::SeqCst);
        }
        let place = match lead {
            Some(Leads::Screen(screen, id) | Leads::Tab(screen, id)) => Some((screen.to_string(), id, String::new())),
            Some(Leads::Source(key)) => Some(("source".to_string(), 0, key)),
            _ => None,
        };
        if let Some((screen, id, key)) = place {
            self.as_mut().close();
            self.as_mut().opened(QString::from(&screen), id, QString::from(&key));
        }
    }
}
