//! I expose technical album facts only from an existing database or stored catalog.
use crate::Chilpa;
use serde_json::json;
impl Chilpa {
    pub fn album_technical(&self, id: i64) -> String {
        if id < 0 {
            let mut document = self.catalog.album_technical(id);
            if let Some(plugin) = document["source"].as_str() {
                let name = self.source_name(plugin);
                if !name.is_empty() { document["source"] = json!(name); }
            }
            return document.to_string();
        }
        let read = |library: &chilpa_library::Library| library.album_technical(id).unwrap_or_else(|_| json!({}));
        self.library.as_ref().map(read).or_else(|| {
            chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok().map(|library| read(&library))
        }).unwrap_or_else(|| json!({})).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn album_projection_keeps_the_facade_read_only_and_does_not_require_media() {
        let library=chilpa_library::Library::in_memory().unwrap();
        library.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES(1,'local','default');
            INSERT INTO albums(id,title,album_artist) VALUES(1,'Album','Artist');
            INSERT INTO tracks(source_id,external_id,title,album_id,added_at,codec) VALUES(1,'/absent/song.flac','Song',1,0,'flac');").unwrap();
        let mut api=Chilpa::new(); api.library=Some(library);
        let changes=api.library.as_ref().unwrap().connection().total_changes();
        let document:serde_json::Value=serde_json::from_str(&api.album_technical(1)).unwrap();
        assert_eq!(document["tracks"],1); assert_eq!(document["format"],"FLAC");
        assert!(document["bytes"].is_null()); assert!(document["rate"].is_null());
        assert_eq!(api.library.as_ref().unwrap().connection().total_changes(),changes);
    }
}
