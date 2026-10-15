//! Read-only album information shared by local records and source catalogs.
use crate::Chilpa;
use serde_json::{json, Value};

impl Chilpa {
    /// I resolve a card's credit only on interaction, without loading its tracks.
    pub fn album_artist_link(&mut self, id: i64, name: &str) -> String {
        let name = name.trim();
        if name.is_empty() { return "{}".into(); }
        let info = self.catalog.card(id).map(|card| {
            if card.kind != crate::source_catalog::store::CardKind::Album { return Value::Null; }
            let mut info = card.metadata; info["plugin"] = json!(card.plugin); info
        });
        if info.as_ref().is_some_and(Value::is_null) { return "{}".into(); }
        let info = info.unwrap_or_else(|| json!({}));
        person(name, info["artist_ref"].as_str().unwrap_or(""), &info).to_string()
    }
    /// I read only the header's linked names; full credits remain on demand.
    pub fn album_artist_information(&mut self, id: i64) -> String {
        let Some(album) = self.album(id) else { return "{}".into(); };
        let mut info = self.catalog.card(id).map(|card| {
            let mut value = card.metadata;
            if !value.is_object() { value = json!({}); }
            value["plugin"] = json!(card.plugin);
            value
        }).unwrap_or_else(|| json!({}));
        let rows = if chilpa_library::is_various(&album.artist) { self.album_track_rows(id) } else { Vec::new() };
        if info["track_credits"].as_array().is_none_or(Vec::is_empty) {
            info["track_credits"] = json!(rows.iter().map(|row| json!({"artist":row.artist})).collect::<Vec<_>>());
        }
        self.album_artist_document(&album.artist, &info, &rows).to_string()
    }

    fn album_artist_document(&mut self, artist: &str, info: &Value, rows: &[crate::TrackRow]) -> Value {
        let mut artists = Vec::new();
        let artist = artist.trim();
        if !chilpa_library::is_various(artist) {
            add_artist(&mut artists, person(artist, info["artist_ref"].as_str().unwrap_or(""), info));
        }
        for credit in info["credits"].as_array().into_iter().flatten() {
            let name = credit["name"].as_str().unwrap_or("");
            let reference = credit["ref"].as_str().map(str::to_owned).unwrap_or_else(|| credit["id"].as_i64().map(|id| format!("artist:{id}")).unwrap_or_default());
            add_artist(&mut artists, person(name, &reference, info));
        }
        if chilpa_library::is_various(artist) {
            for row in rows {
                let metadata = self.catalog.track_by_path(&row.path).map(|(track,_)| track.metadata).unwrap_or(Value::Null);
                for name in self.credit_names(&row.artist) {
                    add_artist(&mut artists, person(&name, metadata["artist_ref"].as_str().unwrap_or(""), info));
                }
            }
            for track in info["track_credits"].as_array().into_iter().flatten() {
                if let Some(name) = track["artist"].as_str() { add_artist(&mut artists, person(name, "", info)); }
            }
        }
        let links = linked_names(&artists);
        let html = json!(if chilpa_library::is_various(artist) && !artists.is_empty() { format!("{} ({links})", escape(artist)) } else if artists.is_empty() { escape(artist) } else { links });
        json!({"artist_html":html,"artist_links":artists})
    }

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
        if info["release_date"].as_str().is_none_or(str::is_empty) {
            info["release_date"] = json!(self.album_release_date(id).or_else(|| detail.album.year.map(|year| year.to_string())).unwrap_or_default());
        }
        info["released"] = json!(crate::album_versions::release_label(info["release_date"].as_str().unwrap_or_default()));
        let rows = self.album_track_rows(id);
        if !info["track_credits"].is_array() || info["track_credits"].as_array().is_some_and(Vec::is_empty) {
            info["track_credits"] = json!(rows.iter().map(|track| {
                let metadata = self.catalog.track_by_path(&track.path).map(|(track,_)| track.metadata).unwrap_or(Value::Null);
                json!({"title":track.title,"artist":track.artist,"number":track.number,"performers":metadata["performers"],"copyright":metadata["copyright"]})
            }).collect::<Vec<_>>());
        }
        let artists = self.album_artist_document(&detail.album.artist, &info, &rows);
        info["artist_html"] = artists["artist_html"].clone();
        info["artist_links"] = artists["artist_links"].clone();
        let mut tracks = info["track_credits"].as_array().cloned().unwrap_or_default();
        for (index, track) in tracks.iter_mut().enumerate() {
            if track["artist"].as_str().is_none_or(str::is_empty) {
                if let Some(row) = rows.get(index).filter(|row| track["title"] == row.title) { track["artist"] = json!(row.artist); }
            }
            track["participants"] = json!(participants(&track["performers"], &info));
        }
        info["track_credits"] = json!(tracks);
        let basic = format!("{} — {}", detail.album.title, detail.album.artist);
        let mut full = vec![basic.clone()];
        for field in ["label", "released", "genre", "quality_label", "copyright", "upc"] {
            if let Some(text) = info[field].as_str().filter(|text| !text.is_empty()) { full.push(if field == "upc" { format!("UPC: {text}") } else { text.into() }); }
        }
        info["copy_basic"] = json!(basic);
        info["copy_header"] = json!(full.join("\n"));
        for track in &tracks {
            full.push(format!("\n{}", track["title"].as_str().unwrap_or("")));
            if let Some(artist) = track["artist"].as_str().filter(|name| !name.is_empty()) { full.push(artist.into()); }
            for participant in track["participants"].as_array().into_iter().flatten() {
                full.push(format!("{} — {}", participant["name"].as_str().unwrap_or(""), participant["roles"].as_str().unwrap_or("")));
            }
            if let Some(copyright) = track["copyright"].as_str() { full.push(copyright.into()); }
        }
        info["copy_credits"] = json!(full.join("\n"));
        info["external_links"] = external_links(&detail.album.artist, &detail.album.title);
        info["copy_actions"] = json!([{"text":chilpa_i18n::t("Copy album and artist")},{"text":chilpa_i18n::t("Copy full information")}]);
        info["link_actions"] = json!([{"text":chilpa_i18n::t("Open in browser")},{"text":chilpa_i18n::t("Copy URL to clipboard")}]);
        info.to_string()
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn person(name: &str, reference: &str, info: &Value) -> Value {
    let plugin = info["plugin"].as_str().unwrap_or("");
    let key = if plugin.is_empty() { name.to_string() }
        else if !reference.is_empty() { format!("{plugin} native:{reference}") }
        else { format!("{plugin} matching-artist-v3:{name}") };
    json!({"name":name.trim(),"target":if plugin.is_empty() { "artist" } else { "source" },"key":key})
}
fn add_artist(artists: &mut Vec<Value>, artist: Value) {
    let name = artist["name"].as_str().unwrap_or("");
    if !name.is_empty() && !chilpa_library::is_various(name) && !artists.iter().any(|item| item["name"].as_str().unwrap_or("").to_lowercase() == name.to_lowercase()) { artists.push(artist); }
}
fn linked_names(artists: &[Value]) -> String {
    artists.iter().enumerate().map(|(index, artist)| format!("<a href=\"{index}\">{}</a>", escape(artist["name"].as_str().unwrap_or("")))).collect::<Vec<_>>().join(", ")
}
fn role_label(role: &str) -> String {
    let mut label = String::new();
    let mut lower = false;
    for ch in role.chars() {
        if lower && ch.is_uppercase() { label.push(' '); }
        label.push(ch);
        lower = ch.is_lowercase();
    }
    label
}
fn participants(value: &Value, info: &Value) -> Vec<Value> {
    let entries: Vec<Value> = if let Some(text) = value.as_str() {
        text.lines().flat_map(|line| line.split(" - ")).filter_map(|entry| {
            let mut parts = entry.split(", ");
            let name = parts.next()?.trim();
            if name.is_empty() { return None; }
            Some(json!({"name":name,"roles":parts.map(role_label).collect::<Vec<_>>().join(", ")}))
        }).collect()
    } else { value.as_array().cloned().unwrap_or_default() };
    entries.into_iter().filter_map(|entry| {
        let name = entry["name"].as_str()?.trim();
        if name.is_empty() || chilpa_library::is_various(name) { return None; }
        let known = info["artist_links"].as_array().into_iter().flatten().find(|artist| artist["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)));
        let reference = entry["ref"].as_str().unwrap_or("");
        let mut participant = known.cloned().unwrap_or_else(|| person(name, reference, info));
        participant["roles"] = if let Some(roles) = entry["roles"].as_array() { json!(roles.iter().filter_map(Value::as_str).map(role_label).collect::<Vec<_>>().join(", ")) } else { json!(role_label(entry["roles"].as_str().unwrap_or(""))) };
        Some(participant)
    }).collect()
}
fn external_links(artist: &str, title: &str) -> Value {
    let query = format!("{artist} {title}");
    let mut brainz = url::Url::parse("https://musicbrainz.org/search").unwrap();
    brainz.query_pairs_mut().append_pair("query", &query).append_pair("type", "release").append_pair("method", "indexed");
    let mut discogs = url::Url::parse("https://www.discogs.com/search/").unwrap();
    discogs.query_pairs_mut().append_pair("q", &query).append_pair("type", "release");
    let mut last = url::Url::parse("https://www.last.fm/music/").unwrap();
    last.path_segments_mut().unwrap().pop_if_empty().push(artist).push(title);
    json!([{"name":"MusicBrainz","url":brainz.as_str()},{"name":"Discogs","url":discogs.as_str()},{"name":"Last.fm","url":last.as_str()}])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credits_keep_people_separate_and_reuse_known_artist_routes() {
        let info = json!({"plugin":"fixture","artist_links":[{"name":"Band","target":"source","key":"fixture native:artist:7"}]});
        let credits = participants(&json!("Band, MainArtist - Jean-Michel, MixingEngineer - Player, BackgroundVocalist, Guitar"), &info);
        assert_eq!(credits.len(), 3);
        assert_eq!(credits[0]["key"], "fixture native:artist:7");
        assert_eq!(credits[1]["name"], "Jean-Michel");
        assert_eq!(credits[1]["roles"], "Mixing Engineer");
        assert_eq!(credits[2]["roles"], "Background Vocalist, Guitar");
        assert_eq!(credits[2]["key"], "fixture matching-artist-v3:Player");
    }
    #[test]
    fn compilation_artists_exclude_placeholders_and_escape_markup() {
        let mut artists = Vec::new();
        for name in ["Various Artists", "Player & <Band>", "player & <band>", "V.A.", ""] { add_artist(&mut artists, person(name, "", &json!({}))); }
        assert_eq!(artists.len(), 1);
        assert_eq!(linked_names(&artists), "<a href=\"0\">Player &amp; &lt;Band&gt;</a>");
        assert_eq!(artists[0]["target"], "artist");
    }
    #[test]
    fn external_urls_preserve_reserved_characters_in_names() {
        let links = external_links("AC/DC", "A & B?#");
        let brainz = url::Url::parse(links[0]["url"].as_str().unwrap()).unwrap();
        assert_eq!(brainz.query_pairs().find(|(key,_)| key == "query").unwrap().1, "AC/DC A & B?#");
        assert_eq!(links[2]["url"], "https://www.last.fm/music/AC%2FDC/A%20&%20B%3F%23");
    }
    #[test]
    fn source_information_does_not_require_native_page_capabilities() {
        let mut api = Chilpa::new();
        let album = api.catalog.adopt("fixture.source",None,&crate::source_search::RemoteFound {
            source:"fixture.source".into(),name:"Source".into(),id:"album:7".into(),title:"Record".into(),
            detail:"Band".into(),art:String::new(),duration_ms:0,collection:true,entity:"album".into(),
            item:Some(json!({"kind":"album","ref":"album:7","title":"Record","artist":"Band","description":"Review","copyright":"Publisher","track_credits":[{"title":"Song","performers":"Player, Guitar"}]})),
            no_audio:false,
        });
        let info: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(info["description"], "Review");
        assert_eq!(info["copyright"], "Publisher");
        assert_eq!(info["track_credits"][0]["performers"], "Player, Guitar");
        let header: Value = serde_json::from_str(&api.album_artist_information(album)).unwrap();
        assert_eq!(header["artist_html"], info["artist_html"]);
        assert_eq!(header["artist_links"], info["artist_links"]);
        assert!(header.get("track_credits").is_none());
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
        assert_eq!(info["released"], "2001");
        assert_eq!(info["track_credits"][0]["artist"], "Performer");
        let header: Value = serde_json::from_str(&api.album_artist_information(album)).unwrap();
        assert_eq!(header["artist_html"], info["artist_html"]);
        assert_eq!(header["artist_links"], info["artist_links"]);
        assert!(header.get("track_credits").is_none(), "the header does not serialize full credits");
        api.library.as_ref().unwrap().connection().execute("UPDATE tracks SET release_date = '2001-09-11' WHERE album_id = ?1", [album]).unwrap();
        let dated: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(dated["release_date"], "2001-09-11", "the whole date its file says");
        assert_eq!(dated["released"], "September 11, 2001");
        assert!(info["plugin"].is_null());
        assert!(info["description"].is_null());
        api.library.as_ref().unwrap().connection().execute("UPDATE albums SET album_artist='Various Artists' WHERE id=?1", [album]).unwrap();
        let compilation: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(compilation["artist_html"], "Various Artists (<a href=\"0\">Performer</a>)");
        assert_eq!(compilation["artist_links"][0]["key"], "Performer");
        let header: Value = serde_json::from_str(&api.album_artist_information(album)).unwrap();
        assert_eq!(header["artist_html"], compilation["artist_html"]);
        assert_eq!(header["artist_links"], compilation["artist_links"]);
        api.library.as_ref().unwrap().connection().execute("UPDATE tracks SET artist='Various Artists' WHERE album_id=?1", [album]).unwrap();
        let mislabeled: Value = serde_json::from_str(&api.album_information(album)).unwrap();
        assert_eq!(mislabeled["artist_html"], "Various Artists");
        assert_eq!(mislabeled["artist_links"], json!([]));
    }
}
