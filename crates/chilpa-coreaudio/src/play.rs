//! Playing through a Mac's own audio.
//!
//! Three things have to be true for a track to reach the DAC as it left the
//! file, and each is a separate ask that can be refused:
//!
//! 1. **The device is this program's.** Hog mode. Without it CoreAudio mixes
//!    whatever else is playing and the rate is whoever asked first's.
//! 2. **The device runs at the track's rate.** The nominal rate is set. A
//!    device sitting at 44.1 given a 96 kHz track converts it, silently and
//!    well, which is the failure this whole crate is written against.
//! 3. **The stream carries the track's samples.** The physical format is the
//!    wire, and the virtual format is what the program is handed. If they
//!    differ CoreAudio converts between them, so both are set to the same
//!    integer format. A device that will not take an integer virtual format
//!    is one where bit-perfect is not available, and that is said rather
//!    than papered over with a float conversion nobody was told about.
//!
//! The device calls rather than being written to, so a buffer sits between
//! its thread and the writer -- the same shape as JACK, for the same reason.

use crate::ffi::*;
use crate::read;
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::sink::{AudioSink, SinkError};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the device's thread and the writer share.
struct Shared {
    /// Bytes waiting, already in the format the wire carries.
    waiting: Mutex<std::collections::VecDeque<u8>>,
    /// Set when the device stopped or went away.
    lost: AtomicBool,
}

/// The token CoreAudio gives back for a registered callback.
///
/// It is an opaque handle, not a pointer to anything this program owns, and
/// the calls that take it -- stop, destroy -- may be made from any thread.
/// So it may move to the one that plays, which is the only reason this
/// wrapper exists: Rust will not move a raw pointer across threads on its
/// own say-so, and it is right to ask.
struct ProcToken(AudioDeviceIOProcID);

// Sound because the token is opaque and its calls are thread-agnostic. What
// is NOT sound and is not done here: sharing it. One sink owns one token.
unsafe impl Send for ProcToken {}

/// A stream to a Mac's output device.
pub struct CoreAudioSink {
    device: AudioObjectID,
    proc_id: ProcToken,
    shared: Arc<Shared>,
    stream: StreamFormat,
    /// Whether this took the device to itself and therefore has to give it
    /// back. Letting go is not optional: a hogged device is unusable by
    /// anything else until the holder dies.
    hogged: bool,
    ceiling: usize,
    /// Kept alive for as long as the device may call: the pointer handed to
    /// CoreAudio points into it.
    _client: Box<Arc<Shared>>,
}

/// Whether a property can be written, asked of the object itself.
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

fn set<T>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
    value: &T,
) -> Result<(), OSStatus> {
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

/// What Chilpa's sample layout looks like on the wire.
///
/// The width alone does not say it. A DAC that carries 24 bits may want them
/// packed into three bytes or left-justified inside four, and those are not
/// interchangeable: hand one to a device expecting the other and the bytes
/// land in the wrong places, which is noise and not music. `S24In32` is
/// left-justified by definition, which is what CoreAudio calls aligned-high.
fn wire(format: SampleFormat) -> Option<(u32, u32, u32)> {
    // (valid bits, bytes per channel, the flag that says the layout)
    Some(match format {
        SampleFormat::S16 => (16, 2, kAudioFormatFlagIsPacked),
        SampleFormat::S24Packed => (24, 3, kAudioFormatFlagIsPacked),
        SampleFormat::S24In32 => (24, 4, kAudioFormatFlagIsAlignedHigh),
        SampleFormat::S32 => (32, 4, kAudioFormatFlagIsPacked),
        // A float track is not what a DAC takes; whoever decoded it has
        // already been asked to hand over integers.
        SampleFormat::F32 => return None,
    })
}

/// True when a description is plain interleaved signed little-endian PCM in
/// the exact layout Chilpa's samples are in.
fn carries(f: &AudioStreamBasicDescription, stream: StreamFormat) -> bool {
    let Some((bits, per_channel, layout)) = wire(stream.sample_format) else { return false };
    let channels = u32::from(stream.channels);
    f.mFormatID == kAudioFormatLinearPCM
        && f.mFormatFlags & kAudioFormatFlagIsFloat == 0
        && f.mFormatFlags & kAudioFormatFlagIsSignedInteger != 0
        && f.mFormatFlags & kAudioFormatFlagIsBigEndian == 0
        && f.mFormatFlags & kAudioFormatFlagIsNonInterleaved == 0
        && f.mFormatFlags & (kAudioFormatFlagIsPacked | kAudioFormatFlagIsAlignedHigh) == layout
        && f.mBitsPerChannel == bits
        && f.mChannelsPerFrame == channels
        && f.mBytesPerFrame == per_channel * channels
        && f.mFramesPerPacket == 1
        && f.mBytesPerPacket == f.mBytesPerFrame
        && (f.mSampleRate - f64::from(stream.sample_rate_hz)).abs() < 0.5
}

/// The format to ask a stream for: one the device itself offered.
///
/// Offered and not invented. A fabricated description can be right in every
/// field somebody thought to fill and still be refused for one nobody
/// remembered, and the refusal comes back as a number. The device publishes
/// exactly what it carries, so the job is to pick from that list.
///
/// When the output is wanted alone the non-mixable variant is preferred:
/// that is the other half of hog mode -- hog keeps other programs off the
/// device, this keeps anything from being mixed into the stream -- and it is
/// the flag a DAC looks at before lighting its bit-perfect lamp.
fn offered(
    stream_id: AudioObjectID,
    stream: StreamFormat,
    exclusive: bool,
) -> Option<AudioStreamBasicDescription> {
    let fits: Vec<AudioStreamBasicDescription> = read::available_physical_formats(stream_id)
        .into_iter()
        .filter(|f| carries(f, stream))
        .collect();
    if exclusive {
        if let Some(alone) =
            fits.iter().find(|f| f.mFormatFlags & kAudioFormatFlagIsNonMixable != 0)
        {
            return Some(*alone);
        }
    }
    fits.into_iter().next()
}

/// Waits for a device to really be at that rate.
///
/// Two seconds, which is longer than any DAC takes to re-lock and short
/// enough that a device that is never going to switch does not hang the
/// thread that asked. Polling and not a property listener: a listener means
/// a run loop, and this is called from whatever thread is starting a track.
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

/// Waits for a stream to really be in that format, and says what it ended
/// up in.
///
/// The same patience the rate needs, for the same reason: the set hands the
/// ask to the driver and returns. What comes back is whatever it settled on,
/// which is then compared -- this waits, it does not decide.
fn format_settles(
    stream_id: AudioObjectID,
    which: AudioObjectPropertySelector,
    asked: &AudioStreamBasicDescription,
) -> Option<AudioStreamBasicDescription> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut last = read::format(stream_id, which);
    loop {
        match &last {
            Some(got) if disagrees(asked, got).is_none() => return last,
            _ if std::time::Instant::now() >= deadline => return last,
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                last = read::format(stream_id, which);
            }
        }
    }
}

/// Why what a stream opened as is not what was asked for, or `None`.
///
/// Checked in full and not just on the rate. The one that would otherwise go
/// unnoticed is interleaving: a device that hands back one buffer per
/// channel instead of one buffer with every channel would be fed this
/// stream's bytes in order, and what comes out is the left channel playing
/// the first half of the track and the right channel the second. It would
/// sound like a bug in the file.
fn disagrees(wanted: &AudioStreamBasicDescription, got: &AudioStreamBasicDescription) -> Option<String> {
    if (got.mSampleRate - wanted.mSampleRate).abs() > 0.5 {
        return Some(format!("{} Hz", got.mSampleRate as u32));
    }
    if got.mBitsPerChannel != wanted.mBitsPerChannel {
        return Some(format!("{} bits", got.mBitsPerChannel));
    }
    if got.mChannelsPerFrame != wanted.mChannelsPerFrame {
        return Some(format!("{} channels", got.mChannelsPerFrame));
    }
    if got.mFormatID != kAudioFormatLinearPCM {
        return Some("a format that is not linear PCM".to_string());
    }
    if got.mFormatFlags & kAudioFormatFlagIsFloat != 0 {
        return Some("floating point".to_string());
    }
    if got.mFormatFlags & kAudioFormatFlagIsSignedInteger == 0 {
        return Some("unsigned samples".to_string());
    }
    if got.mFormatFlags & kAudioFormatFlagIsBigEndian != 0 {
        return Some("big-endian samples".to_string());
    }
    if got.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0 {
        return Some("one buffer per channel".to_string());
    }
    if got.mBytesPerFrame != wanted.mBytesPerFrame {
        // The same width with different room around it. The bytes handed
        // over would land in the wrong places, which is noise, not music.
        return Some(format!("{} bytes a frame", got.mBytesPerFrame));
    }
    if got.mFormatFlags & (kAudioFormatFlagIsAlignedHigh | kAudioFormatFlagIsPacked)
        != wanted.mFormatFlags & (kAudioFormatFlagIsAlignedHigh | kAudioFormatFlagIsPacked)
    {
        return Some("the valid bits somewhere else in the word".to_string());
    }
    if got.mFramesPerPacket != wanted.mFramesPerPacket
        || got.mBytesPerPacket != wanted.mBytesPerPacket
    {
        // A packet that is not one frame means the device is not carrying
        // plain PCM, whatever the rest of the description says.
        return Some("packets that are not one frame".to_string());
    }
    // Whether the stream ended up non-mixable is not compared: it is asked
    // for, and a device that will not give it still carries the samples
    // untouched. What it costs is that something else could be mixed in,
    // and hog mode has already made that impossible.
    None
}

/// What the device asks for, every period.
///
/// It runs on the device's own thread and must not block: taking the lock is
/// tried once and a period that cannot have it is silence, which is a gap
/// heard once rather than a stall the device never recovers from.
extern "C" fn feed(
    _device: AudioObjectID,
    _now: *const AudioTimeStamp,
    _input: *const AudioBufferList,
    _input_time: *const AudioTimeStamp,
    output: *mut AudioBufferList,
    _output_time: *const AudioTimeStamp,
    client: *mut c_void,
) -> OSStatus {
    if output.is_null() || client.is_null() {
        return 0;
    }
    let shared = unsafe { &*(client as *const Arc<Shared>) };
    let list = unsafe { &mut *output };
    let buffers =
        unsafe { std::slice::from_raw_parts_mut(list.mBuffers.as_mut_ptr(), list.mNumberBuffers as usize) };

    let mut waiting = match shared.waiting.try_lock() {
        Ok(w) => w,
        Err(_) => {
            for buffer in buffers.iter_mut() {
                if !buffer.mData.is_null() {
                    unsafe {
                        std::ptr::write_bytes(buffer.mData as *mut u8, 0, buffer.mDataByteSize as usize)
                    };
                }
            }
            return 0;
        }
    };

    for buffer in buffers.iter_mut() {
        if buffer.mData.is_null() {
            continue;
        }
        let wanted = buffer.mDataByteSize as usize;
        let out = unsafe { std::slice::from_raw_parts_mut(buffer.mData as *mut u8, wanted) };
        for slot in out.iter_mut() {
            // Running dry writes silence rather than the last period again,
            // which is what a repeated buffer sounds like: a click.
            *slot = waiting.pop_front().unwrap_or(0);
        }
    }
    0
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
        if wire(stream.sample_format).is_none() {
            return Err(SinkError::Backend(chilpa_i18n::t(
                "a Mac's outputs take integer samples, not floating point",
            )));
        }

        // 1. The device to ourselves, before anything is changed on it:
        // changing the rate of a device somebody else is using would move
        // their music too.
        let mut hogged = false;
        if exclusive {
            let holder = read::hogged_by(device);
            if holder != kAudioHogModeNobody && holder != std::process::id() as i32 {
                return Err(SinkError::Backend(chilpa_i18n::t_args(
                    "another program has \"{}\" to itself; close it, or turn bit-perfect off to \
                     share it",
                    &[name],
                )));
            }
            // Failing to take it is an error and not a shrug. This set a
            // flag and carried on, which meant a track played into a shared
            // device while the stack went on saying Exact and the window
            // went on saying bit-perfect -- the one claim this project
            // exists to stop making, made by the code written to make it
            // true.
            let mine = std::process::id() as i32;
            set(device, &global(kAudioDevicePropertyHogMode), &mine).map_err(|s| {
                SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" would not be given to Chilpa alone ({}); turn bit-perfect off to \
                     share it",
                    &[name, &s.to_string()],
                ))
            })?;
            // And read back, because asking is not having.
            if read::hogged_by(device) != mine {
                return Err(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" would not be given to Chilpa alone; turn bit-perfect off to share it",
                    &[name],
                )));
            }
            hogged = true;
        }

        let sink = |e: SinkError| -> SinkError {
            if hogged {
                let _ = set(device, &global(kAudioDevicePropertyHogMode), &kAudioHogModeNobody);
            }
            e
        };

        // 2. The device at the track's rate.
        //
        // Setting it is a request, not a change: CoreAudio hands the ask to
        // the driver and returns, and the device switches when it is ready.
        // A DAC re-locking its clock takes a moment, and reading the rate
        // straight back gives the old one -- which is how the format set
        // right after this lands at the wrong rate and the whole thing comes
        // apart with a message about the device refusing. So it is asked
        // for, and then waited for.
        let rate = f64::from(stream.sample_rate_hz);
        if (read::current_rate(device) - rate).abs() > 0.5 {
            set(device, &global(kAudioDevicePropertyNominalSampleRate), &rate).map_err(|s| {
                sink(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" would not run at {} Hz ({})",
                    &[name, &stream.sample_rate_hz.to_string(), &s.to_string()],
                )))
            })?;
            if !settles_at(device, rate) {
                return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" did not switch to {} Hz",
                    &[name, &stream.sample_rate_hz.to_string()],
                ))));
            }
        }

        // 3. The wire carrying the track's own samples, and the program
        // handed the same thing. Setting only the first leaves CoreAudio
        // converting into whatever the second happens to be, which is float
        // on every device this was tried on.
        let streams = read::output_streams(device);
        if streams.is_empty() {
            return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                "\"{}\" has no output stream",
                &[name],
            ))));
        }
        let mut bytes_per_frame = 0u32;
        for stream_id in streams {
            let Some(wanted) = offered(stream_id, stream, exclusive) else {
                return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" does not carry {} bit samples at {} Hz; turn bit-perfect off to let \
                     the Mac convert",
                    &[
                        name,
                        &stream.sample_format.valid_bits().to_string(),
                        &stream.sample_rate_hz.to_string(),
                    ],
                ))));
            };
            bytes_per_frame = wanted.mBytesPerFrame;

            // The wire first, then what the program is handed. The virtual
            // side never asks to be non-mixable: that is a property of the
            // stream on the wire.
            let mut presented = wanted;
            presented.mFormatFlags &= !kAudioFormatFlagIsNonMixable;

            for (address, asked, which) in [
                (kAudioStreamPropertyPhysicalFormat, wanted, "physical"),
                (kAudioStreamPropertyVirtualFormat, presented, "virtual"),
            ] {
                set(stream_id, &global(address), &asked).map_err(|s| {
                    sink(SinkError::Backend(chilpa_i18n::t_args(
                        "\"{}\" refused the {} format ({})",
                        &[name, which, &s.to_string()],
                    )))
                })?;
                // Read back, and wait for it like the rate.
                //
                // Setting a format is a request too, and it took a second
                // round on real hardware to see it: a track whose rate
                // already matched skipped the rate wait, the format was read
                // back a microsecond after being asked for, and it still
                // said float. The one whose rate had to change worked --
                // because waiting for the clock had given the driver time to
                // do both.
                let Some(got) = format_settles(stream_id, address, &asked) else {
                    return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                        "\"{}\" would not say what format it opened",
                        &[name],
                    ))));
                };
                if let Some(wrong) = disagrees(&asked, &got) {
                    return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                        "\"{}\" opened as {} instead; turn bit-perfect off to let the Mac convert",
                        &[name, &wrong],
                    ))));
                }
            }
        }

        let shared = Arc::new(Shared {
            waiting: Mutex::new(std::collections::VecDeque::new()),
            lost: AtomicBool::new(false),
        });
        // The pointer CoreAudio keeps has to stay valid for as long as it
        // may call, so the handle it points at is owned by the sink.
        let client = Box::new(Arc::clone(&shared));
        let mut proc_id: AudioDeviceIOProcID = std::ptr::null_mut();
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                device,
                feed,
                &*client as *const Arc<Shared> as *mut c_void,
                &mut proc_id,
            )
        };
        if status != 0 || proc_id.is_null() {
            return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                "\"{}\" would not open ({})",
                &[name, &status.to_string()],
            ))));
        }
        let status = unsafe { AudioDeviceStart(device, proc_id) };
        if status != 0 {
            unsafe { AudioDeviceDestroyIOProcID(device, proc_id) };
            return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                "\"{}\" would not start ({})",
                &[name, &status.to_string()],
            ))));
        }

        let frame = bytes_per_frame.max(1) as usize;
        Ok(CoreAudioSink {
            device,
            proc_id: ProcToken(proc_id),
            shared,
            stream,
            hogged,
            // A quarter of a second: long enough to ride out a scheduling
            // hiccup, short enough that stopping is not heard as a tail.
            ceiling: (stream.sample_rate_hz as usize / 4) * frame,
            _client: client,
        })
    }
}

impl AudioSink for CoreAudioSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.shared.lost.load(Ordering::Relaxed) {
            return Err(SinkError::DeviceLost);
        }
        let mut waiting = self.shared.waiting.lock().map_err(|_| SinkError::Closed)?;
        if waiting.len() >= self.ceiling {
            // Full: nothing was taken and the caller comes back, which is
            // what a partial write means here.
            return Ok(0);
        }
        waiting.extend(frames.iter().copied());
        // In frames, as the trait asks: the renderer multiplies by the size.
        Ok(frames.len() / self.stream.frame_bytes())
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        self.shared.waiting.lock().map_err(|_| SinkError::Closed)?.clear();
        Ok(())
    }

    /// Waits for the device to take what is queued: a quarter of a second
    /// at most, and a second more before giving up on one that stopped.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let limit = std::time::Instant::now() + std::time::Duration::from_millis(1_250);
        while std::time::Instant::now() < limit && !give_up() {
            if self.shared.lost.load(Ordering::Relaxed) {
                return Err(SinkError::DeviceLost);
            }
            if self.shared.waiting.lock().map_err(|_| SinkError::Closed)?.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        if self.proc_id.0.is_null() {
            return Ok(());
        }
        unsafe {
            AudioDeviceStop(self.device, self.proc_id.0);
            AudioDeviceDestroyIOProcID(self.device, self.proc_id.0);
        }
        self.proc_id.0 = std::ptr::null_mut();
        // Giving the device back is not optional: a hogged device stays
        // unusable by everything else until whoever took it lets go, and a
        // player that forgets leaves the Mac silent for every other program.
        if self.hogged {
            let _ = set(self.device, &global(kAudioDevicePropertyHogMode), &kAudioHogModeNobody);
            self.hogged = false;
        }
        Ok(())
    }
}

impl Drop for CoreAudioSink {
    fn drop(&mut self) {
        // Whatever path got here, the device goes back.
        let _ = self.close();
    }
}
