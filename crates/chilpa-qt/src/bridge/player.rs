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
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
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
        /// The output, as the stack names it: "Fosi Audio ZH3".
        #[qproperty(QString, device)]
        /// The stack and the policy: "ALSA · bit-perfect only".
        #[qproperty(QString, route)]
        /// Only what the engine states; never inferred here.
        #[qproperty(bool, bit_perfect)]
        #[qproperty(bool, shuffle)]
        /// 0 off, 1 the whole queue, 2 the track.
        #[qproperty(i32, repeat)]
        /// The playback switches: one track into the next, one loudness,
        /// the queue in tempo order. Normalizing is only applied with
        /// bit-perfect off, which `normalize_applies` says.
        /// The bit-perfect policy, as chosen: nothing reaches the device
        /// changed. Not `bit_perfect`, which says whether what plays is.
        #[qproperty(bool, bit_perfect_mode)]
        #[qproperty(bool, gapless)]
        #[qproperty(bool, normalize)]
        #[qproperty(bool, normalize_applies)]
        #[qproperty(bool, automix)]
        /// Why the mix did not start, when it did not.
        #[qproperty(QString, automix_note)]
        #[qproperty(QString, error)]
        /// Bumped when the queue changed: another track, more or fewer queued.
        #[qproperty(i32, queue_rev)]
        /// The lyrics of what plays, one line per entry, and the line being
        /// sung (-1 when they carry no timings or none has come yet).
        #[qproperty(QStringList, lyrics)]
        #[qproperty(i32, lyrics_line)]
        #[qproperty(bool, lyrics_synced)]
        type Player = super::PlayerRust;

        /// Empties the queue and stops.
        #[qinvokable]
        fn clear_queue(self: Pin<&mut Player>);

        /// Turns the bit-perfect policy on or off, on the output in use.
        #[qinvokable]
        fn choose_bit_perfect(self: Pin<&mut Player>, on: bool);
        #[qinvokable]
        fn choose_gapless(self: Pin<&mut Player>, on: bool);
        #[qinvokable]
        fn choose_normalize(self: Pin<&mut Player>, on: bool);
        #[qinvokable]
        fn choose_automix(self: Pin<&mut Player>, on: bool);

        /// Reads the switches again: Settings changed one.
        #[qinvokable]
        fn read_switches(self: Pin<&mut Player>);

        /// A time as the player shows it: "3:07".
        #[qinvokable]
        fn clock(self: &Player, seconds: f64) -> QString;

        /// Removes a queued track, counted from the one that plays.
        #[qinvokable]
        fn remove_queued(self: Pin<&mut Player>, row: i32);

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

        /// Moves the position by that many seconds, within the track.
        #[qinvokable]
        fn nudge(self: Pin<&mut Player>, seconds: f64);

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
use cxx_qt_lib::{QList, QString, QStringList, QUrl};
use std::pin::Pin;

const TICK: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
pub struct PlayerRust {
    bit_perfect_mode: bool,
    gapless: bool,
    normalize: bool,
    normalize_applies: bool,
    automix: bool,
    automix_note: QString,
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
    device: QString,
    route: QString,
    bit_perfect: bool,
    shuffle: bool,
    repeat: i32,
    error: QString,
    queue_rev: i32,
    lyrics: QStringList,
    lyrics_line: i32,
    lyrics_synced: bool,
    /// The file the text properties were last set for.
    shown: String,
    /// Where the queue was at the last tick: which track and how many.
    queue_at: (usize, usize),
}

impl qobject::Player {
    pub fn choose_bit_perfect(mut self: Pin<&mut Self>, on: bool) {
        app::with(|api| api.set_bit_perfect(on));
        self.as_mut().read_switches();
    }

    pub fn choose_gapless(mut self: Pin<&mut Self>, on: bool) {
        app::with(|api| api.set_gapless(on));
        self.as_mut().read_switches();
    }

    pub fn choose_normalize(mut self: Pin<&mut Self>, on: bool) {
        app::with(|api| api.set_normalize(on));
        self.as_mut().read_switches();
    }

    pub fn choose_automix(mut self: Pin<&mut Self>, on: bool) {
        let note = app::with(|api| {
            api.set_automix(on);
            if on && !api.automix_on() { api.now_playing().error.clone() } else { String::new() }
        });
        self.as_mut().set_automix_note(QString::from(&note));
        self.as_mut().read_switches();
        // The queue's order changed; the panel lists it in the new one.
        self.as_mut().bump_queue();
    }

    pub fn clock(&self, seconds: f64) -> QString {
        QString::from(&format::clock((seconds.max(0.0) * 1000.0) as u64))
    }

    pub fn read_switches(mut self: Pin<&mut Self>) {
        let (strict, gapless, normalize, mix) = app::with(|api| {
            let (_, bit_perfect) = api.chosen_output();
            (bit_perfect, api.gapless(), api.normalize(), api.automix_on())
        });
        self.as_mut().set_bit_perfect_mode(strict);
        let applies = !strict;
        self.as_mut().set_gapless(gapless);
        self.as_mut().set_normalize(normalize);
        self.as_mut().set_normalize_applies(applies);
        self.as_mut().set_automix(mix);
    }

    pub fn clear_queue(mut self: Pin<&mut Self>) {
        app::with(|api| api.clear_queue());
        self.as_mut().bump_queue();
    }

    pub fn remove_queued(mut self: Pin<&mut Self>, row: i32) {
        // The panel's rows are the queue in the order it will play, without
        // what was taken out: its row is not a distance from the playing one.
        let removed = app::with(|api| match api.queue_order().get(row.max(0) as usize) {
            Some(&at) => api.remove_from_queue(at),
            None => false,
        });
        if removed {
            self.as_mut().bump_queue();
        }
    }

    fn bump_queue(mut self: Pin<&mut Self>) {
        let next = self.queue_rev().wrapping_add(1);
        self.as_mut().set_queue_rev(next);
    }

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

    pub fn nudge(self: Pin<&mut Self>, seconds: f64) {
        let (at, end) = (*self.position(), *self.duration());
        if end <= 0.0 {
            return;
        }
        self.seek((at + seconds).clamp(0.0, end));
    }

    pub fn toggle_shuffle(mut self: Pin<&mut Self>) {
        let on = !self.rust().shuffle;
        app::with(|api| api.set_shuffle(on));
        self.as_mut().set_shuffle(on);
        // Shuffling replaces the mix.
        self.as_mut().set_automix(false);
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
            // A new queue drops the mix; the switch follows.
            let mix = app::with(|api| api.automix_on());
            self.as_mut().set_automix(mix);
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
            let (device, route) = app::with(|api| {
                let (output, bit_perfect) = api.chosen_output();
                let stack = api.stack_of(&output);
                let device = api
                    .stack_outputs(&stack)
                    .into_iter()
                    .find(|o| o.id == output)
                    .map(|o| o.label)
                    .unwrap_or(output);
                let label = api.stacks().into_iter().find(|s| s.key == stack).map(|s| s.label).unwrap_or(stack);
                let policy = if bit_perfect { api.t("bit-perfect only") } else { api.t("conversion allowed") };
                (device, format!("{label} · {policy}"))
            });
            self.as_mut().set_device(QString::from(&device));
            self.as_mut().set_route(QString::from(&route));
            self.as_mut().set_duration(now.total_seconds);
            self.as_mut().set_path(QString::from(&now.path));
            let mut lines = QList::<QString>::default();
            for line in &now.lyrics {
                lines.append(QString::from(line));
            }
            self.as_mut().set_lyrics(QStringList::from(&lines));
            self.as_mut().set_lyrics_synced(now.lyrics_synced);
        }
        if now.lyrics_line != self.rust().lyrics_line {
            self.as_mut().set_lyrics_line(now.lyrics_line);
        }
        if (now.index, now.queue_len) != self.rust().queue_at {
            self.as_mut().rust_mut().queue_at = (now.index, now.queue_len);
            self.as_mut().bump_queue();
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
        self.as_mut().read_switches();
        crate::registry::register_player(self.qt_thread());
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
