//! What outputs this machine has.
//!
//! Only direct devices are listed (`hw:card,device`), because they are the
//! only ones Chilpa plays through. What the list says about each one comes
//! from opening it and measuring, not from guessing.

use crate::probe;
use chilpa_audio::negotiate::DeviceCaps;
use std::fs;

/// An output that is there.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Device {
    /// What gets handed to ALSA: `hw:0,0`.
    pub id: String,
    /// The card's name, as the system reports it.
    pub card: String,
    /// The device's name inside the card.
    pub name: String,
}

impl Device {
    /// How it reads in a list: "ALC3254 Analog (hw:0,0)".
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

/// Lists the direct outputs by reading `/proc/asound`.
///
/// `/proc` and not ALSA's own enumeration, because listing must not open
/// anything: opening a busy device takes it away from whoever is using it.
/// Measuring what a device accepts does open it, which is why it lives
/// apart ([`capabilities`]).
pub fn list() -> Vec<Device> {
    let mut outputs = Vec::new();
    let Ok(cards) = fs::read_to_string("/proc/asound/cards") else {
        return outputs;
    };
    for line in cards.lines() {
        // " 0 [PCH            ]: HDA-Intel - HDA Intel PCH"
        let Some((number, rest)) = line.trim_start().split_once(' ') else {
            continue;
        };
        let Ok(card) = number.parse::<u32>() else {
            continue;
        };
        let card_name = rest
            .split_once(':')
            .map(|(_, d)| d.trim())
            .unwrap_or(rest)
            .to_string();
        for device in card_devices(card) {
            outputs.push(Device {
                id: format!("hw:{card},{}", device.0),
                card: card_name.clone(),
                name: device.1,
            });
        }
    }
    outputs
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

/// Measures a device by opening it. Fails if it is busy.
pub fn capabilities(id: &str) -> Result<DeviceCaps, chilpa_audio::sink::SinkError> {
    probe(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_joins_name_and_identifier() {
        let d = Device {
            id: "hw:0,0".into(),
            card: "HDA Intel PCH".into(),
            name: "ALC3254 Analog".into(),
        };
        assert_eq!(d.label(), "ALC3254 Analog (hw:0,0)");
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
