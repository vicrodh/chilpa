//! Reading what CoreAudio has, without opening anything.
//!
//! Every call here is a property read. None of them takes a device from
//! whoever is using it, which is what makes this safe to run while music is
//! playing -- and a listing does run then.

// CoreAudio's constants keep their own names, and a `match` on one is
// still a constant: the allow has to cover patterns too.
#![allow(non_upper_case_globals)]

use crate::ffi::*;
use std::os::raw::c_void;

/// A property, read into a value of whatever shape it has.
///
/// The size is asked for first because CoreAudio decides it: a device list
/// is as long as the machine's devices, and guessing a length is how a read
/// of somebody else's memory starts.
fn read<T: Copy>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<T> {
    let mut size = std::mem::size_of::<T>() as u32;
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            value.as_mut_ptr() as *mut c_void,
        )
    };
    (status == 0 && size as usize == std::mem::size_of::<T>())
        .then(|| unsafe { value.assume_init() })
}

/// The same, for a property that is a list of them.
fn read_all<T: Copy>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Vec<T> {
    let mut size = 0u32;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(object, address, 0, std::ptr::null(), &mut size) };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let count = size as usize / std::mem::size_of::<T>();
    let mut out: Vec<T> = Vec::with_capacity(count);
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            out.as_mut_ptr() as *mut c_void,
        )
    };
    if status != 0 {
        return Vec::new();
    }
    unsafe { out.set_len(size as usize / std::mem::size_of::<T>()) };
    out
}

/// A property that is a string, as a `String`.
///
/// CoreAudio hands back a `CFStringRef` it has already retained on our
/// behalf, so it is released here. Leaking one per device per refresh is the
/// kind of leak that only shows up after an afternoon.
fn read_string(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<String> {
    let handle: CFStringRef = read(object, address)?;
    if handle.is_null() {
        return None;
    }
    let mut buffer = [0i8; 512];
    let ok = unsafe {
        CFStringGetCString(
            handle,
            buffer.as_mut_ptr(),
            buffer.len() as CFIndex,
            kCFStringEncodingUTF8,
        )
    };
    unsafe { CFRelease(handle) };
    (ok != 0).then(|| {
        let bytes: Vec<u8> = buffer.iter().take_while(|b| **b != 0).map(|b| *b as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Every audio object the machine has, playing or recording.
pub fn every_device() -> Vec<AudioObjectID> {
    read_all(kAudioObjectSystemObject, &global(kAudioHardwarePropertyDevices))
}

/// The one the system is currently sending sound to.
pub fn default_device() -> Option<AudioObjectID> {
    read(kAudioObjectSystemObject, &global(kAudioHardwarePropertyDefaultOutputDevice))
}

/// How many channels a device can send out.
///
/// Zero means it is a microphone, or an app's virtual input, and it has no
/// business in a list of outputs.
pub fn output_channels(device: AudioObjectID) -> u32 {
    let address = output(kAudioDevicePropertyStreamConfiguration);
    let mut size = 0u32;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(device, &address, 0, std::ptr::null(), &mut size) };
    if status != 0 || size == 0 {
        return 0;
    }
    let mut raw = vec![0u8; size as usize];
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            raw.as_mut_ptr() as *mut c_void,
        )
    };
    if status != 0 {
        return 0;
    }
    // An AudioBufferList is a count followed by that many buffers, and the
    // struct declares one: the rest follow it in the same block.
    let list = raw.as_ptr() as *const AudioBufferList;
    let count = unsafe { (*list).mNumberBuffers } as usize;
    let buffers = unsafe { std::ptr::addr_of!((*list).mBuffers) } as *const AudioBuffer;
    (0..count).map(|i| unsafe { (*buffers.add(i)).mNumberChannels }).sum()
}

/// What a device is called, as its maker named it.
pub fn name(device: AudioObjectID) -> String {
    read_string(device, &global(kAudioObjectPropertyName)).unwrap_or_default()
}

/// How it is attached.
pub fn transport(device: AudioObjectID) -> u32 {
    read(device, &global(kAudioDevicePropertyTransportType)).unwrap_or(0)
}

/// The rate it is set to right now.
pub fn current_rate(device: AudioObjectID) -> f64 {
    read(device, &global(kAudioDevicePropertyNominalSampleRate)).unwrap_or(0.0)
}

/// Every rate it will accept.
///
/// CoreAudio answers in ranges. A device that takes a fixed set says each one
/// as a range whose ends are equal; one with a converter inside says a real
/// span, and then every rate between them is available.
pub fn available_rates(device: AudioObjectID) -> Vec<AudioValueRange> {
    read_all(device, &global(kAudioDevicePropertyAvailableNominalSampleRates))
}

/// The streams a device sends out on. A format is a property of one of
/// these, not of the device.
pub fn output_streams(device: AudioObjectID) -> Vec<AudioObjectID> {
    read_all(device, &output(kAudioDevicePropertyStreams))
}

/// Every format the hardware really carries, across its output streams.
///
/// This is the one that matters for bit-perfect, and it is not the same as
/// the rates above: the nominal rate is what the device is set to, and the
/// physical formats are what the wire does. A device that reports 24 bits
/// here and is set to 44100 will carry 24-bit samples at 44100 untouched.
pub fn physical_formats(device: AudioObjectID) -> Vec<AudioStreamBasicDescription> {
    output_streams(device).into_iter().flat_map(available_physical_formats).collect()
}

/// One of a stream's two formats, right now.
///
/// The one to read back after asking for a format: a set that returns
/// success and a device that gives something else is the ordinary case.
/// Both sides are worth reading -- the wire, and what the program is handed.
pub fn format(
    stream: AudioObjectID,
    which: AudioObjectPropertySelector,
) -> Option<AudioStreamBasicDescription> {
    read(stream, &global(which))
}

/// Every format one stream carries, as the device published them.
///
/// The list to pick from rather than to check against: a description built
/// by hand can be right in every field somebody thought of and refused for
/// one nobody did.
pub fn available_physical_formats(stream: AudioObjectID) -> Vec<AudioStreamBasicDescription> {
    let ranged: Vec<AudioStreamRangedDescription> =
        read_all(stream, &global(kAudioStreamPropertyAvailablePhysicalFormats));
    ranged.into_iter().map(|f| f.mFormat).collect()
}

/// Who has the device to themselves, or nobody.
///
/// CoreAudio answers with a process id, and -1 means nobody. It is read and
/// never set here: taking a device is opening it, and this file does not
/// open anything.
pub fn hogged_by(device: AudioObjectID) -> i32 {
    read(device, &global(kAudioDevicePropertyHogMode)).unwrap_or(-1)
}

/// Where the device's own knob sits, from 0 to 1.
///
/// `None` when it has none, which is the usual answer for a USB DAC with a
/// knob on the front: the hardware owns the volume and says nothing about
/// it. Inventing one by scaling the samples would be the end of bit-perfect,
/// so nothing is invented.
pub fn volume(device: AudioObjectID) -> Option<f32> {
    read(device, &output(kAudioDevicePropertyVolumeScalar))
}

/// Whether it is muted, when it can be.
pub fn muted(device: AudioObjectID) -> Option<bool> {
    let on: u32 = read(device, &output(kAudioDevicePropertyMute))?;
    Some(on != 0)
}

/// How a transport reads in the window.
pub fn transport_name(transport: u32) -> &'static str {
    match transport {
        kAudioDeviceTransportTypeUSB => "USB",
        kAudioDeviceTransportTypeBuiltIn => "internal",
        kAudioDeviceTransportTypeHDMI => "HDMI",
        kAudioDeviceTransportTypeDisplayPort => "DisplayPort",
        kAudioDeviceTransportTypeBluetooth | kAudioDeviceTransportTypeBluetoothLE => "Bluetooth",
        kAudioDeviceTransportTypeThunderbolt => "Thunderbolt",
        kAudioDeviceTransportTypeAirPlay => "AirPlay",
        kAudioDeviceTransportTypeAggregate => "aggregate",
        kAudioDeviceTransportTypeVirtual => "virtual",
        _ => "",
    }
}

/// Whether what is behind this is hardware rather than another program.
///
/// An aggregate device and a virtual one both play, and neither is a way out
/// of the machine: they are other software, and sending music into one is
/// not choosing an output.
pub fn is_hardware(transport: u32) -> bool {
    !matches!(
        transport,
        kAudioDeviceTransportTypeVirtual | kAudioDeviceTransportTypeAggregate | 0
    )
}
