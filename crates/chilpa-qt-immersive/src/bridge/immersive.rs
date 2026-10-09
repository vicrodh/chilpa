//! The immersive view as QML reads it: whether it is open, which view it
//! shows (focus with one panel, or split with a panel beside the cover),
//! which shader scene covers the panels, and what a key over it comes
//! to. The rules are the facade's (`chilpa_api::immersive`); this reads
//! them, keeps what the listener changes, and tells the visualizer what
//! is on screen so it runs only for that.
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
        /// Whether the overlay is up. Every write, from QML or from a key,
        /// goes through one setter: the edges are where the view is
        /// decided and the visualizer told.
        #[qproperty(bool, open, READ = open, WRITE = turn, NOTIFY = open_changed)]
        /// 0 focus, 1 split. Written only through `setView`.
        #[qproperty(i32, view_mode)]
        /// The focus panel: 0 Album Reactive, 1 Static, 2 Coverflow, 3
        /// Spectrum, 4 Lyrics, 5 Queue, 6 Wave Bed, 7 Goniometer, 8
        /// Oscilloscope, 9 Reactive Rings.
        #[qproperty(i32, mode)]
        /// The split panel: 0 Lyrics, 3 Queue, 4 Now Playing, 5 Now
        /// Playing with one line of lyrics.
        #[qproperty(i32, split_panel)]
        /// The shader scene over the panels, 0 for none.
        #[qproperty(i32, scene)]
        /// Whether the renderer is accelerated: the scopes and the scenes
        /// need it. The overlay says, from its graphics info.
        #[qproperty(bool, accelerated)]
        /// What Enter does with a search result: disabled, replace, next
        /// or queue.
        #[qproperty(QString, search_action)]
        /// Whether a development hook (`CHILPA_IMMERSIVE=1`) asks the
        /// overlay to open at start.
        #[qproperty(bool, dev_open)]
        /// The bar's margin above the bottom, the alpha of its fill and
        /// whether it blurs what lies under it: 16, 0.85 and on, which the
        /// owner compares against QBZ's 24, 0.50 and flat through
        /// `CHILPA_IMMERSIVE_BAR_BOTTOM`, `CHILPA_IMMERSIVE_BAR_ALPHA` and
        /// `CHILPA_IMMERSIVE_BAR_BLUR=0`.
        #[qproperty(i32, bar_bottom)]
        #[qproperty(f64, bar_alpha)]
        #[qproperty(bool, bar_blur)]
        /// Whether what plays can be marked as a favorite, and whether it
        /// is: read again on `refreshFavorite`.
        #[qproperty(bool, markable)]
        #[qproperty(bool, favorite)]
        type Immersive = super::ImmersiveRust;

        /// Shift+I: flips the overlay without touching the view.
        #[qinvokable]
        fn toggle(self: Pin<&mut Immersive>);
        /// The View menu: the first focus panel, then open; the open edge
        /// restores what is remembered over it.
        #[qinvokable]
        fn open_from_menu(self: Pin<&mut Immersive>);
        /// Opens the overlay on a given view, over whatever is remembered:
        /// the player bar's cover opens it on Now Playing.
        #[qinvokable]
        fn open_view(self: Pin<&mut Immersive>, view_mode: i32, mode: i32, split_panel: i32);
        /// The only way the view changes: kept for the next open while the
        /// default is "remember".
        #[qinvokable]
        fn set_view(self: Pin<&mut Immersive>, view_mode: i32, mode: i32, split_panel: i32);
        /// The scene over the panels, kept the same way.
        #[qinvokable]
        fn choose_scene(self: Pin<&mut Immersive>, scene: i32);
        /// The g key: the next shipped scene, and none after the last.
        #[qinvokable]
        fn cycle_scene(self: Pin<&mut Immersive>);
        /// The action a key over the overlay comes to, or empty.
        #[qinvokable]
        fn key_pressed(self: Pin<&mut Immersive>, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString;
        /// A preference was kept elsewhere: reads the overlay's own again.
        #[qinvokable]
        fn reload(self: Pin<&mut Immersive>);
        /// Marks what plays as a favorite, or takes the mark off.
        #[qinvokable]
        fn like_playing(self: Pin<&mut Immersive>);
        /// The track or its marks changed: reads the heart again.
        #[qinvokable]
        fn refresh_favorite(self: Pin<&mut Immersive>);

        #[cxx_name = "open"]
        fn open(self: &Immersive) -> bool;
        #[cxx_name = "setOpen"]
        fn turn(self: Pin<&mut Immersive>, on: bool);
        #[qsignal]
        #[cxx_name = "openChanged"]
        fn open_changed(self: Pin<&mut Immersive>);
    }

    impl cxx_qt::Initialize for Immersive {}
}

use chilpa_api::{immersive, Pressed, Scope};
use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct ImmersiveRust {
    open: bool,
    view_mode: i32,
    mode: i32,
    split_panel: i32,
    scene: i32,
    accelerated: bool,
    search_action: QString,
    dev_open: bool,
    bar_bottom: i32,
    bar_alpha: f64,
    bar_blur: bool,
    markable: bool,
    favorite: bool,
}

/// A number from the environment, or the default when unset or unreadable.
fn knob<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

impl Default for ImmersiveRust {
    fn default() -> Self {
        Self {
            open: false,
            view_mode: immersive::DEFAULT_VIEW_MODE,
            mode: immersive::DEFAULT_MODE,
            split_panel: immersive::DEFAULT_SPLIT_PANEL,
            scene: 0,
            accelerated: true,
            search_action: QString::from("replace"),
            dev_open: false,
            bar_bottom: knob("CHILPA_IMMERSIVE_BAR_BOTTOM", 16),
            bar_alpha: knob("CHILPA_IMMERSIVE_BAR_ALPHA", 0.85_f64).clamp(0.0, 1.0),
            bar_blur: std::env::var("CHILPA_IMMERSIVE_BAR_BLUR").map_or(true, |v| v.trim() != "0"),
            markable: false,
            favorite: false,
        }
    }
}

impl qobject::Immersive {
    pub fn open(&self) -> bool {
        self.rust().open
    }

    /// The one setter. Opening decides the view from what is kept and
    /// claims the visualizer; closing lets it go.
    pub fn turn(mut self: Pin<&mut Self>, on: bool) {
        if on == self.rust().open {
            return;
        }
        self.as_mut().rust_mut().open = on;
        if on {
            let accelerated = self.rust().accelerated;
            let (view, streams) = app::with(|api| (api.immersive_opening(accelerated), api.visualizer()));
            self.as_mut().set_view_mode(view.view_mode);
            self.as_mut().set_mode(view.mode);
            self.as_mut().set_split_panel(view.split_panel);
            self.as_mut().set_scene(view.scene);
            streams.open();
            streams.set_view(view.view_mode, view.mode);
            streams.set_scene_active(view.scene > 0);
        } else {
            app::with(|api| api.visualizer()).close();
        }
        self.as_mut().open_changed();
    }

    pub fn toggle(self: Pin<&mut Self>) {
        let next = !self.rust().open;
        self.turn(next);
    }

    pub fn open_from_menu(mut self: Pin<&mut Self>) {
        self.as_mut().set_mode(0);
        self.turn(true);
    }

    pub fn open_view(mut self: Pin<&mut Self>, view_mode: i32, mode: i32, split_panel: i32) {
        self.as_mut().turn(true);
        self.set_view(view_mode, mode, split_panel);
    }

    pub fn set_view(mut self: Pin<&mut Self>, view_mode: i32, mode: i32, split_panel: i32) {
        let accelerated = self.rust().accelerated;
        let mode = immersive::focus_mode_for_tier(view_mode, mode, accelerated);
        let split_panel = immersive::split_panel_kept(split_panel);
        self.as_mut().set_view_mode(view_mode);
        self.as_mut().set_mode(mode);
        self.as_mut().set_split_panel(split_panel);
        app::with(|api| {
            api.remember_immersive_view(view_mode, mode, split_panel);
            api.visualizer().set_view(view_mode, mode);
        });
    }

    pub fn choose_scene(mut self: Pin<&mut Self>, scene: i32) {
        let scene = if immersive::SCENES.contains(&scene) && self.rust().accelerated { scene } else { 0 };
        self.as_mut().set_scene(scene);
        app::with(|api| {
            api.remember_immersive_scene(scene);
            api.visualizer().set_scene_active(scene > 0);
        });
    }

    pub fn cycle_scene(self: Pin<&mut Self>) {
        let next = immersive::next_scene(self.rust().scene);
        self.choose_scene(next);
    }

    pub fn key_pressed(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString {
        match app::with(|api| api.key_pressed_in(Scope::Immersive, key, modifiers, &text.to_string(), typing)) {
            Pressed::Run(action) => QString::from(action),
            Pressed::Pass | Pressed::Taken => QString::default(),
        }
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        let action = app::with(|api| api.immersive_search_action());
        self.as_mut().set_search_action(QString::from(&action));
    }

    pub fn like_playing(mut self: Pin<&mut Self>) {
        app::with(|api| api.like_playing());
        self.as_mut().refresh_favorite();
    }

    pub fn refresh_favorite(mut self: Pin<&mut Self>) {
        let (markable, favorite) = app::with(|api| api.playing_favorite());
        self.as_mut().set_markable(markable);
        self.as_mut().set_favorite(favorite);
    }
}

impl cxx_qt::Initialize for qobject::Immersive {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().reload();
        let asked = std::env::var("CHILPA_IMMERSIVE").is_ok_and(|v| v.trim() == "1");
        self.as_mut().set_dev_open(asked);
    }
}
