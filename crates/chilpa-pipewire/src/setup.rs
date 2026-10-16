//! What the HiFi Wizard asks of the machine, ported from QBZ (`qbz-audio`:
//! `health.rs`, `dac_capabilities.rs`, `dac_probe.rs`, and the `pw-dump`
//! enumeration of `pipewire_backend.rs`). It only reads: it runs the
//! stack's own tools with the crate's patience, reads `/proc` and
//! `os-release`, and never opens a stream or writes a file.

use super::run;

/// Whether the pieces bit-perfect playback leans on are there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub wireplumber_active: bool,
    pub has_pw_dump: bool,
    /// ALSA lists the `pipewire` PCM: the bridge from ALSA to PipeWire.
    pub alsa_sees_pipewire: bool,
    pub has_pactl: bool,
    pub any_devices: bool,
}

impl Health {
    pub fn is_ready(&self) -> bool {
        self.wireplumber_active && self.alsa_sees_pipewire && self.any_devices
    }
}

/// A program somewhere on `PATH`, as `command -v` finds it.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// The five probes. Blocking: they run the stack's tools.
pub fn health() -> Health {
    let wireplumber_active = run("systemctl", &["--user", "is-active", "wireplumber"]).is_some_and(|out| out.trim() == "active")
        || run("pgrep", &["-x", "wireplumber"]).is_some();
    Health {
        wireplumber_active,
        has_pw_dump: on_path("pw-dump"),
        alsa_sees_pipewire: run("aplay", &["-L"]).is_some_and(|out| out.lines().any(|line| line == "pipewire")),
        has_pactl: on_path("pactl"),
        any_devices: run("pw-dump", &[]).is_some_and(|out| out.contains("Audio/Sink")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distro {
    Debian,
    Antix,
    Fedora,
    Arch,
    Artix,
    OpenSuse,
    Gentoo,
    Void,
    NixOS,
    Other,
}

impl Distro {
    pub const ALL: [Distro; 10] = [
        Distro::Debian,
        Distro::Antix,
        Distro::Fedora,
        Distro::Arch,
        Distro::Artix,
        Distro::OpenSuse,
        Distro::Gentoo,
        Distro::Void,
        Distro::NixOS,
        Distro::Other,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&d| d == self).unwrap_or(Self::ALL.len() - 1)
    }

    /// The dropdown's label: the systemd-free families named so their
    /// commands make sense.
    pub fn label(self) -> &'static str {
        match self {
            Distro::Debian => "Ubuntu / Debian / Mint / Pop!_OS",
            Distro::Antix => "antiX (systemd-free Debian)",
            Distro::Fedora => "Fedora / RHEL",
            Distro::Arch => "Arch / Manjaro / EndeavourOS",
            Distro::Artix => "Artix (systemd-free Arch)",
            Distro::OpenSuse => "openSUSE",
            Distro::Gentoo => "Gentoo / Funtoo",
            Distro::Void => "Void Linux",
            Distro::NixOS => "NixOS",
            Distro::Other => chilpa_i18n::mark("Other"),
        }
    }
}

/// The running init: it is not the distro's (Gentoo runs OpenRC or
/// systemd), so service commands follow this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitSystem {
    Systemd,
    OpenRc,
    Runit,
    S6,
    Dinit,
    Unknown,
}

impl InitSystem {
    pub const ALL: [InitSystem; 6] = [InitSystem::Systemd, InitSystem::OpenRc, InitSystem::Runit, InitSystem::S6, InitSystem::Dinit, InitSystem::Unknown];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&i| i == self).unwrap_or(Self::ALL.len() - 1)
    }

    pub fn label(self) -> &'static str {
        match self {
            InitSystem::Systemd => "systemd",
            InitSystem::OpenRc => "OpenRC",
            InitSystem::Runit => "runit",
            InitSystem::S6 => "s6",
            InitSystem::Dinit => "dinit",
            InitSystem::Unknown => chilpa_i18n::mark("Other / unknown"),
        }
    }
}

/// The packaging sandbox. Inside one, `os-release`, `/run` and `/proc/1`
/// are the sandbox's, not the host's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sandbox {
    None,
    Flatpak,
    Snap,
}

pub fn sandbox() -> Sandbox {
    if std::path::Path::new("/.flatpak-info").exists() {
        Sandbox::Flatpak
    } else if std::env::var_os("SNAP").is_some() {
        Sandbox::Snap
    } else {
        Sandbox::None
    }
}

/// The init that runs: `/run/systemd/system` is `sd_booted()`, the others
/// each supervisor's runtime directory, then PID 1's name. In a sandbox it
/// cannot be told, and the wizard's own choice decides.
pub fn init_system() -> InitSystem {
    use std::path::Path;
    if sandbox() != Sandbox::None {
        return InitSystem::Unknown;
    }
    if Path::new("/run/systemd/system").exists() {
        return InitSystem::Systemd;
    }
    if Path::new("/run/openrc").exists() {
        return InitSystem::OpenRc;
    }
    if Path::new("/run/runit").exists() || Path::new("/etc/runit").exists() {
        return InitSystem::Runit;
    }
    if Path::new("/run/s6-rc").exists() || Path::new("/run/s6").exists() {
        return InitSystem::S6;
    }
    if Path::new("/run/dinitctl").exists() {
        return InitSystem::Dinit;
    }
    std::fs::read_to_string("/proc/1/comm").map(|comm| init_from_comm(comm.trim())).unwrap_or(InitSystem::Unknown)
}

fn init_from_comm(comm: &str) -> InitSystem {
    match comm {
        "systemd" => InitSystem::Systemd,
        "openrc-init" | "openrc" => InitSystem::OpenRc,
        "runit" | "runsvdir" | "runit-init" => InitSystem::Runit,
        "s6-svscan" | "s6-linux-init" => InitSystem::S6,
        "dinit" => InitSystem::Dinit,
        _ => InitSystem::Unknown,
    }
}

/// The host's distro, from the host's `os-release`: a sandbox exposes it
/// under its own path.
pub fn distro() -> Distro {
    let host = match sandbox() {
        Sandbox::Flatpak => Some("/run/host/os-release"),
        Sandbox::Snap => Some("/var/lib/snapd/hostfs/etc/os-release"),
        Sandbox::None => None,
    };
    host.and_then(|path| std::fs::read_to_string(path).ok())
        .or_else(|| std::fs::read_to_string("/etc/os-release").ok())
        .map(|text| distro_from(&text))
        .unwrap_or(Distro::Other)
}

/// `ID`, then `ID_LIKE`. The systemd-free derivatives go before their
/// families: antiX says it is like Debian and Artix like Arch.
fn distro_from(os_release: &str) -> Distro {
    let (mut id, mut like) = (String::new(), String::new());
    for line in os_release.lines() {
        // Bare, double-quoted or single-quoted (Gentoo's).
        let value = |v: &str| v.trim().trim_matches(|c| c == '"' || c == '\'').to_lowercase();
        if let Some(v) = line.strip_prefix("ID=") {
            id = value(v);
        } else if let Some(v) = line.strip_prefix("ID_LIKE=") {
            like = value(v);
        }
    }
    let haystack = format!("{id} {like}");
    let has = |needle: &str| haystack.contains(needle);
    if has("antix") {
        Distro::Antix
    } else if has("artix") {
        Distro::Artix
    } else if has("nixos") {
        Distro::NixOS
    } else if has("ubuntu") || has("debian") || has("mint") || has("pop") {
        Distro::Debian
    } else if has("fedora") || has("rhel") || has("centos") {
        Distro::Fedora
    } else if has("arch") || has("manjaro") || has("endeavour") {
        Distro::Arch
    } else if has("suse") {
        Distro::OpenSuse
    } else if has("gentoo") {
        Distro::Gentoo
    } else if has("void") {
        Distro::Void
    } else {
        Distro::Other
    }
}

/// One sink of the graph, as `pw-dump` describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SinkNode {
    /// `node.name`: what the manual entry asks for.
    pub node_name: String,
    pub description: String,
    /// "usb", "pci", "bluetooth" or empty.
    pub bus: String,
    pub is_default: bool,
    /// Backed by ALSA: a card, not a virtual sink.
    pub is_hardware: bool,
}

/// The graph's sinks: `pw-dump` first, which needs neither the Pulse layer
/// nor the ALSA bridge, then `pactl`. Blocking.
pub fn sink_nodes() -> Vec<SinkNode> {
    let dumped = run("pw-dump", &[]).map(|json| sinks_in_dump(&json)).unwrap_or_default();
    if !dumped.is_empty() {
        return dumped;
    }
    super::sinks()
        .into_iter()
        .map(|sink| SinkNode { node_name: sink.name, description: sink.description, ..SinkNode::default() })
        .collect()
}

/// The sinks of a `pw-dump`: its nodes of class `Audio/Sink`, with the bus
/// from the node or from its device, and the default from the `default`
/// metadata.
fn sinks_in_dump(json: &str) -> Vec<SinkNode> {
    let Ok(serde_json::Value::Array(objects)) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let kind = |o: &serde_json::Value| o["type"].as_str().unwrap_or_default().to_string();
    let default = objects
        .iter()
        .filter(|o| kind(o) == "PipeWire:Interface:Metadata" && o["props"]["metadata.name"] == "default")
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|entry| entry["key"] == "default.audio.sink")
        .and_then(|entry| entry["value"]["name"].as_str().map(str::to_string));
    let buses: std::collections::HashMap<i64, String> = objects
        .iter()
        .filter(|o| kind(o) == "PipeWire:Interface:Device")
        .filter_map(|o| Some((o["id"].as_i64()?, o["info"]["props"]["device.bus"].as_str()?.to_string())))
        .collect();
    objects
        .iter()
        .filter(|o| kind(o) == "PipeWire:Interface:Node")
        .map(|o| &o["info"]["props"])
        .filter(|props| props["media.class"] == "Audio/Sink")
        .filter_map(|props| {
            let node_name = props["node.name"].as_str()?.to_string();
            let text = |key: &str| props[key].as_str().map(str::to_string);
            Some(SinkNode {
                description: text("node.description").or_else(|| text("node.nick")).unwrap_or_else(|| node_name.clone()),
                bus: text("device.bus").or_else(|| props["device.id"].as_i64().and_then(|id| buses.get(&id).cloned())).unwrap_or_default(),
                is_default: default.as_deref() == Some(node_name.as_str()),
                is_hardware: props["device.api"] == "alsa" || props["factory.name"].as_str().is_some_and(|f| f.contains("alsa")),
                node_name,
            })
        })
        .collect()
}

/// The rates a sink's card lists, out of `/proc/asound/cardN/stream0`
/// (what the USB driver writes), the card found by `pactl`. `None` when it
/// cannot be told, or when the card takes any rate in a range.
pub fn sink_rates(node_name: &str) -> Option<Vec<u32>> {
    let card = card_of_sink(&run("pactl", &["list", "sinks"])?, node_name)?;
    rates_in_stream(&std::fs::read_to_string(format!("/proc/asound/card{card}/stream0")).ok()?)
}

/// The `alsa.card` of the sink named `node_name` in `pactl list sinks`.
fn card_of_sink(listing: &str, node_name: &str) -> Option<String> {
    let mut inside = false;
    for line in listing.lines().map(str::trim) {
        if line.starts_with("Sink #") {
            if inside {
                return None;
            }
        } else if let Some(name) = line.strip_prefix("Name:") {
            inside = name.trim() == node_name;
        } else if inside {
            if let Some(card) = line.strip_prefix("alsa.card = ") {
                return Some(card.trim_matches('"').to_string());
            }
        }
    }
    None
}

/// The playback rates of a `stream0`, every alternate setting's, sorted.
fn rates_in_stream(stream: &str) -> Option<Vec<u32>> {
    let (mut playback, mut rates) = (false, Vec::new());
    for line in stream.lines().map(str::trim) {
        if line == "Playback:" {
            playback = true;
        } else if line == "Capture:" {
            playback = false;
        }
        if let Some(listed) = line.strip_prefix("Rates:").filter(|_| playback) {
            if listed.contains("continuous") {
                return None;
            }
            for rate in listed.split(',').filter_map(|r| r.trim().parse::<u32>().ok()) {
                if !rates.contains(&rate) {
                    rates.push(rate);
                }
            }
        }
    }
    rates.sort_unstable();
    (!rates.is_empty()).then_some(rates)
}

/// What a card is running at while a stream is open on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Negotiated {
    pub rate: u32,
    /// ALSA's format: 24 bit usually travels as `S32_LE`.
    pub format: String,
    pub channels: u32,
}

/// One `hw_params`: `None` while the card is closed.
fn negotiated_in(hw_params: &str) -> Option<Negotiated> {
    let text = hw_params.trim();
    if text.is_empty() || text == "closed" {
        return None;
    }
    let (mut rate, mut format, mut channels) = (None, String::new(), 0);
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("rate:") {
            rate = rest.split_whitespace().next().and_then(|r| r.parse().ok());
        } else if let Some(rest) = line.strip_prefix("format:") {
            format = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("channels:") {
            channels = rest.trim().parse().unwrap_or(0);
        }
    }
    Some(Negotiated { rate: rate?, format, channels })
}

/// The first card playing anything, whichever it is: there is one output
/// at a time. `None` when none is open.
pub fn negotiated_now() -> Option<Negotiated> {
    (0..16).find_map(|card| {
        (0..4).find_map(|pcm| std::fs::read_to_string(format!("/proc/asound/card{card}/pcm{pcm}p/sub0/hw_params")).ok().and_then(|text| negotiated_in(&text)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distros_are_told_by_id_then_id_like() {
        assert_eq!(distro_from("NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n"), Distro::Debian);
        assert_eq!(distro_from("ID=pop\nID_LIKE=\"ubuntu debian\"\n"), Distro::Debian);
        assert_eq!(distro_from("ID=endeavouros\nID_LIKE=arch\n"), Distro::Arch);
        assert_eq!(distro_from("ID=fedora\n"), Distro::Fedora);
        assert_eq!(distro_from("ID=opensuse-tumbleweed\nID_LIKE=\"suse opensuse\"\n"), Distro::OpenSuse);
        assert_eq!(distro_from("ID='gentoo'\n"), Distro::Gentoo, "Gentoo quotes with single quotes");
        assert_eq!(distro_from("ID=void\n"), Distro::Void);
        assert_eq!(distro_from("ID=slackware\n"), Distro::Other);
        assert_eq!(distro_from(""), Distro::Other);
        assert_eq!(distro_from("ID=antix\nID_LIKE=debian\n"), Distro::Antix, "before its family");
        assert_eq!(distro_from("ID=artix\nID_LIKE=arch\n"), Distro::Artix);
        assert_eq!(distro_from("ID=nixos\nID_LIKE=\"\"\n"), Distro::NixOS);
        for d in Distro::ALL {
            assert_eq!(Distro::ALL[d.index()], d);
        }
    }

    #[test]
    fn the_init_is_told_by_pid_1() {
        assert_eq!(init_from_comm("systemd"), InitSystem::Systemd);
        assert_eq!(init_from_comm("openrc-init"), InitSystem::OpenRc);
        assert_eq!(init_from_comm("runit"), InitSystem::Runit);
        assert_eq!(init_from_comm("s6-svscan"), InitSystem::S6);
        assert_eq!(init_from_comm("dinit"), InitSystem::Dinit);
        assert_eq!(init_from_comm("busybox"), InitSystem::Unknown);
    }

    #[test]
    fn sinks_come_out_of_a_dump_with_their_bus_and_the_default() {
        let dump = r#"[
          {"id": 30, "type": "PipeWire:Interface:Device", "info": {"props": {"device.bus": "usb"}}},
          {"id": 31, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Audio/Sink",
            "node.name": "alsa_output.usb-Fosi-00.analog-stereo", "node.description": "ZH3 Analog Stereo",
            "device.id": 30, "device.api": "alsa"}}},
          {"id": 32, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Audio/Sink",
            "node.name": "easyeffects_sink", "node.nick": "Easy Effects Sink", "factory.name": "support.null-audio-sink"}}},
          {"id": 33, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Audio/Source", "node.name": "mic"}}},
          {"id": 0, "type": "PipeWire:Interface:Metadata", "props": {"metadata.name": "default"},
            "metadata": [{"key": "default.audio.sink", "value": {"name": "alsa_output.usb-Fosi-00.analog-stereo"}}]}
        ]"#;
        let sinks = sinks_in_dump(dump);
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0], SinkNode { node_name: "alsa_output.usb-Fosi-00.analog-stereo".into(), description: "ZH3 Analog Stereo".into(), bus: "usb".into(), is_default: true, is_hardware: true });
        assert_eq!((sinks[1].description.as_str(), sinks[1].is_hardware, sinks[1].is_default), ("Easy Effects Sink", false, false));
        assert!(sinks_in_dump("not json").is_empty());
    }

    #[test]
    fn a_sink_s_card_and_its_rates_are_read_from_their_listings() {
        let listing = "Sink #1\n\tName: other\n\tProperties:\n\t\talsa.card = \"0\"\nSink #2\n\tName: alsa_output.usb-x\n\tProperties:\n\t\talsa.card = \"2\"\n";
        assert_eq!(card_of_sink(listing, "alsa_output.usb-x").as_deref(), Some("2"));
        assert_eq!(card_of_sink(listing, "missing"), None);
        let stream = "USB Audio\n\nPlayback:\n  Status: Stop\n  Interface 1\n    Rates: 44100, 48000, 96000\n  Interface 1\n    Rates: 192000, 96000\n\nCapture:\n    Rates: 8000\n";
        assert_eq!(rates_in_stream(stream), Some(vec![44100, 48000, 96000, 192000]));
        assert_eq!(rates_in_stream("Playback:\n    Rates: 8000 - 192000 (continuous)\n"), None);
        assert_eq!(rates_in_stream(""), None);
    }

    #[test]
    fn the_card_s_live_state_is_read_and_a_closed_card_says_nothing() {
        let open = "access: MMAP_INTERLEAVED\nformat: S32_LE\nsubformat: STD\nchannels: 2\nrate: 192000 (192000/1)\nperiod_size: 2048\n";
        assert_eq!(negotiated_in(open), Some(Negotiated { rate: 192_000, format: "S32_LE".into(), channels: 2 }));
        assert_eq!(negotiated_in("closed\n"), None);
        assert_eq!(negotiated_in(""), None);
        assert_eq!(negotiated_in("format: S16_LE\nchannels: 2\n"), None, "no rate, nothing to say");
    }
}
