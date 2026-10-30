//! Optional DSP catalogs are loaded off-thread; only validated parameters
//! reach the PCM renderer. Preferences are scoped to output identifiers.
mod import;
mod graphic;
#[cfg(test)]
mod tests;
use crate::Chilpa;
use chilpa_core::dsp::{Catalog, Preset};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::mpsc, time::{Duration, Instant}};

type Signature = Vec<(String, String, u64)>;
struct Provider { id: String, name: String, catalog: Catalog }
struct Loaded { signature: Signature, providers: Vec<Provider>, error: String }
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selection {
    plugin: String, preset: String, enabled: bool, edited: Option<Preset>,
    graphic_origin: Option<graphic::Origin>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Saved { outputs: BTreeMap<String, Selection>, custom: Vec<Preset>, favorites: Vec<String> }

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
        let selection = self.saved.outputs.get(output).cloned().unwrap_or_else(fallback);
        if self.providers.iter().any(|p| p.id == selection.plugin) { selection } else { fallback() }
    }
    fn preset(&self, selected: &Selection) -> Option<Preset> {
        let provider = self.providers.iter().find(|p| p.id == selected.plugin)?;
        selected.edited.clone().or_else(|| self.saved.custom.iter().chain(provider.catalog.presets.iter()).find(|p| p.id == selected.preset).cloned())
    }
    fn load_preferences(&mut self) {
        if self.initialized { return; }
        self.initialized = true;
        self.saved = chilpa_core::settings::load().window.get("audio.dsp.settings")
            .filter(|text| text.len() <= 512*1024).and_then(|text| serde_json::from_str(text).ok()).unwrap_or_default();
        self.saved.custom.retain(|p| p.validate().is_ok());
        self.saved.custom.truncate(64);
        self.saved.favorites.truncate(128);
        self.saved.outputs.retain(|_, s| s.edited.as_ref().is_none_or(|p| p.validate().is_ok()));
        for selection in self.saved.outputs.values_mut() {
            if selection.graphic_origin.as_ref().is_some_and(|origin| !origin.valid()) {
                selection.graphic_origin = None;
            }
        }
    }
    fn persist(&self) -> Result<(), String> {
        let mut settings = chilpa_core::settings::load();
        settings.window.insert("audio.dsp.settings".into(), serde_json::to_string(&self.saved).map_err(|e| e.to_string())?);
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
                                .and_then(|mut p| p.call_short(id, "chilpa_dsp_presets", br#"{"protocol":1}"#))
                                .and_then(|bytes| {
                                    if bytes.len() > 512*1024 { return Err(chilpa_i18n::t("DSP catalog is too large.")); }
                                    let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|_| chilpa_i18n::t("Invalid DSP catalog."))?;
                                    catalog.validate()?; Ok(catalog)
                                });
                            match result { Ok(catalog) => providers.push(Provider { id: id.clone(), name: name.clone(), catalog }), Err(error) => errors.push(error) }
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
        let presets: Vec<_> = self.dsp.providers.iter().flat_map(|p| p.catalog.presets.iter().map(move |s| json!({"plugin":p.id,"preset":p.catalog.display(s, chilpa_i18n::current_language())})))
            .chain(self.dsp.saved.custom.iter().map(|s| json!({"plugin":selection.plugin,"preset":s}))).collect();
        let displayed = preset.as_ref().map(|preset| {
            self.dsp.providers.iter().find(|p| p.id == selection.plugin).filter(|_| selection.edited.is_none())
                .map_or_else(|| preset.clone(), |p| p.catalog.display(preset, chilpa_i18n::current_language()))
        });
        let blocked = if self.casting.active() { chilpa_i18n::t("DSP applies to local output. Cast sends the original audio.") }
            else if self.now.playing && self.dsp.passthrough { chilpa_i18n::t("Native DSD and DoP bypass DSP. Select PCM output to process them.") } else { String::new() };
        let control = self.player.as_ref().map(|p| p.dsp());
        json!({"available":!providers.is_empty(),"loading":self.dsp.pending.is_some(),"providers":providers,
            "presets":presets,"selection":selection,"preset":displayed,"output":output,"favorites":self.dsp.saved.favorites,
            "headroom_db":self.dsp.analysis.as_ref().map(|(_, preamp, ..)| preamp),
            "curve":self.dsp.analysis.as_ref().map(|(_, _, curve, _)| curve),
            "levels":self.dsp.analysis.as_ref().map(|(.., levels)| levels),
            "graphic":preset.as_ref().is_some_and(graphic::matches),
            "active":preset.is_some() && blocked.is_empty() && control.as_ref().is_some_and(|c| c.active()),
            "clipped":control.as_ref().map_or(0, |c| c.clipped_samples()),"blocked":blocked,"error":self.dsp.error}).to_string()
    }

    pub fn dsp_action(&mut self, action: &str, input: &str) {
        if input.len() > 128*1024 { self.dsp.error = chilpa_i18n::t("DSP input is too large."); return; }
        let result = serde_json::from_str::<Value>(input).map_err(|_| chilpa_i18n::t("Invalid DSP input."))
            .and_then(|input| self.change_dsp(action, input));
        self.dsp.error = result.err().unwrap_or_default();
        self.sync_dsp();
    }
    fn change_dsp(&mut self, action: &str, input: Value) -> Result<(), String> {
        let (output, _) = self.chosen_output();
        let mut selection = self.dsp.selection(&output);
        if action == "retry" { self.dsp.error.clear(); self.dsp.providers.clear(); self.dsp.next_scan = None; return Ok(()); }
        if !self.dsp.providers.iter().any(|p| p.id == selection.plugin) { return Err(chilpa_i18n::t("Enable a DSP plugin first.")); }
        match action {
            "favorite" => {
                let key = input["key"].as_str().filter(|s| s.len() <= 256).ok_or_else(|| chilpa_i18n::t("Choose a DSP favorite."))?;
                if self.dsp.saved.favorites.iter().any(|s| s == key) { self.dsp.saved.favorites.retain(|s| s != key); }
                else if self.dsp.saved.favorites.len() < 128 { self.dsp.saved.favorites.push(key.into()); }
                return self.dsp.persist();
            }
            "enable" => selection.enabled = input["enabled"].as_bool().ok_or_else(|| chilpa_i18n::t("Choose DSP on or bypass."))?,
            "graphic_start" => {
                let preset = self.dsp.preset(&selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
                selection.begin_graphic(preset);
            }
            "graphic_restore" => selection.restore_graphic()?,
            "choose" => {
                selection.plugin = input["plugin"].as_str().ok_or_else(|| chilpa_i18n::t("Choose a DSP plugin."))?.into();
                selection.preset = input["preset"].as_str().ok_or_else(|| chilpa_i18n::t("Choose a preset."))?.into();
                selection.edited = None;
                selection.graphic_origin = None;
                if self.dsp.preset(&selection).is_none() { return Err(chilpa_i18n::t("DSP preset is unavailable.")); }
                selection.enabled = true;
            }
            "edit" => {
                let mut preset: Preset = serde_json::from_value(input).map_err(|_| chilpa_i18n::t("Invalid DSP parameters."))?;
                if !preset.name.ends_with(" · ∗") { preset.name.push_str(" · ∗"); }
                preset.validate()?; selection.edited = Some(preset); selection.enabled = true;
            }
            "import" => {
                let text = input["text"].as_str().ok_or_else(|| chilpa_i18n::t("Paste a parametric EQ profile."))?;
                let name = input["name"].as_str().map(str::to_string).unwrap_or_else(|| chilpa_i18n::t("Imported"));
                selection.edited = Some(import::parse(text, &name)?); selection.enabled = true;
                selection.graphic_origin = None;
            }
            "save" => {
                if self.dsp.saved.custom.len() >= 64 { return Err(chilpa_i18n::t("Keep at most 64 personal presets.")); }
                let mut preset = self.dsp.preset(&selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
                preset.name = input["name"].as_str().ok_or_else(|| chilpa_i18n::t("Name this preset."))?.trim().into();
                preset.category = "Personal".into();
                let id = (1..).map(|i| format!("user-{i}")).find(|id| !self.dsp.saved.custom.iter().any(|p| p.id == *id)).unwrap();
                preset.id = id.clone(); preset.validate()?;
                self.dsp.saved.custom.push(preset); selection.preset = id; selection.edited = None;
            }
            "delete" => {
                if !selection.preset.starts_with("user-") { return Err(chilpa_i18n::t("Only personal presets can be deleted.")); }
                self.dsp.saved.custom.retain(|p| p.id != selection.preset);
                self.dsp.saved.favorites.retain(|key| !key.ends_with(&format!("/{}", selection.preset)));
                for saved in self.dsp.saved.outputs.values_mut().filter(|s| s.preset == selection.preset) {
                    saved.preset = "flat".into(); saved.edited = None; saved.enabled = false;
                }
                selection.preset = "flat".into(); selection.edited = None; selection.enabled = false;
                selection.graphic_origin = None;
            }
            "reset_meter" => { if let Some(player) = &self.player { player.dsp().reset_meter(); } return Ok(()); }
            _ => return Err(chilpa_i18n::t("Unknown DSP action.")),
        }
        self.dsp.saved.outputs.insert(output, selection);
        self.dsp.persist()?;
        Ok(())
    }
}
