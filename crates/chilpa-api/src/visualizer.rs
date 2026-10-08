//! The visualizer as the facade runs it: the producer over the tap, the
//! cells that keep the latest frame of each stream, the wake of the
//! window's drain, and the pack the shader scenes read. Port of QBZ's
//! `viz_qt.rs` without the Qt half: the window owns a thread that parks,
//! is woken here when a frame it draws has landed, and hands what
//! [`Visualizer::drain`] returns to its bridge.
//!
//! The cost controls are QBZ's. The producer does not exist until the
//! first view opens, and parks while nothing is open or the player is
//! paused. Only the streams the open view draws wake the drain: with the
//! immersive view open that is bars, energy and waveform, every frame;
//! the scope streams and the ribbon's bands ride along on those wakes;
//! the scopes' DSP runs only while their panel is on screen and no scene
//! covers it. The pack ticks once per fresh bars frame, which is the
//! producer's cadence, so its envelopes decay at 30 Hz and not faster.

use chilpa_core::tap::Tap;
use chilpa_core::visualizer::{self, Controls, Frame, Sink, BARS, ENERGY_BANDS, GONIOMETER, OSCILLOSCOPE, WAVE_POINTS};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::Thread;

/// Who wants the tap running: the immersive view, and a visualizer in
/// the player bar when there is one. The producer runs while either does.
pub const IMMERSIVE: u32 = 0b01;
pub const BAR: u32 = 0b10;

/// The phase wraps at a multiple of eight, so a scene's `& 7` ring math
/// stays seamless across the wrap.
const PHASE_WRAP: f32 = 4096.0;

/// The facade's handle on the visualizer; clones share one state.
#[derive(Clone)]
pub struct Visualizer(Arc<Shared>);

struct Shared {
    tap: Tap,
    controls: Controls,
    cells: Cells,
    producer: OnceLock<Thread>,
    drain: OnceLock<Thread>,
    sources: AtomicU32,
    marshal_all: AtomicBool,
    panel_scopes: AtomicU32,
    scene_active: AtomicBool,
    pack: Mutex<Pack>,
}

/// One slot per stream, latest wins: a stalled window drops frames
/// instead of growing a queue.
#[derive(Default)]
struct Cells {
    bars: Mutex<Option<[f32; BARS]>>,
    energy: Mutex<Option<[f32; ENERGY_BANDS]>>,
    wave: Mutex<Option<Box<[f32; WAVE_POINTS * 2]>>>,
    spectral: Mutex<Option<Vec<f32>>>,
    transient: Mutex<Option<f32>>,
    goniometer: Mutex<Option<(Box<[f32; 512]>, f32)>>,
    oscilloscope: Mutex<Option<Box<[f32; 512]>>>,
}

/// What one pass of the drain hands the window: the latest of each stream
/// that was waiting, and the pack when a bars frame arrived.
#[derive(Default, Debug, PartialEq)]
pub struct Drained {
    pub bars: Option<[f32; BARS]>,
    pub energy: Option<[f32; ENERGY_BANDS]>,
    pub wave: Option<Box<[f32; WAVE_POINTS * 2]>>,
    pub goniometer: Option<(Box<[f32; 512]>, f32)>,
    pub oscilloscope: Option<Box<[f32; 512]>>,
    pub pack: Option<String>,
}

impl Visualizer {
    pub fn new(tap: Tap) -> Self {
        Self(Arc::new(Shared {
            tap,
            controls: Controls::new(),
            cells: Cells::default(),
            producer: OnceLock::new(),
            drain: OnceLock::new(),
            sources: AtomicU32::new(0),
            marshal_all: AtomicBool::new(false),
            panel_scopes: AtomicU32::new(0),
            scene_active: AtomicBool::new(false),
            pack: Mutex::new(Pack::default()),
        }))
    }

    /// The window's drain thread, told once: it is unparked when a frame
    /// it draws has landed, and when the view opens.
    pub fn attach_drain(&self, thread: Thread) {
        let _ = self.0.drain.set(thread);
    }

    /// Whether anybody wants the tap running. The drain parks while not.
    pub fn enabled(&self) -> bool {
        self.0.controls.enabled()
    }

    /// The immersive view opened: every stream it draws publishes each
    /// frame from here on, and the producer runs if it was not.
    pub fn open(&self) {
        self.0.marshal_all.store(true, Ordering::Relaxed);
        self.set_source(IMMERSIVE, true);
        if self.enabled() {
            self.wake_drain();
        }
    }

    /// The immersive view closed: its scopes go idle and its claim on the
    /// tap is let go. Another claim keeps the producer running.
    pub fn close(&self) {
        self.0.panel_scopes.store(0, Ordering::Relaxed);
        self.apply_scopes();
        self.set_source(IMMERSIVE, false);
        self.0.marshal_all.store(false, Ordering::Relaxed);
    }

    /// Which panel the immersive view shows: a scope's DSP runs only
    /// while its panel is the one on screen, in focus.
    pub fn set_view(&self, view_mode: i32, mode: i32) {
        let mask = if self.0.marshal_all.load(Ordering::Relaxed) && view_mode == 0 { scopes_for(mode) } else { 0 };
        self.0.panel_scopes.store(mask, Ordering::Relaxed);
        self.apply_scopes();
    }

    /// A shader scene covers every panel: the scope asked for stays
    /// asked for, so coming back is instant, but its DSP stops meanwhile.
    pub fn set_scene_active(&self, active: bool) {
        self.0.scene_active.store(active, Ordering::Relaxed);
        self.apply_scopes();
    }

    /// Mirrors the transport: paused, the producer parks instead of
    /// analyzing the same samples again.
    pub fn set_paused(&self, paused: bool) {
        if self.0.controls.paused() == paused {
            return;
        }
        self.0.controls.set_paused(paused);
        if !paused && self.enabled() {
            if let Some(producer) = self.0.producer.get() {
                producer.unpark();
            }
        }
    }

    /// Takes what is waiting. The guards are let go cell by cell, never
    /// held while the window is told, so the producer never waits on it.
    pub fn drain(&self) -> Drained {
        let cells = &self.0.cells;
        let mut out = Drained::default();
        if !self.0.marshal_all.load(Ordering::Relaxed) {
            return out;
        }
        out.bars = cells.bars.lock().unwrap_or_else(|e| e.into_inner()).take();
        out.energy = cells.energy.lock().unwrap_or_else(|e| e.into_inner()).take();
        out.wave = cells.wave.lock().unwrap_or_else(|e| e.into_inner()).take();
        out.goniometer = cells.goniometer.lock().unwrap_or_else(|e| e.into_inner()).take();
        out.oscilloscope = cells.oscilloscope.lock().unwrap_or_else(|e| e.into_inner()).take();
        let spectral = cells.spectral.lock().unwrap_or_else(|e| e.into_inner()).take();
        let transient = cells.transient.lock().unwrap_or_else(|e| e.into_inner()).take();
        // A bars frame comes with every producer frame, so it is the
        // cadence: a wake that found the cells drained must not tick, or
        // the envelopes would decay faster than the producer runs.
        let mut pack = self.0.pack.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bars) = out.bars {
            pack.bars = bars;
        }
        if let Some(energy) = out.energy {
            pack.energy = energy;
        }
        if out.bars.is_some() {
            out.pack = Some(pack.tick(transient, spectral.as_deref()));
        }
        out
    }

    fn set_source(&self, bit: u32, on: bool) {
        let before = self.0.sources.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |m| Some(with_bit(m, bit, on)));
        let Ok(before) = before else { return };
        let (was, now) = (before != 0, with_bit(before, bit, on) != 0);
        if was != now {
            self.apply_enabled(now);
        }
    }

    fn apply_enabled(&self, on: bool) {
        self.0.controls.set_enabled(on);
        if !on {
            // The producer and the drain park on their own; nothing to do.
            return;
        }
        let shared = self.0.clone();
        let producer = self.0.producer.get_or_init(|| {
            let sink: Arc<dyn Sink> = Arc::new(Latch(shared.clone()));
            visualizer::spawn(shared.tap.clone(), shared.controls.clone(), sink).thread().clone()
        });
        producer.unpark();
        self.wake_drain();
    }

    fn apply_scopes(&self) {
        let requested = self.0.panel_scopes.load(Ordering::Relaxed);
        self.0.controls.set_scopes(visible_scopes(requested, self.0.scene_active.load(Ordering::Relaxed)));
    }

    fn wake_drain(&self) {
        self.0.wake_drain();
    }
}

impl Shared {
    /// Waking the drain is one atomic swap while it is awake, and the
    /// permit it leaves is kept for its next park, so a wake is never lost
    /// between the latch and the park.
    fn wake_drain(&self) {
        if let Some(drain) = self.drain.get() {
            drain.unpark();
        }
    }
}

/// The producer's side: latches a frame and wakes the drain if the open
/// view draws that stream. Runs on the producer's thread.
struct Latch(Arc<Shared>);

impl Sink for Latch {
    fn submit(&self, frame: Frame) {
        let all = self.0.marshal_all.load(Ordering::Relaxed);
        let cells = &self.0.cells;
        let wake = match frame {
            Frame::Bars(b) => {
                *cells.bars.lock().unwrap_or_else(|e| e.into_inner()) = Some(b);
                wakes(all, Stream::Bars)
            }
            Frame::Energy(e) => {
                *cells.energy.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                wakes(all, Stream::Energy)
            }
            Frame::Wave(w) => {
                *cells.wave.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
                wakes(all, Stream::Wave)
            }
            // The scenes' two streams are kept only while a view is open,
            // and never wake: they ride the next wake of the three above.
            Frame::Spectral(s) => {
                if all {
                    *cells.spectral.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
                }
                false
            }
            Frame::Transient(t) => {
                if all {
                    *cells.transient.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
                }
                false
            }
            Frame::Goniometer { points, correlation } => {
                *cells.goniometer.lock().unwrap_or_else(|e| e.into_inner()) = Some((points, correlation));
                wakes(all, Stream::Goniometer)
            }
            Frame::Oscilloscope(points) => {
                *cells.oscilloscope.lock().unwrap_or_else(|e| e.into_inner()) = Some(points);
                wakes(all, Stream::Oscilloscope)
            }
        };
        if wake {
            self.0.wake_drain();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stream {
    Bars,
    Wave,
    Energy,
    Goniometer,
    Oscilloscope,
}

/// Which streams wake the drain: with a view open, the three it draws
/// every frame; the scopes ride along. With none open, nothing does.
fn wakes(marshal_all: bool, stream: Stream) -> bool {
    marshal_all && matches!(stream, Stream::Bars | Stream::Wave | Stream::Energy)
}

/// The scope a focus panel asks for: 7 the goniometer, 8 the oscilloscope.
fn scopes_for(mode: i32) -> u32 {
    match mode {
        7 => GONIOMETER,
        8 => OSCILLOSCOPE,
        _ => 0,
    }
}

fn visible_scopes(requested: u32, scene_active: bool) -> u32 {
    if scene_active { 0 } else { requested }
}

fn with_bit(mask: u32, bit: u32, on: bool) -> u32 {
    if on { mask | bit } else { mask & !bit }
}

/// The scenes' uniforms, derived once per producer frame: the envelopes
/// of the transient, the level and its slow mean, the eight paired bands
/// and the phase that drives the forward motion. Port of QBZ's
/// `ShaderPackState`, in the same order: the fresh transient maxes into
/// both envelopes, then they decay, then the rest derives from the
/// decayed values.
#[derive(Default)]
struct Pack {
    bars: [f32; BARS],
    energy: [f32; ENERGY_BANDS],
    level_smooth: f32,
    beat: f32,
    /// The slow floor the beat is measured against: on busy material the
    /// envelope ripples around a high value and stops being a beat, so
    /// what counts is the part above the local density. It follows the
    /// beat down fast and creeps up slowly.
    beat_floor: f32,
    transient: f32,
    phase: f32,
    spectral_peak: f32,
}

impl Pack {
    fn tick(&mut self, transient: Option<f32>, spectral: Option<&[f32]>) -> String {
        if let Some(x) = transient {
            self.transient = x.max(self.transient);
            self.beat = x.max(self.beat);
        }
        self.transient *= 0.85;
        self.beat *= 0.88;
        self.beat_floor = if self.beat < self.beat_floor { self.beat_floor * 0.90 } else { self.beat_floor * 0.98 + self.beat * 0.02 };

        let mut bands = [0.0f32; 8];
        for (i, band) in bands.iter_mut().enumerate() {
            *band = (self.bars[2 * i] + self.bars[2 * i + 1]) * 0.5;
        }
        let level = self.energy.iter().sum::<f32>() * 0.2;
        self.level_smooth = self.level_smooth * 0.96 + level * 0.04;
        self.phase += 0.012 + level * 0.02 + self.beat * 0.02;
        if self.phase >= PHASE_WRAP {
            self.phase -= PHASE_WRAP;
        }
        // The highest band with signal, as a share of the top, smoothed.
        if let Some(bins) = spectral {
            if bins.len() > 1 {
                let highest = bins.iter().enumerate().filter(|(_, v)| **v > 0.05).map(|(i, _)| i).last().unwrap_or(0);
                let target = highest as f32 / (bins.len() - 1) as f32;
                self.spectral_peak = self.spectral_peak * 0.85 + target * 0.15;
            }
        }
        // The beat above its floor, normalized so a scene can use it where
        // it used the beat: 0 at the floor, 1 at a full onset above it.
        let beat_above = ((self.beat - self.beat_floor).max(0.0) / (1.0 - self.beat_floor).max(0.15)).clamp(0.0, 1.0);
        let e = &self.energy;
        format!(
            "{{\"phase\":{},\"beat\":{},\"beatAc\":{},\"level\":{},\"levelSmooth\":{},\"transient\":{},\
             \"energyLo\":[{},{},{},{}],\"energyHi\":[{},{},0,0],\"bandsLo\":[{},{},{},{}],\"bandsHi\":[{},{},{},{}]}}",
            self.phase, self.beat, beat_above, level, self.level_smooth, self.transient,
            e[0], e[1], e[2], e[3], e[4], self.spectral_peak,
            bands[0], bands[1], bands[2], bands[3], bands[4], bands[5], bands[6], bands[7],
        )
    }
}

impl crate::Chilpa {
    /// The visualizer's handle, for the window's drain.
    pub fn handle(&self) -> Visualizer {
        self.visualizer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("the pack is JSON")
    }

    #[test]
    fn the_claims_on_the_tap_add_up_and_one_letting_go_keeps_the_other() {
        let mut mask = 0;
        mask = with_bit(mask, IMMERSIVE, true);
        assert_ne!(mask, 0);
        mask = with_bit(mask, BAR, true);
        assert_eq!(mask, IMMERSIVE | BAR);
        assert_eq!(with_bit(mask, IMMERSIVE, false), BAR, "the bar's claim survives the view closing");
        assert_eq!(with_bit(with_bit(mask, IMMERSIVE, false), BAR, false), 0);
    }

    #[test]
    fn only_an_open_view_wakes_the_drain_and_only_on_what_it_draws() {
        for s in [Stream::Bars, Stream::Wave, Stream::Energy] {
            assert!(wakes(true, s), "{s:?}");
            assert!(!wakes(false, s), "{s:?}");
        }
        assert!(!wakes(true, Stream::Goniometer), "the scopes ride along");
        assert!(!wakes(true, Stream::Oscilloscope));
    }

    #[test]
    fn the_scopes_run_for_their_panel_and_stop_under_a_scene() {
        assert_eq!(scopes_for(7), GONIOMETER);
        assert_eq!(scopes_for(8), OSCILLOSCOPE);
        assert_eq!(scopes_for(3), 0);
        assert_eq!(visible_scopes(GONIOMETER, false), GONIOMETER);
        assert_eq!(visible_scopes(GONIOMETER, true), 0);
    }

    #[test]
    fn opening_claims_the_tap_and_a_view_in_focus_asks_for_its_scope() {
        let handle = Visualizer::new(Tap::new());
        assert!(!handle.enabled());
        handle.set_view(0, 7);
        assert_eq!(handle.0.controls.scopes(), 0, "nothing is asked for while closed");
        handle.open();
        assert!(handle.enabled());
        handle.set_view(0, 7);
        assert_eq!(handle.0.controls.scopes(), GONIOMETER);
        handle.set_view(1, 7);
        assert_eq!(handle.0.controls.scopes(), 0, "split draws no scope");
        handle.set_view(0, 8);
        handle.set_scene_active(true);
        assert_eq!(handle.0.controls.scopes(), 0, "a scene covers the panel");
        handle.set_scene_active(false);
        assert_eq!(handle.0.controls.scopes(), OSCILLOSCOPE);
        handle.close();
        assert!(!handle.enabled());
        assert_eq!(handle.0.controls.scopes(), 0);
    }

    #[test]
    fn the_drain_takes_the_latest_of_each_stream_and_ticks_the_pack_on_bars() {
        let handle = Visualizer::new(Tap::new());
        let latch = Latch(handle.0.clone());
        handle.0.marshal_all.store(true, Ordering::Relaxed);
        latch.submit(Frame::Bars([0.1; BARS]));
        latch.submit(Frame::Bars([0.2; BARS]));
        latch.submit(Frame::Energy([0.5; ENERGY_BANDS]));
        latch.submit(Frame::Transient(0.5));
        let out = handle.drain();
        assert_eq!(out.bars, Some([0.2; BARS]), "the latest wins");
        assert_eq!(out.energy, Some([0.5; ENERGY_BANDS]));
        assert!(out.wave.is_none());
        let doc = parsed(out.pack.as_deref().expect("a bars frame ticks the pack"));
        assert!((doc["beat"].as_f64().unwrap() - 0.44).abs() < 1e-6);
        assert!((doc["level"].as_f64().unwrap() - 0.5).abs() < 1e-6);
        latch.submit(Frame::Energy([0.1; ENERGY_BANDS]));
        let again = handle.drain();
        assert!(again.bars.is_none());
        assert!(again.pack.is_none(), "no bars, no tick");
        assert_eq!(again.energy, Some([0.1; ENERGY_BANDS]));
    }

    #[test]
    fn closed_the_drain_hands_over_nothing() {
        let handle = Visualizer::new(Tap::new());
        Latch(handle.0.clone()).submit(Frame::Bars([0.3; BARS]));
        assert_eq!(handle.drain(), Drained::default());
    }

    #[test]
    fn the_pack_pairs_the_bars_and_smooths_the_level() {
        let mut p = Pack { bars: [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 0.1, 0.3, 0.5, 0.7, 0.9, 0.2, 0.4, 0.6, 0.8, 1.0], energy: [0.1, 0.2, 0.3, 0.4, 0.5], ..Default::default() };
        let doc = parsed(&p.tick(None, None));
        assert!((doc["bandsLo"][0].as_f64().unwrap() - 0.1).abs() < 1e-6);
        assert!((doc["bandsHi"][3].as_f64().unwrap() - 0.9).abs() < 1e-6);
        assert!((doc["level"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert!((doc["levelSmooth"].as_f64().unwrap() - 0.012).abs() < 1e-6);
        let doc = parsed(&p.tick(None, None));
        assert!((doc["levelSmooth"].as_f64().unwrap() - 0.02352).abs() < 1e-5);
        assert!((doc["energyHi"][0].as_f64().unwrap() - 0.5).abs() < 1e-6, "air leads the high vector");
        assert_eq!(doc["energyHi"][2].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn the_envelopes_take_the_max_in_and_then_decay() {
        let mut p = Pack::default();
        let doc = parsed(&p.tick(Some(0.5), None));
        assert!((doc["beat"].as_f64().unwrap() - 0.44).abs() < 1e-6);
        assert!((doc["transient"].as_f64().unwrap() - 0.425).abs() < 1e-6);
        let doc = parsed(&p.tick(Some(0.1), None));
        assert!((doc["beat"].as_f64().unwrap() - 0.44 * 0.88).abs() < 1e-5, "a weaker hit does not lower it");
        assert!((doc["transient"].as_f64().unwrap() - 0.425 * 0.85).abs() < 1e-5);
    }

    #[test]
    fn the_phase_moves_forward_and_wraps_at_4096() {
        let mut p = Pack { phase: 4096.0 - 0.005, ..Default::default() };
        let doc = parsed(&p.tick(None, None));
        assert!((doc["phase"].as_f64().unwrap() - 0.007).abs() < 2e-3, "an f32 ulp at 4096 is about 5e-4");
        let mut p = Pack { energy: [0.5; 5], ..Default::default() };
        let doc = parsed(&p.tick(Some(0.5), None));
        let expected = 0.012 + 0.5 * 0.02 + 0.44 * 0.02;
        assert!((doc["phase"].as_f64().unwrap() - expected).abs() < 1e-4);
    }

    #[test]
    fn the_spectral_peak_follows_the_highest_band_with_signal() {
        let mut p = Pack::default();
        let mut bins = vec![0.0f32; 101];
        bins[75] = 0.5;
        let doc = parsed(&p.tick(None, Some(&bins)));
        assert!((doc["energyHi"][1].as_f64().unwrap() - 0.75 * 0.15).abs() < 1e-5);
        let doc = parsed(&p.tick(None, Some(&vec![0.0f32; 101])));
        assert!((doc["energyHi"][1].as_f64().unwrap() - 0.75 * 0.15 * 0.85).abs() < 1e-4, "it decays toward silence");
        let before = p.spectral_peak;
        p.tick(None, Some(&[1.0]));
        assert_eq!(p.spectral_peak, before, "one bin says nothing");
    }
}
