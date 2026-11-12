use super::*;
use crate::model::LayerKind;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Pronunciation is an opt-in derivation, using the existing lyrics worker and disk cache.
impl Pipeline {
    pub fn with_romanizer(mut self, romanizer: Arc<dyn Romanizer>) -> Self {
        self.romanizer = Some(romanizer);
        self
    }

    pub fn romanize(&self, request: &Request, document: &Document) -> Option<Layer> {
        if !request.romanize || document.romanization_for().is_some() || document.lines.len() > 200 {
            return None;
        }
        let lines: Vec<String> = document.lines.iter().map(|line| line.text.clone()).collect();
        // A conservative filter only. The service checks Unicode scripts, including non-BMP letters.
        if lines.iter().any(|line| line.chars().count() > 500)
            || !lines.iter().any(|line| line.chars().any(|c| c.is_alphabetic() && !c.is_ascii())) { return None; }
        let context = serde_json::json!(["romanization-v1", "standard", document.language, request.track.title, request.track.artist, lines]).to_string();
        // Leave room for the HTTP envelope inside the service's 128 KiB limit.
        if context.len() > 120 * 1024 { return None; }
        let key = format!("romanization:{:x}", Sha256::digest(context.as_bytes()));
        match self.cache.lock().unwrap_or_else(|e| e.into_inner()).get_translation(&key) {
            Some(TranslationHit::Found(layer)) => return pronunciation(layer, lines.len()),
            Some(TranslationHit::None) => return None,
            None => {}
        }
        if request.offline || !request.allow_remote { return None; }
        let romanizer = self.romanizer.as_ref()?;
        if self.romanizer_cooling.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|until| until > Instant::now()) { return None; }
        for attempt in 0..3 {
            match romanizer.romanize(&request.track, &lines, document.language.as_deref(), self.limits.translate) {
                Translation::Found(layer) => {
                    let layer = pronunciation(layer, lines.len());
                    if let Some(layer) = &layer { self.cache.lock().unwrap_or_else(|e| e.into_inner()).put_translation(&key, Some(layer)); }
                    return layer;
                }
                Translation::None => {
                    self.cache.lock().unwrap_or_else(|e| e.into_inner()).put_translation(&key, None);
                    return None;
                }
                Translation::Failed { retry_after, .. } => match retry_after {
                    Some(wait) if wait <= TRANSLATION_WAIT && attempt < 2 => std::thread::sleep(wait),
                    other => {
                        let pause = other.unwrap_or(TRANSLATION_PAUSE).min(Duration::from_secs(3600));
                        *self.romanizer_cooling.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + pause);
                        return None;
                    }
                }
            }
        }
        None
    }
}

fn pronunciation(layer: Layer, count: usize) -> Option<Layer> {
    let mut seen = HashSet::new();
    (layer.kind == LayerKind::Romanization && layer.language.split('-').any(|part| part.eq_ignore_ascii_case("Latn"))
        && !layer.lines.is_empty() && layer.lines.len() <= count && layer.lines.iter().all(|line|
            (line.line as usize) < count && seen.insert(line.line) && !line.text.trim().is_empty()
                && line.text.len() <= 16_000 && line.segments.is_empty())).then_some(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LayerLine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Generator { calls: AtomicUsize }
    impl Romanizer for Generator {
        fn romanize(&self, _: &Track, _: &[String], _: Option<&str>, _: Duration) -> Translation {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Translation::Found(layer())
        }
    }

    fn layer() -> Layer {
        Layer { kind: LayerKind::Romanization, language: "ja-Latn-hepburn".into(), provider: Some("service".into()),
            lines: vec![LayerLine { line: 0, text: "Tōkyō".into(), segments: vec![] }] }
    }

    fn request() -> Request {
        Request { track: Track { title: "Synthetic".into(), artist: "Fixture".into(), ..Track::default() },
            path: None, source: None, language: None, translate: false, romanize: false,
            allow_remote: true, offline: false, remote_providers: None }
    }

    #[test]
    fn pronunciation_is_opt_in_prefers_supplied_readings_and_keeps_offline_cache() {
        let generator = Arc::new(Generator { calls: AtomicUsize::new(0) });
        let pipeline = Pipeline::new(Cache::in_memory().unwrap(), None, Limits::default()).with_romanizer(generator.clone());
        let original = crate::lrc::parse("[00:01.00]東京\n[00:03.00]Hello");
        let mut asked = request();
        assert_eq!(pipeline.romanize(&asked, &original), None);
        assert_eq!(generator.calls.load(Ordering::SeqCst), 0);
        asked.romanize = true;
        asked.offline = true;
        assert_eq!(pipeline.romanize(&asked, &original), None);
        asked.offline = false;
        assert_eq!(pipeline.romanize(&asked, &original), Some(layer()));
        asked.offline = true;
        assert_eq!(pipeline.romanize(&asked, &original), Some(layer()));
        assert_eq!(generator.calls.load(Ordering::SeqCst), 1);
        asked.romanize = false;
        assert_eq!(pipeline.romanize(&asked, &original), None, "off also hides cached generations");
        asked.romanize = true;
        let mut supplied = original.clone();
        supplied.layers.push(layer());
        assert_eq!(pipeline.romanize(&asked, &supplied), None);
        assert_eq!(generator.calls.load(Ordering::SeqCst), 1, "the provider's layer wins");
        assert_eq!(original.lines[0].start_ms, Some(1000));
        assert_eq!(original.lines[0].text, "東京");
    }

    #[test]
    fn generated_layers_must_match_the_document_and_cannot_invent_timing() {
        let mut wrong = layer();
        wrong.kind = LayerKind::Translation;
        assert_eq!(pronunciation(wrong, 1), None);
        let mut wrong = layer();
        wrong.lines[0].line = 1;
        assert_eq!(pronunciation(wrong, 1), None);
        let mut wrong = layer();
        wrong.lines.push(wrong.lines[0].clone());
        assert_eq!(pronunciation(wrong, 2), None);
        let mut wrong = layer();
        wrong.lines[0].segments.push(crate::model::Segment { text: "Tōkyō".into(), start_ms: 1000, end_ms: None, space_after: false, background: false });
        assert_eq!(pronunciation(wrong, 1), None);
    }
}
