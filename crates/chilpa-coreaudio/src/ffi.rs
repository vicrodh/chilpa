//! The handful of CoreAudio calls this needs, declared by hand.
//!
//! By hand and not through a binding crate: what is used here is two
//! functions and a dozen constants, and a generator would bring a build-time
//! dependency on libclang plus a surface nobody reads. Every item below is
//! from `<CoreAudio/AudioHardware.h>`, and the four-character codes are the
//! same ones the header spells as `'dev#'`.

// Apple's own names, kept as Apple spells them. Renaming a field of a
// `#[repr(C)]` struct to look like Rust would make it one letter harder to
// check against the header, which is the only way anybody can verify this
// file is right.
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

use std::os::raw::{c_char, c_void};

pub type OSStatus = i32;
pub type AudioObjectID = u32;
pub type AudioObjectPropertySelector = u32;
pub type AudioObjectPropertyScope = u32;
pub type AudioObjectPropertyElement = u32;

/// Apple writes these as four characters; this is the same number.
const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}

pub const kAudioObjectSystemObject: AudioObjectID = 1;
pub const kAudioObjectPropertyElementMain: AudioObjectPropertyElement = 0;

pub const kAudioObjectPropertyScopeGlobal: AudioObjectPropertyScope = fourcc(b"glob");
pub const kAudioObjectPropertyScopeOutput: AudioObjectPropertyScope = fourcc(b"outp");

pub const kAudioHardwarePropertyDevices: AudioObjectPropertySelector = fourcc(b"dev#");
pub const kAudioHardwarePropertyDefaultOutputDevice: AudioObjectPropertySelector = fourcc(b"dOut");
pub const kAudioObjectPropertyName: AudioObjectPropertySelector = fourcc(b"lnam");
pub const kAudioDevicePropertyStreamConfiguration: AudioObjectPropertySelector = fourcc(b"slay");
pub const kAudioDevicePropertyAvailableNominalSampleRates: AudioObjectPropertySelector =
    fourcc(b"nsr#");
pub const kAudioDevicePropertyNominalSampleRate: AudioObjectPropertySelector = fourcc(b"nsrt");
pub const kAudioDevicePropertyTransportType: AudioObjectPropertySelector = fourcc(b"tran");
pub const kAudioDevicePropertyHogMode: AudioObjectPropertySelector = fourcc(b"oink");
pub const kAudioStreamPropertyAvailablePhysicalFormats: AudioObjectPropertySelector =
    fourcc(b"pfta");
pub const kAudioDevicePropertyStreams: AudioObjectPropertySelector = fourcc(b"stm#");

/// How a device is attached, as CoreAudio says it.
pub const kAudioDeviceTransportTypeBuiltIn: u32 = fourcc(b"bltn");
pub const kAudioDeviceTransportTypeUSB: u32 = fourcc(b"usb ");
pub const kAudioDeviceTransportTypeHDMI: u32 = fourcc(b"hdmi");
pub const kAudioDeviceTransportTypeDisplayPort: u32 = fourcc(b"dprt");
pub const kAudioDeviceTransportTypeBluetooth: u32 = fourcc(b"blue");
pub const kAudioDeviceTransportTypeBluetoothLE: u32 = fourcc(b"blea");
pub const kAudioDeviceTransportTypeVirtual: u32 = fourcc(b"virt");
pub const kAudioDeviceTransportTypeAggregate: u32 = fourcc(b"grup");
pub const kAudioDeviceTransportTypeAirPlay: u32 = fourcc(b"airp");
pub const kAudioDeviceTransportTypeThunderbolt: u32 = fourcc(b"thun");

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioObjectPropertyAddress {
    pub mSelector: AudioObjectPropertySelector,
    pub mScope: AudioObjectPropertyScope,
    pub mElement: AudioObjectPropertyElement,
}

/// One channel group of a device. The count is what says whether a device
/// can play at all.
#[repr(C)]
pub struct AudioBuffer {
    pub mNumberChannels: u32,
    pub mDataByteSize: u32,
    pub mData: *mut c_void,
}

#[repr(C)]
pub struct AudioBufferList {
    pub mNumberBuffers: u32,
    pub mBuffers: [AudioBuffer; 1],
}

/// A range of rates. A device that can do anything between two numbers says
/// so with a range whose ends differ; most say one rate per entry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioValueRange {
    pub mMinimum: f64,
    pub mMaximum: f64,
}

/// What a stream really carries, as opposed to what the device is set to.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioStreamBasicDescription {
    pub mSampleRate: f64,
    pub mFormatID: u32,
    pub mFormatFlags: u32,
    pub mBytesPerPacket: u32,
    pub mFramesPerPacket: u32,
    pub mBytesPerFrame: u32,
    pub mChannelsPerFrame: u32,
    pub mBitsPerChannel: u32,
    pub mReserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioStreamRangedDescription {
    pub mFormat: AudioStreamBasicDescription,
    pub mSampleRateRange: AudioValueRange,
}

pub type CFStringRef = *const c_void;
pub type CFIndex = isize;
pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    pub fn AudioObjectGetPropertyDataSize(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const c_void,
        outDataSize: *mut u32,
    ) -> OSStatus;

    pub fn AudioObjectGetPropertyData(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const c_void,
        ioDataSize: *mut u32,
        outData: *mut c_void,
    ) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFStringGetCString(
        theString: CFStringRef,
        buffer: *mut c_char,
        bufferSize: CFIndex,
        encoding: u32,
    ) -> u8;
    pub fn CFRelease(cf: *const c_void);
}

/// An address for the whole of a device, which is what most reads want.
pub const fn global(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// The same, for what a device sends out rather than what it takes in.
pub const fn output(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: kAudioObjectPropertyElementMain,
    }
}
