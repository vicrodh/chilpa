//! The seek bar's waveform as QML reads it: whether the player bar stands
//! tall with it, how many columns the view has room for, and the document
//! reduced to them. The pass over the track and the cache are the facade's
//! (`chilpa_api::seek_waveform`); this reads them and keeps the one choice
//! the listener makes.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qlist.h");
        type QList_f32 = cxx_qt_lib::QList<f32>;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Whether the player bar stands tall with the waveform. Kept for
        /// the next time.
        #[qproperty(bool, open, READ = open, WRITE = turn, NOTIFY = open_changed)]
        /// How many columns the view has room for; the values follow it.
        #[qproperty(i32, columns, READ = columns, WRITE = fit, NOTIFY = columns_changed)]
        /// One value per column, 0 to 1, the loudest column at 1; empty
        /// while there is no document.
        #[qproperty(QList_f32, values)]
        /// How much of the track the document covers, 0 to 1, and whether
        /// it covers all of it.
        #[qproperty(f64, analyzed)]
        #[qproperty(bool, complete)]
        type Waveform = super::WaveformRust;

        /// Reads the document again: the view asks while it grows and when
        /// the track changes.
        #[qinvokable]
        fn refresh(self: Pin<&mut Waveform>);

        #[cxx_name = "open"]
        fn open(self: &Waveform) -> bool;
        #[cxx_name = "setOpen"]
        fn turn(self: Pin<&mut Waveform>, on: bool);
        #[qsignal]
        #[cxx_name = "openChanged"]
        fn open_changed(self: Pin<&mut Waveform>);
        #[cxx_name = "columns"]
        fn columns(self: &Waveform) -> i32;
        #[cxx_name = "setColumns"]
        fn fit(self: Pin<&mut Waveform>, count: i32);
        #[qsignal]
        #[cxx_name = "columnsChanged"]
        fn columns_changed(self: Pin<&mut Waveform>);
    }

    impl cxx_qt::Initialize for Waveform {}
}

use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QList;
use std::pin::Pin;

/// The most columns a view can ask for: one per bin.
const COLUMNS_MAX: i32 = 512;

#[derive(Default)]
pub struct WaveformRust {
    open: bool,
    columns: i32,
    values: QList<f32>,
    analyzed: f64,
    complete: bool,
    /// The document's revision the values were reduced from.
    seen: u64,
}

impl qobject::Waveform {
    pub fn open(&self) -> bool {
        self.rust().open
    }

    pub fn turn(mut self: Pin<&mut Self>, on: bool) {
        if on == self.rust().open {
            return;
        }
        self.as_mut().rust_mut().open = on;
        app::with(|api| api.remember_seek_waveform_open(on));
        self.as_mut().open_changed();
    }

    pub fn columns(&self) -> i32 {
        self.rust().columns
    }

    pub fn fit(mut self: Pin<&mut Self>, count: i32) {
        let count = count.clamp(0, COLUMNS_MAX);
        if count == self.rust().columns {
            return;
        }
        self.as_mut().rust_mut().columns = count;
        self.as_mut().columns_changed();
        self.reread(true);
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.reread(false);
    }

    /// The document as it stands; the values only when it moved, or when
    /// the columns did.
    fn reread(mut self: Pin<&mut Self>, force: bool) {
        let (seen, count) = (self.rust().seen, self.rust().columns);
        let (snapshot, values) = app::with(|api| {
            let snapshot = api.seek_waveform_snapshot();
            let values = (force || snapshot.revision != seen).then(|| api.seek_waveform_columns(count.max(0) as usize));
            (snapshot, values)
        });
        if let Some(values) = values {
            let mut list = QList::<f32>::default();
            list.reserve(values.len() as isize);
            for v in values {
                list.append(v);
            }
            self.as_mut().set_values(list);
            self.as_mut().rust_mut().seen = snapshot.revision;
        }
        let analyzed = f64::from(snapshot.analyzed) / chilpa_api::seek_waveform::BINS as f64;
        self.as_mut().set_analyzed(analyzed.clamp(0.0, 1.0));
        self.as_mut().set_complete(snapshot.complete);
    }
}

impl cxx_qt::Initialize for qobject::Waveform {
    fn initialize(mut self: Pin<&mut Self>) {
        let open = app::with(|api| api.seek_waveform_open());
        self.as_mut().rust_mut().open = open;
    }
}
