//! Optional DSP catalogs are loaded off-thread; only validated parameters
//! reach the PCM renderer. Preferences are scoped to output identifiers.
mod import;
mod graphic;
mod browser;
mod personal;
mod exchange;
mod actions;
mod status;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod management_tests;
use crate::Chilpa;
use chilpa_core::dsp::{Catalog, Preset};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::mpsc, time::{Duration, Instant}};

type Signature = Vec<(String, String, u64)>;
struct Provider { id: String, name: String, catalog: Catalog, paged: bool }
struct Loaded { signature: Signature, providers: Vec<Provider>, error: String }
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selection {
    plugin: String, preset: String, enabled: bool, edited: Option<Preset>,
    graphic_origin: Option<graphic::Origin>,
    resolved: Option<Preset>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Saved {
    outputs: BTreeMap<String, Selection>, custom: Vec<Preset>, favorites: Vec<String>,
    autosave: bool, rules: BTreeMap<String, Selection>, output_classes: BTreeMap<String, String>,
}

#[derive(Default)]
pub(crate) struct Dsp {
    initialized: bool,
    saved: Saved,
    providers: Vec<Provider>,
    signature: Signature,
    pending: Option<mpsc::Receiver<Loaded>>,
    next_scan: Option<Instant>,
    applied: Option<String>,
    /// When the output was last chosen for `applied`.
    synced_at: Option<Instant>,
    error: String,
    /// The preset's preamp, response curve and graphic band levels, by rate and preset.
    analysis: Option<(String, f64, Vec<f64>, Vec<f64>)>,
    browser: browser::Browser,
    pub(crate) passthrough: bool,
}

/// How long the output stays as chosen while the poll asks again. Choosing
/// it used to list every device, and on PipeWire that started a process on
/// the window's thread: asked on every poll, a slow sound server froze the
/// window for seconds at a time. The listing now lives on its own thread
/// (`outputs::listing`); this keeps the poll from reading the settings file
/// every tick for an answer that only changes by hand.
const OUTPUT_RECHECK: Duration = Duration::from_secs(5);

/// Whether the poll should choose the output again: nothing applied yet, or
/// the last choice is old enough that the device may have changed.
fn recheck_due(applied: bool, synced_at: Option<Instant>, now: Instant) -> bool {
    !applied || synced_at.is_none_or(|at| now.duration_since(at) >= OUTPUT_RECHECK)
}

impl Dsp {
    fn selection(&self, output: &str) -> Selection {
        let fallback = || Selection {
            plugin: self.providers.first().map(|p| p.id.clone()).unwrap_or_default(),
            preset: "flat".into(), ..Selection::default()
        };
        let selection = self.saved.outputs.get(output)
            .or_else(|| self.saved.output_classes.get(output).and_then(|class| self.saved.rules.get(class)))
            .or_else(|| self.saved.rules.get("default")).cloned().unwrap_or_else(fallback);
        if self.providers.iter().any(|p| p.id == selection.plugin) { selection } else { fallback() }
    }
    fn preset(&self, selected: &Selection) -> Option<Preset> {
        let provider = self.providers.iter().find(|p| p.id == selected.plugin)?;
        selected.edited.clone().or_else(|| self.saved.custom.iter().chain(provider.catalog.presets.iter()).find(|p| p.id == selected.preset).cloned())
            .or_else(|| selected.resolved.as_ref().filter(|preset| preset.id == selected.preset).cloned())
    }
    fn load_preferences(&mut self) {
        if self.initialized { return; }
        self.initialized = true;
        self.saved = chilpa_core::settings::load().window.get("audio.dsp.settings")
            .filter(|text| text.len() <= 4*1024*1024).and_then(|text| serde_json::from_str(text).ok()).unwrap_or_default();
        self.saved.custom.retain(|p| p.validate().is_ok());
        self.saved.custom.truncate(64);
        self.saved.favorites.truncate(128);
        self.saved.outputs.retain(|output, s| output.len() <= 512 && s.edited.as_ref().is_none_or(|p| p.validate().is_ok())
            && s.resolved.as_ref().is_none_or(|p| p.validate().is_ok() && p.id == s.preset));
        self.saved.outputs = std::mem::take(&mut self.saved.outputs).into_iter().take(64).collect();
        self.saved.rules.retain(|key, s| personal::valid_scope(key) && s.edited.as_ref().is_none_or(|p| p.validate().is_ok())
            && s.resolved.as_ref().is_none_or(|p| p.validate().is_ok() && p.id == s.preset));
        self.saved.output_classes.retain(|output, class| output.len() <= 512 && personal::valid_class(class));
        self.saved.output_classes = std::mem::take(&mut self.saved.output_classes).into_iter().take(64).collect();
        for selection in self.saved.outputs.values_mut().chain(self.saved.rules.values_mut()) {
            if selection.graphic_origin.as_ref().is_some_and(|origin| !origin.valid()) {
                selection.graphic_origin = None;
            }
        }
    }
    fn persist(&self) -> Result<(), String> {
        let mut settings = chilpa_core::settings::load();
        let text = serde_json::to_string(&self.saved).map_err(|e| e.to_string())?;
        if text.len() > 4*1024*1024 { return Err(chilpa_i18n::t("DSP preferences exceed the storage limit.")); }
        settings.window.insert("audio.dsp.settings".into(), text);
        chilpa_core::settings::save(&settings).map_err(|e| e.to_string())
    }
}

impl Chilpa {
    pub(crate) fn dsp_configuration(&self, output: &str) -> Option<Preset> {
        let selected = self.dsp.selection(output);
        selected.enabled.then(|| self.dsp.preset(&selected)).flatten()
    }
    pub(crate) fn sync_dsp(&mut self) {
        self.dsp.synced_at = Some(Instant::now());
        let preset = if self.dsp.providers.is_empty() { None } else {
            let (output, _) = self.chosen_output(); self.dsp_configuration(&output)
        };
        let fingerprint = serde_json::to_string(&preset).unwrap_or_default();
        if self.dsp.applied.as_ref() == Some(&fingerprint) { return; }
        if let Some(player) = &self.player {
            if let Err(error) = player.dsp().set(preset) { self.dsp.error = error; }
        }
        self.dsp.applied = Some(fingerprint);
    }
    pub(crate) fn advance_dsp(&mut self) {
        self.dsp.load_preferences();
        if let Some(loaded) = self.dsp.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.dsp.pending = None;
            if loaded.signature == self.dsp.signature {
                self.dsp.providers = loaded.providers;
                self.dsp.error = loaded.error;
                self.dsp.applied = None;
            }
        }
        if self.dsp.next_scan.is_none_or(|at| Instant::now() >= at) {
            self.dsp.next_scan = Some(Instant::now() + Duration::from_millis(250));
            let signature = self.plugins.as_ref().and_then(|p| p.try_lock().ok().map(|p| p.dsp_sources()));
            if let Some(signature) = signature {
                if signature != self.dsp.signature {
                    self.dsp.signature = signature.clone();
                    self.dsp.providers.clear();
                    self.dsp.error.clear();
                    self.dsp.applied = None;
                }
                if self.dsp.pending.is_none() && !signature.is_empty() && self.dsp.providers.is_empty() && self.dsp.error.is_empty() {
                    let plugins = self.plugins.as_ref().unwrap().clone();
                    let (tx, rx) = mpsc::channel(); self.dsp.pending = Some(rx);
                    std::thread::spawn(move || {
                        let mut providers = Vec::new(); let mut errors = Vec::new();
                        for (id, name, _) in &signature {
                            let result = plugins.lock().map_err(|_| chilpa_i18n::t("DSP host is unavailable."))
                                .and_then(|mut host| host.call_short(id, "chilpa_dsp_presets", br#"{"protocol":2}"#)
                                    .or_else(|_| host.call_short(id, "chilpa_dsp_presets", br#"{"protocol":1}"#)))
                                .and_then(|bytes| browser::bootstrap(&bytes));
                            match result { Ok((catalog, paged)) => providers.push(Provider { id: id.clone(), name: name.clone(), catalog, paged }), Err(error) => errors.push(error) }
                        }
                        let _ = tx.send(Loaded { signature, providers, error: errors.join(" ") });
                    });
                }
            }
        }
        if recheck_due(self.dsp.applied.is_some(), self.dsp.synced_at, Instant::now()) {
            self.sync_dsp();
        }
    }

    pub fn dsp_json(&mut self) -> String {
        self.advance_dsp();
        self.advance_dsp_browser();
        let output = if self.dsp.providers.is_empty() { String::new() } else { self.chosen_output().0 };
        let selection = self.dsp.selection(&output);
        let preset = self.dsp.preset(&selection);
        let rate = if self.sample_rate == 0 { 48_000 } else { self.sample_rate };
        let analysis_key = format!("{rate}:{}", serde_json::to_string(&preset).unwrap_or_default());
        if self.dsp.analysis.as_ref().is_none_or(|(key, ..)| *key != analysis_key) {
            let (preamp, curve, levels) = preset.as_ref().map(|p| (p.preamp_for_rate(rate), p.curve(rate), graphic::levels(p, rate))).unwrap_or_default();
            self.dsp.analysis = Some((analysis_key, preamp, curve, levels));
        }
        let providers: Vec<_> = self.dsp.providers.iter().map(|p| json!({"id":p.id,"name":p.name})).collect();
        let mut presets: Vec<_> = self.dsp.providers.iter().flat_map(|p| p.catalog.presets.iter().map(move |s| json!({"plugin":p.id,"preset":p.catalog.display(s, chilpa_i18n::current_language())})))
            .chain(self.dsp.saved.custom.iter().map(|s| json!({"plugin":selection.plugin,"preset":s}))).collect();
        let displayed = preset.as_ref().map(|preset| {
            self.dsp.providers.iter().find(|p| p.id == selection.plugin).filter(|_| selection.edited.is_none())
                .map_or_else(|| preset.clone(), |p| p.catalog.display(preset, chilpa_i18n::current_language()))
        });
        if let Some(current) = &displayed {
            if !presets.iter().any(|row| row["plugin"] == selection.plugin && row["preset"]["id"] == current.id) {
                presets.push(json!({"plugin":selection.plugin,"preset":current}));
            }
        }
        let blocked = if self.casting.active() { chilpa_i18n::t("DSP applies to local output. Cast sends the original audio.") }
            else if rate > 1_536_000 { chilpa_i18n::t("DSP supports PCM up to 1536 kHz; higher rates bypass processing.") }
            else if self.now.playing && self.dsp.passthrough { chilpa_i18n::t("Native DSD and DoP bypass DSP. Select PCM output to process them.") } else { String::new() };
        let control = self.player.as_ref().map(|p| p.dsp());
        if let Some(control) = &control { control.collect(); }
        let frequencies = preset.as_ref().map_or_else(|| chilpa_core::dsp::GraphicLayout::default().frequencies(), graphic::frequencies);
        let browser = self.dsp.browser_json(&selection.plugin);
        let headroom = control.as_ref().and_then(|control| control.applied_preamp_db())
            .or_else(|| self.dsp.analysis.as_ref().map(|(_, preamp, ..)| *preamp));
        json!({"available":!providers.is_empty(),"loading":self.dsp.pending.is_some(),"providers":providers,
            "presets":presets,"browser":browser,"autosave":self.dsp.saved.autosave,
            "assignments":self.dsp.saved.rules.iter().map(|(scope, selection)| json!({"scope":scope,"name":self.dsp.preset(selection).map(|preset|preset.name)})).collect::<Vec<_>>(),
            "selection_scope":self.dsp.selection_scope(&output),"output_class":self.dsp.saved.output_classes.get(&output),
            "output_name":self.output_names(&output).0,"selection":selection,"preset":displayed,"output":output,"favorites":self.dsp.saved.favorites,
            "headroom_db":headroom,
            "curve":self.dsp.analysis.as_ref().map(|(_, _, curve, _)| curve),
            "levels":self.dsp.analysis.as_ref().map(|(.., levels)| levels),
            "graphic":preset.as_ref().is_some_and(graphic::matches),"graphic_frequencies":frequencies,
            "meters":control.as_ref().map_or([0.0;4], |control| control.meters()),
            "active":preset.is_some() && blocked.is_empty() && control.as_ref().is_some_and(|c| c.active()),
            "clipped":control.as_ref().map_or(0, |c| c.clipped_samples()),"blocked":blocked,"error":self.dsp.error}).to_string()
    }

    pub fn dsp_action(&mut self, action: &str, input: &str) {
        if input.len() > 2*1024*1024 { self.dsp.error = chilpa_i18n::t("DSP input is too large."); return; }
        let previous = (!matches!(action, "preview" | "graphic_preview" | "filter_preview" | "search" | "close" | "select")).then(|| self.dsp.saved.clone());
        let result = serde_json::from_str::<Value>(input).map_err(|_| chilpa_i18n::t("Invalid DSP input."))
            .and_then(|input| self.change_dsp(action, input));
        if result.is_err() { if let Some(previous) = previous { self.dsp.saved = previous; } }
        self.dsp.error = result.err().unwrap_or_default();
        self.sync_dsp();
    }
}
