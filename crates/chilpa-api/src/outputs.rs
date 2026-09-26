//! The outputs: which stacks there are, what each offers, which one is
//! chosen and under what policy, its volume and its diagnosis.

use crate::{Output, OutputStack, WizardReport, WizardRow};

/// The card's stack and the one through `plughw`, as identifiers name them.
const CARD: &str = "alsa";
const PLUG: &str = "alsaplug";

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
    /// Moves to another output under the policy already chosen.
    pub fn use_output_as_chosen(&mut self, output: &str) {
        let (_, bit_perfect) = self.chosen_output();
        self.use_output(output, bit_perfect);
    }

    /// Turns bit-perfect on or off on the output already chosen.
    pub fn set_bit_perfect(&mut self, on: bool) {
        let (output, _) = self.chosen_output();
        self.use_output(&output, on);
    }

    /// An output's diagnosis: which formats come out untouched.
    ///
    /// It measures the device for real. It configures nothing: QBZ's wizard
    /// wrote settings blind, and that is worse than not knowing.
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
            .filter(|s| s.kind != chilpa_core::backends::StackKind::AlsaPlug)
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
            PLUG => CARD.to_string(),
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

    /// What the output's stack promises: "exact", "depends" or "never".
    /// One nobody knows promises nothing.
    pub fn fidelity_of(&self, output_id: &str) -> &'static str {
        chilpa_core::backends::StackKind::from_key(own_stack(output_id))
            .and_then(chilpa_core::backends::of)
            .map_or("never", |s| word(s.info().fidelity))
    }

    /// "hw" or "plughw" when the chosen output is a card; empty otherwise.
    pub fn alsa_plugin(&self) -> &'static str {
        match own_stack(&self.chosen_output().0) {
            CARD => "hw",
            PLUG => "plughw",
            _ => "",
        }
    }

    /// Moves the chosen card to the plugin that converts, or back to the
    /// card alone, under the same policy.
    pub fn set_alsa_plugin(&mut self, plughw: bool) {
        let (output, _) = self.chosen_output();
        let moved = match (own_stack(&output), plughw) {
            (CARD, true) => output.strip_prefix("alsa:hw:").map(|c| format!("{PLUG}:plughw:{c}")),
            (PLUG, false) => output.strip_prefix("alsaplug:plughw:").map(|c| format!("{CARD}:hw:{c}")),
            _ => None,
        };
        if let Some(moved) = moved {
            self.use_output_as_chosen(&moved);
        }
    }

    /// The outputs the user can choose from.
    pub fn outputs(&self) -> Vec<Output> {
        chilpa_core::outputs()
            .into_iter()
            .map(|o| Output {
                id: o.id,
                label: o.label,
                detail: o.detail,
                bit_perfect: o.bit_perfect,
            })
            .collect()
    }
}
