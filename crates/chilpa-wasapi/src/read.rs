//! Asking Windows what it has, without taking anything.
//!
//! Every call here either enumerates or asks a question. `IsFormatSupported`
//! is the important one: it is the driver saying whether a format would open
//! in exclusive mode, and it says so without opening it, so it can be asked
//! while somebody else is playing. No other platform here answers that
//! question that cleanly.

use chilpa_audio::backend::Knob;
use chilpa_audio::format::SampleFormat;
use chilpa_audio::negotiate::DeviceCaps;
use windows::core::PCWSTR;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eConsole, eRender, DigitalAudioDisplayDevice, EndpointFormFactor, Handset, Headphones, Headset,
    IAudioClient, IMMDevice, IMMDeviceEnumerator, LineLevel, MMDeviceEnumerator, Microphone,
    RemoteNetworkDevice, Speakers, UnknownDigitalPassthrough, AUDCLNT_E_DEVICE_INVALIDATED,
    AUDCLNT_E_SERVICE_NOT_RUNNING, AUDCLNT_SHAREMODE_EXCLUSIVE, DEVICE_STATE_ACTIVE, SPDIF,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, StructuredStorage::PROPVARIANT,
    CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

/// The device's friendly name, as Windows shows it in the sound control
/// panel. Its `fmtid` is `PKEY_Device_FriendlyName`.
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

/// How the device is attached, so that a USB DAC does not read the same as
/// the chip on the board. `PKEY_AudioEndpoint_FormFactor`.
const PKEY_ENDPOINT_FORM_FACTOR: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 0,
};

/// COM, entered for this thread and undone when the guard goes.
///
/// Entered per call rather than once at startup: this crate has no startup
/// of its own to hook, and a library that leaves a thread's COM state
/// changed behind it breaks its caller.
struct Apartment {
    ours: bool,
}

impl Apartment {
    fn enter() -> windows::core::Result<Self> {
        // Multithreaded, because nothing here is a message pump and the
        // render thread will not be the thread that enumerated.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() {
            // Both answers count, and `S_FALSE` is the one that is easy to
            // get wrong: it means the thread was already in this apartment,
            // but the call still took a reference, and COM asks for one
            // `CoUninitialize` per successful call whichever it returned.
            // Leaving `S_FALSE` unbalanced means every refresh on a thread
            // the caller had already entered adds an initialisation that is
            // never given back.
            Ok(Apartment { ours: true })
        } else if hr == windows::Win32::Foundation::RPC_E_CHANGED_MODE {
            // Somebody else set the apartment first. Their choice stands and
            // the calls still work.
            Ok(Apartment { ours: false })
        } else {
            Err(hr.into())
        }
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.ours {
            unsafe { CoUninitialize() };
        }
    }
}

/// One output, as Windows describes it.
pub struct Endpoint {
    /// The endpoint id, which is what travels. Windows keeps it across
    /// reboots and across unplugging, unlike anything friendlier.
    pub id: String,
    pub name: String,
    /// "internal", "headphones", "S/PDIF"... empty when Windows does not
    /// say, which it often does not.
    pub attached: &'static str,
    /// What it is set to, as the shared-mode mixer runs it. Worth showing
    /// for the same reason it is on a Mac: a device sitting at 48 will take
    /// a 96 kHz track and convert it, unless it is opened exclusively.
    pub mixer_rate: u32,
}

impl Endpoint {
    /// The line under the name.
    pub fn detail(&self) -> String {
        let mut line = self.attached.to_string();
        if self.mixer_rate > 0 {
            if !line.is_empty() {
                line.push_str(" · ");
            }
            line.push_str(&chilpa_i18n::t_args(
                "shared at {}",
                &[&khz(f64::from(self.mixer_rate))],
            ));
        }
        line
    }
}

/// A rate as kilohertz, with a decimal only when it needs one.
fn khz(rate: f64) -> String {
    if (rate / 1000.0).fract() == 0.0 {
        format!("{} kHz", (rate / 1000.0) as u32)
    } else {
        format!("{:.1} kHz", rate / 1000.0)
    }
}

/// What Windows calls a form factor, in words.
///
/// Paired against `EndpointFormFactor` itself rather than against written out
/// numbers, so a misremembered one cannot survive here. Only the ones that
/// mean something different to somebody choosing an output are named; the
/// rest say nothing rather than guessing, and saying nothing is the right
/// answer more often than it looks. `UnknownFormFactor` is common, and the
/// bus a device hangs off -- USB above all -- is not in this property at all.
const ATTACHMENTS: &[(EndpointFormFactor, &str)] = &[
    (Speakers, "internal"),
    (LineLevel, "internal"),
    (RemoteNetworkDevice, "network"),
    (Headphones, "headphones"),
    (Microphone, "microphone"),
    (Headset, "headset"),
    (Handset, "handset"),
    (UnknownDigitalPassthrough, "digital"),
    (SPDIF, "S/PDIF"),
    // The old name for this one was `HDMI` and it mostly still is that, but
    // DisplayPort answers to it too, so it is named for what it is rather
    // than for the cable it usually happens to be.
    (DigitalAudioDisplayDevice, "display"),
];

fn attachment(form_factor: EndpointFormFactor) -> &'static str {
    ATTACHMENTS
        .iter()
        .find(|(kind, _)| *kind == form_factor)
        .map_or("", |(_, name)| *name)
}

/// Every active render endpoint.
pub fn outputs() -> windows::core::Result<Vec<Endpoint>> {
    let _com = Apartment::enter()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
    let count = unsafe { collection.GetCount()? };
    let mut out = Vec::new();
    for index in 0..count {
        let device = unsafe { collection.Item(index)? };
        if let Ok(endpoint) = describe(&device) {
            out.push(endpoint);
        }
    }
    Ok(out)
}

/// Whether there is any output at all.
pub fn any_output() -> windows::core::Result<bool> {
    Ok(!outputs()?.is_empty())
}

/// The one Windows is using, by id.
pub fn default_output() -> windows::core::Result<Option<String>> {
    let _com = Apartment::enter()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    Ok(id_of(&device).ok())
}

/// The endpoint id, copied out of the buffer COM allocated for it.
fn id_of(device: &IMMDevice) -> windows::core::Result<String> {
    let raw = unsafe { device.GetId()? };
    let text = unsafe { raw.to_string() }.unwrap_or_default();
    // COM allocated it; COM frees it. This is the leak that would otherwise
    // grow by one string per device per refresh.
    unsafe { CoTaskMemFree(Some(raw.0 as *const _)) };
    Ok(text)
}

fn describe(device: &IMMDevice) -> windows::core::Result<Endpoint> {
    let id = id_of(device)?;
    let store = unsafe { device.OpenPropertyStore(STGM_READ)? };
    let name = unsafe { store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) }
        .map(|value| text_of(&value))
        .unwrap_or_default();
    let form_factor = unsafe { store.GetValue(&PKEY_ENDPOINT_FORM_FACTOR) }
        .ok()
        .and_then(|value| u32_of(&value))
        .map_or(EndpointFormFactor(-1), |n| EndpointFormFactor(n as i32));
    Ok(Endpoint {
        name: if name.is_empty() { id.clone() } else { name },
        attached: attachment(form_factor),
        mixer_rate: mixer_rate(device).unwrap_or(0),
        id,
    })
}

/// What the shared mixer runs this device at.
fn mixer_rate(device: &IMMDevice) -> Option<u32> {
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None).ok()? };
    let format = unsafe { client.GetMixFormat().ok()? };
    if format.is_null() {
        return None;
    }
    let rate = unsafe { (*format).nSamplesPerSec };
    unsafe { CoTaskMemFree(Some(format as *const _)) };
    Some(rate)
}

/// A property that is a string.
fn text_of(value: &PROPVARIANT) -> String {
    value.to_string()
}

/// A property that is a number.
///
/// Read as a number and not as text that gets parsed: a form factor is a
/// `VT_UI4`, and asking it for its string gives an empty one.
fn u32_of(value: &PROPVARIANT) -> Option<u32> {
    u32::try_from(value).ok()
}

/// The rates and widths Chilpa would ever ask for.
const RATES: &[u32] = &[
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];
/// Each one as Chilpa's name for it, the bits that carry the signal, and the
/// word they travel in. The last two are not the same number and that is the
/// whole point of this table: `S24In32` is 24 valid bits inside a 32-bit
/// word, while `S24Packed` is the same 24 bits in three bytes with nothing
/// around them. Windows takes them as different formats and a DAC often
/// accepts one and refuses the other, so deriving the word from the valid
/// bits -- which is what this did first -- asked about packed 24 and filed
/// the answer under `S24In32`.
const WIDTHS: &[(SampleFormat, u16, u16)] = &[
    (SampleFormat::S16, 16, 16),
    (SampleFormat::S24Packed, 24, 24),
    (SampleFormat::S24In32, 24, 32),
    (SampleFormat::S32, 32, 32),
];

/// What that endpoint really accepts in exclusive mode.
///
/// Measured, and cheaply: `IsFormatSupported` is a question. Every other
/// platform either opens the device to find out or is asked politely and
/// answers for the mixer instead.
pub fn caps(id: &str) -> windows::core::Result<DeviceCaps> {
    let _com = Apartment::enter()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let wide = wide(id);
    let device = unsafe { enumerator.GetDevice(PCWSTR(wide.as_ptr()))? };
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };

    let mut sample_rates = Vec::new();
    let mut formats = Vec::new();
    for &rate in RATES {
        for &(format, bits, container) in WIDTHS {
            let asked = extensible(rate, bits, container, 2);
            let supported = unsafe {
                client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    &asked.Format,
                    None,
                )
            };
            // A device that went away, or an audio service that stopped, is
            // not the same answer as "this format does not open", and
            // treating it as one turns a machine in trouble into a DAC that
            // reportedly accepts nothing. The honest answer there is the
            // error.
            if supported == AUDCLNT_E_DEVICE_INVALIDATED
                || supported == AUDCLNT_E_SERVICE_NOT_RUNNING
            {
                return Err(supported.into());
            }
            // S_OK and nothing else. Exclusive mode does not offer a close
            // match -- a format it will not take comes back as
            // `AUDCLNT_E_UNSUPPORTED_FORMAT` -- but `S_FALSE` and a proposed
            // alternative are what the same call answers in shared mode, and
            // accepting either would let a converted path through under a
            // name that promises no conversion.
            if supported.0 == 0 {
                if !sample_rates.contains(&rate) {
                    sample_rates.push(rate);
                }
                if !formats.contains(&format) {
                    formats.push(format);
                }
            }
        }
    }
    Ok(DeviceCaps {
        sample_rates,
        formats,
        channels: vec![2],
        // Asked of the driver, one format at a time. This is the one
        // platform where that costs nothing.
        probed: true,
    })
}

/// A format description Windows will recognise for integer PCM.
///
/// Extensible and not the plain `WAVEFORMATEX`, because anything above 16
/// bits or two channels has to be, and a driver handed the plain one for 24
/// bits answers about something else.
fn extensible(rate: u32, bits: u16, container: u16, channels: u16) -> WAVEFORMATEXTENSIBLE {
    let block_align = channels * container / 8;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(block_align),
            nBlockAlign: block_align,
            wBitsPerSample: container,
            cbSize: (std::mem::size_of::<WAVEFORMATEXTENSIBLE>()
                - std::mem::size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            // The bits that carry the signal, which is not always the word
            // they arrive in: 24 valid bits with `wBitsPerSample` at 32 is
            // `S24In32`, and the same 24 with it at 24 is `S24Packed`.
            wValidBitsPerSample: bits,
        },
        dwChannelMask: 0x3, // front left and front right
        SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
    }
}

/// That endpoint's own volume, if Windows exposes one.
///
/// Windows almost always does, because the volume it shows is the endpoint's
/// and not the application's. Moving it moves what the machine shows, which
/// is the honest behaviour: it is the same knob.
pub fn volume(_id: &str) -> windows::core::Result<Option<Knob>> {
    // Left unanswered rather than half-answered. The endpoint volume is an
    // interface of its own, and reading it is easy while deciding what it
    // should do in exclusive mode -- where it does not apply -- is not. It
    // gets settled against a machine that is switched on.
    Ok(None)
}

/// A UTF-16 string for the Windows calls that want one.
///
/// Handed back as the buffer and not as a `PCWSTR`, so the caller is the one
/// holding it alive while the call runs. A function returning the pointer
/// alone either dangles or leaks, and this one is called on every refresh.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WAVEFORMATEX` is packed, so every field is copied into a local
    /// before it is looked at: taking a reference to one is undefined
    /// behaviour and the compiler refuses it.
    fn shape(f: &WAVEFORMATEXTENSIBLE) -> (u16, u16, u16, u16, u32) {
        (
            f.Format.wBitsPerSample,
            unsafe { f.Samples.wValidBitsPerSample },
            f.Format.nBlockAlign,
            f.Format.cbSize,
            f.Format.nAvgBytesPerSec,
        )
    }

    /// These only run on Windows, because the crate only exists there. They
    /// are here anyway: the arithmetic is what was wrong first, and a runner
    /// with Windows on it is the only thing that will catch it again.
    #[test]
    fn a_word_is_not_the_bits_that_carry_the_signal() {
        // The bug: the word used to be derived from the valid bits, so 24
        // valid bits produced a 24-bit word -- packed 24 -- and the answer
        // came back filed under `S24In32`. A DAC that takes one and refuses
        // the other would have been described backwards.
        let (word, valid, align, _, _) = shape(&extensible(44_100, 24, 24, 2));
        assert_eq!((word, valid, align), (24, 24, 6), "packed 24");

        let (word, valid, align, _, _) = shape(&extensible(44_100, 24, 32, 2));
        assert_eq!((word, valid, align), (32, 24, 8), "24 inside 32");
    }

    #[test]
    fn the_table_asks_about_each_format_as_windows_knows_it() {
        for &(format, bits, container) in WIDTHS {
            let (_, _, align, _, _) = shape(&extensible(48_000, bits, container, 2));
            assert_eq!(
                usize::from(align),
                format.bytes() * 2,
                "{format:?} was asked about in a word it does not travel in",
            );
            assert!(bits <= container, "{format:?} carries more than it fits in");
        }
    }

    #[test]
    fn a_format_description_says_how_long_it_is() {
        // 22 bytes is what `WAVEFORMATEXTENSIBLE` adds, and a driver handed
        // the wrong `cbSize` reads past the end of what it was given.
        let (_, _, align, extra, bytes_per_sec) = shape(&extensible(96_000, 32, 32, 2));
        assert_eq!(extra, 22);
        assert_eq!(bytes_per_sec, 96_000 * u32::from(align));
    }

    #[test]
    fn a_form_factor_nobody_named_says_nothing() {
        assert_eq!(attachment(SPDIF), "S/PDIF");
        assert_eq!(attachment(Speakers), "internal");
        // Not "internal": it is something on the network.
        assert_eq!(attachment(RemoteNetworkDevice), "network");
        // Windows sends `UnknownFormFactor` for most USB DACs, and an empty
        // line is the honest answer: the property knows nothing about buses.
        assert_eq!(attachment(EndpointFormFactor(10)), "");
        assert_eq!(attachment(EndpointFormFactor(-1)), "");
    }

    #[test]
    fn a_rate_reads_the_way_it_is_written_on_the_sleeve() {
        assert_eq!(khz(44_100.0), "44.1 kHz");
        assert_eq!(khz(48_000.0), "48 kHz");
        assert_eq!(khz(192_000.0), "192 kHz");
    }
}
