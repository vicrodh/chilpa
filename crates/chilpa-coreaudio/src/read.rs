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

/// One of a stream's formats, right now.
///
/// What the wire carries at this moment, which is what a verdict has to be
/// built on: the device decides it, and asking for one is not having it.
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

/// Whether the device is gone: unplugged or turned off.
///
/// Gone means no longer among the machine's devices. Its own "alive" answer
/// is asked first because it is one read, but it is not enough on its own:
/// a Macaron playing DoP once answered no -- or did not answer -- for an
/// instant, still listed with the same id, and the track was dropped as if
/// the cable had been pulled. A device that merely stops taking samples for
/// a while is the renderer's to wait out, up to its own limit.
pub fn gone(device: AudioObjectID) -> bool {
    let alive = read::<u32>(device, &global(kAudioDevicePropertyDeviceIsAlive)).is_some_and(|v| v != 0);
    !alive && !every_device().contains(&device)
}

/// Whether some program is playing through the device right now. Only the
/// test that opens devices asks, so as not to take one from a listener.
#[cfg(test)]
pub fn running_somewhere(device: AudioObjectID) -> bool {
    read::<u32>(device, &global(kAudioDevicePropertyDeviceIsRunningSomewhere)).is_some_and(|v| v != 0)
}

/// Who has the device to themselves, or nobody.
///
/// CoreAudio answers with a process id, and -1 means nobody. It is read and
/// never set here: taking a device is opening it, and this file does not
/// open anything.
pub fn hogged_by(device: AudioObjectID) -> i32 {
    read(device, &global(kAudioDevicePropertyHogMode)).unwrap_or(-1)
}

/// Whether the object really has that property.
///
/// The difference between "this device has no volume" and "reading it did
/// not work", which a read that hands back an option cannot tell apart. It
/// is worth the extra call: the first is an honest answer to show somebody
/// and the second is a bug.
pub fn has(object: AudioObjectID, address: &AudioObjectPropertyAddress) -> bool {
    unsafe { AudioObjectHasProperty(object, address) != 0 }
}

/// Whether it can be written. Having a control and being allowed to move it
/// are different things.
pub fn settable(object: AudioObjectID, address: &AudioObjectPropertyAddress) -> bool {
    let mut yes = 0u8;
    let status = unsafe { AudioObjectIsPropertySettable(object, address, &mut yes) };
    status == 0 && yes != 0
}

/// The elements a device's volume might live on, master first.
///
/// Master first because a device that has one is answered in a single read.
/// Then the stereo pair, because plenty of devices -- the Mac's own speakers
/// among them -- carry no master and a knob per channel, and a reader that
/// only asks element zero decides they have no volume at all. The pair is
/// asked for rather than assumed to be 1 and 2: CoreAudio publishes which
/// two it is.
pub fn volume_elements(device: AudioObjectID) -> Vec<AudioObjectPropertyElement> {
    let mut elements = vec![kAudioObjectPropertyElementMain];
    let stereo: Vec<u32> = read_all(device, &output(kAudioDevicePropertyPreferredChannelsForStereo));
    elements.extend(stereo);
    elements.dedup();
    elements
}

/// Where the device's own knob sits, from 0 to 1.
///
/// `None` when it has none, which is the usual answer for a USB DAC with a
/// knob on the front: the hardware owns the volume and says nothing about
/// it. Inventing one by scaling the samples would be the end of bit-perfect,
/// so nothing is invented.
///
/// When the control is per channel rather than shared, what comes back is
/// their average. CoreAudio has no canonical "the device's volume" in that
/// case, and an average is the honest single number to show.
pub fn volume(device: AudioObjectID) -> Option<f32> {
    let mut found = Vec::new();
    for element in volume_elements(device) {
        let address = output_element(kAudioDevicePropertyVolumeScalar, element);
        if !has(device, &address) {
            continue;
        }
        if let Some(level) = read::<f32>(device, &address) {
            found.push(level);
            // A master answer is the device's answer; the channels are only
            // consulted when there is none.
            if element == kAudioObjectPropertyElementMain {
                return Some(level);
            }
        }
    }
    (!found.is_empty()).then(|| found.iter().sum::<f32>() / found.len() as f32)
}

/// Whether it is muted, when it can be.
///
/// Muting and volume are separate capabilities: a device can have one and
/// not the other, so this asks its own question rather than riding along.
pub fn muted(device: AudioObjectID) -> Option<bool> {
    for element in volume_elements(device) {
        let address = output_element(kAudioDevicePropertyMute, element);
        if !has(device, &address) {
            continue;
        }
        if let Some(on) = read::<u32>(device, &address) {
            return Some(on != 0);
        }
    }
    None
}

/// Every element whose volume can actually be moved.
pub fn movable_volume(device: AudioObjectID) -> Vec<AudioObjectPropertyElement> {
    settable_elements(device, kAudioDevicePropertyVolumeScalar)
}

/// Every element whose mute can actually be moved.
pub fn movable_mute(device: AudioObjectID) -> Vec<AudioObjectPropertyElement> {
    settable_elements(device, kAudioDevicePropertyMute)
}

/// The elements that carry a writable copy of that property.
///
/// Master alone when it has one, because writing both the master and the
/// channels is writing the same thing twice and devices disagree about what
/// that means.
fn settable_elements(
    device: AudioObjectID,
    selector: AudioObjectPropertySelector,
) -> Vec<AudioObjectPropertyElement> {
    let master = output_element(selector, kAudioObjectPropertyElementMain);
    if has(device, &master) && settable(device, &master) {
        return vec![kAudioObjectPropertyElementMain];
    }
    volume_elements(device)
        .into_iter()
        .filter(|e| *e != kAudioObjectPropertyElementMain)
        .filter(|e| {
            let address = output_element(selector, *e);
            has(device, &address) && settable(device, &address)
        })
        .collect()
}

/// How a transport reads in the window.
///
/// The standards and the brand names stay as they are -- USB is USB in every
/// language -- and the ordinary words go through the catalog. Mixing the two
/// kinds in one list and treating them all as names is what left "internal"
/// in English under a Spanish window.
pub fn transport_name(transport: u32) -> String {
    match transport {
        kAudioDeviceTransportTypeUSB => "USB".to_string(),
        kAudioDeviceTransportTypeBuiltIn => chilpa_i18n::t("internal"),
        kAudioDeviceTransportTypeHDMI => "HDMI".to_string(),
        kAudioDeviceTransportTypeDisplayPort => "DisplayPort".to_string(),
        kAudioDeviceTransportTypeBluetooth | kAudioDeviceTransportTypeBluetoothLE => {
            "Bluetooth".to_string()
        }
        kAudioDeviceTransportTypeThunderbolt => "Thunderbolt".to_string(),
        kAudioDeviceTransportTypeAirPlay => "AirPlay".to_string(),
        // Neither of these reaches the picker today, because `is_hardware`
        // drops them before an output is built. They are translated anyway:
        // the function is public and the day that filter changes is not the
        // day to find out they were never in the catalog.
        kAudioDeviceTransportTypeAggregate => chilpa_i18n::t("aggregate"),
        kAudioDeviceTransportTypeVirtual => chilpa_i18n::t("virtual"),
        _ => String::new(),
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
