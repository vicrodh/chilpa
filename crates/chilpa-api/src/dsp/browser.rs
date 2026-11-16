//! One bounded search job and one visible catalog page; stale results never publish.
use super::*;

const PAGE: usize = 32;
#[derive(Clone, Debug, Default, PartialEq)]
struct Query { text: String, group: String, offset: usize }
#[derive(Default)]
pub(super) struct Browser {
    query: Query,
    source: String,
    signature: Signature,
    generation: u64,
    opened: bool,
    wanted: bool,
    pending: Option<mpsc::Receiver<Answer>>,
    rows: Vec<Preset>,
    checked: BTreeMap<String, Preset>,
    total: usize,
    next: Option<usize>,
}
struct Answer { generation: u64, signature: Signature, source: String, result: Result<Page, String> }
struct Page { rows: Vec<Preset>, total: usize }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    protocol: u32, presets: Vec<Preset>, total: usize, next: Option<usize>,
    #[serde(default)] localizations: BTreeMap<String, BTreeMap<String, chilpa_core::dsp::Labels>>,
    revision: String,
}

pub(super) fn bootstrap(bytes: &[u8]) -> Result<(Catalog, bool), String> {
    if bytes.len() > 512 * 1024 { return Err(chilpa_i18n::t("DSP catalog is too large.")); }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| chilpa_i18n::t("Invalid DSP catalog."))?;
    let paged = value["protocol"] == 2 && value["search"] == true;
    let catalog: Catalog = serde_json::from_value(if paged { value["catalog"].clone() } else { value })
        .map_err(|_| chilpa_i18n::t("Invalid DSP catalog."))?;
    catalog.validate()?;
    Ok((catalog, paged))
}
impl Browser {
    pub(super) fn reconcile(&mut self, source: &str, signature: &Signature) {
        if self.source == source && &self.signature == signature { return; }
        self.source = source.into(); self.signature = signature.clone();
        self.rows.clear(); self.checked.clear(); self.total = 0; self.next = None;
        self.generation = self.generation.wrapping_add(1); self.wanted = self.opened;
    }
    pub(super) fn command(&mut self, input: &Value) -> Result<(), String> {
        let text = input["query"].as_str().unwrap_or("").trim();
        let group = input["group"].as_str().unwrap_or("all");
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        if text.len() > 160 || group.len() > 80 || offset > 100_000 { return Err(chilpa_i18n::t("Invalid DSP catalog query.")); }
        let next = Query { text: text.into(), group: group.into(), offset };
        if self.opened && self.query == next && !self.wanted { return Ok(()); }
        self.query = next;
        self.opened = true; self.wanted = true;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub(super) fn close(&mut self) {
        self.opened = false; self.wanted = false;
        self.generation = self.generation.wrapping_add(1);
        self.rows.clear(); self.rows.shrink_to_fit(); self.checked.clear(); self.total = 0; self.next = None;
    }
    pub(super) fn check(&mut self, input: &Value) -> Result<(), String> {
        let id = input["id"].as_str().filter(|id| self.rows.iter().any(|preset| preset.id == *id)).ok_or_else(|| chilpa_i18n::t("Choose a visible DSP preset."))?;
        let selected = input["selected"].as_bool().ok_or_else(|| chilpa_i18n::t("Choose DSP presets."))?;
        if selected {
            if self.checked.len() >= 64 && !self.checked.contains_key(id) { return Err(chilpa_i18n::t("Select at most 64 presets.")); }
            self.checked.insert(id.into(), self.rows.iter().find(|preset| preset.id == id).unwrap().clone());
        } else { self.checked.remove(id); }
        Ok(())
    }
    pub(super) fn selected(&self, id: &str, source: &str, signature: &Signature) -> Option<Preset> {
        if self.source != source || &self.signature != signature { return None; }
        self.rows.iter().find(|row| row.id == id).or_else(|| self.checked.get(id)).cloned()
    }
    pub(super) fn forget(&mut self, ids: &[String]) { self.checked.retain(|id, _| !ids.contains(id)); }
    pub(super) fn source_changed(&mut self) { self.rows.clear(); self.checked.clear(); self.invalidate(); }
    pub(super) fn invalidate(&mut self) {
        if self.opened { self.wanted = true; self.generation = self.generation.wrapping_add(1); }
    }
}
impl Chilpa {
    pub(super) fn advance_dsp_browser(&mut self) {
        let selection = self.dsp.selection(&self.chosen_output().0);
        self.dsp.browser.reconcile(&selection.plugin, &self.dsp.signature);
        if let Some(answer) = self.dsp.browser.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.dsp.browser.pending = None;
            if answer.generation == self.dsp.browser.generation && answer.signature == self.dsp.signature
                && answer.source == selection.plugin && self.dsp.browser.opened {
                match answer.result {
                    Ok(page) => {
                        self.dsp.browser.total = page.total;
                        self.dsp.browser.next = (self.dsp.browser.query.offset + page.rows.len() < page.total)
                            .then_some(self.dsp.browser.query.offset + page.rows.len());
                        self.dsp.browser.rows = page.rows;
                    }
                    Err(error) => self.dsp.error = error,
                }
            }
        }
        if !self.dsp.browser.opened || !self.dsp.browser.wanted || self.dsp.browser.pending.is_some() { return; }
        let Some(provider) = self.dsp.providers.iter().find(|provider| provider.id == selection.plugin) else { return; };
        self.dsp.browser.wanted = false;
        let query = self.dsp.browser.query.clone();
        let personal: Vec<_> = self.dsp.saved.custom.iter().filter(|preset| matches(preset, &query, &selection.plugin, &self.dsp.saved.favorites)).cloned().collect();
        if !provider.paged || query.group == "personal" {
            let rows: Vec<_> = personal.into_iter().chain(provider.catalog.presets.iter().filter(|preset| query.group != "personal"
                && matches(preset, &query, &provider.id, &self.dsp.saved.favorites)).map(|preset| provider.catalog.display(preset, chilpa_i18n::current_language()))).collect();
            self.dsp.browser.total = rows.len();
            self.dsp.browser.rows = rows.into_iter().skip(query.offset).take(PAGE).collect();
            self.dsp.browser.next = (query.offset + self.dsp.browser.rows.len() < self.dsp.browser.total).then_some(query.offset + self.dsp.browser.rows.len());
            return;
        }
        let Some(plugins) = self.plugins.clone() else { return; };
        let source = provider.id.clone();
        let ids: Vec<_> = self.dsp.saved.favorites.iter().filter_map(|key| key.strip_prefix(&(source.clone() + "/"))).map(str::to_string).collect();
        let generation = self.dsp.browser.generation;
        let signature = self.dsp.signature.clone();
        let language = chilpa_i18n::current_language().to_string();
        let (tx, rx) = mpsc::channel(); self.dsp.browser.pending = Some(rx);
        std::thread::spawn(move || {
            let personal_total = personal.len();
            let mut rows: Vec<_> = personal.into_iter().skip(query.offset).take(PAGE).collect();
            let room = PAGE - rows.len();
            let request = json!({"protocol":2,"query":query.text,
                "group":if query.group == "favorites" { "all" } else { &query.group },
                "offset":query.offset.saturating_sub(personal_total),"limit":room.max(1),
                "ids":if query.group == "favorites" { ids.clone() } else { Vec::new() }}).to_string();
            let result = if query.group == "favorites" && ids.is_empty() { Ok(Page { rows, total: personal_total }) } else {
                plugins.lock().map_err(|_| chilpa_i18n::t("DSP host is unavailable."))
                    .and_then(|mut host| host.call(&source, "chilpa_dsp_search", request.as_bytes()))
                    .and_then(|bytes| parse_page(&bytes, &language, room)).map(|page| {
                        rows.extend(page.rows); Page { rows, total: personal_total + page.total }
                    })
            };
            let _ = tx.send(Answer { generation, signature, source, result });
        });
    }
}
fn matches(preset: &Preset, query: &Query, source: &str, favorites: &[String]) -> bool {
    let favorite = favorites.iter().any(|key| key == &format!("{source}/{}", preset.id));
    let group = query.group == "all" || query.group == "personal" && preset.id.starts_with("user-")
        || query.group == "favorites" && favorite || query.group == preset.category;
    let text = format!("{} {} {} {}", preset.name, preset.category, preset.description, preset.source).to_lowercase();
    group && query.text.to_lowercase().split_whitespace().all(|part| text.contains(part))
}
fn parse_page(bytes: &[u8], language: &str, limit: usize) -> Result<Page, String> {
    if bytes.len() > 512 * 1024 { return Err(chilpa_i18n::t("DSP catalog page is too large.")); }
    let reply: Reply = serde_json::from_slice(bytes).map_err(|_| chilpa_i18n::t("Invalid DSP catalog page."))?;
    if reply.protocol != 2 || reply.presets.len() > limit.max(1) || reply.total > 100_000 || reply.total < reply.presets.len()
        || reply.revision.len() > 80 || reply.next.is_some_and(|next| next > reply.total) || reply.localizations.len() > 16 {
        return Err(chilpa_i18n::t("Invalid DSP catalog page."));
    }
    let mut ids = std::collections::BTreeSet::new();
    for preset in &reply.presets { preset.validate()?; if !ids.insert(&preset.id) { return Err(chilpa_i18n::t("Duplicate DSP preset ID.")); } }
    let mut catalog = Catalog { protocol: 1, presets: reply.presets, localizations: reply.localizations };
    for labels in catalog.localizations.values_mut() { labels.retain(|id, _| catalog.presets.iter().any(|preset| &preset.id == id)); }
    if !catalog.presets.is_empty() { catalog.validate()?; }
    Ok(Page { rows: catalog.presets.iter().take(limit).map(|preset| catalog.display(preset, language)).collect(), total: reply.total })
}
impl Dsp {
    pub(super) fn browser_json(&self, _source: &str) -> Value {
        let mut groups = BTreeMap::from([("all".to_string(), chilpa_i18n::t("All groups")),
            ("favorites".into(), chilpa_i18n::t("Favorites")), ("personal".into(), chilpa_i18n::t("Personal"))]);
        for provider in &self.providers { for preset in &provider.catalog.presets {
            groups.insert(preset.category.clone(), provider.catalog.display(preset, chilpa_i18n::current_language()).category);
        } }
        json!({"query":self.browser.query.text,"group":self.browser.query.group,"offset":self.browser.query.offset,
            "total":self.browser.total,"next":self.browser.next,"loading":self.browser.pending.is_some() || self.browser.wanted,
            "selected_ids":self.browser.checked.keys().collect::<Vec<_>>(),"selected_personal":self.browser.checked.keys().all(|id| self.saved.custom.iter().any(|preset| &preset.id == id)),
            "presets":self.browser.rows.iter().map(|preset| json!({"plugin":self.browser.source,"preset":preset})).collect::<Vec<_>>(),
            "groups":groups.into_iter().map(|(key,label)| json!({"key":key,"label":label})).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_or_epoch_changes_clear_rows_and_selection_before_another_lookup() {
        let first = vec![("provider-a".into(),"A".into(),1)];
        let second = vec![("provider-b".into(),"B".into(),1)];
        let mut browser = Browser {opened:true, ..Browser::default()};
        browser.reconcile("provider-a", &first);
        let preset = Preset {id:"external".into(), ..Preset::default()};
        browser.rows.push(preset.clone());
        browser.checked.insert(preset.id.clone(), preset);
        assert!(browser.selected("external", "provider-a", &first).is_some());
        assert!(browser.selected("external", "provider-b", &second).is_none());
        browser.reconcile("provider-b", &second);
        assert!(browser.rows.is_empty() && browser.checked.is_empty() && browser.wanted);
        browser.rows.push(Preset {id:"external".into(), ..Preset::default()});
        let replacement = vec![("provider-b".into(),"B".into(),2)];
        browser.reconcile("provider-b", &replacement);
        assert!(browser.rows.is_empty() && browser.wanted);
        assert!(browser.selected("external", "provider-b", &second).is_none());
    }
}
