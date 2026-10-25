//! I fold already stored track facts without refreshing cards or probing artwork.
use super::{Catalog, store::{CardKind, Track}};
use serde_json::{Value, json};

#[derive(Default)]
struct Scalar { first: Value, known: usize, mixed: bool }
impl Scalar {
    fn add(&mut self, value: Value) {
        if value.is_null() { return; }
        if self.known == 0 { self.first = value; }
        else if self.first != value { self.mixed = true; }
        self.known += 1;
    }
    fn value(&self, total: usize) -> Value {
        if !self.mixed && self.known == total && total > 0 { self.first.clone() } else { Value::Null }
    }
}
fn text(value: Option<&str>) -> Value {
    value.map(str::trim).filter(|v| !v.is_empty()).map(|v| json!(v.to_uppercase())).unwrap_or(Value::Null)
}
fn positive(value: Option<u64>) -> Value { value.filter(|v| *v > 0).map(|v| json!(v)).unwrap_or(Value::Null) }
fn fields(track: &Track, year: Option<i32>, plugin: &str) -> [Value; 7] {
    let format = track.format.as_ref();
    let codec = format.map(|f| f.codec.as_str()).filter(|v| !v.is_empty()).or_else(|| track.metadata["codec"].as_str());
    let dsd = codec.is_some_and(|c| ["dsd","dsf","dff","dst"].iter().any(|known| c.eq_ignore_ascii_case(known)));
    [text(track.metadata["container_format"].as_str()), text(codec),
        positive(format.and_then(|f| f.rate).map(u64::from).or_else(|| track.metadata["sample_rate_hz"].as_u64())),
        if dsd { json!(1) } else { positive(format.and_then(|f| f.bits).map(u64::from).or_else(|| track.metadata["bit_depth"].as_u64())) },
        positive(track.metadata["channels"].as_u64()),
        positive(track.metadata["year"].as_u64().or_else(|| year.and_then(|v| u64::try_from(v).ok()))),
        json!(if track.plugin.is_empty() { plugin } else { &track.plugin })]
}
impl Catalog {
    /// I borrow catalog members under one lock and retain at most seven scalar values.
    pub(crate) fn album_technical(&self, id: i64) -> Value {
        let inner = self.lock();
        let Some(card) = inner.store.cards.get(&id).filter(|c| c.kind == CardKind::Album) else { return json!({}); };
        let artwork = inner.store.chosen.get(&(card.plugin.clone(),card.identity.clone())).unwrap_or(&card.art);
        let mut document = json!({"tracks":card.tracks.as_ref().map(|ids| ids.len() as u64).or_else(|| card.metadata["tracks"].as_u64()),
            "duration_ms":positive(card.metadata["duration_ms"].as_u64()), "format":null,"codec":null,"rate":null,"bits":null,"channels":null,
            "bytes":null,"year":card.year,"source":card.plugin,"artwork":artwork,"mixed_fields":[]});
        let Some(ids) = &card.tracks else { return document; };
        let mut scalars: [Scalar;7] = Default::default();
        let mut duration = Some(0u64);
        for id in ids {
            let Some(track) = inner.store.tracks.get(id) else { duration=None; continue; };
            for (scalar,value) in scalars.iter_mut().zip(fields(track,card.year,&card.plugin)) { scalar.add(value); }
            duration = duration.and_then(|total| (track.duration_ms>0).then(|| total.checked_add(track.duration_ms)).flatten());
        }
        document["duration_ms"] = positive(duration);
        let mut mixed = Vec::new();
        for (key,scalar) in ["format","codec","rate","bits","channels","year","source"].into_iter().zip(scalars) {
            document[key] = scalar.value(ids.len());
            if scalar.mixed { mixed.push(key); }
        }
        document["mixed_fields"] = json!(mixed);
        document
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_catalog::store::{Card, Format};
    #[test]
    fn remote_aggregation_reads_only_stored_members_and_does_not_schedule_work() {
        let catalog = Catalog::new();
        {
            let mut inner = catalog.lock();
            inner.store.cards.insert(-1,Card {id:-1,plugin:"fixture".into(),tracks:Some(vec![-2,-3]),
                art:"/absent/cover.jpg".into(),image:"https://invalid.example/cover".into(),year:Some(2003),..Default::default()});
            for (id,rate) in [(-2,44100),(-3,96000)] {
                inner.store.tracks.insert(id,Track {id,plugin:"fixture".into(),duration_ms:1000,
                    format:Some(Format{codec:"flac".into(),bits:Some(24),rate:Some(rate),..Default::default()}),
                    metadata:json!({"channels":2,"container_format":"flac"}),..Default::default()});
            }
        }
        let result=catalog.album_technical(-1);
        assert_eq!(result["tracks"],2); assert_eq!(result["duration_ms"],2000);
        assert_eq!(result["codec"],"FLAC"); assert_eq!(result["bits"],24);
        assert_eq!(result["mixed_fields"],json!(["rate"])); assert!(result["rate"].is_null());
        assert!(result["bytes"].is_null()); assert_eq!(result["artwork"],"/absent/cover.jpg");
        let inner=catalog.lock();
        assert!(inner.queue.queued.is_empty()); assert!(!inner.loaded);
        assert!(inner.art_received.is_empty());
    }
    #[test]
    fn incomplete_remote_members_do_not_promote_card_quality_to_album_quality() {
        let catalog=Catalog::new();
        let card=Card {id:-1,plugin:"fixture".into(),tracks:Some(vec![-2]),
            format:Some(Format{codec:"flac".into(),bits:Some(24),rate:Some(192000),..Default::default()}),..Default::default()};
        catalog.lock().store.cards.insert(-1,card);
        let result=catalog.album_technical(-1);
        for field in ["format","codec","rate","bits","channels","duration_ms","bytes"] {assert!(result[field].is_null(),"{field}");}
        assert_eq!(result["mixed_fields"],json!([]));
    }
}
