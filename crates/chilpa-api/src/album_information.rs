//! Read-only album information shared by local records and source catalogs.
use crate::Chilpa;
use serde_json::{json, Value};

impl Chilpa {
    pub fn album_information(&mut self, id: i64) -> String {
        let Some(detail) = self.album_detail(id) else { return "{}".into(); };
        let mut info = self.catalog.card(id).map(|card| {
            let mut value = card.metadata;
            if !value.is_object() { value = json!({}); }
            value["plugin"] = json!(card.plugin);
            value
        }).unwrap_or_else(|| json!({}));
        info["title"] = json!(detail.album.title);
        info["artist"] = json!(detail.album.artist);
        info["quality_label"] = json!(detail.quality.long());
        if info["genre"].as_str().is_none_or(str::is_empty) { info["genre"] = json!(detail.genre); }
        if info["release_date"].as_str().is_none_or(str::is_empty) { info["release_date"] = json!(detail.album.year.map(|year| year.to_string()).unwrap_or_default()); }
        if !info["track_credits"].is_array() || info["track_credits"].as_array().is_some_and(Vec::is_empty) {
            info["track_credits"] = json!(self.album_track_rows(id).iter().map(|track| {
                let metadata = self.catalog.track_by_path(&track.path).map(|(track,_)| track.metadata).unwrap_or(Value::Null);
                json!({"title":track.title,"artist":track.artist,"number":track.number,"performers":metadata["performers"],"copyright":metadata["copyright"]})
            }).collect::<Vec<_>>());
        }
        info.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_information_does_not_require_native_page_capabilities() {
        let mut api = Chilpa::new();
        let album = api.catalog.adopt("fixture.source",None,&crate::source_search::RemoteFound {
            source:"fixture.source".into(),name:"Source".into(),id:"album:7".into(),title:"Record".into(),
            detail:"Band".into(),art:String::new(),duration_ms:0,collection:true,entity:"album".into(),
            item:Some(json!({"kind":"album","ref":"album:7","title":"Record","artist":"Band","description":"Review","copyright":"Publisher","track_credits":[{"title":"Song","performers":"Player, Guitar"}]})),
        });
        let info: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(info["description"], "Review");
        assert_eq!(info["copyright"], "Publisher");
        assert_eq!(info["track_credits"][0]["performers"], "Player, Guitar");
    }
    #[test]
    fn local_information_uses_library_metadata_without_a_plugin() {
        let library = chilpa_library::Library::in_memory().unwrap();
        let source = library.source_id("local", "default").unwrap();
        let album = library.source_edition(source,"record","Record","Band",Some(2001)).unwrap();
        library.connection().execute("INSERT INTO tracks(source_id,external_id,title,artist,album_id,added_at) VALUES (?1,'track.flac','Song','Performer',?2,0)",rusqlite::params![source,album]).unwrap();
        let mut api = Chilpa::new(); api.library = Some(library);
        let info: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(info["title"], "Record");
        assert_eq!(info["release_date"], "2001");
        assert_eq!(info["track_credits"][0]["artist"], "Performer");
        assert!(info["plugin"].is_null());
        assert!(info["description"].is_null());
    }
}
