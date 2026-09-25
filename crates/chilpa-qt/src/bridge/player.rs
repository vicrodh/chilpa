//! What is playing and the transport, as the player bar reads them.
//!
//! The tick runs in Rust, four times a second, on a thread of its own that
//! only queues work onto the window's thread: QML has no timer to keep
//! alive. A property is set only when its value changes, so a window that
//! is not playing does not repaint at all, and one that is repaints four
//! times a second, not at the display's rate.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, title)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, album)]
        #[qproperty(QUrl, art)]
        /// The file playing; lists compare it to mark their row.
        #[qproperty(QString, path)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, paused)]
        /// Seconds, published in quarters.
        #[qproperty(f64, position)]
        #[qproperty(f64, duration)]
        /// "1:42" and "-2:22", set when the second changes.
        #[qproperty(QString, elapsed)]
        #[qproperty(QString, remaining)]
        /// "FLAC 24/96", with the bits of the signal.
        #[qproperty(QString, quality_label)]
        /// "FLAC · 24-bit / 96 kHz".
        #[qproperty(QString, quality_long)]
        /// 0 unknown, 1 lossy, 2 lossless, 3 hi-res, 4 DSD.
        #[qproperty(i32, quality_tier)]
        /// What the device was opened with.
        #[qproperty(QString, output_format)]
        /// Only what the engine states; never inferred here.
        #[qproperty(bool, bit_perfect)]
        #[qproperty(bool, shuffle)]
        /// 0 off, 1 the whole queue, 2 the track.
        #[qproperty(i32, repeat)]
        #[qproperty(QString, error)]
        type Player = super::PlayerRust;

        /// Pauses or resumes.
        #[qinvokable]
        fn toggle(self: Pin<&mut Player>);

        #[qinvokable]
        fn next(self: Pin<&mut Player>);

        /// Goes back; on the first track, starts it over.
        #[qinvokable]
        fn previous(self: Pin<&mut Player>);

        #[qinvokable]
        fn seek(self: Pin<&mut Player>, seconds: f64);

        #[qinvokable]
        fn toggle_shuffle(self: Pin<&mut Player>);

        #[qinvokable]
        fn cycle_repeat(self: Pin<&mut Player>);
    }

    impl cxx_qt::Threading for Player {}
    impl cxx_qt::Initialize for Player {}
}

use crate::{app, format};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

const TICK: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
pub struct PlayerRust {
    title: QString,
    artist: QString,
    album: QString,
    art: QUrl,
    path: QString,
    playing: bool,
    paused: bool,
    position: f64,
    duration: f64,
    elapsed: QString,
    remaining: QString,
    quality_label: QString,
    quality_long: QString,
    quality_tier: i32,
    output_format: QString,
    bit_perfect: bool,
    shuffle: bool,
    repeat: i32,
    error: QString,
    /// The file the text properties were last set for.
    shown: String,
}

impl qobject::Player {
    pub fn toggle(mut self: Pin<&mut Self>) {
        let paused = !self.rust().paused;
        app::with(|api| api.set_paused(paused));
        self.as_mut().set_paused(paused);
    }

    pub fn next(self: Pin<&mut Self>) {
        app::with(|api| api.next_track());
    }

    pub fn previous(self: Pin<&mut Self>) {
        app::with(|api| {
            let (device, bit_perfect) = api.chosen_output();
            api.previous_track(&device, bit_perfect);
        });
    }

    pub fn seek(mut self: Pin<&mut Self>, seconds: f64) {
        app::with(|api| api.seek(seconds));
        self.as_mut().set_position(seconds);
    }

    pub fn toggle_shuffle(mut self: Pin<&mut Self>) {
        let on = !self.rust().shuffle;
        app::with(|api| api.set_shuffle(on));
        self.as_mut().set_shuffle(on);
    }

    pub fn cycle_repeat(mut self: Pin<&mut Self>) {
        use chilpa_api::Repeat;
        let next = match self.rust().repeat {
            0 => Repeat::All,
            1 => Repeat::One,
            _ => Repeat::Off,
        };
        app::with(|api| api.set_repeat(next));
        self.as_mut().set_repeat(next as i32);
    }

    /// Collects the engine's events. The tick thread queues it.
    fn tick(mut self: Pin<&mut Self>) {
        let now = app::with(|api| api.poll().clone());
        if now.path != self.rust().shown {
            self.as_mut().rust_mut().shown = now.path.clone();
            self.as_mut().set_title(QString::from(&now.title));
            self.as_mut().set_artist(QString::from(&now.artist));
            self.as_mut().set_album(QString::from(&now.album));
            let art = if now.artwork_path.is_empty() {
                QUrl::default()
            } else {
                QUrl::from_local_file(&QString::from(&now.artwork_path))
            };
            self.as_mut().set_art(art);
            self.as_mut().set_quality_label(QString::from(&now.quality.short()));
            self.as_mut().set_quality_long(QString::from(&now.quality.long()));
            self.as_mut().set_quality_tier(now.quality.tier.rank());
            self.as_mut().set_output_format(QString::from(&now.format));
            self.as_mut().set_duration(now.total_seconds);
            self.as_mut().set_path(QString::from(&now.path));
        }
        if now.bit_perfect != self.rust().bit_perfect {
            self.as_mut().set_bit_perfect(now.bit_perfect);
        }
        if now.playing != self.rust().playing {
            self.as_mut().set_playing(now.playing);
        }
        if now.paused != self.rust().paused {
            self.as_mut().set_paused(now.paused);
        }
        let quarter = (now.seconds * 4.0).floor() / 4.0;
        if quarter != self.rust().position {
            self.as_mut().set_position(quarter);
            let (at, total) = ((now.seconds * 1000.0) as u64, (now.total_seconds * 1000.0) as u64);
            let elapsed = QString::from(&format::clock(at));
            if elapsed != self.rust().elapsed {
                self.as_mut().set_elapsed(elapsed);
                let left = format!("-{}", format::clock(total.saturating_sub(at)));
                self.as_mut().set_remaining(QString::from(&left));
            }
        }
        if QString::from(&now.error) != self.rust().error {
            self.as_mut().set_error(QString::from(&now.error));
        }
    }
}

impl cxx_qt::Initialize for qobject::Player {
    fn initialize(mut self: Pin<&mut Self>) {
        let (shuffle, repeat) = app::with(|api| (api.shuffle(), api.repeat()));
        self.as_mut().set_shuffle(shuffle);
        self.as_mut().set_repeat(repeat as i32);
        let thread = self.qt_thread();
        std::thread::Builder::new()
            .name("chilpa-tick".into())
            .spawn(move || loop {
                std::thread::sleep(TICK);
                // It fails once the object is gone, which is when to stop.
                if thread.queue(|player| player.tick()).is_err() {
                    break;
                }
            })
            .expect("the tick thread starts");
    }
}
