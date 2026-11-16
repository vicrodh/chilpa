//! Personal presets and explicit output rules share stable preset identifiers.
use super::*;

pub(super) fn valid_class(value: &str) -> bool { matches!(value, "headphones" | "speakers" | "bluetooth" | "usb" | "other") }
pub(super) fn valid_scope(value: &str) -> bool { value == "default" || valid_class(value) }
impl Dsp {
    pub(super) fn save_copy(&mut self, selection: &mut Selection, name: &str) -> Result<(), String> {
        let preset = self.preset(selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?;
        let id = self.add_personal(preset, name)?;
        selection.preset = id; selection.edited = None; selection.resolved = None; selection.graphic_origin = None;
        self.browser.invalidate();
        Ok(())
    }
    fn add_personal(&mut self, mut preset: Preset, name: &str) -> Result<String, String> {
        if self.saved.custom.len() >= 64 { return Err(chilpa_i18n::t("Keep at most 64 personal presets.")); }
        let id = (1..).map(|index| format!("user-{index}")).find(|id| !self.saved.custom.iter().any(|preset| preset.id == *id)).unwrap();
        preset.id = id.clone(); preset.name = name.trim().into(); preset.category = "Personal".into();
        preset.validate()?;
        self.saved.custom.push(preset);
        Ok(id)
    }
    pub(super) fn update_personal(&mut self, selection: &mut Selection, name: Option<&str>, replace: bool) -> Result<(), String> {
        let index = self.saved.custom.iter().position(|preset| preset.id == selection.preset)
            .ok_or_else(|| chilpa_i18n::t("Only personal presets can be changed."))?;
        let mut preset = if replace { self.preset(selection).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))? }
            else { self.saved.custom[index].clone() };
        preset.id = self.saved.custom[index].id.clone(); preset.category = "Personal".into();
        preset.name = name.map(str::trim).unwrap_or(&self.saved.custom[index].name).into();
        preset.validate()?;
        self.saved.custom[index] = preset.clone();
        if replace { selection.edited = None; selection.graphic_origin = None; }
        else if let Some(draft) = &mut selection.edited { draft.name = preset.name; }
        self.browser.invalidate();
        Ok(())
    }
    pub(super) fn import_personal(&mut self, presets: Vec<Preset>) -> Result<(), String> {
        if self.saved.custom.len() + presets.len() > 64 { return Err(chilpa_i18n::t("Keep at most 64 personal presets.")); }
        for preset in &presets { preset.validate()?; }
        for preset in presets { let name = preset.name.clone(); self.add_personal(preset, &name)?; }
        self.browser.invalidate();
        Ok(())
    }
    pub(super) fn remove_personal(&mut self, ids: &[String], selection: &mut Selection) -> Result<(), String> {
        if ids.is_empty() || ids.len() > 64 || ids.iter().any(|id| !self.saved.custom.iter().any(|preset| &preset.id == id)) {
            return Err(chilpa_i18n::t("Only personal presets can be deleted."));
        }
        self.saved.custom.retain(|preset| !ids.contains(&preset.id));
        self.browser.forget(ids);
        self.saved.favorites.retain(|key| !ids.iter().any(|id| key.ends_with(&format!("/{id}"))));
        for selected in self.saved.outputs.values_mut().chain(self.saved.rules.values_mut()).chain(std::iter::once(selection)) {
            if ids.contains(&selected.preset) { selected.preset = "flat".into(); selected.edited = None;
                selected.resolved = None; selected.enabled = false; selected.graphic_origin = None; }
        }
        self.browser.invalidate();
        Ok(())
    }
    pub(super) fn selection_scope(&self, output: &str) -> &str {
        if self.saved.outputs.contains_key(output) { "device" }
        else if let Some(class) = self.saved.output_classes.get(output).filter(|class| self.saved.rules.contains_key(*class)) { class }
        else if self.saved.rules.contains_key("default") { "default" }
        else { "bypass" }
    }
}
