//! Exact native DSD streams. PCM never enters this module.
//!
//! One bounded PipeWire loop and half-second ring belong to each live sink.
//! Queries own neither a ring nor a stream. Closing or suspending joins the
//! loop and releases every proxy, mapping and buffer; idle playback owns none.

use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::DeviceCaps,
    sink::{AudioSink, SinkError},
};
use std::{
    ffi::{c_char, CStr, CString},
    ptr::NonNull,
    time::{Duration, Instant},
};

const OPEN_LIMIT: Duration = Duration::from_millis(800);
const DRAIN_LIMIT: Duration = Duration::from_secs(2);
const RATE_BASE: u32 = 88_200;

#[repr(C)]
#[derive(Default, Debug)]
struct Caps {
    serial: u64,
    be_rates: u32,
    le_rates: u32,
}

enum Native {}

unsafe extern "C" {
    fn chilpa_pw_dsd_query(
        name: *const c_char,
        caps: *mut Caps,
        error: *mut c_char,
        size: usize,
    ) -> i32;
    fn chilpa_pw_dsd_open(
        name: *const c_char,
        serial: u64,
        cookie: u32,
        rate: u32,
        interleave: i32,
        error: *mut c_char,
        size: usize,
    ) -> *mut Native;
    fn chilpa_pw_dsd_serial(sink: *mut Native) -> u64;
    fn chilpa_pw_dsd_cookie(sink: *mut Native) -> u32;
    fn chilpa_pw_dsd_write(sink: *mut Native, bytes: *const u8, size: usize) -> i64;
    fn chilpa_pw_dsd_pause(sink: *mut Native, paused: bool) -> i32;
    fn chilpa_pw_dsd_flush(sink: *mut Native) -> i32;
    fn chilpa_pw_dsd_drain(sink: *mut Native) -> i32;
    fn chilpa_pw_dsd_held(sink: *mut Native) -> u64;
    fn chilpa_pw_dsd_error(sink: *mut Native, error: *mut c_char, size: usize);
    fn chilpa_pw_dsd_close(sink: *mut Native);
}

fn name(id: &str) -> Result<CString, SinkError> {
    CString::new(id).map_err(|_| SinkError::Backend("PipeWire: invalid output name".into()))
}

fn message(error: &[c_char; 256]) -> String {
    // The C boundary always terminates its bounded diagnostic buffer.
    unsafe { CStr::from_ptr(error.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn from_caps(caps: Caps) -> Option<DeviceCaps> {
    let (format, mask) = if caps.be_rates != 0 {
        (SampleFormat::DsdU32Be, caps.be_rates)
    } else {
        (SampleFormat::DsdU32Le, caps.le_rates)
    };
    let sample_rates: Vec<u32> = (0..4)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| RATE_BASE << i)
        .collect();
    (caps.serial != 0 && !sample_rates.is_empty()).then_some(DeviceCaps {
        sample_rates,
        formats: vec![format],
        channels: vec![2],
        probed: true,
    })
}

pub(super) fn capabilities(id: &str) -> Option<DeviceCaps> {
    let id = name(id).ok()?;
    let mut caps = Caps::default();
    let mut error = [0; 256];
    // This synchronous query has one 800 ms deadline including all roundtrips.
    let result =
        unsafe { chilpa_pw_dsd_query(id.as_ptr(), &mut caps, error.as_mut_ptr(), error.len()) };
    if result < 0 {
        chilpa_audio::diagnostics::trace(
            "pipewire-dsd",
            "unavailable",
            format_args!("reason={}", message(&error)),
        );
        return None;
    }
    from_caps(caps)
}

pub(super) struct DsdSink {
    native: Option<NonNull<Native>>,
    name: CString,
    format: StreamFormat,
    serial: u64,
    cookie: u32,
    closed: bool,
}

// Each C entry point locks the owned PipeWire loop. The loop does not borrow
// Rust memory, except the CString kept alive until the session is destroyed.
unsafe impl Send for DsdSink {}

impl DsdSink {
    pub(super) fn open(id: &str, format: StreamFormat, exclusive: bool) -> Result<Self, SinkError> {
        if !exclusive
            || format.channels != 2
            || !format.sample_format.is_dsd()
            || !(0..4).any(|i| format.sample_rate_hz == RATE_BASE << i)
        {
            return Err(SinkError::Backend(
                "PipeWire: native DSD requires exclusive stereo U32 at a supported DSD rate".into(),
            ));
        }
        let mut sink = Self {
            native: None,
            name: name(id)?,
            format,
            serial: 0,
            cookie: 0,
            closed: false,
        };
        sink.connect()?;
        Ok(sink)
    }

    fn connect(&mut self) -> Result<(), SinkError> {
        let interleave = match self.format.sample_format {
            SampleFormat::DsdU32Be => 4,
            SampleFormat::DsdU32Le => -4,
            _ => {
                return Err(SinkError::Backend(
                    "PipeWire: unsupported native DSD packing".into(),
                ))
            }
        };
        let mut error = [0; 256];
        let pointer = unsafe {
            chilpa_pw_dsd_open(
                self.name.as_ptr(),
                self.serial,
                self.cookie,
                self.format.sample_rate_hz,
                interleave,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let native = NonNull::new(pointer)
            .ok_or_else(|| SinkError::Backend(format!("PipeWire: {}", message(&error))))?;
        self.serial = unsafe { chilpa_pw_dsd_serial(native.as_ptr()) };
        self.cookie = unsafe { chilpa_pw_dsd_cookie(native.as_ptr()) };
        self.native = Some(native);
        chilpa_audio::diagnostics::trace(
            "pipewire-dsd",
            "opened",
            format_args!("target_serial={} format={:?}", self.serial, self.format),
        );
        Ok(())
    }

    fn pointer(&self) -> Result<*mut Native, SinkError> {
        self.native.map(NonNull::as_ptr).ok_or(SinkError::Closed)
    }

    fn result(&self, result: i64) -> Result<usize, SinkError> {
        match result {
            value if value >= 0 => Ok(value as usize),
            -32 => Err(SinkError::Xrun),
            -19 => Err(SinkError::DeviceLost),
            -110 => Err(SinkError::Timeout(OPEN_LIMIT)),
            _ => {
                let mut error = [0; 256];
                if let Some(native) = self.native {
                    unsafe {
                        chilpa_pw_dsd_error(native.as_ptr(), error.as_mut_ptr(), error.len())
                    };
                }
                let detail = if error[0] == 0 || message(&error) == "Success" {
                    std::io::Error::from_raw_os_error((-result) as i32).to_string()
                } else {
                    message(&error)
                };
                Err(SinkError::Backend(format!("PipeWire: {detail}")))
            }
        }
    }

    fn release(&mut self) {
        if let Some(native) = self.native.take() {
            unsafe { chilpa_pw_dsd_close(native.as_ptr()) };
        }
    }
}

impl AudioSink for DsdSink {
    fn format(&self) -> StreamFormat {
        self.format
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let pointer = self.pointer()?;
        if frames.is_empty() {
            return Ok(0);
        }
        self.result(unsafe { chilpa_pw_dsd_write(pointer, frames.as_ptr(), frames.len()) })
    }

    fn held(&self) -> u64 {
        self.native
            .map(|native| unsafe { chilpa_pw_dsd_held(native.as_ptr()) })
            .unwrap_or(0)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.release();
        self.closed = true;
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), SinkError> {
        self.release();
        Ok(())
    }

    fn wake(&mut self) -> Result<(), SinkError> {
        if self.closed {
            return Err(SinkError::Closed);
        }
        if self.native.is_none() {
            self.connect()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        if let Some(native) = self.native {
            self.result(unsafe { chilpa_pw_dsd_flush(native.as_ptr()) } as i64)?;
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        if let Some(native) = self.native {
            self.result(unsafe { chilpa_pw_dsd_pause(native.as_ptr(), paused) } as i64)?;
        }
        Ok(())
    }

    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let pointer = self.pointer()?;
        let start = Instant::now();
        loop {
            if give_up() {
                return self.flush();
            }
            let done = self.result(unsafe { chilpa_pw_dsd_drain(pointer) } as i64)?;
            if done != 0 {
                return Ok(());
            }
            if start.elapsed() >= DRAIN_LIMIT {
                return Err(SinkError::Timeout(DRAIN_LIMIT));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for DsdSink {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests;
