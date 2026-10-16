//! How the player sits in the window: "floating", "bottom" or "top". The
//! facade keeps it; the window lays itself out by it.
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
        #[qproperty(QString, mode, READ = mode, WRITE = choose, NOTIFY = mode_changed)]
        type PlayerLayout = super::LayoutRust;

        #[cxx_name = "mode"]
        fn mode(self: &PlayerLayout) -> QString;
        #[cxx_name = "setMode"]
        fn choose(self: Pin<&mut PlayerLayout>, mode: QString);
        #[qsignal]
        #[cxx_name = "modeChanged"]
        fn mode_changed(self: Pin<&mut PlayerLayout>);
    }
    impl cxx_qt::Initialize for PlayerLayout {}
}
use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct LayoutRust {
    mode: QString,
}

impl qobject::PlayerLayout {
    pub fn mode(&self) -> QString {
        self.rust().mode.clone()
    }

    pub fn choose(mut self: Pin<&mut Self>, mode: QString) {
        let kept = QString::from(app::with(|api| api.remember_player_layout(&mode.to_string())));
        if kept == self.rust().mode {
            return;
        }
        self.as_mut().rust_mut().mode = kept;
        self.as_mut().mode_changed();
    }
}

impl cxx_qt::Initialize for qobject::PlayerLayout {
    fn initialize(mut self: Pin<&mut Self>) {
        let mode = QString::from(app::with(|api| api.player_layout()));
        self.as_mut().rust_mut().mode = mode;
    }
}
