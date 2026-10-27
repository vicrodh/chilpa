//! The visualizer's streams as QML reads them: the latest bars, energy,
//! waveform, goniometer and oscilloscope, and the pack the scenes read.
//! A thread of this bridge parks until the facade wakes it for a frame
//! the open view draws, takes what waits and hops it onto the window's
//! thread: one publish per produced frame, no timer, nothing while the
//! view is closed.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qlist.h");
        type QList_f32 = cxx_qt_lib::QList<f32>;
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Sixteen bars, 0 to 1.
        #[qproperty(QList_f32, bars)]
        /// Five energy bands, 0 to 1.
        #[qproperty(QList_f32, energy)]
        /// 256 left samples, then 256 right.
        #[qproperty(QList_f32, waveform)]
        /// 256 side/mid pairs.
        #[qproperty(QList_f32, goniometer)]
        /// 512 mono samples locked to the pitch.
        #[qproperty(QList_f32, oscilloscope)]
        /// The correlation of the two channels, -1 to 1.
        #[qproperty(f32, stereo_correlation)]
        /// The scenes' uniforms as one JSON object, once per frame.
        #[qproperty(QString, pack_json)]
        #[qproperty(QByteArray, linebed_heights)]
        #[qproperty(QByteArray, ribbon_frame)]
        #[qproperty(u32, sample_rate_hz)]
        #[qproperty(i32, bit_depth)]
        type Visualizer = super::VisualizerRust;

        #[qinvokable]
        fn set_presented(self: &Visualizer, presented: bool);
        #[qinvokable]
        fn set_compact(self: &Visualizer, presented: bool, mode: i32);
        #[qinvokable]
        fn set_cursor(self: &Visualizer, track: &QString, position: f64, duration: f64);
        #[qinvokable]
        fn ribbon_snapshot(self: &Visualizer) -> QByteArray;
    }

    impl cxx_qt::Threading for Visualizer {}
    impl cxx_qt::Initialize for Visualizer {}
}

use chilpa_api::visualizer::Drained;
use chilpa_qt_kit::app;
use cxx_qt::Threading as _;
use cxx_qt_lib::{QByteArray, QList, QString};
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Default)]
pub struct VisualizerRust {
    bars: QList<f32>,
    energy: QList<f32>,
    waveform: QList<f32>,
    goniometer: QList<f32>,
    oscilloscope: QList<f32>,
    stereo_correlation: f32,
    pack_json: QString,
    linebed_heights: QByteArray,
    ribbon_frame: QByteArray,
    sample_rate_hz: u32,
    bit_depth: i32,
}

fn list_of(values: &[f32]) -> QList<f32> {
    let mut list = QList::<f32>::default();
    list.reserve(values.len() as isize);
    for v in values {
        list.append(*v);
    }
    list
}

fn apply(mut this: Pin<&mut qobject::Visualizer>, out: Drained) {
    let (rate, bits) = app::with(|api| {
        let quality = &api.now_playing().quality;
        (quality.rate_hz.unwrap_or(0), i32::from(quality.bits.unwrap_or(0)))
    });
    this.as_mut().set_sample_rate_hz(rate);
    this.as_mut().set_bit_depth(bits);
    if let Some(bars) = out.bars {
        this.as_mut().set_bars(list_of(&bars));
    }
    if let Some(energy) = out.energy {
        this.as_mut().set_energy(list_of(&energy));
    }
    if let Some(wave) = out.wave {
        this.as_mut().set_waveform(list_of(wave.as_ref()));
    }
    if let Some((points, correlation)) = out.goniometer {
        this.as_mut().set_goniometer(list_of(points.as_ref()));
        this.as_mut().set_stereo_correlation(correlation);
    }
    if let Some(points) = out.oscilloscope {
        this.as_mut().set_oscilloscope(list_of(points.as_ref()));
    }
    if let Some(pack) = out.pack {
        this.as_mut().set_pack_json(QString::from(&pack));
    }
    if let Some(bytes) = out.linebed {
        this.as_mut().set_linebed_heights(QByteArray::from(bytes.as_slice()));
    }
    if let Some(bytes) = out.ribbon {
        this.as_mut().set_ribbon_frame(QByteArray::from(bytes.as_slice()));
    }
}

impl qobject::Visualizer {
    pub fn set_presented(&self, presented: bool) {
        app::with(|api| api.visualizer()).set_presented(presented);
    }

    pub fn set_compact(&self, presented: bool, mode: i32) {
        app::with(|api| api.visualizer()).set_compact(presented, mode);
    }

    pub fn set_cursor(&self, track: &QString, position: f64, duration: f64) {
        app::with(|api| api.visualizer()).set_cursor(&track.to_string(), position, duration);
    }

    pub fn ribbon_snapshot(&self) -> QByteArray {
        app::with(|api| api.visualizer()).ribbon_snapshot().map(|bytes| QByteArray::from(bytes.as_slice())).unwrap_or_default()
    }
}

impl cxx_qt::Initialize for qobject::Visualizer {
    fn initialize(self: Pin<&mut Self>) {
        let handle = app::with(|api| api.visualizer());
        let thread = self.qt_thread();
        let drained = handle.clone();
        let pending = Arc::new(AtomicBool::new(false));
        let drain = std::thread::Builder::new()
            .name("chilpa-viz-drain".into())
            .spawn(move || loop {
                if !drained.enabled() || pending.load(Ordering::Acquire) {
                    std::thread::park();
                    continue;
                }
                let out = drained.drain();
                if out != Drained::default() {
                    // I keep one Qt callback in flight. While the GUI is busy,
                    // new frames overwrite the facade cells instead of queuing.
                    pending.store(true, Ordering::Release);
                    let applied = pending.clone();
                    let worker = std::thread::current();
                    if thread.queue(move |this| {
                        apply(this, out);
                        applied.store(false, Ordering::Release);
                        worker.unpark();
                    }).is_err() { return; }
                }
                // Woken while publishing, the permit is kept: this returns
                // at once and the cells are read again.
                std::thread::park();
            })
            .expect("the visualizer's drain thread starts");
        handle.attach_drain(drain.thread().clone());
    }
}
