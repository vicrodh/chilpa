//! Commands validate before changing saved presets or output selections.
use super::*;

impl Chilpa {
    pub(super) fn change_dsp(&mut self, action: &str, input: Value) -> Result<(), String> {
        if action == "close" { self.dsp.browser.close(); return self.dsp.persist(); }
        let (output, _) = self.chosen_output();
        let mut selection = self.dsp.selection(&output);
        if action == "retry" { self.dsp.error.clear(); self.dsp.providers.clear(); self.dsp.next_scan = None; return Ok(()); }
        if !self.dsp.providers.iter().any(|p| p.id == selection.plugin) { return Err(chilpa_i18n::t("Enable a DSP plugin first.")); }
        self.dsp.browser.reconcile(&selection.plugin, &self.dsp.signature);
        match action {
            "select" => return self.dsp.browser.check(&input),
            "search" => return self.dsp.browser.command(&input),
            "favorite" => {
                let key = input["key"].as_str().filter(|s| s.len() <= 256).ok_or_else(|| chilpa_i18n::t("Choose a DSP favorite."))?;
                if self.dsp.saved.favorites.iter().any(|s| s == key) { self.dsp.saved.favorites.retain(|s| s != key); }
                else if self.dsp.saved.favorites.len() < 128 { self.dsp.saved.favorites.push(key.into()); }
                self.dsp.browser.invalidate();
                return self.dsp.persist();
            }
            "autosave" => {
                self.dsp.saved.autosave = input["enabled"].as_bool().ok_or_else(|| chilpa_i18n::t("Choose an autosave policy."))?;
                return self.dsp.persist();
            }
            "classify_output" => {
                let class = input["class"].as_str().filter(|value| personal::valid_class(value)).ok_or_else(|| chilpa_i18n::t("Choose an output class."))?;
                if !self.dsp.saved.output_classes.contains_key(&output) && self.dsp.saved.output_classes.len() >= 64 {
                    return Err(chilpa_i18n::t("Keep at most 64 DSP output assignments."));
                }
                self.dsp.saved.output_classes.insert(output, class.into());
                return self.dsp.persist();
            }
            "assign" => {
                let scope = input["scope"].as_str().ok_or_else(|| chilpa_i18n::t("Choose an assignment scope."))?;
                if scope == "device" { /* The selection is stored below. */ }
                else if personal::valid_scope(scope) {
                    self.dsp.saved.rules.insert(scope.into(), selection);
                    return self.dsp.persist();
                } else { return Err(chilpa_i18n::t("Choose an assignment scope.")); }
            }
            "clear_rule" => {
                let scope = input["scope"].as_str().filter(|scope| personal::valid_scope(scope)).ok_or_else(|| chilpa_i18n::t("Choose an assignment scope."))?;
                self.dsp.saved.rules.remove(scope); return self.dsp.persist();
            }
            "clear_assignment" => { self.dsp.saved.outputs.remove(&output); return self.dsp.persist(); }
            "enable" => selection.enabled = input["enabled"].as_bool().ok_or_else(|| chilpa_i18n::t("Choose DSP on or bypass."))?,
            "graphic_start" => {
                let preset = self.dsp.preset(&selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
                selection.begin_graphic(preset);
            }
            "graphic_configure" => {
                let layout: chilpa_core::dsp::GraphicLayout = serde_json::from_value(input).map_err(|_| chilpa_i18n::t("Invalid graphic EQ layout."))?;
                if !layout.validate() { return Err(chilpa_i18n::t("Graphic EQ needs 5–32 bands and a valid frequency range.")); }
                let preset = self.dsp.preset(&selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
                selection.configure_graphic(preset, layout);
            }
            "graphic_band" | "graphic_preview" | "graphic_reset" => {
                let mut preset = self.dsp.preset(&selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
                if !graphic::matches(&preset) { selection.begin_graphic(preset); preset = selection.edited.clone().unwrap(); }
                if action == "graphic_reset" { for filter in &mut preset.filters { filter.gain_db = 0.0; } }
                else {
                    let index = input["index"].as_u64().map(|index| index as usize).filter(|&index| index < preset.filters.len()).ok_or_else(|| chilpa_i18n::t("Choose a DSP band."))?;
                    preset.filters[index].gain_db = input["gain"].as_f64().filter(|gain| (-12.0..=12.0).contains(gain)).ok_or_else(|| chilpa_i18n::t("Graphic EQ gain needs −12–12 dB."))?;
                }
                self.dsp.draft(&mut selection, preset, action == "graphic_preview")?;
            }
            "graphic_restore" => { selection.restore_graphic()?;
                if self.dsp.preset(&selection).is_none() { return Err(chilpa_i18n::t("The previous EQ is no longer available.")); }
            }
            "choose" => {
                let plugin = input["plugin"].as_str().ok_or_else(|| chilpa_i18n::t("Choose a DSP plugin."))?;
                if selection.plugin != plugin { self.dsp.browser.source_changed(); }
                selection.plugin = plugin.into();
                selection.preset = input["preset"].as_str().ok_or_else(|| chilpa_i18n::t("Choose a preset."))?.into();
                selection.edited = None; selection.graphic_origin = None;
                selection.resolved = self.dsp.browser.selected(&selection.preset, &selection.plugin, &self.dsp.signature);
                if self.dsp.preset(&selection).is_none() { return Err(chilpa_i18n::t("DSP preset is unavailable.")); }
                selection.enabled = true;
            }
            "edit" | "preview" => {
                let preset: Preset = serde_json::from_value(input).map_err(|_| chilpa_i18n::t("Invalid DSP parameters."))?;
                self.dsp.draft(&mut selection, preset, action == "preview")?;
            }
            "filter_edit" | "filter_add" | "filter_remove" | "filter_move" | "filter_preview" | "filter_commit" => {
                let preset = self.dsp.filter_action(&selection, action, &input)?;
                self.dsp.draft(&mut selection, preset, action == "filter_preview")?;
            }
            "import" => {
                let text = input["text"].as_str().ok_or_else(|| chilpa_i18n::t("Paste a parametric EQ profile."))?;
                let name = input["name"].as_str().unwrap_or("Imported");
                let mut presets = exchange::parse_many(text, name)?;
                if presets.len() != 1 { return Err(chilpa_i18n::t("Use Import to library for a preset bundle.")); }
                selection.edited = presets.pop(); selection.enabled = true;
                selection.graphic_origin = None;
            }
            "import_many" => {
                let presets = exchange::parse_many(input["text"].as_str().ok_or_else(|| chilpa_i18n::t("Paste DSP presets."))?, input["name"].as_str().unwrap_or("Imported"))?;
                self.dsp.import_personal(presets)?;
                return self.dsp.persist();
            }
            "save" => self.dsp.save_copy(&mut selection, input["name"].as_str().ok_or_else(|| chilpa_i18n::t("Name this preset."))?)?,
            "rename" => self.dsp.update_personal(&mut selection, Some(input["name"].as_str().ok_or_else(|| chilpa_i18n::t("Name this preset."))?), false)?,
            "replace" => self.dsp.update_personal(&mut selection, input["name"].as_str(), true)?,
            "delete" => { let id = selection.preset.clone(); self.dsp.remove_personal(&[id], &mut selection)?; }
            "delete_many" => {
                let ids: Vec<String> = serde_json::from_value(input["ids"].clone()).map_err(|_| chilpa_i18n::t("Choose personal presets to delete."))?;
                self.dsp.remove_personal(&ids, &mut selection)?;
            }
            "reset_meter" => { if let Some(player) = &self.player { player.dsp().reset_meter(); } return Ok(()); }
            _ => return Err(chilpa_i18n::t("Unknown DSP action.")),
        }
        if !self.dsp.saved.outputs.contains_key(&output) && self.dsp.saved.outputs.len() >= 64 { return Err(chilpa_i18n::t("Keep at most 64 DSP output assignments.")); }
        self.dsp.saved.outputs.insert(output, selection);
        if !matches!(action, "preview" | "filter_preview" | "graphic_preview") { self.dsp.persist()?; }
        Ok(())
    }
}
impl Dsp {
    fn draft(&mut self, selection: &mut Selection, mut preset: Preset, preview: bool) -> Result<(), String> {
        preset.name = preset.name.trim_end_matches(" · ∗").into();
        preset.validate()?;
        selection.edited = Some(preset); selection.enabled = true;
        if !preview && self.saved.autosave && self.saved.custom.iter().any(|preset| preset.id == selection.preset) {
            self.update_personal(selection, None, true)?;
        }
        Ok(())
    }
    fn filter_action(&self, selection: &Selection, action: &str, input: &Value) -> Result<Preset, String> {
        let mut preset = self.preset(selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
        if action == "filter_add" {
            if preset.filters.len() >= chilpa_core::dsp::MAX_FILTERS { return Err(chilpa_i18n::t("Use at most 64 DSP filters.")); }
            preset.filters.push(chilpa_core::dsp::Filter::default()); return Ok(preset);
        }
        let index = input["index"].as_u64().map(|value| value as usize).filter(|&index| index < preset.filters.len())
            .ok_or_else(|| chilpa_i18n::t("Choose a DSP filter."))?;
        if matches!(action, "filter_preview" | "filter_commit") {
            if preset.filters[index].locked { return Err(chilpa_i18n::t("Unlock the filter before editing it.")); }
            preset.filters[index].frequency = input["frequency"].as_f64().ok_or_else(|| chilpa_i18n::t("Invalid DSP filter frequency."))?;
            if matches!(preset.filters[index].shape, chilpa_core::dsp::Shape::Peak | chilpa_core::dsp::Shape::LowShelf | chilpa_core::dsp::Shape::HighShelf) {
                preset.filters[index].gain_db = input["gain_db"].as_f64().ok_or_else(|| chilpa_i18n::t("Invalid DSP filter gain."))?;
            }
        } else if action == "filter_move" {
            let target = input["target"].as_u64().map(|value| value as usize).filter(|&target| target < preset.filters.len()).ok_or_else(|| chilpa_i18n::t("Choose a DSP filter position."))?;
            let filter = preset.filters.remove(index); preset.filters.insert(target, filter);
        } else if action == "filter_remove" {
            if preset.filters[index].locked { return Err(chilpa_i18n::t("Unlock the filter before editing it.")); }
            preset.filters.remove(index);
        } else {
            let field = input["field"].as_str().filter(|field| ["shape","frequency","gain_db","q","enabled","channel","locked","color"].contains(field))
                .ok_or_else(|| chilpa_i18n::t("Choose a DSP filter parameter."))?;
            if preset.filters[index].locked && field != "locked" { return Err(chilpa_i18n::t("Unlock the filter before editing it.")); }
            let mut filter = serde_json::to_value(&preset.filters[index]).map_err(|error| error.to_string())?;
            filter[field] = input["value"].clone();
            preset.filters[index] = serde_json::from_value(filter).map_err(|_| chilpa_i18n::t("Invalid DSP filter parameter."))?;
        }
        Ok(preset)
    }
}
