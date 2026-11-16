use super::{Chilpa, Dsp};

impl Dsp {
    fn status_name(&self, output: &str, language: &str) -> Option<String> {
        let selected = self.saved.outputs.get(output)
            .or_else(|| self.saved.output_classes.get(output).and_then(|class| self.saved.rules.get(class)))
            .or_else(|| self.saved.rules.get("default"));
        let matched = selected.and_then(|selection| self.providers.iter()
            .find(|provider| provider.id == selection.plugin).map(|provider| (provider, selection)));
        let (provider, selection) = match matched {
            Some((provider, selection)) => (provider, Some(selection)),
            None => (self.providers.first()?, None),
        };
        let id = selection.map_or("flat", |selection| selection.preset.as_str());
        let edited = selection.and_then(|selection| selection.edited.as_ref());
        let preset = edited.or_else(|| self.saved.custom.iter().chain(provider.catalog.presets.iter()).find(|preset| preset.id == id))
            .or_else(|| selection.and_then(|selection| selection.resolved.as_ref()).filter(|preset| preset.id == id))?;
        let primary = language.split(['-', '_']).next().unwrap_or(language);
        let labels = provider.catalog.localizations.get(language).or_else(|| provider.catalog.localizations.get(primary))
            .and_then(|rows| rows.get(&preset.id));
        let name = labels.filter(|labels| edited.is_none() && !labels.name.is_empty())
            .map_or(preset.name.as_str(), |labels| labels.name.as_str());
        Some(name.to_owned())
    }
}

fn unblocked(rate: u32, casting: bool, playing: bool, passthrough: bool) -> bool {
    !casting && rate <= 1_536_000 && !(playing && passthrough)
}

impl Chilpa {
    /// The player's compact presentation keeps the editor document unallocated.
    pub fn dsp_status(&mut self) -> (bool, bool, String) {
        self.advance_dsp();
        self.advance_dsp_browser();
        let output = if self.dsp.providers.is_empty() { String::new() } else { self.chosen_output().0 };
        let name = self.dsp.status_name(&output, chilpa_i18n::current_language());
        let control = self.player.as_ref().map(|player| player.dsp());
        if let Some(control) = &control { control.collect(); }
        let active = name.is_some() && unblocked(self.sample_rate, self.casting.active(), self.now.playing, self.dsp.passthrough)
            && control.as_ref().is_some_and(|control| control.active());
        (!self.dsp.providers.is_empty(), active, name.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests;
