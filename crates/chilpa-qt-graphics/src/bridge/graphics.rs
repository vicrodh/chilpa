//! What draws the window, as QML reads it and reports it.
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
        /// The api Qt resolved, by name: "opengl", "vulkan", "software"...
        #[qproperty(QString, api)]
        /// Whether what draws can run shaders.
        #[qproperty(bool, gpu_tier)]
        /// The interface size's factor, for the window's smallest size.
        #[qproperty(f64, scale)]
        /// How long the window watches its frames before calling the start
        /// alive, in milliseconds.
        #[qproperty(i32, liveness_window)]
        type Graphics = super::GraphicsRust;

        /// The api `GraphicsInfo` gives the window, by name. "unknown" is
        /// what it says before the scene graph resolves and is ignored.
        #[qinvokable]
        fn report_renderer_api(self: Pin<&mut Graphics>, api: &QString);

        /// How many frames the window presented in the liveness window:
        /// frames, not a window, disarm the startup sentinels.
        #[qinvokable]
        fn report_frame_liveness(self: &Graphics, frames: i32);

        /// The window presented its first frame with what it restored.
        #[qinvokable]
        fn startup_shown(self: &Graphics);

        /// What the startup found and the user should hear, once: one line
        /// each, or empty.
        #[qinvokable]
        fn take_notices(self: &Graphics) -> QString;
    }

    impl cxx_qt::Initialize for Graphics {}
}

use chilpa_api::{graphics_boot, startup_chain};
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct GraphicsRust {
    api: QString,
    gpu_tier: bool,
    scale: f64,
    liveness_window: i32,
}

impl Default for GraphicsRust {
    fn default() -> Self {
        Self {
            api: QString::from(&graphics_boot::active_api()),
            gpu_tier: graphics_boot::gpu_tier(),
            scale: f64::from(graphics_boot::interface_scale()),
            liveness_window: graphics_boot::LIVENESS_WINDOW_MS,
        }
    }
}

impl cxx_qt::Initialize for qobject::Graphics {
    fn initialize(self: Pin<&mut Self>) {}
}

impl qobject::Graphics {
    pub fn report_renderer_api(mut self: Pin<&mut Self>, api: &QString) {
        if !graphics_boot::set_active_api(&api.to_string()) {
            return;
        }
        self.as_mut().set_api(QString::from(&graphics_boot::active_api()));
        self.as_mut().set_gpu_tier(graphics_boot::gpu_tier());
    }

    pub fn report_frame_liveness(&self, frames: i32) {
        graphics_boot::disarm_on_liveness(frames);
    }

    pub fn startup_shown(&self) {
        static SHOWN: AtomicBool = AtomicBool::new(false);
        if !SHOWN.swap(true, Ordering::SeqCst) {
            startup_chain::mark_healthy();
        }
    }

    pub fn take_notices(&self) -> QString {
        QString::from(&graphics_boot::take_notices().join("\n"))
    }
}
