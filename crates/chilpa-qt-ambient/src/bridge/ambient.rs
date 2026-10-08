//! The dynamic background, as QML reads it: which of the four modes is on,
//! the three colors of the playing cover, the desktop's wallpaper and where
//! this window sits over it. The rules are the facade's (`chilpa_api::
//! ambient`, `chilpa_api::wallpaper`); this reads them, runs the slow parts
//! off the window's thread and publishes what came out.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// 0 off, 1 the album-colored field, 2 the blurred cover, 3 the
        /// desktop's wallpaper, 4 the wallpaper blurred.
        #[qproperty(i32, mode)]
        /// Whether the background shows right now: a mode is on and, below
        /// the wallpaper modes, a track is loaded. The pattern that draws it
        /// writes this; the theme's `ambient` follows it.
        #[qproperty(bool, on, READ = on, WRITE = turn, NOTIFY = on_changed)]
        /// The playing cover's three colors, or the defaults until a cover
        /// yields some. Never cleared: a cover that yields none keeps the
        /// last.
        #[qproperty(QColor, primary)]
        #[qproperty(QColor, secondary)]
        #[qproperty(QColor, accent)]
        /// The wallpaper and its atmosphere as file URLs; empty until a
        /// mode that paints them resolves one.
        #[qproperty(QString, wallpaper_url)]
        #[qproperty(QString, wallpaper_atmosphere_url)]
        /// This process's windows with their place on the screen, as a
        /// Plasma Wayland compositor reports them: `[{"x","y","w","h"}]`,
        /// "[]" anywhere else.
        #[qproperty(QString, wallpaper_windows_json)]
        /// The wallpaper mode's blur, 0 to 1.
        #[qproperty(f32, wallpaper_blur)]
        /// An image of the user's own behind the wallpaper modes, or empty
        /// for the desktop's.
        #[qproperty(QString, background_image)]
        type Ambient = super::AmbientRust;

        /// Reads the three preferences again: one of them was kept.
        #[qinvokable]
        fn reload(self: Pin<&mut Ambient>);

        /// Keeps a mode, by its index.
        #[qinvokable]
        fn choose(self: Pin<&mut Ambient>, mode: i32);

        /// The cover that plays changed: its colors are read off the
        /// window's thread and published when they are ready.
        #[qinvokable]
        fn follow(self: Pin<&mut Ambient>, art: &QUrl);

        /// Resolves the wallpaper again: the window came back to the front,
        /// the mode changed, an image was chosen.
        #[qinvokable]
        fn refresh_wallpaper(self: Pin<&mut Ambient>);

        /// Starts following this window's place on a Plasma Wayland
        /// desktop. Once; nothing to start anywhere else.
        #[qinvokable]
        fn track_window_position(self: Pin<&mut Ambient>);

        #[cxx_name = "on"]
        fn on(self: &Ambient) -> bool;
        #[cxx_name = "setOn"]
        fn turn(self: Pin<&mut Ambient>, on: bool);
        #[qsignal]
        #[cxx_name = "onChanged"]
        fn on_changed(self: Pin<&mut Ambient>);
    }

    impl cxx_qt::Threading for Ambient {}
    impl cxx_qt::Initialize for Ambient {}
}

use chilpa_api::ambient::{self, Triad};
use chilpa_api::theme::parse_color;
use chilpa_qt_kit::app;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QColor, QString, QUrl};
use std::pin::Pin;

pub struct AmbientRust {
    mode: i32,
    on: bool,
    primary: QColor,
    secondary: QColor,
    accent: QColor,
    wallpaper_url: QString,
    wallpaper_atmosphere_url: QString,
    wallpaper_windows_json: QString,
    wallpaper_blur: f32,
    background_image: QString,
}

fn qcolor(hex: &str) -> QColor {
    parse_color(hex).map(|c| QColor::from_rgba_f(c.r as f32, c.g as f32, c.b as f32, c.a as f32)).unwrap_or_default()
}

impl Default for AmbientRust {
    fn default() -> Self {
        let triad = Triad::default();
        Self {
            mode: 0,
            on: false,
            primary: qcolor(&triad.primary),
            secondary: qcolor(&triad.secondary),
            accent: qcolor(&triad.accent),
            wallpaper_url: QString::default(),
            wallpaper_atmosphere_url: QString::default(),
            wallpaper_windows_json: QString::from("[]"),
            wallpaper_blur: f32::from(ambient::BLUR_DEFAULT) / 100.0,
            background_image: QString::default(),
        }
    }
}

impl qobject::Ambient {
    pub fn reload(mut self: Pin<&mut Self>) {
        let (mode, blur, image) = app::with(|api| (api.background_mode(), api.wallpaper_blur(), api.background_image()));
        self.as_mut().set_mode(mode as i32);
        self.as_mut().set_wallpaper_blur(f32::from(blur) / 100.0);
        self.as_mut().set_background_image(QString::from(&image));
    }

    pub fn choose(self: Pin<&mut Self>, mode: i32) {
        let Some(value) = usize::try_from(mode).ok().and_then(|i| ambient::MODES.get(i)) else { return };
        if app::with(|api| api.adjust(ambient::MODE, value)) {
            chilpa_qt_kit::bridge::prefs::changed();
        }
        self.reload();
    }

    pub fn on(&self) -> bool {
        self.rust().on
    }

    pub fn turn(mut self: Pin<&mut Self>, on: bool) {
        if on == self.rust().on {
            return;
        }
        self.as_mut().rust_mut().on = on;
        self.as_mut().on_changed();
        chilpa_qt_kit::bridge::theme::ask(move |theme| theme.set_ambient(on));
    }

    pub fn follow(self: Pin<&mut Self>, art: &QUrl) {
        let path = art.to_local_file().map(|p| p.to_string()).unwrap_or_default();
        if path.is_empty() {
            return;
        }
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let Some(triad) = ambient::triad_of(std::path::Path::new(&path)) else { return };
            let _ = thread.queue(move |mut this| {
                this.as_mut().set_primary(qcolor(&triad.primary));
                this.as_mut().set_secondary(qcolor(&triad.secondary));
                this.as_mut().set_accent(qcolor(&triad.accent));
            });
        });
    }

    pub fn refresh_wallpaper(self: Pin<&mut Self>) {
        let mode = *self.mode();
        if !ambient::paints_wallpaper(mode as usize) {
            return;
        }
        let image = self.background_image().to_string();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let Some(path) = chilpa_api::wallpaper::resolve(&image) else { return };
            let url = QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref())).to_string();
            // Only the blurred mode draws the atmosphere; the plain one
            // shows the picture itself.
            let atmosphere = (mode == 4)
                .then(|| chilpa_api::header_atmosphere(&path))
                .flatten()
                .map(|p| QUrl::from_local_file(&QString::from(p.to_string_lossy().as_ref())).to_string());
            let _ = thread.queue(move |mut this| {
                let url = QString::from(&url);
                if *this.wallpaper_url() != url {
                    this.as_mut().set_wallpaper_url(url);
                }
                if let Some(atmosphere) = atmosphere.map(|a| QString::from(&a)) {
                    if *this.wallpaper_atmosphere_url() != atmosphere {
                        this.as_mut().set_wallpaper_atmosphere_url(atmosphere);
                    }
                }
            });
        });
    }

    pub fn track_window_position(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        chilpa_api::wallpaper::track_window_position(move |json| {
            let _ = thread.queue(move |mut this| this.as_mut().set_wallpaper_windows_json(QString::from(&json)));
        });
    }
}

impl cxx_qt::Initialize for qobject::Ambient {
    fn initialize(self: Pin<&mut Self>) {
        self.reload();
    }
}
