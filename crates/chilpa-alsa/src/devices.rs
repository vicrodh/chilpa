//! What outputs this machine has, and what each of them says it can do.
//!
//! Only direct devices are listed (`hw:card,device`), because they are the
//! only ones Chilpa plays through. Nothing here opens anything: a listing
//! that opens devices takes one away from whoever is using it, and it is
//! also the listing that runs while music plays. Measuring what a device
//! really accepts does open it, and that lives apart ([`capabilities`]).
//!
//! What it can say without opening comes from `/proc/asound`, and there is
//! a lot of it: a USB DAC publishes its whole format table in `streamN`,
//! and an HDA codec publishes its rates and widths in `codec#N`. Both are
//! what the hardware **declares**, which is not the same as what the driver
//! will hand over -- this machine's built-in codec declares 44.1 and the
//! PCM only takes 48. So it is named what it is, `declared`, and the
//! measured answer stays the wizard's.

use crate::probe;
use chilpa_audio::negotiate::DeviceCaps;
use std::fs;

/// How an output is attached, which is most of what tells one apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    /// A USB DAC. The one people buy on purpose.
    Usb,
    /// A screen or a receiver over HDMI or DisplayPort.
    Hdmi,
    /// Soldered to the board.
    Internal,
}

impl Bus {
    /// The word the window shows.
    pub fn name(&self) -> &'static str {
        match self {
            Bus::Usb => "USB",
            Bus::Hdmi => "HDMI",
            Bus::Internal => "internal",
        }
    }
}

/// What a device says about itself, read and not measured.
///
/// Empty when `/proc` says nothing useful about that card, which is not a
/// failure: it means the only way to know is to measure.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Declared {
    /// Every rate it lists, ascending.
    pub rates: Vec<u32>,
    /// Every sample width it lists, ascending.
    pub bits: Vec<u8>,
    /// Whether it offers a native DSD format.
    pub dsd: bool,
}

impl Declared {
    /// True when there is nothing to show.
    pub fn is_empty(&self) -> bool {
        self.rates.is_empty() && self.bits.is_empty()
    }

    /// The short line a picker shows: "up to 32 bits / 768 kHz, DSD".
    ///
    /// The ceiling and not the whole table, because a list of ten rates in a
    /// dropdown is noise. What each format does is the wizard's answer, and
    /// it measures instead of reading.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        if let Some(bits) = self.bits.last() {
            parts.push(chilpa_i18n::t_args("up to {} bits", &[&bits.to_string()]));
        }
        if let Some(rate) = self.rates.last() {
            parts.push(khz(*rate));
        }
        let mut line = parts.join(" / ");
        if self.dsd {
            line.push_str(", DSD");
        }
        line
    }
}

/// A rate as kilohertz, with a decimal only when it needs one: 44.1, 96, 768.
fn khz(rate: u32) -> String {
    if rate % 1000 == 0 {
        format!("{} kHz", rate / 1000)
    } else {
        format!("{:.1} kHz", rate as f64 / 1000.0)
    }
}

/// An output that is there.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Device {
    /// What gets handed to ALSA: `hw:0,0`.
    pub id: String,
    /// The card, as its maker named it: "Fosi Audio ZH3".
    pub card: String,
    /// The device's name inside the card: "USB Audio", "HDMI 0".
    pub name: String,
    /// How it is attached.
    pub bus: Bus,
    /// What it says it can do, without being opened.
    pub declared: Declared,
    /// Whether the card has more than one playback device, which decides
    /// whether the device's own name is worth showing.
    siblings: bool,
}

impl Device {
    /// How it reads in a list.
    ///
    /// The card's name first, because that is the one a person recognises.
    /// Every USB DAC in the world calls its device "USB Audio", so a list
    /// built on device names shows a Fosi ZH3, a dock and a webcam as three
    /// lines that all read the same -- which is exactly how a DAC gets
    /// missed in a picker that does list it.
    ///
    /// The device's name is added only when the card has more than one, and
    /// only when it says something the card's name does not: on a laptop the
    /// analog output and four HDMI ports are one card.
    pub fn label(&self) -> String {
        let mut line = self.card.clone();
        if self.siblings && !self.card.contains(&self.name) && !self.name.contains(&self.card) {
            line.push_str(" · ");
            line.push_str(&self.name);
        }
        line
    }

    /// The whole line, with how it is attached and what it says it can do.
    pub fn full_label(&self) -> String {
        let mut line = self.label();
        line.push_str(" (");
        line.push_str(self.bus.name());
        let declared = self.declared.summary();
        if !declared.is_empty() {
            line.push_str(", ");
            line.push_str(&declared);
        }
        line.push(')');
        line
    }
}

/// Lists the direct outputs by reading `/proc/asound`.
pub fn list() -> Vec<Device> {
    let mut outputs = Vec::new();
    let Ok(cards) = fs::read_to_string("/proc/asound/cards") else {
        return outputs;
    };
    for line in cards.lines() {
        // " 1 [ZH3            ]: USB-Audio - Fosi Audio ZH3"
        let Some((number, rest)) = line.trim_start().split_once(' ') else {
            continue;
        };
        let Ok(card) = number.parse::<u32>() else {
            continue;
        };
        let card_name = card_name(rest);
        let devices = card_devices(card);
        let siblings = devices.len() > 1;
        let usb = fs::metadata(format!("/proc/asound/card{card}/usbid")).is_ok();
        for (device, name) in devices {
            let bus = if name.contains("HDMI") || name.contains("DisplayPort") {
                Bus::Hdmi
            } else if usb {
                Bus::Usb
            } else {
                Bus::Internal
            };
            outputs.push(Device {
                id: format!("hw:{card},{device}"),
                card: card_name.clone(),
                name,
                bus,
                declared: declared(card, device, bus),
                siblings,
            });
        }
    }
    outputs
}

/// The card's name out of its line in `/proc/asound/cards`.
///
/// The line is "[short]: driver - long name", and it is the long name that a
/// person recognises. Dropping the driver matters: with it, every USB DAC
/// reads "USB-Audio - " before its own name.
fn card_name(rest: &str) -> String {
    let after = rest.split_once(':').map(|(_, d)| d.trim()).unwrap_or(rest);
    after.split_once(" - ").map(|(_, long)| long.trim()).unwrap_or(after).to_string()
}

/// A card's playback devices, with their names.
fn card_devices(card: u32) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir(format!("/proc/asound/card{card}")) else {
        return out;
    };
    let mut entries: Vec<_> = dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // The playback ones end in "p": pcm0p, pcm3p...
            let rest = name.strip_prefix("pcm")?.strip_suffix('p')?;
            rest.parse::<u32>().ok().map(|n| (n, name))
        })
        .collect();
    entries.sort_by_key(|(n, _)| *n);
    for (number, folder) in entries {
        let info = fs::read_to_string(format!("/proc/asound/card{card}/{folder}/info"))
            .unwrap_or_default();
        let name = info
            .lines()
            .find_map(|l| l.strip_prefix("name: "))
            .unwrap_or("output")
            .trim()
            .to_string();
        out.push((number, name));
    }
    out
}

/// What a device says it can do, from whichever file its driver writes.
fn declared(card: u32, device: u32, bus: Bus) -> Declared {
    match bus {
        Bus::Usb => from_usb_stream(card, device),
        _ => from_hda_codec(card),
    }
}

/// A USB device's format table, from `/proc/asound/cardN/streamM`.
///
/// The file lists one block per alternate setting, and a DAC usually has
/// several: 24 bit, 32 bit, and DSD. What is wanted is the union.
fn from_usb_stream(card: u32, device: u32) -> Declared {
    let Ok(text) = fs::read_to_string(format!("/proc/asound/card{card}/stream{device}")) else {
        return Declared::default();
    };
    let mut out = Declared::default();
    // Only the playback half: the same file describes capture below it.
    let playback = match (text.find("Playback:"), text.find("Capture:")) {
        (Some(p), Some(c)) if c > p => &text[p..c],
        (Some(p), _) => &text[p..],
        _ => return out,
    };
    for line in playback.lines() {
        let line = line.trim();
        if let Some(rates) = line.strip_prefix("Rates: ") {
            out.rates.extend(rates.split(',').filter_map(|r| r.trim().parse::<u32>().ok()));
        } else if let Some(bits) = line.strip_prefix("Bits: ") {
            if let Ok(b) = bits.trim().parse::<u8>() {
                out.bits.push(b);
            }
        } else if let Some(format) = line.strip_prefix("Format: ") {
            if format.trim().starts_with("DSD") {
                out.dsd = true;
            }
        }
    }
    tidy(&mut out);
    out
}

/// An HDA codec's rates and widths, from `/proc/asound/cardN/codec#M`.
///
/// Every output node lists its own, and they differ -- a headphone jack and
/// an HDMI port on the same codec are not the same converter. The union is
/// what the card can do somewhere, which is what a picker is claiming.
fn from_hda_codec(card: u32) -> Declared {
    let mut out = Declared::default();
    let Ok(dir) = fs::read_dir(format!("/proc/asound/card{card}")) else {
        return out;
    };
    let mut codecs: Vec<String> = dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("codec#"))
        .collect();
    codecs.sort();
    for codec in codecs {
        let Ok(text) = fs::read_to_string(format!("/proc/asound/card{card}/{codec}")) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            // "rates [0x560]: 44100 48000 96000 192000"
            if let Some(rest) = line.strip_prefix("rates [") {
                if let Some((_, values)) = rest.split_once("]: ") {
                    out.rates.extend(values.split_whitespace().filter_map(|r| r.parse::<u32>().ok()));
                }
            } else if let Some(rest) = line.strip_prefix("bits [") {
                if let Some((_, values)) = rest.split_once("]: ") {
                    out.bits.extend(values.split_whitespace().filter_map(|b| b.parse::<u8>().ok()));
                }
            }
        }
    }
    tidy(&mut out);
    out
}

/// Sorted, without repeats.
fn tidy(d: &mut Declared) {
    d.rates.sort_unstable();
    d.rates.dedup();
    d.bits.sort_unstable();
    d.bits.dedup();
}

/// Measures a device by opening it. Fails if it is busy.
pub fn capabilities(id: &str) -> Result<DeviceCaps, chilpa_audio::sink::SinkError> {
    probe(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(card: &str, name: &str, siblings: bool) -> Device {
        Device {
            id: "hw:1,0".into(),
            card: card.into(),
            name: name.into(),
            bus: Bus::Usb,
            declared: Declared::default(),
            siblings,
        }
    }

    #[test]
    fn a_usb_dac_reads_as_itself_and_not_as_usb_audio() {
        // The whole point. Every USB DAC calls its device "USB Audio", so a
        // list built on device names shows a Fosi ZH3, a dock and a webcam
        // as three identical lines, and the DAC looks like it is missing
        // from a list that does contain it.
        let dac = device("Fosi Audio ZH3", "USB Audio", false);
        assert_eq!(dac.label(), "Fosi Audio ZH3");
        assert_eq!(dac.full_label(), "Fosi Audio ZH3 (USB)");
    }

    #[test]
    fn a_card_with_several_outputs_says_which_one() {
        let analog = Device { bus: Bus::Internal, ..device("HDA Intel PCH", "ALC3254 Analog", true) };
        let hdmi = Device { bus: Bus::Hdmi, ..device("HDA Intel PCH", "HDMI 0", true) };
        assert_eq!(analog.label(), "HDA Intel PCH · ALC3254 Analog");
        assert_eq!(hdmi.full_label(), "HDA Intel PCH · HDMI 0 (HDMI)");
    }

    #[test]
    fn a_name_that_repeats_the_card_is_not_said_twice() {
        let d = device("Fosi Audio ZH3", "Fosi Audio ZH3", true);
        assert_eq!(d.label(), "Fosi Audio ZH3");
    }

    #[test]
    fn the_card_name_drops_the_driver_in_front_of_it() {
        assert_eq!(card_name("[ZH3            ]: USB-Audio - Fosi Audio ZH3"), "Fosi Audio ZH3");
        assert_eq!(card_name("[PCH            ]: HDA-Intel - HDA Intel PCH"), "HDA Intel PCH");
        // A card whose line carries no driver is taken whole.
        assert_eq!(card_name("[X ]: Something"), "Something");
    }

    #[test]
    fn the_summary_says_the_ceiling_and_whether_there_is_dsd() {
        let d = Declared { rates: vec![44_100, 96_000, 768_000], bits: vec![24, 32], dsd: true };
        assert_eq!(d.summary(), "up to 32 bits / 768 kHz, DSD");
        let plain = Declared { rates: vec![44_100, 48_000], bits: vec![16], dsd: false };
        assert_eq!(plain.summary(), "up to 16 bits / 48 kHz");
        assert_eq!(Declared::default().summary(), "");
    }

    #[test]
    fn a_rate_that_is_not_whole_keeps_its_decimal() {
        assert_eq!(khz(44_100), "44.1 kHz");
        assert_eq!(khz(48_000), "48 kHz");
        assert_eq!(khz(768_000), "768 kHz");
    }

    #[test]
    fn listing_does_not_crash_without_cards() {
        // On a machine with no audio the list comes back empty, not failing.
        let _ = list();
    }

    #[test]
    fn every_identifier_is_a_direct_device() {
        for d in list() {
            assert!(d.id.starts_with("hw:"), "{} is not a direct device", d.id);
        }
    }
}
