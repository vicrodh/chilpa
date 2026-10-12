//! Playing through a Mac's own audio.
//!
//! An output unit bound to the chosen device, handed floats at the track's
//! rate. The unit converts whatever the device needs: the rate, when the
//! device runs at another one, and the floats into the integers the wire
//! carries. This is how QBZ plays on a Mac, and it is what works on every
//! output this was tried on.
//!
//! Two things make that conversion keep every sample:
//!
//! 1. **The device runs at the track's rate**, when it lists that rate. Then
//!    the unit has no rate to convert, and a float made from an integer of
//!    up to 24 bits goes back into the wire's integer unchanged. It is asked
//!    for whether the output is shared or not, as QBZ's first Mac port did:
//!    a shared device converts whatever else plays, not the track.
//! 2. **The device is this program's.** Hog mode, in exclusive mode only.
//!    Without it CoreAudio mixes whatever else is playing into the same
//!    stream, and the verdict says so.
//!
//! Integer stream formats are not asked for. The version before this set
//! the physical and virtual formats to integers and refused whatever came
//! back different: a Mac's own speakers carry nothing but floats, and a USB
//! DAC offers integers on the program's side only in its non-mixable
//! variant. Every track was skipped on the first and failed after two
//! seconds on the second.

use crate::ffi::*;
use crate::read;
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::sink::{AudioSink, SinkError};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Samples waiting for the device, as floats.
///
/// The writer fills it and the device's thread empties it, and neither ever
/// waits for the other: a lock the device's thread has to take is a period
/// of silence every time the writer happens to hold it, and that is a click.
/// One writer and one reader, so two counters are enough.
struct Ring {
    slots: Box<[AtomicU32]>,
    /// Samples ever written. Only the writer moves it.
    head: AtomicUsize,
    /// Samples ever taken. Only the device's thread moves it.
    tail: AtomicUsize,
    /// Where a flush left things: nothing before it is played. The writer
    /// cannot move the reader's counter, so it moves this one instead.
    floor: AtomicUsize,
}

impl Ring {
    fn new(capacity: usize) -> Self {
        Ring {
            slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            floor: AtomicUsize::new(0),
        }
    }

    fn start(&self) -> usize {
        self.tail.load(Ordering::Acquire).max(self.floor.load(Ordering::Acquire))
    }

    /// Samples written and not played yet.
    fn held(&self) -> usize {
        self.head.load(Ordering::Acquire).saturating_sub(self.start())
    }

    fn free(&self) -> usize {
        self.slots.len().saturating_sub(self.held())
    }

    /// Writer's side. The caller has checked there is room.
    fn push(&self, samples: impl Iterator<Item = f32>) {
        let mut head = self.head.load(Ordering::Relaxed);
        for s in samples {
            self.slots[head % self.slots.len()].store(s.to_bits(), Ordering::Relaxed);
            head += 1;
        }
        self.head.store(head, Ordering::Release);
    }

    /// Device's side: fills `out` with what there is and silence after it.
    /// Says how many were real samples.
    fn pop_into(&self, out: &mut [f32]) -> usize {
        let start = self.start();
        let ready = self.head.load(Ordering::Acquire).saturating_sub(start).min(out.len());
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = if i < ready {
                f32::from_bits(self.slots[(start + i) % self.slots.len()].load(Ordering::Relaxed))
            } else {
                // Running dry writes silence rather than the last period
                // again, which is what a repeated buffer sounds like: a click.
                0.0
            };
        }
        self.tail.store(start + ready, Ordering::Release);
        ready
    }

    /// Writer's side: everything written so far is not to be played.
    fn clear(&self) {
        self.floor.store(self.head.load(Ordering::Relaxed), Ordering::Release);
    }
}

/// A sample of the stream's format, as the float the unit is handed.
///
/// Exact for every integer of up to 24 bits: a float carries 24 bits of
/// mantissa, and dividing by a power of two only moves the exponent. A
/// 24-bit word left-justified in 32 has its low byte empty, so it is exact
/// too. A full 32-bit integer is rounded to 24 bits here, and the verdict
/// already says no for it: no wire on a Mac carries more than 24.
fn to_float(format: SampleFormat, bytes: &[u8]) -> f32 {
    match format {
        SampleFormat::S16 => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0,
        SampleFormat::S24Packed => {
            // Into the top three bytes of a word, then back down with the sign.
            let v = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
            v as f32 / 8_388_608.0
        }
        SampleFormat::S24In32 | SampleFormat::S32 => {
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0
        }
        SampleFormat::F32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        SampleFormat::DsdU32Be | SampleFormat::DsdU32Le => unreachable!("refused when opening"),
    }
}

/// What the unit is handed: interleaved floats at the track's rate.
fn float_stream(stream: StreamFormat) -> AudioStreamBasicDescription {
    let channels = u32::from(stream.channels);
    AudioStreamBasicDescription {
        mSampleRate: f64::from(stream.sample_rate_hz),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4 * channels,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4 * channels,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

/// The ring, and what the device's thread needs to read from it. Owned by
/// the sink and pointed at by the unit for as long as it may call.
struct Shared {
    ring: Ring,
}

/// What the unit asks for, every period, on the device's thread.
///
/// It must not block and it does not: it takes what is ready and pads the
/// rest with silence.
extern "C" fn render(
    client: *mut c_void,
    flags: *mut u32,
    _time: *const AudioTimeStamp,
    _bus: u32,
    _frames: u32,
    data: *mut AudioBufferList,
) -> OSStatus {
    if data.is_null() || client.is_null() {
        return 0;
    }
    let shared = unsafe { &*(client as *const Shared) };
    let list = unsafe { &mut *data };
    // The format set on the unit is interleaved, so this is one buffer; it is
    // walked as a list anyway because that is what the unit hands over.
    let buffers = unsafe {
        std::slice::from_raw_parts_mut(list.mBuffers.as_mut_ptr(), list.mNumberBuffers as usize)
    };
    let mut real = 0;
    for buffer in buffers.iter_mut() {
        if buffer.mData.is_null() {
            continue;
        }
        let out = unsafe {
            std::slice::from_raw_parts_mut(buffer.mData as *mut f32, buffer.mDataByteSize as usize / 4)
        };
        real += shared.ring.pop_into(out);
    }
    if real == 0 && !flags.is_null() {
        unsafe { *flags |= kAudioUnitRenderAction_OutputIsSilence };
    }
    0
}

fn set<T>(object: AudioObjectID, address: &AudioObjectPropertyAddress, value: &T) -> Result<(), OSStatus> {
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            value as *const T as *const c_void,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Writes one of a device's output properties, on one element.
///
/// The element matters: a device whose volume lives on its channels has
/// nothing at the master, and writing there succeeds at nothing.
pub(crate) fn write_element<T>(
    object: AudioObjectID,
    which: AudioObjectPropertySelector,
    element: AudioObjectPropertyElement,
    value: &T,
) -> Result<(), OSStatus> {
    set(object, &output_element(which, element), value)
}

fn set_unit<T>(unit: AudioUnit, which: u32, scope: u32, value: &T) -> Result<(), OSStatus> {
    let status = unsafe {
        AudioUnitSetProperty(
            unit,
            which,
            scope,
            0,
            value as *const T as *const c_void,
            std::mem::size_of::<T>() as u32,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Waits for a device to really be at that rate.
///
/// Setting it is a request, not a change: CoreAudio hands the ask to the
/// driver and returns, and a DAC re-locking its clock takes a moment. Two
/// seconds is longer than any DAC takes and short enough that one that is
/// never going to switch does not hang the thread that asked.
fn settles_at(device: AudioObjectID, rate: f64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if (read::current_rate(device) - rate).abs() <= 0.5 {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// Whether the device lists that rate as one it runs at.
pub(crate) fn runs_at(device: AudioObjectID, rate: f64) -> bool {
    read::available_rates(device)
        .iter()
        .any(|r| rate >= r.mMinimum - 0.5 && rate <= r.mMaximum + 0.5)
}

/// Takes the device for this program, or says who has it.
fn hog(device: AudioObjectID, name: &str) -> Result<(), SinkError> {
    let mine = std::process::id() as i32;
    let holder = read::hogged_by(device);
    if holder == mine {
        return Ok(());
    }
    if holder != kAudioHogModeNobody {
        return Err(SinkError::Backend(chilpa_i18n::t_args(
            "another program has \"{}\" to itself; close it, or turn exclusive mode off to share it",
            &[name],
        )));
    }
    // Failing to take it is an error and not a shrug: a track played into a
    // shared device while the window said bit-perfect is the one claim this
    // project exists to stop making.
    set(device, &global(kAudioDevicePropertyHogMode), &mine).map_err(|s| {
        SinkError::Backend(chilpa_i18n::t_args(
            "\"{}\" would not be given to Chilpa alone ({}); turn exclusive mode off to share it",
            &[name, &s.to_string()],
        ))
    })?;
    // And read back, because asking is not having.
    if read::hogged_by(device) != mine {
        return Err(SinkError::Backend(chilpa_i18n::t_args(
            "\"{}\" would not be given to Chilpa alone; turn exclusive mode off to share it",
            &[name],
        )));
    }
    Ok(())
}

/// Gives the device back, if this program has it.
///
/// Not optional: a hogged device stays unusable by every other program until
/// whoever took it lets go, and a player that forgets leaves the Mac silent.
fn give_back(device: AudioObjectID) {
    if read::hogged_by(device) == std::process::id() as i32 {
        let _ = set(device, &global(kAudioDevicePropertyHogMode), &kAudioHogModeNobody);
    }
}

/// A device held in exclusive mode, and the volume it had before.
///
/// QBZ's `CoreAudioExclusiveGuard` (`qbz-audio/src/coreaudio_direct.rs`):
/// the device's own volume is read before anything is touched, the player
/// bar moves it while the device is held, and it is put back before the
/// device is given back -- while it is still this program's, so nobody else
/// can move it in between. A device with no volume of its own has nothing
/// to put back.
struct Hold {
    device: AudioObjectID,
    volume: Option<f32>,
}

/// A hold a track left for the next one, with the turn it was parked on: a
/// timer only gives back the one it was started for.
static PARKED: Mutex<Option<(u64, Hold)>> = Mutex::new(None);
static TURN: AtomicU64 = AtomicU64::new(0);

/// How long a parked hold waits for the next track, as a parked card does on
/// Linux (`chilpa-alsa/src/reserve.rs`, `PATIENCE`).
const PATIENCE: Duration = Duration::from_secs(3);

impl Hold {
    /// The device for this program: the hold the last track parked, or a new
    /// one. Claiming the parked one is what keeps the volume read before the
    /// first track, rather than the one Chilpa set since.
    fn take(device: AudioObjectID, name: &str) -> Result<Self, SinkError> {
        let parked = PARKED.lock().ok().and_then(|mut p| p.take());
        match parked {
            Some((_, hold)) if hold.device == device && read::hogged_by(device) == std::process::id() as i32 => {
                return Ok(hold)
            }
            // Parked for another device, or lost meanwhile: it goes back now.
            Some((_, other)) => drop(other),
            None => {}
        }
        let volume = read::volume(device);
        hog(device, name)?;
        Ok(Hold { device, volume })
    }

    /// Keeps the device for the track that follows. If none comes, it goes
    /// back by itself.
    fn park(self) {
        let turn = TURN.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut parked) = PARKED.lock() {
            // Whatever was parked before goes back first.
            drop(parked.replace((turn, self)));
        }
        std::thread::spawn(move || {
            std::thread::sleep(PATIENCE);
            let expired = PARKED
                .lock()
                .ok()
                .and_then(|mut p| if p.as_ref().is_some_and(|(t, _)| *t == turn) { p.take() } else { None });
            drop(expired);
        });
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(level) = self.volume {
            for element in read::movable_volume(self.device) {
                let _ = write_element(self.device, kAudioDevicePropertyVolumeScalar, element, &level);
            }
        }
        give_back(self.device);
    }
}

/// Sets the device to the track's rate, when it has that rate.
///
/// Asked for, never required, as QBZ asks (`switch_sample_rate_if_needed`):
/// a device that does not list the rate, refuses it or does not settle at
/// it in time is left to the unit to convert. The track plays, and the
/// verdict reads the device and says it was converted.
fn match_rate(device: AudioObjectID, rate_hz: u32) {
    let rate = f64::from(rate_hz);
    if (read::current_rate(device) - rate).abs() <= 0.5 || !runs_at(device, rate) {
        return;
    }
    let outcome = match set(device, &global(kAudioDevicePropertyNominalSampleRate), &rate) {
        Err(status) => format!("refused ({status})"),
        Ok(()) if !settles_at(device, rate) => "did not settle".to_string(),
        Ok(()) => return,
    };
    chilpa_audio::diagnostics::trace(
        "coreaudio",
        "rate_not_switched",
        format_args!("device={device} rate={rate_hz} outcome={outcome}"),
    );
}

/// The widest integer word the wire takes at the rate the device is at.
///
/// A device keeps the word it was last left at. The Macaron sat at 16 bits,
/// so a 24-bit track was cut to 16 on the way out, and DoP -- whose markers
/// live in the top bits of a 24-bit word -- would have reached the DAC as
/// noise. QBZ never sets it; here it is asked for like the rate, read back,
/// and never required: the verdict reads the wire as it ended up.
fn widen_wire(device: AudioObjectID) {
    let rate = read::current_rate(device);
    let integer = |f: &AudioStreamBasicDescription| {
        f.mFormatID == kAudioFormatLinearPCM
            && f.mFormatFlags & kAudioFormatFlagIsFloat == 0
            && f.mFormatFlags & kAudioFormatFlagIsSignedInteger != 0
    };
    for stream in read::output_streams(device) {
        let widest = read::available_physical_formats(stream)
            .into_iter()
            // A format published with no rate of its own takes any.
            .filter(|f| integer(f) && (f.mSampleRate == 0.0 || (f.mSampleRate - rate).abs() < 0.5))
            .max_by_key(|f| (f.mBitsPerChannel, f.mFormatFlags & kAudioFormatFlagIsNonMixable == 0));
        let Some(mut widest) = widest else { continue };
        widest.mSampleRate = rate;
        let now = read::format(stream, kAudioStreamPropertyPhysicalFormat);
        if now.is_some_and(|f| integer(&f) && f.mBitsPerChannel >= widest.mBitsPerChannel) {
            continue;
        }
        let outcome = match set(stream, &global(kAudioStreamPropertyPhysicalFormat), &widest) {
            Err(status) => format!("refused ({status})"),
            Ok(()) if !wire_settles(stream, widest.mBitsPerChannel) => "did not settle".to_string(),
            Ok(()) => continue,
        };
        chilpa_audio::diagnostics::trace(
            "coreaudio",
            "wire_not_widened",
            format_args!("stream={stream} bits={} outcome={outcome}", widest.mBitsPerChannel),
        );
    }
}

/// Waits for a stream's wire to carry at least that many bits: the same
/// patience as the rate, for the same reason.
fn wire_settles(stream: AudioObjectID, bits: u32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if read::format(stream, kAudioStreamPropertyPhysicalFormat).is_some_and(|f| f.mBitsPerChannel >= bits) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// The output unit, bound to one device and started. Torn down on drop.
struct Unit(AudioUnit);

// The unit is an opaque handle whose calls may be made from any thread, and
// one sink owns it: it moves to the thread that plays, and is never shared.
unsafe impl Send for Unit {}

impl Unit {
    fn open(device: AudioObjectID, name: &str, stream: StreamFormat, shared: &Shared) -> Result<Self, SinkError> {
        let fail = |what: &str, status: OSStatus| {
            SinkError::Backend(chilpa_i18n::t_args(
                "\"{}\" would not open ({} {})",
                &[name, what, &status.to_string()],
            ))
        };
        let wanted = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_HALOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        let component = unsafe { AudioComponentFindNext(std::ptr::null_mut(), &wanted) };
        if component.is_null() {
            return Err(fail("AudioComponentFindNext", -1));
        }
        let mut raw: AudioUnit = std::ptr::null_mut();
        let status = unsafe { AudioComponentInstanceNew(component, &mut raw) };
        if status != 0 || raw.is_null() {
            return Err(fail("AudioComponentInstanceNew", status));
        }
        // From here on, dropping it disposes of it.
        let unit = Unit(raw);
        set_unit(unit.0, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, &device)
            .map_err(|s| fail("CurrentDevice", s))?;
        set_unit(unit.0, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, &float_stream(stream))
            .map_err(|s| fail("StreamFormat", s))?;
        let callback = AURenderCallbackStruct {
            inputProc: render,
            inputProcClientData: shared as *const Shared as *mut c_void,
        };
        set_unit(unit.0, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, &callback)
            .map_err(|s| fail("SetRenderCallback", s))?;
        let status = unsafe { AudioUnitInitialize(unit.0) };
        if status != 0 {
            return Err(fail("AudioUnitInitialize", status));
        }
        unit.start(name)?;
        Ok(unit)
    }

    fn start(&self, name: &str) -> Result<(), SinkError> {
        let status = unsafe { AudioOutputUnitStart(self.0) };
        if status != 0 {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "\"{}\" would not start ({})",
                &[name, &status.to_string()],
            )));
        }
        Ok(())
    }

    fn stop(&self) {
        unsafe { AudioOutputUnitStop(self.0) };
    }
}

impl Drop for Unit {
    fn drop(&mut self) {
        unsafe {
            AudioOutputUnitStop(self.0);
            AudioUnitUninitialize(self.0);
            AudioComponentInstanceDispose(self.0);
        }
    }
}

/// A stream to a Mac's output device.
pub struct CoreAudioSink {
    device: AudioObjectID,
    name: String,
    stream: StreamFormat,
    exclusive: bool,
    /// Declared before `shared` so it is torn down first: the unit stops
    /// calling before what it points at goes.
    unit: Option<Unit>,
    /// Pointed at by the unit for as long as it may call.
    shared: Box<Shared>,
    /// The device held, in exclusive mode.
    hold: Option<Hold>,
    /// Whether the device was let go in a long pause.
    suspended: bool,
}

impl CoreAudioSink {
    /// Opens a device for that format.
    pub fn open(name: &str, stream: StreamFormat, exclusive: bool) -> Result<Self, SinkError> {
        let Some(device) = crate::find(name) else {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "this Mac has no output called \"{}\" any more",
                &[name],
            )));
        };
        if stream.sample_format.is_dsd() {
            return Err(SinkError::Backend(chilpa_i18n::t(
                "a Mac's outputs take PCM, not native DSD",
            )));
        }
        let shared = Box::new(Shared {
            // A quarter of a second: long enough to ride out a scheduling
            // hiccup, short enough that stopping is not heard as a tail.
            ring: Ring::new((stream.sample_rate_hz as usize / 4).max(4_096) * usize::from(stream.channels)),
        });
        let mut sink = CoreAudioSink {
            device,
            name: name.to_string(),
            stream,
            exclusive,
            unit: None,
            shared,
            hold: None,
            suspended: false,
        };
        // On failure the sink is dropped, and dropping gives the device back.
        sink.take()?;
        Ok(sink)
    }

    /// Takes the device as asked and starts the unit on it.
    fn take(&mut self) -> Result<(), SinkError> {
        if self.exclusive {
            // The device first, in exclusive mode, so nothing else is
            // playing through it when its rate moves.
            self.hold = Some(Hold::take(self.device, &self.name)?);
        }
        match_rate(self.device, self.stream.sample_rate_hz);
        widen_wire(self.device);
        self.unit = Some(Unit::open(self.device, &self.name, self.stream, &self.shared)?);
        Ok(())
    }

    /// Lets the device go: the unit stops and is disposed of, and a held
    /// device gets its volume back and is given back.
    fn release(&mut self) {
        self.unit = None;
        self.hold = None;
    }
}

impl AudioSink for CoreAudioSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.unit.is_none() {
            return Err(SinkError::Closed);
        }
        let channels = usize::from(self.stream.channels).max(1);
        let frame_bytes = self.stream.frame_bytes().max(1);
        let room = self.shared.ring.free() / channels;
        let take = room.min(frames.len() / frame_bytes);
        if take == 0 {
            // Full. A device that was unplugged stops asking, and full is
            // what that looks like from here; one that only paused is
            // waited on by the renderer.
            if read::gone(self.device) {
                return Err(SinkError::DeviceLost);
            }
            return Ok(0);
        }
        let width = self.stream.sample_format.bytes();
        let format = self.stream.sample_format;
        self.shared
            .ring
            .push(frames[..take * frame_bytes].chunks_exact(width).map(|s| to_float(format, s)));
        Ok(take)
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        self.shared.ring.clear();
        Ok(())
    }

    fn held(&self) -> u64 {
        (self.shared.ring.held() / usize::from(self.stream.channels).max(1)) as u64
    }

    /// Waits for the device to take what is queued: a quarter of a second
    /// at most, and a second more before giving up on one that stopped.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let limit = std::time::Instant::now() + std::time::Duration::from_millis(1_250);
        while std::time::Instant::now() < limit && !give_up() {
            if self.shared.ring.held() == 0 {
                break;
            }
            if read::gone(self.device) {
                return Err(SinkError::DeviceLost);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        // Stopping the unit keeps what it holds for when it starts again,
        // instead of playing it out as the last of the music before silence.
        match (&self.unit, paused) {
            (Some(unit), true) => unit.stop(),
            (Some(unit), false) => unit.start(&self.name)?,
            (None, _) => {}
        }
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), SinkError> {
        self.shared.ring.clear();
        self.release();
        self.suspended = true;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), SinkError> {
        if !self.suspended {
            return Ok(());
        }
        // As strictly as the first time: somebody else may have the device
        // now, and that is said rather than played around.
        self.take()?;
        self.suspended = false;
        Ok(())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.release();
        Ok(())
    }

    /// Between two tracks that cannot share a stream: the unit goes, and the
    /// device stays held for the next one, with the volume it had before the
    /// first.
    fn handoff(&mut self) -> Result<(), SinkError> {
        self.unit = None;
        if let Some(hold) = self.hold.take() {
            hold.park();
        }
        Ok(())
    }
}

impl Drop for CoreAudioSink {
    fn drop(&mut self) {
        // Whatever path got here, the device goes back.
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_of_up_to_24_bits_become_floats_that_come_back_the_same() {
        // What the device does with a float on a 24-bit wire is multiply by
        // 2^23 and round. For that to give back the sample, the float has to
        // be the sample over 2^23 exactly.
        for v in [-8_388_608i32, -1, 0, 1, 12_345, 8_388_607] {
            let in32 = (v << 8).to_le_bytes();
            let f = to_float(SampleFormat::S24In32, &in32);
            assert_eq!((f64::from(f) * 8_388_608.0) as i32, v, "24 in 32: {v}");
            let packed = &v.to_le_bytes()[..3];
            let g = to_float(SampleFormat::S24Packed, packed);
            assert_eq!(f, g, "packed and in 32 are the same sample: {v}");
        }
        for v in [i16::MIN, -1, 0, 1, i16::MAX] {
            let f = to_float(SampleFormat::S16, &v.to_le_bytes());
            assert_eq!((f64::from(f) * 32_768.0) as i16, v);
        }
    }

    #[test]
    fn the_ring_plays_what_was_written_in_order_and_pads_with_silence() {
        let ring = Ring::new(8);
        ring.push([1.0, 2.0, 3.0].into_iter());
        assert_eq!(ring.held(), 3);
        let mut out = [9.0f32; 4];
        assert_eq!(ring.pop_into(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0, 0.0]);
        assert_eq!(ring.held(), 0);
        assert_eq!(ring.free(), 8);
    }

    #[test]
    fn the_ring_wraps_and_a_flush_drops_what_was_waiting() {
        let ring = Ring::new(4);
        ring.push([1.0, 2.0, 3.0].into_iter());
        let mut two = [0.0f32; 2];
        ring.pop_into(&mut two);
        ring.push([4.0, 5.0, 6.0].into_iter());
        assert_eq!(ring.free(), 0);
        let mut all = [0.0f32; 4];
        assert_eq!(ring.pop_into(&mut all), 4);
        assert_eq!(all, [3.0, 4.0, 5.0, 6.0], "across the end of the slots");

        ring.push([7.0, 8.0].into_iter());
        ring.clear();
        assert_eq!(ring.held(), 0, "a flush leaves nothing held");
        ring.push([9.0].into_iter());
        let mut one = [0.0f32; 2];
        assert_eq!(ring.pop_into(&mut one), 1);
        assert_eq!(one, [9.0, 0.0], "what was flushed is never played");
    }

    /// The device's thread takes what was written: what is held goes down
    /// without anything more being written. Silence only.
    fn pulls(sink: &mut CoreAudioSink, label: &str) {
        let silence = vec![0u8; sink.format().frame_bytes() * 2_048];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            match sink.write(&silence) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => panic!("{label}: writing silence failed: {e}"),
            }
        }
        let full = sink.held();
        assert!(full > 0, "{label}: nothing was taken");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let later = sink.held();
        assert!(later < full, "{label}: the device took nothing in 300 ms ({full} held, then {later})");
    }

    /// Opens every output of this Mac with silence, shared and alone, and
    /// reads back what the device did.
    ///
    /// Ignored by default: it takes each device for a moment and re-clocks
    /// DACs. Nothing it writes is anything but zeros, and a device another
    /// program is playing through is skipped. Run it on purpose:
    /// `cargo test -p chilpa-coreaudio --lib -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore = "takes every output of this Mac for a moment; run it on purpose"]
    fn every_output_plays_silence_shared_and_alone_and_says_what_it_kept() {
        use chilpa_audio::backend::OutputBackend;
        let mine = std::process::id() as i32;
        let outputs = crate::CoreAudio.outputs();
        assert!(!outputs.is_empty(), "this Mac lists no output");
        for output in outputs {
            let name = output.id.clone();
            let device = crate::find(&name).expect("listed, so found");
            if read::running_somewhere(device) {
                eprintln!("{name}: another program is playing through it, skipped");
                continue;
            }
            let before = read::current_rate(device);

            // Shared: the device is nobody's, it still moves to the track's
            // rate when it has it, and shared is never untouched.
            let cd = StreamFormat::new(SampleFormat::S16, 44_100, 2);
            let mut sink = CoreAudioSink::open(&name, cd, false)
                .unwrap_or_else(|e| panic!("{name}: shared open failed: {e}"));
            assert_ne!(read::hogged_by(device), mine, "{name}: sharing took the device");
            pulls(&mut sink, &format!("{name} shared"));
            if runs_at(device, 44_100.0) {
                assert!((read::current_rate(device) - 44_100.0).abs() < 0.5, "{name}: shared did not move to the track's rate");
            }
            assert!(!crate::untouched(&name, 44_100, Some(16)), "{name}: shared is never untouched");
            drop(sink);

            for rate in [44_100u32, 48_000, 96_000, 176_400] {
                let label = format!("{name} alone at {rate}");
                let format = StreamFormat::new(SampleFormat::S24In32, rate, 2);
                let mut sink = CoreAudioSink::open(&name, format, true)
                    .unwrap_or_else(|e| panic!("{label}: open failed: {e}"));
                assert_eq!(read::hogged_by(device), mine, "{label}: not hogged");
                let native = runs_at(device, f64::from(rate));
                if native {
                    assert!(
                        (read::current_rate(device) - f64::from(rate)).abs() < 0.5,
                        "{label}: the device did not move to the track's rate"
                    );
                }
                pulls(&mut sink, &label);
                let kept = crate::untouched(&name, rate, Some(24));
                let wire: Vec<String> = read::output_streams(device)
                    .into_iter()
                    .filter_map(|s| read::format(s, kAudioStreamPropertyPhysicalFormat))
                    .map(|f| format!("{} bits flags {:#x} at {}", f.mBitsPerChannel, f.mFormatFlags, f.mSampleRate))
                    .collect();
                eprintln!("{label}: native={native} untouched={kept} wire={wire:?}");
                if !native || !output.bit_perfect {
                    assert!(!kept, "{label}: called untouched over a conversion");
                } else {
                    assert!(kept, "{label}: an integer wire at the track's rate, alone, and not untouched");
                }

                // A long pause gives the device back and takes it again.
                sink.suspend().expect("suspend");
                assert_ne!(read::hogged_by(device), mine, "{label}: still hogged while suspended");
                sink.wake().unwrap_or_else(|e| panic!("{label}: wake failed: {e}"));
                assert_eq!(read::hogged_by(device), mine, "{label}: not hogged after waking");
                pulls(&mut sink, &format!("{label}, woken"));

                drop(sink);
                assert_ne!(read::hogged_by(device), mine, "{label}: not given back");
            }

            // As it was found.
            if (read::current_rate(device) - before).abs() > 0.5 {
                let _ = set(device, &global(kAudioDevicePropertyNominalSampleRate), &before);
                settles_at(device, before);
            }
        }
    }

    /// QBZ's guard: the volume found before the first track is the one
    /// given back, even when a change of track parked the device and Chilpa
    /// moved the volume in between. On whichever output has a volume of its
    /// own -- a Mac's speakers do -- and with nothing but silence.
    #[test]
    #[ignore = "takes an output with a volume of its own for a moment; run it on purpose"]
    fn the_volume_found_is_the_volume_given_back() {
        use chilpa_audio::backend::OutputBackend;
        let mine = std::process::id() as i32;
        let Some(output) = crate::CoreAudio
            .outputs()
            .into_iter()
            .find(|o| crate::find(&o.id).is_some_and(|d| !read::movable_volume(d).is_empty()))
        else {
            eprintln!("no output here has a volume Chilpa can move");
            return;
        };
        let device = crate::find(&output.id).expect("listed, so found");
        if read::running_somewhere(device) {
            eprintln!("{}: another program is playing through it, skipped", output.id);
            return;
        }
        let (before, rate) = (read::volume(device).expect("it has one"), read::current_rate(device));

        let cd = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let mut first = CoreAudioSink::open(&output.id, cd, true).expect("first track");
        crate::CoreAudio.set_volume(&output.id, 40).expect("the player bar moves it");
        first.handoff().expect("handoff");
        assert_eq!(read::hogged_by(device), mine, "parked between tracks, still held");

        let next = StreamFormat::new(SampleFormat::S24In32, 48_000, 2);
        let second = CoreAudioSink::open(&output.id, next, true).expect("next track");
        let now = read::volume(device).expect("it has one");
        assert!((now - 0.40).abs() < 0.02, "the volume Chilpa set stays while it plays: {now}");
        drop(second);
        drop(first);

        let after = read::volume(device).expect("it has one");
        eprintln!("{}: volume {before} before, 0.40 while held, {after} after", output.id);
        assert!((after - before).abs() < 0.01, "given back as it was found: {before} then {after}");
        assert_ne!(read::hogged_by(device), mine, "given back");
        if (read::current_rate(device) - rate).abs() > 0.5 {
            let _ = set(device, &global(kAudioDevicePropertyNominalSampleRate), &rate);
            settles_at(device, rate);
        }
    }
}
