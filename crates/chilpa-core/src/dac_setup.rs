//! The HiFi Wizard's text, ported from QBZ (`qbz-dac-wizard-core`): the
//! check step's verdict and the commands that fix what is missing, per
//! distro and init; the DACs step's candidates; the three files the review
//! step offers to paste; and the test step's read-back.
//!
//! It writes nothing and runs no command: the probes are
//! `chilpa_pipewire::setup`'s, and everything here only formats text.

use chilpa_pipewire::setup::{self, Distro, InitSystem, Sandbox};
pub use chilpa_pipewire::setup::{Health, Negotiated};

pub fn distro_options() -> Vec<String> {
    Distro::ALL.iter().map(|d| chilpa_i18n::t(d.label())).collect()
}

pub fn init_options() -> Vec<String> {
    InitSystem::ALL.iter().map(|i| chilpa_i18n::t(i.label())).collect()
}

pub fn detected_distro_index() -> i32 {
    setup::distro().index() as i32
}

pub fn detected_init_index() -> i32 {
    setup::init_system().index() as i32
}

/// "Flatpak", "Snap", or empty outside a sandbox.
pub fn sandbox_name() -> &'static str {
    match setup::sandbox() {
        Sandbox::Flatpak => "Flatpak",
        Sandbox::Snap => "Snap",
        Sandbox::None => "",
    }
}

fn distro_at(index: i32) -> Distro {
    Distro::ALL.get(index.max(0) as usize).copied().unwrap_or(Distro::Other)
}

fn init_at(index: i32) -> InitSystem {
    InitSystem::ALL.get(index.max(0) as usize).copied().unwrap_or(InitSystem::Unknown)
}

/// One fix: what it is for and the command to paste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remediation {
    pub caption: String,
    pub command: String,
}

#[derive(Clone, Debug, Default)]
pub struct CheckView {
    pub health_ok: bool,
    /// Empty in a sandbox: there is no verdict, only reference commands.
    pub summary: String,
    pub remediations: Vec<Remediation>,
}

/// The check step, for a finished probe and the two dropdowns.
pub fn check_view(health: Health, distro_index: i32, init_index: i32, sandboxed: bool) -> CheckView {
    let (distro, init) = (distro_at(distro_index), init_at(init_index));
    if sandboxed {
        return CheckView { health_ok: false, summary: String::new(), remediations: reference_commands(distro, init) };
    }
    let rows = remediations(health, distro, init);
    let ok = health.is_ready();
    let summary = if ok {
        chilpa_i18n::t("Your audio stack is ready for bit-perfect playback.")
    } else {
        let n = rows.len();
        chilpa_i18n::tf(
            "{} item needs attention before bit-perfect playback will work.",
            "{} items need attention before bit-perfect playback will work.",
            n as i64,
            &[&n.to_string()],
        )
    };
    CheckView { health_ok: ok, summary, remediations: rows }
}

/// One fix per missing piece. Installing and restarting are separate
/// blocks, so Gentoo's lines are never joined with `&&`.
fn remediations(h: Health, d: Distro, init: InitSystem) -> Vec<Remediation> {
    // NixOS is declarative: one configuration block, not packages.
    if d == Distro::NixOS {
        if h.is_ready() {
            return Vec::new();
        }
        return vec![Remediation { caption: chilpa_i18n::t("Enable PipeWire in your NixOS configuration"), command: NIXOS_PIPEWIRE_BLOCK.to_string() }];
    }
    let fix = |caption: &str, command: String| Remediation { caption: chilpa_i18n::t(caption), command };
    let mut out = Vec::new();
    let mut restart = false;
    if !h.has_pw_dump {
        out.push(fix(chilpa_i18n::mark("Install the PipeWire tools (pw-dump)"), install(d, pkg_pw_tools(d))));
        restart = true;
    }
    if !h.alsa_sees_pipewire {
        // The ALSA to PipeWire bridge: without it nothing is listed and
        // nothing plays.
        out.push(fix(chilpa_i18n::mark("Install the ALSA bridge so playback can reach PipeWire"), install(d, "pipewire-alsa")));
        restart = true;
    }
    if !h.has_pactl {
        out.push(fix(chilpa_i18n::mark("Install the Pulse compatibility tools (optional fallback)"), install(d, pkg_pulse(d))));
        restart = true;
    }
    if !h.any_devices {
        out.push(fix(chilpa_i18n::mark("No sinks detected — reinstall the ALSA UCM profiles, then reboot"), install_reinstall(d, "alsa-ucm-conf")));
    }
    // With the init that runs here, not the one the distro usually has.
    if !h.wireplumber_active || restart {
        out.push(fix(chilpa_i18n::mark("(Re)start the PipeWire audio services"), restart_cmd(init).to_string()));
    }
    out
}

/// PipeWire runs in the user's session: only systemd restarts it with
/// `--user`; the others use their user supervisor or a new login.
fn restart_cmd(init: InitSystem) -> &'static str {
    match init {
        InitSystem::Systemd => "systemctl --user restart pipewire pipewire-pulse wireplumber",
        InitSystem::OpenRc => "# OpenRC: PipeWire runs in your user session, not as an OpenRC service.\n# Log out and back in to restart it.",
        InitSystem::Runit => "sv restart pipewire wireplumber   # if set up as runit user services; otherwise log out and back in",
        InitSystem::S6 => "# s6: restart via your supervision tree, or log out and back in",
        InitSystem::Dinit => "dinitctl restart pipewire wireplumber   # or log out and back in",
        InitSystem::Unknown => "# Restart PipeWire via your init system, or log out and back in",
    }
}

/// The review step's restart, for a dropdown index.
pub fn restart_command_for(init_index: i32) -> String {
    restart_cmd(init_at(init_index)).to_string()
}

fn pkg_pw_tools(d: Distro) -> &'static str {
    match d {
        Distro::Debian | Distro::Antix => "pipewire-bin",
        Distro::Fedora => "pipewire-utils",
        _ => "pipewire",
    }
}

fn pkg_pulse(d: Distro) -> &'static str {
    match d {
        Distro::Debian | Distro::Antix => "pipewire-pulse pulseaudio-utils",
        Distro::Fedora => "pipewire-pulseaudio",
        _ => "pipewire-pulse",
    }
}

/// The package manager is the distro family's, not the init's.
fn install(d: Distro, pkgs: &str) -> String {
    match d {
        Distro::Debian | Distro::Antix => format!("sudo apt install {pkgs}"),
        Distro::Fedora => format!("sudo dnf install {pkgs}"),
        Distro::Arch | Distro::Artix => format!("sudo pacman -S {pkgs}"),
        Distro::OpenSuse => format!("sudo zypper install {pkgs}"),
        Distro::Gentoo => format!("sudo emerge {pkgs}   # package name may differ on Gentoo"),
        Distro::Void => format!("sudo xbps-install -S {pkgs}"),
        Distro::NixOS => format!("# NixOS: add to configuration.nix (see the PipeWire block) — {pkgs}"),
        Distro::Other => format!("Install with your package manager: {pkgs}"),
    }
}

fn install_reinstall(d: Distro, pkg: &str) -> String {
    match d {
        Distro::Debian | Distro::Antix => format!("sudo apt install --reinstall {pkg}"),
        Distro::Fedora => format!("sudo dnf reinstall {pkg}"),
        _ => install(d, pkg),
    }
}

const NIXOS_PIPEWIRE_BLOCK: &str = "# /etc/nixos/configuration.nix:\n\
     services.pipewire = {\n\
     \u{20}\u{20}enable = true;\n\
     \u{20}\u{20}alsa.enable = true;\n\
     \u{20}\u{20}pulse.enable = true;\n\
     \u{20}\u{20}wireplumber.enable = true;\n\
     };\n\
     # then apply:\n\
     sudo nixos-rebuild switch";

/// In a sandbox nothing is probed: the whole stack's install and restart.
fn reference_commands(d: Distro, init: InitSystem) -> Vec<Remediation> {
    if d == Distro::NixOS {
        return vec![Remediation { caption: chilpa_i18n::t("Enable PipeWire in your NixOS configuration"), command: NIXOS_PIPEWIRE_BLOCK.to_string() }];
    }
    vec![
        Remediation { caption: chilpa_i18n::t("Install the PipeWire audio stack"), command: install(d, full_stack_pkgs(d)) },
        Remediation { caption: chilpa_i18n::t("(Re)start the PipeWire audio services"), command: restart_cmd(init).to_string() },
    ]
}

fn full_stack_pkgs(d: Distro) -> &'static str {
    match d {
        Distro::Debian | Distro::Antix => "pipewire pipewire-pulse pipewire-alsa wireplumber alsa-utils",
        Distro::Fedora => "pipewire pipewire-pulseaudio pipewire-alsa wireplumber alsa-utils",
        Distro::Arch | Distro::Artix => "pipewire pipewire-pulse pipewire-alsa wireplumber alsa-utils",
        Distro::OpenSuse => "pipewire pipewire-pulseaudio pipewire-alsa wireplumber alsa-utils",
        Distro::Gentoo => "media-video/pipewire media-video/wireplumber media-sound/alsa-utils",
        Distro::Void => "pipewire wireplumber alsa-utils",
        Distro::NixOS => "",
        Distro::Other => "pipewire pipewire-pulse wireplumber alsa-utils",
    }
}

/// One sink offered to configure.
#[derive(Clone, Debug, Default)]
pub struct DacCandidate {
    pub id: String,
    pub description: String,
    pub bus: String,
    pub is_default: bool,
    /// A card on USB or PCI: checked from the start.
    pub looks_like_dac: bool,
    /// "44.1 / 96 / 192 kHz".
    pub rates_label: String,
}

/// The rates assumed when the card does not list them: a range of rates,
/// a card that is not USB, a `stream0` that is not there.
const FALLBACK_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/// A sink's rates: the ones its card lists, or the common set.
fn dac_rates(node_name: &str) -> Vec<u32> {
    setup::sink_rates(node_name).filter(|rates| !rates.is_empty()).unwrap_or_else(|| FALLBACK_RATES.to_vec())
}

/// The graph's sinks, with the rates of the ones that look like a DAC.
/// Blocking.
pub fn detect_blocking() -> Vec<DacCandidate> {
    setup::sink_nodes()
        .into_iter()
        .map(|sink| {
            let looks_like_dac = sink.is_hardware && (sink.bus == "usb" || sink.bus == "pci");
            DacCandidate {
                rates_label: if looks_like_dac {
                    setup::sink_rates(&sink.node_name).filter(|r| !r.is_empty()).map(|r|format_rates(&r))
                        .unwrap_or_else(||chilpa_i18n::t_args("Assumed fallback: {}",&[&format_rates(&FALLBACK_RATES)]))
                } else { String::new() },
                description: if sink.description.is_empty() { sink.node_name.clone() } else { sink.description },
                bus: sink.bus,
                is_default: sink.is_default,
                looks_like_dac,
                id: sink.node_name,
            }
        })
        .collect()
}

/// A pasted `node.name` is one of ALSA's.
pub fn validate_node_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && (name.contains("alsa_output") || name.contains("alsa_input"))
}

pub fn detect_dac_type(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("usb-") || lower.contains(".usb") {
        "usb"
    } else if lower.contains("pci-") || lower.contains(".pci") {
        "pci"
    } else if lower.contains("bluez") || lower.contains("bluetooth") {
        "bluetooth"
    } else if lower.contains("virtual") || lower.contains("null") || lower.contains("dummy") {
        "virtual"
    } else {
        "unknown"
    }
}

/// "44.1 / 96 / 192 kHz"; a decimal only where there is one.
pub fn format_rates(rates: &[u32]) -> String {
    if rates.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = rates.iter().map(|&r| if r % 1000 == 0 { format!("{}", r / 1000) } else { format!("{:.1}", f64::from(r) / 1000.0) }).collect();
    format!("{} kHz", parts.join(" / "))
}

/// The read-back's two lines, and whether the card runs at what was asked.
#[derive(Clone, Debug, Default)]
pub struct PollView {
    pub requested_label: String,
    pub negotiated_label: String,
    pub rate_matched: bool,
}

/// What Chilpa asked the device for against what the card runs at.
pub fn poll_view(requested_rate: u32, requested_bits: u32, negotiated: Option<Negotiated>) -> PollView {
    let requested_label = if requested_rate > 0 {
        chilpa_i18n::t_args("Chilpa requesting {} · {}-bit", &[&khz(requested_rate), &requested_bits.to_string()])
    } else {
        chilpa_i18n::t("Nothing playing")
    };
    match negotiated {
        // The card's own parameters: its rate, ALSA's container format
        // (24 bit in S32_LE) and its channels.
        Some(n) => PollView {
            requested_label,
            negotiated_label: chilpa_i18n::t_args("DAC: {} · {} · {} ch", &[&khz(n.rate), &n.format, &n.channels.to_string()]),
            rate_matched: requested_rate > 0 && n.rate == requested_rate,
        },
        None => PollView { requested_label, negotiated_label: chilpa_i18n::t("Waiting for the DAC to start playing…"), rate_matched: false },
    }
}

pub fn negotiated_output(output: &str) -> Option<Negotiated> {
    setup::negotiated_output(output)
}

pub fn probe_health() -> Health {
    setup::health()
}

fn khz(hz: u32) -> String {
    if hz % 1000 == 0 {
        format!("{} kHz", hz / 1000)
    } else {
        format!("{:.1} kHz", f64::from(hz) / 1000.0)
    }
}

/// One DAC's three files, to paste. Chilpa never writes them.
#[derive(Clone, Debug, Default)]
pub struct DacConfig {
    pub name: String,
    pub node_name: String,
    pub pipewire_conf: String,
    pub pulse_conf: String,
    pub wireplumber_conf: String,
}

/// Each DAC's rates read again and its three files. Blocking.
pub fn gen_configs_blocking(dacs: Vec<(String, String)>) -> Vec<DacConfig> {
    dacs.into_iter()
        .map(|(node_name, name)| {
            let rates = dac_rates(&node_name);
            let short = short_name(&name, &node_name);
            DacConfig {
                pipewire_conf: pipewire_conf(&short, &rates),
                pulse_conf: pulse_conf(&short),
                wireplumber_conf: wireplumber_conf(&short, &node_name, &rates, &name),
                name,
                node_name,
            }
        })
        .collect()
}

/// The done step's list: three paths per DAC, in the review step's order.
pub fn created_paths(configs: &[DacConfig]) -> Vec<String> {
    configs
        .iter()
        .flat_map(|d| {
            let short = short_name(&d.name, &d.node_name);
            [
                format!("~/.config/pipewire/pipewire.conf.d/99-chilpa-dac-{short}.conf"),
                format!("~/.config/pipewire/client.conf.d/99-chilpa-bitperfect-{short}.conf"),
                format!("~/.config/wireplumber/wireplumber.conf.d/99-chilpa-dac-{short}.conf"),
            ]
        })
        .collect()
}

pub const BACKUP_CMD: &str = "BACKUP=~/.config/chilpa/backups/pipewire-$(date +%Y%m%d-%H%M%S)\nmkdir -p \"$BACKUP\"\ncp -a ~/.config/pipewire \"$BACKUP/\" 2>/dev/null || true\ncp -a ~/.config/wireplumber \"$BACKUP/\" 2>/dev/null || true\necho \"Backup created at: $BACKUP\"";

/// A file-safe name: the description's slug, else the node's, else "dac".
fn short_name(name: &str, node_name: &str) -> String {
    [slugify(name), slugify(node_name)].into_iter().find(|s| !s.is_empty()).unwrap_or_else(|| "dac".to_string())
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn rates_list(rates: &[u32]) -> String {
    let rates = if rates.is_empty() { &FALLBACK_RATES[..] } else { rates };
    rates.iter().map(u32::to_string).collect::<Vec<_>>().join(" ")
}

fn pipewire_conf(short: &str, rates: &[u32]) -> String {
    [
        "mkdir -p ~/.config/pipewire/pipewire.conf.d".to_string(),
        format!("cat > ~/.config/pipewire/pipewire.conf.d/99-chilpa-dac-{short}.conf << 'EOF'"),
        "# Chilpa DAC Setup - Sample Rate Switching".to_string(),
        "context.properties = {".to_string(),
        format!("  default.clock.allowed-rates = [ {} ]", rates_list(rates)),
        "}".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

fn pulse_conf(short: &str) -> String {
    [
        "mkdir -p ~/.config/pipewire/client.conf.d".to_string(),
        format!("cat > ~/.config/pipewire/client.conf.d/99-chilpa-bitperfect-{short}.conf << 'EOF'"),
        "# Chilpa DAC Setup - Per-App Bit-Perfect".to_string(),
        "stream.rules = [".to_string(),
        "  {".to_string(),
        "    matches = [".to_string(),
        "      { application.process.binary = \"chilpa-qt\" }".to_string(),
        "      { application.name = \"PipeWire ALSA [chilpa-qt]\" }".to_string(),
        "    ]".to_string(),
        "    actions = { update-props = { resample.disable = true, channelmix.disable = true } }".to_string(),
        "  }".to_string(),
        "]".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

fn wireplumber_conf(short: &str, node_name: &str, rates: &[u32], description: &str) -> String {
    [
        "mkdir -p ~/.config/wireplumber/wireplumber.conf.d".to_string(),
        format!("cat > ~/.config/wireplumber/wireplumber.conf.d/99-chilpa-dac-{short}.conf << 'EOF'"),
        format!("# Chilpa DAC Setup - {description}"),
        "monitor.alsa.rules = [".to_string(),
        "  {".to_string(),
        "    matches = [".to_string(),
        format!("      {{ node.name = \"{node_name}\", media.class = \"Audio/Sink\" }}"),
        "    ]".to_string(),
        "    actions = {".to_string(),
        "      update-props = {".to_string(),
        format!("        audio.allowed-rates = [ {} ]", rates_list(rates)),
        "        resample.disable = true".to_string(),
        "        channelmix.disable = true".to_string(),
        "      }".to_string(),
        "    }".to_string(),
        "  }".to_string(),
        "]".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Health = Health { wireplumber_active: true, has_pw_dump: true, alsa_sees_pipewire: true, has_pactl: true, any_devices: true };

    #[test]
    fn a_pasted_node_name_is_one_of_alsa_s() {
        assert!(validate_node_name("alsa_output.usb-Cambridge-00.analog-stereo"));
        assert!(validate_node_name("alsa_input.pci-0000_00.analog-stereo"));
        assert!(!validate_node_name(""));
        assert!(!validate_node_name("   "));
        assert!(!validate_node_name("bluez_output.AA_BB"));
        assert_eq!(detect_dac_type("alsa_output.usb-Cambridge-00.analog-stereo"), "usb");
        assert_eq!(detect_dac_type("alsa_output.pci-0000_00_1f.3.analog-stereo"), "pci");
        assert_eq!(detect_dac_type("bluez_output.AA"), "bluetooth");
        assert_eq!(detect_dac_type("alsa_output.virtual-dummy"), "virtual");
        assert_eq!(detect_dac_type("something.else"), "unknown");
    }

    #[test]
    fn rates_read_in_kilohertz() {
        assert_eq!(format_rates(&[44_100, 96_000, 192_000]), "44.1 / 96 / 192 kHz");
        assert_eq!(format_rates(&[]), "");
        assert_eq!(slugify("DacMagic Plus Analog Stereo"), "dacmagic-plus-analog-stereo");
        assert_eq!(slugify("  weird__name!! "), "weird-name");
        assert_eq!(short_name("", "!!"), "dac");
    }

    #[test]
    fn the_wireplumber_file_pins_the_node_and_its_rates() {
        let c = wireplumber_conf("dacmagic", "alsa_output.usb-x.analog-stereo", &[44_100, 192_000], "DacMagic");
        assert!(c.contains("node.name = \"alsa_output.usb-x.analog-stereo\""));
        assert!(c.contains("audio.allowed-rates = [ 44100 192000 ]"));
        assert!(c.contains("99-chilpa-dac-dacmagic.conf"));
        assert!(c.contains("resample.disable = true"));
        assert!(pulse_conf("x").contains("application.process.binary = \"chilpa-qt\""));
        assert!(pipewire_conf("x", &[]).contains("[ 44100 48000 88200 96000 176400 192000 ]"), "the common set when none is listed");
    }

    #[test]
    fn three_paths_per_dac_in_the_review_step_s_order() {
        let cfg = DacConfig { name: "DacMagic Plus".into(), node_name: "alsa_output.usb-x.analog-stereo".into(), ..Default::default() };
        let paths = created_paths(&[cfg]);
        assert_eq!(paths.len(), 3);
        assert!(paths[0].contains("pipewire.conf.d/99-chilpa-dac-dacmagic-plus.conf"));
        assert!(paths[1].contains("client.conf.d/99-chilpa-bitperfect-dacmagic-plus.conf"));
        assert!(paths[2].contains("wireplumber.conf.d/99-chilpa-dac-dacmagic-plus.conf"));
    }

    #[test]
    fn a_sandbox_gets_reference_commands_and_no_verdict() {
        let view = check_view(READY, 0, 0, true);
        assert!(view.summary.is_empty());
        assert!(!view.health_ok);
        assert_eq!(view.remediations.len(), 2);
    }

    #[test]
    fn what_is_missing_is_fixed_with_this_distro_s_manager_and_this_init_s_restart() {
        let missing = Health { alsa_sees_pipewire: false, ..READY };
        let view = check_view(missing, Distro::Fedora.index() as i32, InitSystem::OpenRc.index() as i32, false);
        assert!(!view.health_ok);
        assert_eq!(view.remediations[0].command, "sudo dnf install pipewire-alsa");
        assert!(view.remediations[1].command.starts_with("# OpenRC"), "the restart follows the init, not the distro");
        assert!(check_view(READY, 0, 0, false).remediations.is_empty());
        let nix = check_view(missing, Distro::NixOS.index() as i32, 0, false);
        assert_eq!(nix.remediations.len(), 1, "NixOS is one configuration block");
    }

    #[test]
    fn the_read_back_says_what_was_asked_and_what_the_card_runs_at() {
        let card = Negotiated { rate: 96_000, format: "S32_LE".into(), channels: 2 };
        let view = poll_view(96_000, 24, Some(card.clone()));
        assert!(view.rate_matched);
        assert!(view.requested_label.contains("96 kHz") && view.negotiated_label.contains("S32_LE"));
        assert!(!poll_view(44_100, 16, Some(card)).rate_matched);
        let idle = poll_view(0, 0, None);
        assert!(!idle.rate_matched);
        assert_eq!(idle.requested_label, "Nothing playing");
    }
}
