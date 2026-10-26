//! I read the selected PCM's declarations and streams without opening ALSA.
use chilpa_audio::evidence::*;
use std::{fs::File, io::Read};

fn text(path: &str) -> String {
    let mut out = String::new();
    if let Ok(file) = File::open(path) { let _ = file.take(256 * 1024).read_to_string(&mut out); }
    out
}
fn address(id: &str) -> Option<(u32, u32)> {
    let (card, device) = id.strip_prefix("hw:")?.split_once(',')?;
    Some((card.parse().ok()?, device.parse().ok()?))
}
pub fn inspect(id: &str) -> Inspection {
    let mut doc = Inspection::unavailable("alsa", id, crate::Alsa.generation());
    let Some((card, device)) = address(id) else { return doc };
    doc.identity.transport = "alsa_pcm".into();
    // A card id and bus path disambiguate equal names; neither certifies a serial.
    let bus = std::fs::canonicalize(format!("/sys/class/sound/card{card}/device"))
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    doc.identity.stable_id = format!("{}:{bus}:{device}", text(&format!("/proc/asound/card{card}/id")).trim());
    doc.method = "alsa_usb_alternate_settings".into();
    doc.formats = usb_formats(&text(&format!("/proc/asound/card{card}/stream{device}")));
    doc.state = if doc.formats.is_empty() { State::Unknown } else { State::Available };
    doc.observation = observation(id);
    doc.bound(); doc
}
use chilpa_audio::backend::OutputBackend;

fn field<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    block.lines().find_map(|line| {
        let (key,value) = line.split_once(':')?;
        (key.trim() == name.trim_end_matches(':')).then_some(value.trim())
    })
}
fn width(name: &str) -> Option<(&'static str, u16, u16)> {
    match name.trim() {
        "S16_LE" => Some(("pcm",16,16)), "S24_3LE" => Some(("pcm",24,24)),
        "S24_LE" => Some(("pcm",24,32)), "S32_LE" => Some(("pcm",32,32)),
        "FLOAT_LE" => Some(("float",32,32)), "DSD_U32_BE" | "DSD_U32_LE" => Some(("native_dsd",1,32)),
        _ => None,
    }
}
fn ranges(rates: &str) -> Vec<(f64,f64)> {
    rates.split(',').take(32).filter_map(|rate| {
        let rate = rate.trim().trim_end_matches("(continuous)").trim();
        if let Some((lo,hi)) = rate.split_once('-') { Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?)) }
        else { let rate = rate.parse().ok()?; Some((rate,rate)) }
    }).collect()
}
fn usb_formats(text: &str) -> Vec<FormatRange> {
    let Some(playback) = text.split_once("Playback:").map(|(_,s)| s.split("Capture:").next().unwrap_or_default()) else { return Vec::new() };
    let mut rows = Vec::new();
    for (index,block) in playback.split("Altset ").skip(1).take(64).enumerate() {
        let (Some(format),Some(channels),Some(rates)) = (field(block,"Format:"),field(block,"Channels:"),field(block,"Rates:")) else { continue };
        let Some((encoding,bits,container)) = width(format) else { continue };
        let bits = field(block,"Bits:").and_then(|b|b.parse::<u16>().ok()).filter(|b|*b > 0 && *b <= container).unwrap_or(bits);
        let Ok(channels) = channels.parse() else { continue };
        for (rate_min,rate_max) in ranges(rates) {
            rows.push(FormatRange { encoding: encoding.into(),rate_min,rate_max,useful_bits: bits,
                container_bits:container,channels,stream:format!("altset-{index}"),scope:Scope::HardwareDeclaration });
            if rows.len() == 256 { return rows; }
        }
    }
    rows
}
pub fn observation(id: &str) -> Observation {
    let mut result = Observation::default();
    let Some((card,device)) = address(id) else { return result };
    for sub in 0..32 {
        let status = text(&format!("/proc/asound/card{card}/pcm{device}p/sub{sub}/status"));
        let Some(owner) = field(&status,"owner_pid:").and_then(|p|p.parse::<u32>().ok()) else {continue};
        let ours = owner == std::process::id() || field(&text(&format!("/proc/{owner}/status")),"Tgid:")
            .and_then(|p|p.parse::<u32>().ok()) == Some(std::process::id());
        if !ours { continue; }
        let raw = text(&format!("/proc/asound/card{card}/pcm{device}p/sub{sub}/hw_params"));
        let (Some(format),Some(rate),Some(channels)) = (field(&raw,"format:"),field(&raw,"rate:"),field(&raw,"channels:")) else { continue };
        let (Some((encoding,bits,container)),Some(rate),Some(channels)) =
            (width(format),rate.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()),channels.parse().ok()) else { continue };
        result.formats.push(FormatRange { encoding:encoding.into(),rate_min:rate,rate_max:rate,useful_bits:bits,
            container_bits:container,channels,stream:format!("hw:{card},{device},{sub}"),scope:Scope::Readback });
    }
    if result.formats.len() != 1 { result.formats.clear(); }
    result.complete = result.formats.len() == 1;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternate_settings_never_become_a_cartesian_product() {
        let raw = "Playback:\n Altset 1\n Format: S16_LE\n Channels: 2\n Rates: 44100, 48000\n Altset 2\n Format: S24_3LE\n Channels: 4\n Rates: 96000\nCapture:\n Altset 3\n Format: S32_LE\n Channels: 8\n Rates: 192000\n";
        let rows = usb_formats(raw);
        assert_eq!(rows.len(),3);
        assert!(rows.iter().any(|r| r.rate_min == 96_000.0 && r.useful_bits == 24 && r.channels == 4));
        assert!(!rows.iter().any(|r| r.rate_min == 96_000.0 && r.channels == 2));
        assert!(!rows.iter().any(|r| r.rate_min == 192_000.0));
    }
    #[test]
    fn continuous_ranges_keep_their_endpoints() {
        assert_eq!(ranges("8000 - 192000 (continuous)"),vec![(8000.0,192000.0)]);
        assert_eq!(address("plughw:0,1"),None);
        assert_eq!(address("hw:1,3"),Some((1,3)));
        assert_eq!(field("owner_pid   : 1234\nstate: RUNNING","owner_pid:"),Some("1234"));
    }
}
