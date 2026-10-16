//! I project source entries into the medium-specific views.
use super::*;

impl Chilpa {
    /// One entry as the window draws it.
    pub(super) fn entry_json(&self, source: &str, e: &Entry, sounding: bool) -> serde_json::Value {
        let s = &self.sources;
        let finite = e.kind == EntryKind::Item && !e.live;
        let heard = s.heard.get(&e.id).filter(|_| source == s.current);
        let length = heard.map(|h| h.duration_ms).filter(|d| *d > 0).unwrap_or(e.duration_ms);
        let progress = match heard {
            Some(h) if h.played => 1.0,
            Some(h) if length > 0 => (h.position_ms as f64 / length as f64).clamp(0.0, 1.0),
            _ => 0.0,
        };
        let mut meta: Vec<String> = Vec::new();
        if let Some(day) = e.published_at.and_then(crate::format::day) {
            meta.push(day);
        }
        if finite && length > 0 {
            meta.push(crate::format::clock(length));
        }
        let quality = s.presentation.get(&e.id).filter(|_| source == s.current)
            .map(|value| value.advertised_quality.label()).unwrap_or_default();
        serde_json::json!({
            "id": e.id,
            "key": format!("{source} {}", e.id),
            "title": e.title,
            "subtitle": e.subtitle,
            "image": e.image_url.clone().unwrap_or_default(),
            "kind": kind_name(e.kind),
            "presentation": if source == s.current { s.presentation.get(&e.id) } else { None },
            "description": e.description.chars().take(420).collect::<String>(),
            "facts": e.facts,
            "published_at": e.published_at,
            "duration_ms": e.duration_ms,
            "duration": if e.duration_ms > 0 { crate::format::clock(e.duration_ms) } else { String::new() },
            "quality_label": if quality.is_empty() { quality } else {
                format!("{}: {quality}", chilpa_i18n::t("Plugin-reported quality"))
            },
            "live": e.live,
            "finite": finite,
            "meta": meta.join(" · "),
            "progress": progress,
            "played": heard.is_some_and(|h| h.played),
            // How far along it is, in words for its row.
            "heard": match heard {
                Some(h) if h.played => chilpa_i18n::t("Heard"),
                Some(_) if progress > 0.0 => format!("{} %", (progress * 100.0).round() as u32),
                _ => String::new(),
            },
            "playable": e.playable,
            "whyNot": e.why_not,
            "saved": if source == s.current { s.saved.contains(&e.id) } else { self.saved_elsewhere(source, &e.id) },
            "sounding": sounding,
            "resolving": e.id == s.resolving,
        })
    }

}
