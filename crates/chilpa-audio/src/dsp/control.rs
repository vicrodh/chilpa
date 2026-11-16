//! A bounded mailbox transfers fully prepared banks to the delivery thread.
use super::{Preset, engine::Bank};
use crate::format::StreamFormat;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};

#[derive(Default)]
pub(super) struct Mailbox {
    preset: Option<Preset>,
    format: Option<StreamFormat>,
    pub stream: u64,
    pub pending: Option<Option<Bank>>,
    pub retired: Option<Bank>,
}
#[derive(Default)]
pub(super) struct Shared {
    pub mailbox: Mutex<Mailbox>,
    pub revision: AtomicU64,
    pub stream: AtomicU64,
    pub touched: AtomicBool,
    pub active_stream: AtomicU64,
    pub clipped: AtomicU64,
    pub headroom: AtomicU64,
    pub input_peak: AtomicU64,
    pub output_peak: AtomicU64,
    pub compression: AtomicU64,
    pub limiting: AtomicU64,
}

#[derive(Clone, Default)]
pub struct Control(pub(super) Arc<Shared>);
impl Control {
    pub fn begin_stream(&self) {
        self.0.touched.store(false, Ordering::Release);
        self.0.active_stream.store(0, Ordering::Release);
        self.reset_meter();
    }
    pub fn set(&self, preset: Option<Preset>) -> Result<(), String> {
        if let Some(preset) = &preset { preset.validate()?; }
        let mut mailbox = self.0.mailbox.lock().map_err(|_| chilpa_i18n::t("DSP control is unavailable."))?;
        if mailbox.stream != self.0.stream.load(Ordering::Acquire) { mailbox.format = None; mailbox.pending = None; }
        mailbox.retired = None;
        if mailbox.preset.as_ref().zip(preset.as_ref()).is_some_and(|(old, next)| old.processing_eq(next))
            || mailbox.preset.is_none() && preset.is_none() { return Ok(()); }
        mailbox.retired = None;
        // All coefficient, headroom and buffer work happens on the caller,
        // never in Processor::process. One unpublished update replaces another.
        mailbox.pending = Some(mailbox.format.and_then(|format| prepare(preset.as_ref(), format)));
        mailbox.preset = preset;
        self.0.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub(super) fn register(&self, format: StreamFormat) -> u64 {
        let mut mailbox = self.0.mailbox.lock().unwrap_or_else(|error| error.into_inner());
        mailbox.stream = self.0.stream.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        mailbox.format = Some(format);
        mailbox.pending = Some(prepare(mailbox.preset.as_ref(), format));
        mailbox.retired = None;
        self.0.revision.fetch_add(1, Ordering::Release);
        mailbox.stream
    }
    pub(super) fn unregister(&self, stream: u64) {
        let _ = self.0.active_stream.compare_exchange(stream, 0, Ordering::AcqRel, Ordering::Acquire);
        if self.0.stream.compare_exchange(stream, stream.wrapping_add(1), Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
        if let Ok(mut mailbox) = self.0.mailbox.try_lock() {
            if mailbox.stream == stream { mailbox.format = None; mailbox.pending = None; mailbox.retired = None; }
        }
    }
    /// Called on the facade thread; it frees banks retired by the renderer.
    pub fn collect(&self) {
        if let Ok(mut mailbox) = self.0.mailbox.try_lock() {
            mailbox.retired = None;
            if mailbox.stream != self.0.stream.load(Ordering::Acquire) { mailbox.format = None; mailbox.pending = None; }
        }
    }
    pub fn touched(&self) -> bool { self.0.touched.load(Ordering::Acquire) }
    pub fn active(&self) -> bool { let active = self.0.active_stream.load(Ordering::Acquire); active != 0 && active == self.0.stream.load(Ordering::Acquire) }
    pub fn clipped_samples(&self) -> u64 { self.0.clipped.load(Ordering::Relaxed) }
    pub fn applied_preamp_db(&self) -> Option<f64> { self.active().then(|| f64::from_bits(self.0.headroom.load(Ordering::Relaxed))) }
    pub fn meters(&self) -> [f64; 4] {
        [&self.0.input_peak, &self.0.output_peak, &self.0.compression, &self.0.limiting].map(|value| f64::from_bits(value.load(Ordering::Relaxed)))
    }
    pub fn reset_meter(&self) {
        for value in [&self.0.clipped, &self.0.input_peak, &self.0.output_peak, &self.0.compression, &self.0.limiting] { value.store(0, Ordering::Relaxed); }
    }
    pub fn requested(&self) -> bool {
        self.0.mailbox.lock().map(|mailbox| mailbox.preset.as_ref().is_some_and(|preset| !preset.is_identity())).unwrap_or(false)
    }
}
fn prepare(preset: Option<&Preset>, format: StreamFormat) -> Option<Bank> {
    if format.sample_format.is_dsd() || format.channels == 0 || format.channels > 32
        || !(1000..=1_536_000).contains(&format.sample_rate_hz) { return None; }
    preset.filter(|preset| !preset.is_identity()).map(|preset| Bank::new(Arc::new(preset.clone()), format))
}
