//! Public music identity from Open Graph and Schema.org, without a source plugin.
use quick_xml::{Reader, events::Event};
use serde_json::Value;
use std::collections::HashMap;

/// Searchable metadata only; source references remain owned by plugins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub kind: String,
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl Identity {
    pub fn usable(&self) -> bool {
        self.kind == "playlist" || !self.title.is_empty()
            && (self.kind == "artist" || !self.artist.is_empty())
    }
}

fn name(value: &Value) -> String {
    if let Some(rows) = value.as_array() { return rows.first().map(name).unwrap_or_default(); }
    value.as_str().or_else(|| value["name"].as_str()).unwrap_or_default().trim().to_string()
}

fn structured(value: &Value, depth: usize) -> Option<Identity> {
    if depth > 12 { return None; }
    if let Some(rows) = value.as_array() { return rows.iter().find_map(|row| structured(row, depth + 1)); }
    let types = &value["@type"];
    let has = |kind: &str| types == kind || types.as_array().is_some_and(|rows| rows.iter().any(|row| row == kind));
    let kind = if has("MusicRecording") { "track" } else if has("MusicAlbum") { "album" }
        else if has("MusicPlaylist") { "playlist" } else if has("MusicGroup") { "artist" } else { "" };
    if !kind.is_empty() {
        return Some(Identity { kind: kind.into(), title: name(&value["name"]),
            artist: name(&value["byArtist"]), album: name(&value["inAlbum"]) });
    }
    value.get("@graph").or_else(|| value.get("mainEntity")).and_then(|row| structured(row, depth + 1))
}

/// The caller bounds the response to 2 MiB. No scripts are executed or retained.
pub fn identify(html: &str) -> Option<Identity> {
    let mut remaining = html;
    let mut reader = Reader::from_str(remaining);
    reader.config_mut().check_end_names = false;
    let mut meta = HashMap::new();
    let mut schema = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                let tag_name = tag.name();
                if tag_name.as_ref().eq_ignore_ascii_case(b"meta") {
                    let mut key = String::new();
                    let mut content = String::new();
                    for attr in tag.html_attributes().flatten() {
                        let value = reader.decoder().decode(attr.value.as_ref()).unwrap_or_default();
                        let value = quick_xml::escape::unescape(&value).unwrap_or_default().into_owned();
                        match attr.key.as_ref().to_ascii_lowercase().as_slice() {
                            b"property" | b"name" => key = value.to_ascii_lowercase(),
                            b"content" => content = value.trim().to_string(),
                            _ => {}
                        }
                    }
                    if matches!(key.as_str(), "og:title" | "og:type" | "og:description" | "music:musician_description") {
                        meta.entry(key).or_insert(content);
                    }
                } else if tag_name.as_ref().eq_ignore_ascii_case(b"script") || tag_name.as_ref().eq_ignore_ascii_case(b"style") {
                    let linked = tag.html_attributes().flatten().any(|attr| attr.key.as_ref() == b"type"
                        && attr.value.as_ref().eq_ignore_ascii_case(b"application/ld+json"));
                    // Raw script/style text is HTML, so comparisons containing
                    // '<' must not go through the XML token reader.
                    let closing: &[u8] = if tag_name.as_ref().eq_ignore_ascii_case(b"script") { b"</script" } else { b"</style" };
                    let tail = &remaining[reader.buffer_position() as usize..];
                    let Some(end) = tail.as_bytes().windows(closing.len()).position(|text| text.eq_ignore_ascii_case(closing)) else { break; };
                    if linked && schema.is_none() {
                        schema = serde_json::from_str::<Value>(&tail[..end]).ok().and_then(|value| structured(&value, 0));
                    }
                    let Some(close) = tail[end..].find('>') else { break; };
                    remaining = &tail[end + close + 1..];
                    reader = Reader::from_str(remaining);
                    reader.config_mut().check_end_names = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let get = |key: &str| meta.get(key).cloned().unwrap_or_default();
    let kind = match get("og:type").as_str() {
        "music.song" => "track", "music.album" => "album", "music.playlist" => "playlist", "music.musician" => "artist", _ => "",
    };
    let mut found = schema.unwrap_or_default();
    if found.kind.is_empty() { found.kind = kind.into(); }
    if found.title.is_empty() { found.title = get("og:title"); }
    if found.artist.is_empty() { found.artist = get("music:musician_description"); }
    // Compact music descriptions use artist, album, media kind and year.
    let description = get("og:description");
    let parts: Vec<_> = description.split('·').map(str::trim).collect();
    if !kind.is_empty() && parts.len() >= 3 {
        if found.artist.is_empty() { found.artist = parts[0].into(); }
        if found.kind == "track" && found.album.is_empty() && parts.len() >= 4 { found.album = parts[1].into(); }
    }
    found.usable().then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_track_metadata_needs_no_plugin_and_decodes_entities() {
        let html = r#"<!doctype html><html><head>
            <meta property='og:title' content='Song &amp; Sound'>
            <meta content='music.song' property='og:type'>
            <meta property='og:description' content='Band · Record · Song · 2026'>
            <meta name='music:musician_description' content='Band'>
            </head></html>"#;
        assert_eq!(identify(html), Some(Identity {kind:"track".into(), title:"Song & Sound".into(), artist:"Band".into(), album:"Record".into()}));
    }

    #[test]
    fn linked_data_handles_graphs_arrays_and_multiple_types() {
        let html = r#"<script type="application/ld+json">{"@graph":[{"@type":["CreativeWork","MusicRecording"],
            "name":"Song","byArtist":[{"name":"Band"}],"inAlbum":{"name":"Record"}}]}</script>"#;
        assert_eq!(identify(html).unwrap().artist, "Band");
        assert_eq!(identify(html).unwrap().album, "Record");
    }

    #[test]
    fn incomplete_linked_data_can_use_open_graph_artist() {
        let html = r#"<meta property="og:type" content="music.album"><meta name="music:musician_description" content="Band">
            <script type="application/ld+json">[{"@type":"MusicAlbum","name":"Record"}]</script>"#;
        assert_eq!(identify(html).unwrap().title, "Record");
    }

    #[test]
    fn ordinary_pages_and_uncredited_music_are_not_searches() {
        assert!(identify(r#"<meta property="og:title" content="Home"><meta property="og:type" content="website">"#).is_none());
        assert!(identify(r#"<meta property="og:title" content="Song"><meta property="og:type" content="music.song">"#).is_none());
    }

    #[test]
    fn playlists_and_artist_links_keep_their_kind() {
        assert_eq!(identify(r#"<meta property="og:type" content="music.playlist">"#).unwrap().kind, "playlist");
        assert_eq!(identify(r#"<script type="application/ld+json">{"@type":"MusicGroup","name":"Band"}</script>"#).unwrap().kind, "artist");
    }

    #[test]
    fn raw_scripts_and_styles_do_not_hide_later_metadata() {
        let html = r#"<script>if (a < b) { window.value = '</not-a-tag>'; }</SCRIPT>
            <style>p::before { content: '<'; }</style>
            <meta property='og:type' content='music.album'><meta property='og:title' content='Record'>
            <meta name='music:musician_description' content='Band'>"#;
        assert_eq!(identify(html).unwrap().artist, "Band");
    }
}
