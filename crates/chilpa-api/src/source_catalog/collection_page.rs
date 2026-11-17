use super::store::{Shelf, Store};
use serde_json::{Value, json};
use std::{collections::HashSet, io::Write};

pub(super) fn collection(value: &Value) -> bool {
    matches!(value["layout"].as_str(), Some("collection" | "mixed"))
}

pub(super) fn same_document(stored: &Value, incoming: &Value) -> bool {
    if !collection(incoming) {
        return stored == incoming;
    }
    let (Some(old), Some(new)) = (stored.as_object(), incoming.as_object()) else {
        return false;
    };
    old.len() == new.len()
        && new.iter().all(|(key, value)| {
            if key != "items" {
                return old.get(key) == Some(value);
            }
            let (Some(before), Some(after)) = (old[key].as_array(), value.as_array()) else {
                return false;
            };
            before.len() == after.len()
                && before.iter().zip(after).all(|(before, after)| {
                    let (Some(before), Some(after)) = (before.as_object(), after.as_object())
                    else {
                        return false;
                    };
                    before.len() == after.len() + 1
                        && after
                            .iter()
                            .all(|(key, value)| before.get(key) == Some(value))
                        && before.contains_key("_host")
                })
        })
}

struct Budget(usize);
impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > 4 * 1024 * 1024 - self.0 {
            return Err(std::io::Error::other("The collection page exceeds 4 MiB"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn label(value: &Value, key: &str, limit: usize) -> bool {
    value[key].as_str().is_some_and(|text| {
        !text.is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
    })
}
fn exclusions(field: &Value) -> bool {
    field.get("excludes").is_none_or(|value| {
        value.as_array().is_some_and(|keys| {
            keys.len() <= 32
                && keys.iter().all(|key| {
                    key.as_str()
                        .is_some_and(|key| !key.is_empty() && key.len() <= 64)
                })
        })
    })
}
fn options(value: &Value, count: &mut usize) -> bool {
    let Some(options) = value.as_array().filter(|options| options.len() <= 256) else {
        return false;
    };
    *count += options.len();
    *count <= 512
        && options.iter().all(|option| {
            option["value"]
                .as_str()
                .is_some_and(|value| value.len() <= 512)
                && label(option, "label", 256)
                && exclusions(option)
        })
}
fn controls(value: &Value) -> bool {
    let mut count = 0;
    let mut keys = HashSet::new();
    if let Some(fields) = value.get("filter_controls") {
        let Some(fields) = fields.as_array().filter(|fields| fields.len() <= 32) else {
            return false;
        };
        for field in fields {
            if !label(field, "key", 64)
                || !label(field, "label", 256)
                || !keys.insert(field["key"].as_str().unwrap())
            {
                return false;
            }
            match field["type"].as_str() {
                Some("choice") if options(&field["options"], &mut count) => {}
                Some("toggle" | "search") => {}
                _ => return false,
            }
            if !exclusions(field) {
                return false;
            }
        }
    }
    value
        .get("order_options")
        .is_none_or(|value| options(value, &mut count))
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    if !collection(value) {
        return Ok(());
    }
    if !controls(value) {
        return Err("The collection controls exceed their limits".into());
    }
    let items = value["items"]
        .as_array()
        .ok_or("The collection page has no item list")?;
    if items.len() > 100 {
        return Err("The collection page exceeds 100 items".into());
    }
    let mut refs = HashSet::new();
    for item in items {
        let reference = item["ref"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 4096
                    && !s
                        .chars()
                        .any(|c| c.is_control() || c.is_whitespace() || c == '|')
            })
            .ok_or("A collection item has no valid reference")?;
        if !refs.insert(reference) || item.get("_host").is_some() {
            return Err("The collection page repeated or reserved an item identity".into());
        }
        if !item["kind"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 64)
        {
            return Err("A collection item has no valid kind".into());
        }
    }
    serde_json::to_writer(Budget(0), value)
        .map_err(|_| "The collection page exceeds 4 MiB".to_string())
}

pub(super) fn adopt(store: &mut Store, plugin: &str, shelf: &mut Shelf, item: &Value, stamp: u64) -> (i64, i64) {
    if item["kind"] == "track" {
        if let Some(id) = store.take_item_track(plugin, item, None, stamp) {
            let row = shelf.tracks.len() as i64;
            shelf.tracks.push(id);
            return (0, row);
        }
    } else if let Some(id) = store.take_item(plugin, item, stamp) {
        shelf.cards.push(id);
        return (id, -1);
    }
    (0, -1)
}

pub(super) fn ingest(store: &mut Store, plugin: &str, shelf: &mut Shelf, stamp: u64) {
    let Some(items) = shelf.document["items"].as_array_mut() else { return; };
    let mut items = std::mem::take(items);
    for item in &mut items {
        let (card, track) = adopt(store, plugin, shelf, item, stamp);
        item["_host"] = json!({"key":item["ref"],"card_id":card,"track_row":track});
    }
    shelf.document["items"] = Value::Array(items);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_rows_keep_their_wire_order_and_separate_track_indices() {
        let wire = json!({"layout":"mixed","items":[{"kind":"album","ref":"album:a","title":"A"},
            {"kind":"track","ref":"track:1","title":"One"},{"kind":"artist","ref":"artist:a","title":"Artist"},
            {"kind":"track","ref":"track:2","title":"Two"},{"kind":"topic","ref":"topic:a","title":"Topic"}]});
        validate(&wire).unwrap();
        let mut shelf = Shelf {
            document: wire.clone(),
            ..Default::default()
        };
        ingest(&mut Store::default(), "fixture", &mut shelf, 1);
        assert_eq!(shelf.cards.len(), 2);
        assert_eq!(shelf.tracks.len(), 2);
        assert_eq!(shelf.document["items"][1]["_host"]["track_row"], 0);
        assert_eq!(shelf.document["items"][3]["_host"]["track_row"], 1);
        assert_eq!(shelf.document["items"][4]["ref"], "topic:a");
        assert_eq!(shelf.document["items"][4]["_host"]["card_id"], 0);
        assert!(same_document(&shelf.document, &wire));
        let mut changed = wire;
        changed["items"][0]["title"] = "Changed".into();
        assert!(!same_document(&shelf.document, &changed));
    }
    #[test]
    fn an_uncached_manual_page_is_a_forced_refresh_not_an_ordinary_read() {
        let catalog = super::super::Catalog::new();
        catalog.manual_page("fixture native:opaque-page");
        let shelf = catalog.stored_shelf("fixture native:opaque-page").unwrap();
        assert_eq!(shelf.read_at, 0);
        assert_eq!(shelf.epoch, 0);
    }
    #[test]
    fn collection_controls_bound_widgets_and_accept_an_empty_choice() {
        let valid = json!({"layout":"collection","items":[],"filter_controls":[{"key":"genre","label":"Genre","type":"choice","options":[{"value":"","label":"All"}]}]});
        assert!(validate(&valid).is_ok());
        let mut invalid = valid.clone();
        invalid["filter_controls"][0]["options"][0]["excludes"] = json!("unexpected");
        assert!(validate(&invalid).is_err());
        invalid = valid.clone();
        invalid["filter_controls"] = json!(vec![valid["filter_controls"][0].clone(); 33]);
        assert!(validate(&invalid).is_err());
        for reference in ["opaque page", "opaque|shelf"] {
            assert!(
                validate(
                    &json!({"layout":"collection","items":[{"kind":"topic","ref":reference}]})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn collection_limits_and_host_fields_are_checked_before_ingestion() {
        assert!(validate(&json!({"layout":"mixed","items":vec![json!({"kind":"track","ref":"repeated"});101]})).is_err());
        assert!(validate(&json!({"layout":"mixed","items":[{"kind":"track","ref":"a","_host":{"track_row":99}}]})).is_err());
    }
}
