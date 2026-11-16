//! Delivery must not allocate or free banks while controls change.
use chilpa_audio::{dsp::{Control, Preset, Processor}, format::{SampleFormat, StreamFormat}};
use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell};

struct TrackedAllocator;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static EVENTS: Cell<usize> = const { Cell::new(0) };
}
fn record() { let _ = TRACK.try_with(|track| { if track.get() { let _ = EVENTS.try_with(|count| count.set(count.get() + 1)); } }); }
unsafe impl GlobalAlloc for TrackedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { record(); unsafe { System.alloc(layout) } }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) { record(); unsafe { System.dealloc(pointer, layout) } }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 { record(); unsafe { System.realloc(pointer, layout, size) } }
}
#[global_allocator]
static ALLOCATOR: TrackedAllocator = TrackedAllocator;
fn measured(action: impl FnOnce()) {
    EVENTS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    action();
    TRACK.with(|track| track.set(false));
    let events = EVENTS.with(Cell::get);
    assert_eq!(events, 0, "delivery allocated or freed heap memory during a DSP change");
}
#[test]
fn live_updates_and_bypass_do_not_allocate_on_delivery() {
    let control = Control::default();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, 2));
    let mut bytes: Vec<_> = vec![0.25_f32; 8192].into_iter().flat_map(f32::to_le_bytes).collect();
    control.set(Some(Preset {preamp_db:-6.0, ..Preset::default()})).unwrap();
    measured(|| processor.process(&mut bytes));
    control.set(Some(Preset {preamp_db:-9.0, ..Preset::default()})).unwrap();
    measured(|| processor.process(&mut bytes));
    control.set(None).unwrap();
    measured(|| processor.process(&mut bytes));
    measured(|| processor.process(&mut bytes));
    assert!(!control.active());
}

#[test]
fn full_processing_stages_and_changes_keep_delivery_allocation_free() {
    use chilpa_audio::dsp::{Filter, Limiter};
    let control = Control::default();
    let mut preset = Preset {auto_headroom:false, delay_left_ms:10.0, delay_right_ms:10.0,
        compressor_ratio:2.0, limiter:Limiter {enabled:true, ..Limiter::default()},
        filters:(0..64).map(|index| Filter {frequency:100.0+index as f64*100.0, gain_db:-0.1, ..Filter::default()}).collect(), ..Preset::default()};
    preset.tone.bass_db = 3.0;
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32,48_000,2));
    let mut bytes: Vec<_> = vec![0.25_f32;8192].into_iter().flat_map(f32::to_le_bytes).collect();
    control.set(Some(preset.clone())).unwrap();
    measured(|| processor.process(&mut bytes));
    preset.tone.bass_db = 4.0;
    control.set(Some(preset)).unwrap();
    measured(|| processor.process(&mut bytes));
    control.set(None).unwrap();
    measured(|| processor.process(&mut bytes));
}
