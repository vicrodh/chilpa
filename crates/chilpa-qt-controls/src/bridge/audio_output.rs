//! I adapt the facade's revisioned output document to one shared panel.
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
        #[qproperty(QString, snapshot)]
        #[qproperty(QString, tab)]
        #[qproperty(bool, shown)]
        type AudioOutput = super::AudioOutputRust;
        #[qinvokable] fn show(self: Pin<&mut AudioOutput>, tab: &QString);
        #[qinvokable] fn close(self: Pin<&mut AudioOutput>);
        #[qinvokable] fn refresh(self: Pin<&mut AudioOutput>);
        #[qinvokable] fn command(self: Pin<&mut AudioOutput>, action: &QString, argument: &QString);
        #[qinvokable] fn take_request(self: Pin<&mut AudioOutput>);
        #[qinvokable] fn signal_path(self: &AudioOutput) -> QString;
        /// The audio settings are shown, or no longer: the listing keeps
        /// itself current while they are.
        #[qinvokable] fn watch_outputs(self: Pin<&mut AudioOutput>, shown: bool);
        #[qsignal] fn output_selected(self: Pin<&mut AudioOutput>);
        /// The stacks and outputs as listed changed: they first answered,
        /// or a card came or went. Whoever draws them reads them again.
        #[qsignal] fn outputs_listed(self: Pin<&mut AudioOutput>);
    }
    impl cxx_qt::Threading for AudioOutput {}
    impl cxx_qt::Initialize for AudioOutput {}
}
use cxx_qt::{CxxQtType, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;
static THREAD: std::sync::OnceLock<cxx_qt::CxxQtThread<qobject::AudioOutput>> = std::sync::OnceLock::new();
static LISTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The facade's outputs revision, published from the transport heartbeat
/// like the queue's activity. True when it moved, and the panel then tells
/// whoever draws the lists; the window never polls the lists themselves.
pub fn publish(revision: u64) -> bool {
    if LISTED.swap(revision, std::sync::atomic::Ordering::Relaxed) == revision { return false; }
    if let Some(thread) = THREAD.get() { let _ = thread.queue(|panel| panel.outputs_listed()); }
    true
}
impl cxx_qt::Initialize for qobject::AudioOutput {
    fn initialize(self: Pin<&mut Self>) { let _ = THREAD.set(self.qt_thread()); }
}
#[derive(Default)]
pub struct AudioOutputRust { snapshot:QString,tab:QString,shown:bool,revision:u64 }
impl qobject::AudioOutput {
    pub fn signal_path(&self) -> QString {
        QString::from(&chilpa_qt_kit::app::with(|api| api.audio_signal_path()))
    }
    pub fn watch_outputs(self: Pin<&mut Self>, shown: bool) {
        chilpa_qt_kit::app::with(|api| api.watch_outputs(shown));
    }
    pub fn show(mut self: Pin<&mut Self>, tab: &QString) {
        self.as_mut().set_tab(tab.clone());
        chilpa_qt_kit::app::with(|api| api.audio_output_command("open",""));
        self.as_mut().set_shown(true);
        self.refresh();
    }
    pub fn close(mut self: Pin<&mut Self>) {
        chilpa_qt_kit::app::with(|api| api.audio_output_command("close",""));
        self.as_mut().set_shown(false); self.as_mut().set_snapshot(QString::from("{}"));
        self.as_mut().rust_mut().revision = 0;
    }
    pub fn command(mut self: Pin<&mut Self>, action: &QString, argument: &QString) {
        let accepted = chilpa_qt_kit::app::with(|api| api.audio_output_command(&action.to_string(),&argument.to_string()));
        if accepted && action.to_string() == "use" { self.as_mut().output_selected(); }
        self.refresh();
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        if !self.shown { return; }
        let seen = self.revision;
        if let Some((revision,document)) = chilpa_qt_kit::app::with(|api| api.audio_output_doc(seen)) {
            self.as_mut().rust_mut().revision = revision;
            self.set_snapshot(QString::from(&document));
        }
    }
    pub fn take_request(mut self: Pin<&mut Self>) {
        if chilpa_qt_kit::app::with(|api| api.take_audio_output_request()) { self.as_mut().show(&QString::from("equipment")); }
    }
}
