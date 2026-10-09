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
        type Visualizer = super::VisualizerRust;
    }

    impl cxx_qt::Threading for Visualizer {}
    impl cxx_qt::Initialize for Visualizer {}
}

use chilpa_api::visualizer::Drained;
use chilpa_qt_kit::app;
use cxx_qt::Threading as _;
use cxx_qt_lib::{QList, QString};
use std::pin::Pin;

#[derive(Default)]
pub struct VisualizerRust {
    bars: QList<f32>,
    energy: QList<f32>,
    waveform: QList<f32>,
    goniometer: QList<f32>,
    oscilloscope: QList<f32>,
    stereo_correlation: f32,
    pack_json: QString,
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
}

impl cxx_qt::Initialize for qobject::Visualizer {
    fn initialize(self: Pin<&mut Self>) {
        let handle = app::with(|api| api.visualizer());
        let thread = self.qt_thread();
        let drained = handle.clone();
        let drain = std::thread::Builder::new()
            .name("chilpa-viz-drain".into())
            .spawn(move || loop {
                if !drained.enabled() {
                    std::thread::park();
                    continue;
                }
                let out = drained.drain();
                if out != Drained::default() && thread.queue(move |this| apply(this, out)).is_err() {
                    return;
                }
                // Woken while publishing, the permit is kept: this returns
                // at once and the cells are read again.
                std::thread::park();
            })
            .expect("the visualizer's drain thread starts");
        handle.attach_drain(drain.thread().clone());
    }
}
