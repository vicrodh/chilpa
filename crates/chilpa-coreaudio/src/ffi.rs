//! The handful of CoreAudio calls this needs, declared by hand.
//!
//! By hand and not through a binding crate: what is used here is a dozen
//! functions and a few dozen constants, and a generator would bring a
//! build-time dependency on libclang plus a surface nobody reads. The device
//! items are from `<CoreAudio/AudioHardware.h>`, the output unit's from
//! `<AudioToolbox/AudioUnit.h>`, and the four-character codes are the same
//! ones the headers spell as `'dev#'`.

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
/// Zero once the device is gone: unplugged, or turned off.
pub const kAudioDevicePropertyDeviceIsAlive: AudioObjectPropertySelector = fourcc(b"livn");
/// Whether any program, this one or another, is playing through it.
#[cfg(test)]
pub const kAudioDevicePropertyDeviceIsRunningSomewhere: AudioObjectPropertySelector =
    fourcc(b"gone");
/// The knob, from 0 to 1. A device that has none simply does not answer.
pub const kAudioDevicePropertyVolumeScalar: AudioObjectPropertySelector = fourcc(b"volm");
pub const kAudioDevicePropertyMute: AudioObjectPropertySelector = fourcc(b"mute");
/// Which two elements are the left and right of a stereo output. Volume
/// often lives on these and not on the master element.
pub const kAudioDevicePropertyPreferredChannelsForStereo: AudioObjectPropertySelector =
    fourcc(b"dch2");
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

/// What a stream carries on the wire right now.
///
/// Read to decide a verdict, never set: the output unit is handed floats,
/// and the device turns them into this. An integer format at least as wide
/// as the track is what lets that last step keep every sample.
pub const kAudioStreamPropertyPhysicalFormat: AudioObjectPropertySelector = fourcc(b"pft ");

pub const kAudioFormatLinearPCM: u32 = fourcc(b"lpcm");
pub const kAudioFormatFlagIsFloat: u32 = 1 << 0;
pub const kAudioFormatFlagIsSignedInteger: u32 = 1 << 2;
pub const kAudioFormatFlagIsPacked: u32 = 1 << 3;

/// Nobody has the device. CoreAudio spells "no owner" as -1.
pub const kAudioHogModeNobody: i32 = -1;

/// When a period of samples is wanted. Nothing here reads it; the device
/// asks and this answers with whatever is ready.
#[repr(C)]
pub struct AudioTimeStamp {
    pub mSampleTime: f64,
    pub mHostTime: u64,
    pub mRateScalar: f64,
    pub mWordClockTime: u64,
    pub mSMPTETime: [u8; 16],
    pub mFlags: u32,
    pub mReserved: u32,
}

/// An instance of an audio unit. Opaque: only AudioToolbox looks inside.
pub type AudioUnit = *mut c_void;
pub type AudioComponent = *mut c_void;

/// Which unit is wanted, by kind, variant and maker.
#[repr(C)]
pub struct AudioComponentDescription {
    pub componentType: u32,
    pub componentSubType: u32,
    pub componentManufacturer: u32,
    pub componentFlags: u32,
    pub componentFlagsMask: u32,
}

/// The output unit that talks to one device, whichever it is.
///
/// Not the default-output unit, which follows whatever the system is using
/// and would move the music when somebody changes the Mac's output.
pub const kAudioUnitType_Output: u32 = fourcc(b"auou");
pub const kAudioUnitSubType_HALOutput: u32 = fourcc(b"ahal");
pub const kAudioUnitManufacturer_Apple: u32 = fourcc(b"appl");

pub const kAudioUnitScope_Global: u32 = 0;
/// The side the program writes into. Its format is the track's.
pub const kAudioUnitScope_Input: u32 = 1;

pub const kAudioUnitProperty_StreamFormat: u32 = 8;
pub const kAudioUnitProperty_SetRenderCallback: u32 = 23;
pub const kAudioOutputUnitProperty_CurrentDevice: u32 = 2000;

/// Said to the unit when a period is all zeros, so it can skip the work.
pub const kAudioUnitRenderAction_OutputIsSilence: u32 = 1 << 4;

/// What the unit calls, on the device's thread, every period.
pub type AURenderCallback = extern "C" fn(
    inClientData: *mut c_void,
    ioActionFlags: *mut u32,
    inTimeStamp: *const AudioTimeStamp,
    inBusNumber: u32,
    inNumberFrames: u32,
    ioData: *mut AudioBufferList,
) -> OSStatus;

#[repr(C)]
pub struct AURenderCallbackStruct {
    pub inputProc: AURenderCallback,
    pub inputProcClientData: *mut c_void,
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

    /// Whether the object has that property at all.
    ///
    /// The difference between "this device has no volume" and "reading it
    /// failed", which a read that returns an option cannot tell apart.
    pub fn AudioObjectHasProperty(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
    ) -> u8;

    /// Whether it can be written, which is not the same as having it.
    pub fn AudioObjectIsPropertySettable(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        outIsSettable: *mut u8,
    ) -> OSStatus;

    pub fn AudioObjectSetPropertyData(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const c_void,
        inDataSize: u32,
        inData: *const c_void,
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

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    pub fn AudioComponentFindNext(
        inComponent: AudioComponent,
        inDesc: *const AudioComponentDescription,
    ) -> AudioComponent;
    pub fn AudioComponentInstanceNew(inComponent: AudioComponent, outInstance: *mut AudioUnit)
        -> OSStatus;
    pub fn AudioComponentInstanceDispose(inInstance: AudioUnit) -> OSStatus;

    pub fn AudioUnitSetProperty(
        inUnit: AudioUnit,
        inID: u32,
        inScope: u32,
        inElement: u32,
        inData: *const c_void,
        inDataSize: u32,
    ) -> OSStatus;
    pub fn AudioUnitInitialize(inUnit: AudioUnit) -> OSStatus;
    pub fn AudioUnitUninitialize(inUnit: AudioUnit) -> OSStatus;

    pub fn AudioOutputUnitStart(ci: AudioUnit) -> OSStatus;
    pub fn AudioOutputUnitStop(ci: AudioUnit) -> OSStatus;
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
    output_element(selector, kAudioObjectPropertyElementMain)
}

/// One element of a device's output: the master, or a single channel.
///
/// Worth having by hand because volume is the property that is often not on
/// the master at all. A device whose left and right each carry their own
/// knob answers nothing at element zero, and a reader that only asks there
/// concludes the device has no volume.
pub const fn output_element(
    selector: AudioObjectPropertySelector,
    element: AudioObjectPropertyElement,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: element,
    }
}
