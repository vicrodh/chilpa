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

/// The format description for a track, as CoreAudio spells it.
///
/// Interleaved signed integers, packed, little-endian -- which is what every
/// file Chilpa decodes already is, and what makes the copy into the device's
/// buffer a copy rather than a conversion.
fn describe(stream: StreamFormat) -> Option<AudioStreamBasicDescription> {
    let bits: u32 = match stream.sample_format {
        SampleFormat::S16 => 16,
        SampleFormat::S24Packed => 24,
        SampleFormat::S24In32 | SampleFormat::S32 => 32,
        // A float track is not what a DAC takes; whoever decoded it has
        // already been asked to hand over integers.
        SampleFormat::F32 => return None,
    };
    let channels = u32::from(stream.channels);
    let bytes_per_frame = (bits / 8) * channels;
    Some(AudioStreamBasicDescription {
        mSampleRate: f64::from(stream.sample_rate_hz),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: channels,
        mBitsPerChannel: bits,
        mReserved: 0,
    })
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
        let Some(wanted) = describe(stream) else {
            return Err(SinkError::Backend(chilpa_i18n::t(
                "a Mac's outputs take integer samples, not floating point",
            )));
        };

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
            let mine = std::process::id() as i32;
            if set(device, &global(kAudioDevicePropertyHogMode), &mine).is_ok() {
                hogged = true;
            }
        }

        let sink = |e: SinkError| -> SinkError {
            if hogged {
                let _ = set(device, &global(kAudioDevicePropertyHogMode), &kAudioHogModeNobody);
            }
            e
        };

        // 2. The device at the track's rate.
        let rate = f64::from(stream.sample_rate_hz);
        if (read::current_rate(device) - rate).abs() > 0.5 {
            set(device, &global(kAudioDevicePropertyNominalSampleRate), &rate).map_err(|s| {
                sink(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" would not run at {} Hz ({})",
                    &[name, &stream.sample_rate_hz.to_string(), &s.to_string()],
                )))
            })?;
        }

        // 3. The wire carrying the track's own samples, and the program
        // handed the same thing. Setting only the first leaves CoreAudio
        // converting into whatever the second happens to be.
        for stream_id in read::output_streams(device) {
            let physical = set(stream_id, &global(kAudioStreamPropertyPhysicalFormat), &wanted);
            let virtual_ = set(stream_id, &global(kAudioStreamPropertyVirtualFormat), &wanted);
            if physical.is_err() || virtual_.is_err() {
                return Err(sink(SinkError::Backend(chilpa_i18n::t_args(
                    "\"{}\" will not carry {} bit samples untouched; turn bit-perfect off to let \
                     the Mac convert",
                    &[name, &wanted.mBitsPerChannel.to_string()],
                ))));
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

        let frame = usize::from(stream.channels) * (wanted.mBitsPerChannel as usize / 8);
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
        Ok(frames.len())
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
