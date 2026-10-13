//! A scene keeps one artist identity with every available destination.
use chilpa_meta::scene::{identity::normalize_name, Candidate};
use serde_json::{json, Value};
use std::collections::HashMap;

pub(crate) const PAGE_SIZE: usize = 100;
pub(crate) const INITIAL_TARGET: usize = 20;
pub(crate) fn fill(mut offset: usize, total: usize, more: bool, mut visit: impl FnMut(std::ops::Range<usize>) -> usize) -> usize {
    loop {
        let end = offset.saturating_add(PAGE_SIZE).min(total);
        let count = visit(offset..end);
        offset = end;
        if more || offset >= total || count >= INITIAL_TARGET { return offset; }
    }
}

pub(crate) fn merge(lanes: &[Value]) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    let mut positions = HashMap::new();
    for row in lanes.iter().flat_map(|lane| lane["artists"].as_array().into_iter().flatten()) {
        let Some(id) = row["mbid"].as_str() else { continue };
        let position = *positions.entry(id.to_string()).or_insert_with(|| {
            rows.push(row.clone()); rows.len() - 1
        });
        let kept = &mut rows[position];
        for field in ["sources", "genres"] {
            let mut values = kept[field].as_array().cloned().unwrap_or_default();
            for value in row[field].as_array().into_iter().flatten() {
                let duplicate = values.iter().any(|old| if field == "sources" {
                    old["plugin"] == value["plugin"] && old["ref"] == value["ref"] && old["key"] == value["key"]
                } else { old == value });
                if !duplicate { values.push(value.clone()); }
            }
            kept[field] = json!(values);
        }
        kept["score"] = json!(kept["score"].as_i64().unwrap_or(0).max(row["score"].as_i64().unwrap_or(0)));
    }
    rows.sort_by(|a, b| b["score"].as_i64().cmp(&a["score"].as_i64()).then_with(|| a["mbid"].as_str().cmp(&b["mbid"].as_str())));
    rows
}

pub(crate) struct Local(HashMap<String, crate::Artist>);
impl Local {
    pub(crate) fn new(artists: Vec<crate::Artist>) -> Self {
        Self(artists.into_iter().map(|artist| (normalize_name(&artist.name), artist)).collect())
    }
    pub(crate) fn project(&self, candidate: &Candidate) -> Option<Value> {
        let artist = self.0.get(&normalize_name(&candidate.name))?;
        let image = url::Url::from_file_path(&artist.artwork_path).ok().map(|url| url.to_string()).unwrap_or_default();
        Some(json!({"mbid":candidate.id,"title":candidate.name,"genres":candidate.genres,"score":candidate.score,
            "sources":[{"local":true,"name":"Chilpa","target":"artist","key":artist.name,"albums":artist.album_count,
                "image":image,"icon":"qrc:/qt/qml/app/chilpa/kit/assets/logo.png"}]}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_identity_keeps_local_and_catalog_routes_without_collapsing_namesakes() {
        let candidate = Candidate { id:"one".into(), name:"Band".into(), genres:vec!["rock".into()], score:40 };
        let local = Local::new(vec![crate::Artist { name:"BAND".into(), album_count:3, track_count:30, artwork_path:String::new() }]);
        let local = local.project(&candidate).unwrap();
        let remote = |id| crate::scene_catalog::projection(id, id, &candidate, &json!({"ref":"artist:7","title":"Band"}));
        let mut namesake = remote("other"); namesake["mbid"] = json!("two");
        let rows = merge(&[json!({"artists":[local]}), json!({"artists":[remote("first"),remote("second"),remote("first"),namesake]})]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["sources"].as_array().unwrap().len(), 3);
        assert_eq!(rows[0]["sources"][0]["key"], "BAND");
        assert_eq!(rows[0]["sources"][0]["albums"], 3);
        assert_eq!(rows[0]["sources"][2]["plugin"], "second");
    }
    #[test]
    fn sparse_first_batch_continues_to_twenty_and_later_pages_keep_their_cursor() {
        let mut checked = Vec::new();
        let end = fill(0, 350, false, |range| {
            checked.push(range.clone());
            if range.start == 0 { 5 } else { 23 }
        });
        assert_eq!(checked, [0..100, 100..200]);
        assert_eq!(end, 200);
        let end = fill(end, 350, true, |range| { assert_eq!(range, 200..300); 24 });
        assert_eq!(end, 300);
        assert_eq!(fill(300, 350, false, |_| 7), 350, "exhaustion leaves truthful results instead of padding the count");
    }
}
