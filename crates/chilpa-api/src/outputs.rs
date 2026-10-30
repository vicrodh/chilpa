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
mod listing;
pub(crate) use listing::{Listing, WATCH};

/// How long a play with nothing remembered waits for the first listing
/// where nothing can be named without it. Never on Linux, where the
/// system stack's one output is named without asking; elsewhere the one
/// stack answers from the operating system in milliseconds, and this is
/// the ceiling, not the expectation.
const FIRST_ANSWER: std::time::Duration = std::time::Duration::from_secs(1);

/// File capture is a development output, separate from normal listening devices.
fn developer_outputs() -> bool { std::env::var_os("CHILPA_DEV").is_some_and(|value| value == "1") }

/// Whether a listed output is offered: file capture only to a developer.
fn offered(o: &chilpa_core::Output) -> bool { !o.id.starts_with("file:") || developer_outputs() }

/// An output as the window reads it, out of one as the core listed it.
fn output_of(o: &chilpa_core::Output) -> Output {
    Output { id: o.id.clone(), label: o.label.clone(), detail: o.detail.clone(), bit_perfect: o.bit_perfect }
}

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
        if self.outputs_listing.generation.is_empty() {
            self.outputs_listing.generation = now;
            return false;
        }
        if now == self.outputs_listing.generation {
            return false;
        }
        self.outputs_listing.generation = now;
        // A card came or went: the list is made again now, not on its timer.
        self.outputs_listing.ask();
        true
    }

    /// Whether the stacks have answered once since Chilpa started. Until
    /// then every list here is empty, and the output chosen is the one
    /// remembered or the one that is there without asking.
    pub fn outputs_listed(&self) -> bool {
        self.outputs_listing.listed()
    }

    /// Grows when the stacks or outputs as listed change: when they first
    /// answer, when a card comes or goes. Zero before the first answer.
    /// Whoever draws them rebuilds on a new number, never on a tick.
    pub fn outputs_revision(&self) -> u64 {
        self.outputs_listing.revision()
    }

    /// Why the outputs are not being listed, when they are not: the thread
    /// that lists them could not be started, or stopped unexpectedly. The
    /// lists then stand as last found. Asking for them again starts over.
    pub fn outputs_failure(&self) -> Option<String> {
        self.outputs_listing.failure()
    }

    /// A Chilpa whose stacks and outputs are these, listed at once and
    /// never asked of the machine. For tests that must not touch its sound
    /// servers; hidden because nothing else has a reason to call it.
    #[doc(hidden)]
    pub fn with_listed(stacks: Vec<chilpa_core::backends::Stack>, outputs: Vec<chilpa_core::Output>) -> Self {
        Self::with_listing(Listing::with(std::sync::Arc::new(move || listing::Found {
            listed: true,
            stacks: stacks.clone(),
            outputs: outputs.clone(),
            clipped: Vec::new(),
        })))
    }

    /// Moves to another output under the policy already chosen.
    pub fn use_output_as_chosen(&mut self, output: &str) {
        let (_, bit_perfect) = self.chosen_output();
        self.use_output(output, bit_perfect);
        // Whoever chose is looking at the list: it is made again now.
        self.outputs_listing.ask();
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
            summary: chilpa_i18n::t_args("Negotiation prediction, not a playback test: {}",&[&d.summary]),
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
                        chilpa_core::wizard::Verdict::BitPerfect => chilpa_i18n::t("Predicted unchanged PCM; DoP decoding and audible output are untested"),
                        chilpa_core::wizard::Verdict::Converted { a } => {
                            chilpa_i18n::t_args("converted to {}", &[&a])
                        }
                        chilpa_core::wizard::Verdict::Unsupported => chilpa_i18n::t("cannot be done"),
                    },
                })
                .collect(),
        }
    }

    /// The first output there is, as last listed. Before the stacks have
    /// answered once, the one that is there without asking them: the window
    /// is not kept waiting for a sound server to name a default.
    ///
    /// One snapshot, read once: whether the stacks have answered and what
    /// they said travel together, so the empty list from before the answer
    /// is never taken for the empty list of a machine with nothing.
    pub(crate) fn device_or_first(&self) -> String {
        let found = self.outputs_listing.found();
        if let Some(first) = found.outputs.iter().find(|o| offered(o)) {
            return first.id.clone();
        }
        if found.listed {
            return String::new();
        }
        if let Some(standing) = chilpa_core::backends::standing_output() {
            return standing;
        }
        // Nothing is named without asking on this platform, so the thread
        // that is asking is waited for, this once and this long at most.
        self.outputs_listing.first_answer(FIRST_ANSWER).outputs.iter().find(|o| offered(o)).map(|o| o.id.clone()).unwrap_or_default()
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
    ///
    /// As last listed, like every list here: the window never asks the
    /// stacks itself (`listing`). Empty until they have answered once. A
    /// stack that offered more outputs than are kept says so in its caveat,
    /// so a list that ends is known to end.
    pub fn stacks(&self) -> Vec<OutputStack> {
        let found = self.outputs_listing.found();
        found.stacks
            .iter()
            .filter(|s| !matches!(s.kind, chilpa_core::backends::StackKind::AlsaPlug | chilpa_core::backends::StackKind::AlsaPcm)
                && (s.kind != chilpa_core::backends::StackKind::File || developer_outputs()))
            .map(|s| OutputStack {
                key: s.kind.key().to_string(),
                label: s.label(),
                present: s.present,
                missing: s.missing.clone(),
                fidelity: word(s.fidelity).to_string(),
                caveat: if found.clipped.contains(&s.kind) {
                    let cut = chilpa_i18n::t_args("Only the first {} outputs are listed.", &[&chilpa_core::MOST_OUTPUTS_PER_STACK.to_string()]);
                    if s.caveat.is_empty() { cut } else { format!("{} {cut}", s.caveat) }
                } else {
                    s.caveat.clone()
                },
            })
            .collect()
    }

    /// What one stack offers. Empty for one that is not here, and for one
    /// that is here with nothing plugged into it -- which are different
    /// things, and `stacks` is where they are told apart.
    pub fn stack_outputs(&self, key: &str) -> Vec<Output> {
        if key == "file" && !developer_outputs() { return Vec::new(); }
        self.outputs_listing.found().outputs
            .iter()
            .filter(|o| own_stack(&o.id) == key)
            .map(output_of)
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
        let found = self.outputs_listing.found();
        let device = found.outputs.iter().find(|o| o.id == output_id).map(|o| o.label.clone());
        let stack = found.stacks.iter().find(|s| s.kind.key() == key).map(|s| s.label());
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
    /// One nobody knows promises nothing, and neither does any stack until
    /// they have been listed once.
    pub fn fidelity_of(&self, output_id: &str) -> &'static str {
        let key = own_stack(output_id);
        self.outputs_listing.found().stacks
            .iter()
            .find(|s| s.kind.key() == key)
            .map_or("never", |s| word(s.fidelity))
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

    /// The outputs the user can choose from, as last listed.
    pub fn outputs(&self) -> Vec<Output> {
        self.outputs_listing.found().outputs.iter().filter(|o| offered(o)).map(output_of).collect()
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

    fn stack(kind: chilpa_core::backends::StackKind, fidelity: chilpa_core::backends::Fidelity) -> chilpa_core::backends::Stack {
        chilpa_core::backends::Stack { kind, present: true, missing: String::new(), provider: String::new(), fidelity, caveat: String::new() }
    }

    #[test]
    fn the_window_reads_what_was_last_listed_and_never_asks_the_stacks() {
        use super::listing::tests::{output, wait_until, Gate};
        use chilpa_core::backends::{Fidelity, StackKind};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let (at_gate, counted) = (Arc::clone(&gate), Arc::clone(&calls));
        // The stand-in is there from the start: no listing of the machine's
        // stacks is ever started by this test.
        let api = crate::Chilpa::with_listing(Listing::with(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            at_gate.wait();
            super::listing::Found {
                listed: true,
                stacks: vec![stack(StackKind::Alsa, Fidelity::Exact), stack(StackKind::PipeWire, Fidelity::Depends)],
                outputs: vec![output("alsa:hw:1,0", "Card"), output("pipewire:card", "Card through the graph")],
                clipped: Vec::new(),
            }
        })));
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 1, Duration::from_secs(5)), "the listing did not start");
        // The stacks are being asked right now, and the window does not wait.
        let before = Instant::now();
        assert!(api.outputs().is_empty() && api.stacks().is_empty() && api.stack_outputs("alsa").is_empty());
        assert_eq!(api.fidelity_of("alsa:hw:1,0"), "never", "nothing is promised before the stacks answer");
        assert_eq!(api.output_names("alsa:hw:1,0"), ("alsa:hw:1,0".to_string(), "alsa".to_string()));
        assert_eq!(api.outputs_revision(), 0);
        #[cfg(target_os = "linux")]
        assert_eq!(api.device_or_first(), "system:default", "an output is named without asking");
        assert!(before.elapsed() < Duration::from_secs(1), "the window waited on the listing");
        gate.open();
        assert!(wait_until(|| api.outputs_listed(), Duration::from_secs(5)), "the stacks answered and nothing was kept");
        assert_eq!(api.outputs_revision(), 1);
        assert_eq!(api.outputs().iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["alsa:hw:1,0", "pipewire:card"]);
        assert_eq!(api.device_or_first(), "alsa:hw:1,0");
        assert_eq!(api.stack_outputs("pipewire").iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["pipewire:card"]);
        assert_eq!(api.stacks().iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["alsa", "pipewire"]);
        assert_eq!((api.fidelity_of("alsa:hw:1,0"), api.fidelity_of("pipewire:card")), ("exact", "depends"));
        assert_eq!(api.output_names("pipewire:card"), ("Card through the graph".to_string(), "PipeWire".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "reading listed the stacks again");
    }

    #[test]
    fn a_stack_cut_at_the_cap_says_so_where_the_window_reads_its_caveat() {
        use super::listing::tests::{output, wait_until};
        use chilpa_core::backends::{Fidelity, StackKind};
        use std::time::Duration;
        let api = crate::Chilpa::with_listing(Listing::with(std::sync::Arc::new(|| super::listing::Found {
            listed: true,
            stacks: vec![stack(StackKind::PipeWire, Fidelity::Depends), stack(StackKind::Pulse, Fidelity::Never)],
            outputs: vec![output("pipewire:one", "One"), output("pulse:two", "Two")],
            clipped: vec![StackKind::PipeWire],
        })));
        assert!(wait_until(|| api.outputs_listed(), Duration::from_secs(5)));
        let stacks = api.stacks();
        assert!(stacks[0].caveat.ends_with("Only the first 128 outputs are listed."), "{}", stacks[0].caveat);
        assert!(stacks[1].caveat.is_empty(), "a stack under the cap says nothing: {}", stacks[1].caveat);
    }

    #[test]
    fn with_nothing_remembered_the_first_output_is_never_called_nothing() {
        // The empty snapshot from before the stacks answer and the empty
        // list of a machine with nothing look alike; the snapshot tells
        // them apart itself, and the window reads it once.
        use super::listing::tests::{output, wait_until, Gate};
        use std::sync::Arc;
        use std::time::Duration;
        let gate = Arc::new(Gate::default());
        let at_gate = Arc::clone(&gate);
        let api = crate::Chilpa::with_listing(Listing::with(Arc::new(move || {
            at_gate.wait();
            super::listing::tests::answered(vec![output("alsa:hw:1,0", "Card")])
        })));
        #[cfg(target_os = "linux")]
        assert_eq!(api.device_or_first(), "system:default");
        gate.open();
        assert!(wait_until(|| api.outputs_listed(), Duration::from_secs(5)));
        assert_eq!(api.device_or_first(), "alsa:hw:1,0");
        let none = crate::Chilpa::with_listed(Vec::new(), Vec::new());
        assert!(wait_until(|| none.outputs_listed(), Duration::from_secs(5)));
        assert_eq!(none.device_or_first(), "", "a machine with nothing listed has nothing to name");
    }
}
