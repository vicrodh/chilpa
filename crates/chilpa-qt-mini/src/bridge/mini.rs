//! The mini player as QML reads it: whether it is up, which surface it
//! shows, whether its card carries the blurred cover, and what a key in
//! its own window comes to. The rules are the facade's (`chilpa_api::mini`
//! and its shortcuts); this reads them and keeps what the listener changes.
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
        /// Whether the mini player is up. The host shows and hides the two
        /// windows on its edges.
        #[qproperty(bool, open, READ = open, WRITE = turn, NOTIFY = open_changed)]
        /// 0 micro, 1 compact, 2 artwork, 3 queue, 4 lyrics. Writing it
        /// keeps it for the next time.
        #[qproperty(i32, surface, READ = surface, WRITE = choose_surface, NOTIFY = surface_changed)]
        /// Whether the card carries its own blurred cover behind it.
        #[qproperty(bool, background_blur)]
        /// Whether play has a queue with tracks to start on.
        #[qproperty(bool, queue_has_target)]
        /// The surface a development hook (`CHILPA_MINI`) asks the mini
        /// player to open on at start; -1 for none.
        #[qproperty(i32, dev_surface)]
        type Mini = super::MiniRust;

        /// Puts the mini player up; up already, it is raised again.
        #[qinvokable]
        fn enter(self: Pin<&mut Mini>);
        #[qinvokable]
        fn exit(self: Pin<&mut Mini>);
        #[qinvokable]
        fn toggle(self: Pin<&mut Mini>);
        /// Flips the card's own blurred cover and keeps the choice.
        #[qinvokable]
        fn toggle_background_blur(self: Pin<&mut Mini>);
        /// The queue changed: reads again whether play has a target.
        #[qinvokable]
        fn refresh_queue_target(self: Pin<&mut Mini>);
        /// The action a key in the mini player's window comes to, or
        /// empty: the window runs it.
        #[qinvokable]
        fn key_pressed(self: Pin<&mut Mini>, key: i32, modifiers: i32, text: &QString) -> QString;
        /// Marks what plays as a favorite, or takes the mark off.
        #[qinvokable]
        fn like_playing(self: Pin<&mut Mini>);
        /// The card's close button: leaves the mini player, then asks the
        /// window to close the way it closes.
        #[qinvokable]
        fn close_app(self: Pin<&mut Mini>);
        /// A preference was kept elsewhere: reads the card's own again.
        #[qinvokable]
        fn reload(self: Pin<&mut Mini>);
        #[qsignal]
        fn close_asked(self: Pin<&mut Mini>);

        #[cxx_name = "open"]
        fn open(self: &Mini) -> bool;
        #[cxx_name = "setOpen"]
        fn turn(self: Pin<&mut Mini>, on: bool);
        #[qsignal]
        #[cxx_name = "openChanged"]
        fn open_changed(self: Pin<&mut Mini>);
        #[cxx_name = "surface"]
        fn surface(self: &Mini) -> i32;
        #[cxx_name = "setSurface"]
        fn choose_surface(self: Pin<&mut Mini>, surface: i32);
        #[qsignal]
        #[cxx_name = "surfaceChanged"]
        fn surface_changed(self: Pin<&mut Mini>);
    }

    impl cxx_qt::Initialize for Mini {}
}

use chilpa_api::{mini, Pressed, Scope};
use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct MiniRust {
    open: bool,
    surface: i32,
    background_blur: bool,
    queue_has_target: bool,
    dev_surface: i32,
}

impl qobject::Mini {
    pub fn open(&self) -> bool {
        self.rust().open
    }

    pub fn turn(mut self: Pin<&mut Self>, on: bool) {
        if on == self.rust().open {
            return;
        }
        self.as_mut().rust_mut().open = on;
        self.as_mut().open_changed();
    }

    pub fn enter(mut self: Pin<&mut Self>) {
        // Up already: the host shows and raises it on the signal, and a
        // second ask is a wish to see it, not a change of state.
        if self.rust().open {
            self.as_mut().open_changed();
            return;
        }
        self.turn(true);
    }

    pub fn exit(self: Pin<&mut Self>) {
        self.turn(false);
    }

    pub fn toggle(self: Pin<&mut Self>) {
        if self.rust().open {
            self.exit();
        } else {
            self.enter();
        }
    }

    pub fn surface(&self) -> i32 {
        self.rust().surface
    }

    /// Kept on every write, changed or not, as QBZ keeps it.
    pub fn choose_surface(mut self: Pin<&mut Self>, surface: i32) {
        let surface = surface.clamp(0, i32::from(mini::SURFACES) - 1);
        if surface != self.rust().surface {
            self.as_mut().rust_mut().surface = surface;
            self.as_mut().surface_changed();
        }
        app::with(|api| api.adjust(mini::SURFACE, &surface.to_string()));
    }

    pub fn toggle_background_blur(mut self: Pin<&mut Self>) {
        let on = !self.rust().background_blur;
        self.as_mut().set_background_blur(on);
        app::with(|api| api.adjust(mini::BLUR, if on { "true" } else { "false" }));
    }

    pub fn refresh_queue_target(mut self: Pin<&mut Self>) {
        let has = app::with(|api| api.queue_has_target());
        self.as_mut().set_queue_has_target(has);
    }

    pub fn key_pressed(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString) -> QString {
        match app::with(|api| api.key_pressed_in(Scope::Mini, key, modifiers, &text.to_string(), false)) {
            Pressed::Run(action) => QString::from(action),
            Pressed::Pass | Pressed::Taken => QString::default(),
        }
    }

    pub fn like_playing(self: Pin<&mut Self>) {
        app::with(|api| api.like_playing());
    }

    pub fn close_app(mut self: Pin<&mut Self>) {
        self.as_mut().exit();
        self.close_asked();
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        let blur = app::with(|api| api.mini_background_blur());
        self.as_mut().set_background_blur(blur);
    }
}

impl cxx_qt::Initialize for qobject::Mini {
    fn initialize(mut self: Pin<&mut Self>) {
        let (surface, blur, target) = app::with(|api| (api.mini_opening_surface(), api.mini_background_blur(), api.queue_has_target()));
        self.as_mut().rust_mut().surface = i32::from(surface);
        self.as_mut().set_background_blur(blur);
        self.as_mut().set_queue_has_target(target);
        let asked = std::env::var("CHILPA_MINI")
            .ok()
            .and_then(|v| v.trim().parse::<i32>().ok())
            .filter(|n| (0..i32::from(mini::SURFACES)).contains(n))
            .unwrap_or(-1);
        self.as_mut().set_dev_surface(asked);
    }
}
