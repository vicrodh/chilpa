//! The outputs: which stacks there are, what each offers, which one is
//! chosen and under what policy, its volume and its diagnosis.

use crate::{Output, OutputStack, WizardReport, WizardRow};

/// Whether cards are asked of the sound server before they are opened.
pub(crate) const RESERVE: &str = "audio.reserve";

/// Whether PipeWire's graph follows each track's rate.
pub(crate) const FORCE_RATE: &str = "audio.forceRate";

/// The HiFi Wizard's row. Its configuration is PipeWire's, so it is shown
/// on Linux only, under every stack there.
pub(crate) const HIFI_WIZARD: &str = "audio.hifiWizard";

#[cfg(target_os = "linux")]
mod hifi_wizard;

/// File capture is a development output, separate from normal listening devices.
fn developer_outputs() -> bool { std::env::var_os("CHILPA_DEV").is_some_and(|value| value == "1") }

/// Off Linux the wizard has nothing to set up: its row is hidden, and the
/// window's calls find a closed document.
#[cfg(not(target_os = "linux"))]
impl crate::Chilpa {
    pub fn wizard_doc(&mut self, _seen: u64) -> Option<(u64, String)> {
        None
    }
    pub fn wizard_command(&mut self, _action: &str, _argument: &str) {}
    pub(crate) fn wizard_maintenance(&mut self, _key: &str) -> Option<String> {
        None
    }
}

impl crate::Chilpa {
    pub fn diagnosis_requires_stop(&self) -> bool {
        let (output, _) = self.chosen_output();
        self.now_playing().playing && (self.exclusive() || self.stack_of(&output) == "alsa")
    }

    #[cfg(target_os = "linux")]
    pub fn choose_alsa_hardware_volume(&mut self, choice: &str) -> Result<(), String> {
        let (output, _) = self.chosen_output();
        chilpa_core::select_alsa_hardware_volume(&output, choice)?;
        self.set_hardware_volume(self.hardware_volume());
        Ok(())
    }

    /// The wizard's row, hidden where it cannot finish, and the diagnosis
    /// with it off Linux: there is nothing to configure on a Mac, which is
    /// so neither appears there, and the diagnosis would only say the
    /// device cannot be probed.
    pub(crate) fn wizard_row(&self, row: &mut crate::SettingsRow) -> bool {
        let linux = cfg!(target_os = "linux");
        if row.key != HIFI_WIZARD && (linux || row.key != "audio.wizard") {
            return false;
        }
        if !linux {
            row.control = String::new();
        }
        true
    }
}

/// The card's stack and the ones through `plughw` and ALSA's PCM layer, as
/// identifiers name them.
const CARD: &str = "alsa";
const PLUG: &str = "alsaplug";
const PCM: &str = "alsapcm";

/// The card and device a card output names, whichever plugin it goes
/// through: `alsa:hw:1,0`, `alsaplug:plughw:1,0`, `alsapcm:sysdefault:CARD=1`.
fn card_and_device(output_id: &str) -> Option<(u32, u32)> {
    let (_, name) = output_id.split_once(':')?;
    if let Some(card) = name.strip_prefix("sysdefault:CARD=") {
        return Some((card.parse().ok()?, 0));
    }
    let (card, device) = name.strip_prefix("hw:").or_else(|| name.strip_prefix("plughw:"))?.split_once(',')?;
    Some((card.parse().ok()?, device.parse().ok()?))
}

fn word(fidelity: chilpa_core::backends::Fidelity) -> &'static str {
    match fidelity {
        chilpa_core::backends::Fidelity::Exact => "exact",
        chilpa_core::backends::Fidelity::Depends => "depends",
        chilpa_core::backends::Fidelity::Never => "never",
    }
}

/// The stack an output identifier names, exactly.
fn own_stack(output_id: &str) -> &str {
    output_id.split_once(':').map_or("", |(key, _)| key)
}

impl crate::Chilpa {
    /// Whether the machine's outputs changed since this was last asked.
    ///
    /// The window calls it on its timer so that plugging a DAC in makes it
    /// appear without anybody pressing anything. It is cheap on purpose: the
    /// stacks that can answer without opening or asking over a socket hand
    /// back something short, and only when that moves does the window go and
    /// rebuild the list.
    ///
    /// The first call is always false. There is nothing to have changed from
    /// yet, and answering true would make the window rebuild the list one
    /// tick after it had just built it.
    pub fn outputs_changed(&mut self) -> bool {
        let now = chilpa_core::backends::generation();
        if self.outputs_generation.is_empty() {
            self.outputs_generation = now;
            return false;
        }
        if now == self.outputs_generation {
            return false;
        }
        self.outputs_generation = now;
        true
    }

    /// Moves to another output under the policy already chosen.
    pub fn use_output_as_chosen(&mut self, output: &str) {
        let (_, bit_perfect) = self.chosen_output();
        self.use_output(output, bit_perfect);
    }

    /// An output's diagnosis: which formats come out untouched.
    ///
    /// It measures the device for real and configures nothing.
    pub fn wizard(&mut self, device: &str) -> WizardReport {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        let caps = match chilpa_core::probe_device(&id) {
            Ok(c) => c,
            Err(e) => {
                return WizardReport {
                    device: id,
                    probed: false,
                    summary: chilpa_i18n::t_args("the device could not be opened: {}", &[&e.to_string()]),
                    rates: Vec::new(),
                    formats: Vec::new(),
                    rows: Vec::new(),
                }
            }
        };
        let d = chilpa_core::wizard::diagnose(&id, &caps);
        WizardReport {
            device: d.device,
            probed: d.probed,
            summary: d.summary,
            rates: d.sample_rates,
            formats: d.formats,
            rows: d
                .rows
                .into_iter()
                .map(|f| WizardRow {
                    // The name and the spelled out format are marked where
                    // they are built, in the wizard; translating them here is
                    // what makes the panel follow the chosen language.
                    name: chilpa_i18n::t(&f.name),
                    format: f.format,
                    verdict: match f.verdict {
                        chilpa_core::wizard::Verdict::BitPerfect => "bit-perfect".into(),
                        chilpa_core::wizard::Verdict::Converted { a } => {
                            chilpa_i18n::t_args("converted to {}", &[&a])
                        }
                        chilpa_core::wizard::Verdict::Unsupported => chilpa_i18n::t("cannot be done"),
                    },
                })
                .collect(),
        }
    }

    /// The chosen output, or the first one there is.
    pub(crate) fn device_or_first(&self) -> String {
        self.outputs().first().map(|o| o.id.clone()).unwrap_or_default()
    }

    /// What the chosen output's own volume control is doing.
    ///
    /// Chilpa never scales samples, so the only volume there is is the
    /// device's. An output without one says so rather than growing a slider
    /// that would silently stop the audio being bit-perfect.
    pub fn volume(&mut self, device: &str) -> chilpa_core::VolumeState {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        chilpa_core::volume_of(&id)
    }

    /// Moves the output's own volume control. Says why if it cannot.
    pub fn set_volume(&mut self, device: &str, percent: u8) -> bool {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        match chilpa_core::set_volume(&id, percent) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = e;
                false
            }
        }
    }

    /// Mutes or unmutes the output. Says why if it cannot.
    pub fn set_muted(&mut self, device: &str, muted: bool) -> bool {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        match chilpa_core::set_muted(&id, muted) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = e;
                false
            }
        }
    }

    /// The audio stacks, present or not.
    ///
    /// The ones that are not here come with their reason rather than being
    /// dropped: a picker that silently leaves out what it could not find
    /// teaches nobody anything, and the reason is usually one package away.
    ///
    /// The card through `plughw` is not among them: it is the card with
    /// another plugin, and the window offers it as that (`alsa_plugin`).
    pub fn stacks(&self) -> Vec<OutputStack> {
        chilpa_core::backends::described()
            .into_iter()
            .filter(|s| !matches!(s.kind, chilpa_core::backends::StackKind::AlsaPlug | chilpa_core::backends::StackKind::AlsaPcm)
                && (s.kind != chilpa_core::backends::StackKind::File || developer_outputs()))
            .map(|s| OutputStack {
                key: s.kind.key().to_string(),
                label: s.label(),
                present: s.present,
                missing: s.missing,
                fidelity: word(s.fidelity).to_string(),
                caveat: s.caveat,
            })
            .collect()
    }

    /// What one stack offers. Empty for one that is not here, and for one
    /// that is here with nothing plugged into it -- which are different
    /// things, and `stacks` is where they are told apart.
    pub fn stack_outputs(&self, key: &str) -> Vec<Output> {
        if key == "file" && !developer_outputs() { return Vec::new(); }
        let Some(kind) = chilpa_core::backends::StackKind::from_key(key) else {
            return Vec::new();
        };
        chilpa_core::backends::outputs_of(kind)
            .into_iter()
            .map(|o| Output {
                id: o.id,
                label: o.label,
                detail: o.detail,
                bit_perfect: o.bit_perfect,
            })
            .collect()
    }

    /// The stack an output belongs to, out of its identifier, as `stacks`
    /// offers them: the card through `plughw` is the card.
    pub fn stack_of(&self, output_id: &str) -> String {
        match own_stack(output_id) {
            PLUG | PCM => CARD.to_string(),
            key => key.to_string(),
        }
    }

    /// The outputs beside this one: its stack, and its plugin.
    pub fn outputs_beside(&self, output_id: &str) -> Vec<Output> {
        self.stack_outputs(own_stack(output_id))
    }

    /// The output and its stack as the window names them: the device's
    /// label and "ALSA (plughw)".
    pub fn output_names(&self, output_id: &str) -> (String, String) {
        let key = own_stack(output_id);
        let device = self.stack_outputs(key).into_iter().find(|o| o.id == output_id).map(|o| o.label);
        let stack = chilpa_core::backends::StackKind::from_key(key)
            .and_then(chilpa_core::backends::of)
            .map(|s| s.info().label());
        (device.unwrap_or_else(|| output_id.to_string()), stack.unwrap_or_else(|| key.to_string()))
    }

    /// The chosen output as the player shows it: the device, and its stack
    /// with how the music reaches it ("ALSA (plughw) · direct").
    pub fn output_route(&self) -> (String, String) {
        let (output, _) = self.chosen_output();
        let (device, label) = self.output_names(&output);
        let route = match (self.fidelity_of(&output), self.exclusive()) {
            ("exact", _) => chilpa_i18n::t("direct"),
            ("depends", true) => chilpa_i18n::t("exclusive"),
            _ => chilpa_i18n::t("shared"),
        };
        (device, format!("{label} · {route}"))
    }

    /// What the output's stack promises: "exact", "depends" or "never".
    /// One nobody knows promises nothing.
    pub fn fidelity_of(&self, output_id: &str) -> &'static str {
        chilpa_core::backends::StackKind::from_key(own_stack(output_id))
            .and_then(chilpa_core::backends::of)
            .map_or("never", |s| word(s.info().fidelity))
    }

    /// Whether a card is asked of the sound server that holds it before it
    /// is opened, and given back when Chilpa closes it or lets go of it in a
    /// long pause.
    pub fn reserving(&self) -> bool {
        self.preference(RESERVE).as_deref() == Some("true")
    }

    pub fn set_reserving(&mut self, on: bool) {
        self.remember_preference(RESERVE, &on.to_string());
        chilpa_core::backends::set_reserving(on);
    }

    /// Whether PipeWire's graph is set to each track's rate while Chilpa
    /// has the output alone, and set back when it stops.
    pub fn forcing_rate(&self) -> bool {
        self.preference(FORCE_RATE).as_deref() == Some("true")
    }

    /// The rate is set when the output opens, so what is playing is opened
    /// again where it is.
    pub fn set_forcing_rate(&mut self, on: bool) {
        self.remember_preference(FORCE_RATE, &on.to_string());
        chilpa_core::backends::set_forcing_rate(on);
        let (output, bit_perfect) = self.chosen_output();
        self.move_output(&output, bit_perfect);
    }

    /// "hw", "plughw" or "pcm" when the chosen output is a card; empty
    /// otherwise.
    pub fn alsa_plugin(&self) -> &'static str {
        match own_stack(&self.chosen_output().0) {
            CARD => "hw",
            PLUG => "plughw",
            PCM => "pcm",
            _ => "",
        }
    }

    /// The plugins the chosen card can be reached through. `pcm` names a
    /// card's first device only, so another device -- an HDMI port of the
    /// board -- has the other two.
    pub fn alsa_plugins(&self) -> &'static [&'static str] {
        match card_and_device(&self.chosen_output().0) {
            Some((_, 0)) => &["hw", "plughw", "pcm"],
            Some(_) => &["hw", "plughw"],
            None => &[],
        }
    }

    /// Moves the chosen card to another plugin, under the same policy.
    pub fn set_alsa_plugin(&mut self, plugin: &str) {
        let (output, _) = self.chosen_output();
        let Some((card, device)) = card_and_device(&output).filter(|_| own_stack(&output) != "") else { return };
        let moved = match plugin {
            "hw" => format!("{CARD}:hw:{card},{device}"),
            "plughw" => format!("{PLUG}:plughw:{card},{device}"),
            "pcm" if device == 0 => format!("{PCM}:sysdefault:CARD={card}"),
            _ => return,
        };
        if moved != output {
            self.use_output_as_chosen(&moved);
        }
    }

    /// The outputs the user can choose from.
    pub fn outputs(&self) -> Vec<Output> {
        chilpa_core::outputs()
            .into_iter()
            .filter(|output| !output.id.starts_with("file:") || developer_outputs())
            .map(|o| Output {
                id: o.id,
                label: o.label,
                detail: o.detail,
                bit_perfect: o.bit_perfect,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plugin_names_the_same_card() {
        assert_eq!(card_and_device("alsa:hw:1,0"), Some((1, 0)));
        assert_eq!(card_and_device("alsaplug:plughw:0,3"), Some((0, 3)));
        assert_eq!(card_and_device("alsapcm:sysdefault:CARD=1"), Some((1, 0)));
        assert_eq!(card_and_device("pipewire:alsa_output.usb"), None);
        assert_eq!(card_and_device("system:default"), None);
    }
}
