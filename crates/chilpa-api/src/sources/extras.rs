//! I fetch optional episode reading material away from the UI thread.
use super::*;
use serde_json::{json, Value};

impl Chilpa {
    pub(super) fn load_media_extra(&mut self, id: &str, kind: &str) {
        if self.sources.medium != "podcasts" || !["chapters", "transcript"].contains(&kind) { return; }
        if self.entry_by_id(id).is_none() { return; }
        if self.sources.extras["loading"] == true && self.sources.extras["id"] == id && self.sources.extras["kind"] == kind { return; }
        let key = format!("{kind}_url");
        let Some(address) = self.sources.presentation.get(id).and_then(|item| item.metadata.get(&key)).cloned() else { return };
        let Ok(url) = url::Url::parse(&address) else { return };
        if !["https", "http"].contains(&url.scheme()) || !url.username().is_empty() || url.password().is_some() { return; }
        let (generation, id, kind, tx) = (self.sources.generation, id.to_string(), kind.to_string(), self.sources.tx.clone());
        self.sources.extras = json!({"id":id,"kind":kind,"loading":true});
        std::thread::spawn(move || {
            let result = chilpa_core::fetch(&chilpa_core::Fetch {
                url: &address, user_agent: "Chilpa", accept: "application/json, text/vtt, text/plain",
                timeout: Duration::from_secs(15), max_bytes: 2 * 1024 * 1024,
            }).map_err(|_| chilpa_i18n::t("This episode material could not be loaded."))
                .and_then(|reply| {
                    if reply.status != 200 { return Err(chilpa_i18n::t("This episode material could not be loaded.")); }
                    parse(&kind, &reply.body)
                });
            let _ = tx.send(Answer::Extra { generation, id, kind, result });
        });
    }
}

fn parse(kind: &str, bytes: &[u8]) -> Result<Value, String> {
    if kind == "chapters" {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| chilpa_i18n::t("This chapter format is not supported."))?;
        let rows = value["chapters"].as_array().ok_or_else(|| chilpa_i18n::t("This chapter format is not supported."))?;
        return Ok(rows.iter().take(2000).filter_map(|row| {
            let start = row["startTime"].as_f64().filter(|time| time.is_finite() && *time >= 0.0)?;
            let title = row["title"].as_str()?.chars().take(512).collect::<String>();
            Some(json!({"start":start,"text":title}))
        }).collect());
    }
    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('{') || text.trim_start().starts_with('[') {
        let value: Value = serde_json::from_str(&text).map_err(|_| chilpa_i18n::t("This transcript format is not supported."))?;
        let segments = value.get("segments").unwrap_or(&value).as_array().ok_or_else(|| chilpa_i18n::t("This transcript format is not supported."))?;
        let mut rows = segments.iter().take(10_000).filter_map(|row| {
            let start = row["startTime"].as_f64().or_else(|| row["start"].as_f64()).filter(|time| time.is_finite() && *time >= 0.0)?;
            let words = row["body"].as_str().or_else(|| row["text"].as_str())?;
            Some(json!({"start":start,"text":words.chars().take(8192).collect::<String>()}))
        }).collect::<Vec<_>>();
        rows.sort_by(|a, b| a["start"].as_f64().unwrap().total_cmp(&b["start"].as_f64().unwrap()));
        return Ok(rows.into());
    }
    if text.trim_start().starts_with('<') { return Err(chilpa_i18n::t("This transcript format is not supported.")); }
    let mut rows = Vec::new();
    let mut start = None;
    let mut words = Vec::new();
    for line in text.lines().chain(std::iter::once("")) {
        let line = line.trim();
        if let Some((from, _)) = line.split_once("-->") {
            start = timestamp(from.trim());
            words.clear();
        } else if line.is_empty() {
            if let Some(at) = start.take() {
                if !words.is_empty() { rows.push(json!({"start":at,"text":words.join(" ")})); }
            }
            words.clear();
        } else if start.is_some() { words.push(line); }
        if rows.len() >= 10_000 { break; }
    }
    if rows.is_empty() && !text.trim().is_empty() {
        rows.push(json!({"text": text.chars().take(100_000).collect::<String>()}));
    }
    Ok(rows.into())
}

fn timestamp(text: &str) -> Option<f64> {
    let parts = text.replace(',', ".").split(':').map(str::parse::<f64>).collect::<Result<Vec<_>, _>>().ok()?;
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|part| !part.is_finite() || *part < 0.0) { return None; }
    Some(parts.into_iter().fold(0.0, |time, part| time * 60.0 + part))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chapters_keep_start_times_and_drop_invalid_rows() {
        let rows = parse("chapters", br#"{"chapters":[{"startTime":12.5,"title":"Opening"},{"startTime":-1,"title":"Bad"}]}"#).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["start"], 12.5);
    }
    #[test]
    fn json_transcripts_sort_segments_without_exposing_raw_json() {
        let rows = parse("transcript", br#"{"segments":[{"startTime":8,"body":"Later"},{"startTime":1,"body":"First"}]}"#).unwrap();
        assert_eq!(rows[0], json!({"start":1.0,"text":"First"}));
        assert!(parse("transcript", br#"{"unknown":[]}"#).is_err());
    }
    #[test]
    fn timed_transcripts_preserve_multiline_cues_and_plain_text_has_no_seek() {
        let rows = parse("transcript", b"WEBVTT\n\n00:01.250 --> 00:04.000\nFirst line\nSecond line\n").unwrap();
        assert_eq!(rows[0], json!({"start":1.25,"text":"First line Second line"}));
        assert_eq!(parse("transcript", b"Plain notes").unwrap()[0]["start"], Value::Null);
    }
}
