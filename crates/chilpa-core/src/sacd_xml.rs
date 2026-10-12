//! Metadata exported beside a SACD image by sacd_extract.
//!
//! The image remains the authority for track addresses and audio. An XML
//! must name exactly those stereo tracks before its text can override the
//! image. A Chilpa sidecar is applied after this layer.

use chilpa_tags::Tags;
use chilpa_tags::edit::Snapshot;
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_XML: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct XmlTrack {
    values: BTreeMap<String, String>,
}

impl XmlTrack {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str).filter(|value| !value.is_empty())
    }
}

#[derive(Clone, Debug)]
pub struct XmlMetadata {
    pub path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub catalog_number: Option<String>,
    pub disc_number: Option<u16>,
    pub disc_total: Option<u16>,
    pub tracks: BTreeMap<u8, XmlTrack>,
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|child| child.is_element() && child.tag_name().name() == name)
}

fn text(node: Option<Node<'_, '_>>) -> Option<String> {
    node.and_then(|node| node.text()).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn attribute(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn parse(path: &Path, bytes: &[u8], expected: &BTreeSet<u8>) -> Option<XmlMetadata> {
    let content = std::str::from_utf8(bytes).ok()?;
    let doc = roxmltree::Document::parse(content).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "AudioMetadata" { return None; }
    let album = child(root, "Album")?;
    let disc = child(album, "Disc")?;
    let area = disc.children().find(|node| node.is_element() && node.tag_name().name() == "Area" && node.attribute("id") == Some("0"))?;
    let mut tracks = BTreeMap::new();
    for node in area.children().filter(|node| node.is_element() && node.tag_name().name() == "track") {
        let id = node.attribute("id")?.parse::<u8>().ok()?;
        let mut values = BTreeMap::new();
        for meta in node.children().filter(|child| child.is_element() && child.tag_name().name() == "meta") {
            let Some(name) = meta.attribute("name") else { continue };
            let Some(value) = meta.attribute("value") else { continue };
            values.insert(name.trim().to_ascii_uppercase(), value.trim().to_owned());
        }
        if tracks.insert(id, XmlTrack { values }).is_some() { return None; }
    }
    if tracks.keys().copied().collect::<BTreeSet<_>>() != *expected { return None; }
    let first = tracks.values().next();
    let title = text(child(album, "Title")).or_else(|| text(child(disc, "Title")))
        .or_else(|| first.and_then(|track| track.get("ALBUM")).map(str::to_owned));
    let artist = text(child(album, "Artist")).or_else(|| text(child(disc, "Artist")))
        .or_else(|| first.and_then(|track| track.get("ALBUM ARTIST")).map(str::to_owned));
    Some(XmlMetadata {
        path: path.to_owned(), title, artist,
        genre: attribute(album, "genre").or_else(|| attribute(disc, "genre")),
        date: attribute(disc, "creation_date"),
        catalog_number: attribute(album, "catalog_number").or_else(|| attribute(disc, "catalog_number")),
        disc_number: attribute(album, "sequence_number").and_then(|v| v.parse().ok()),
        disc_total: attribute(album, "set_size").and_then(|v| v.parse().ok()),
        tracks,
    })
}

fn xml_files(image: &Path) -> Vec<PathBuf> {
    let Some(parent) = image.parent() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(parent) else { return Vec::new() };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("xml")))
        .collect();
    paths.sort();
    paths
}

fn image_count(image: &Path) -> usize {
    image.parent().and_then(|parent| fs::read_dir(parent).ok()).map_or(0, |entries| {
        entries.filter_map(Result::ok).filter(|entry| super::sacd_tracks::is_image(&entry.path())).count()
    })
}

fn name_matches(image: &Path, xml: &Path) -> bool {
    let (Some(image), Some(xml)) = (image.file_stem(), xml.file_stem()) else { return false };
    let image = image.to_string_lossy().to_lowercase();
    let xml = xml.to_string_lossy().to_lowercase();
    image.contains(&xml) || xml.contains(&image)
}

/// Find an unambiguous stereo XML. Identical duplicate exports count as one;
/// otherwise the one whose name is part of the ISO's name must stand alone.
pub fn read_for(image: &Path, expected: &[u8]) -> Option<XmlMetadata> {
    let expected: BTreeSet<_> = expected.iter().copied().collect();
    if expected.is_empty() { return None; }
    let mut candidates = Vec::new();
    for path in xml_files(image) {
        let Ok(meta) = fs::metadata(&path) else { continue };
        if meta.len() > MAX_XML { continue; }
        let Ok(bytes) = fs::read(&path) else { continue };
        if let Some(parsed) = parse(&path, &bytes, &expected) { candidates.push((parsed, bytes)); }
    }
    if candidates.len() == 1 {
        let parsed = candidates.pop()?.0;
        return (image_count(image) <= 1 || name_matches(image, &parsed.path)).then_some(parsed);
    }
    if candidates.is_empty() { return None; }
    if candidates.windows(2).all(|pair| pair[0].1 == pair[1].1) {
        if image_count(image) > 1 {
            let mut matching = candidates.into_iter().filter(|(parsed, _)| name_matches(image, &parsed.path));
            let first = matching.next()?.0;
            return matching.next().is_none().then_some(first);
        }
        candidates.sort_by_key(|(parsed, _)| (parsed.path.file_name().map_or(usize::MAX, |name| name.len()), parsed.path.clone()));
        return candidates.into_iter().next().map(|(parsed, _)| parsed);
    }
    let mut matching = candidates.into_iter().filter(|(parsed, _)| name_matches(image, &parsed.path));
    let first = matching.next()?.0;
    matching.next().is_none().then_some(first)
}

/// A stable change marker for a scanner that otherwise looks only at ISO
/// size and time. The XML can be added, replaced or removed without touching
/// the image.
pub fn stamp(image: &Path) -> u64 {
    let paths = xml_files(image);
    if paths.is_empty() { return 0; }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for path in paths {
        path.file_name().hash(&mut hash);
        if let Ok(meta) = fs::metadata(&path) {
            meta.len().hash(&mut hash);
            meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|time| time.as_nanos()).hash(&mut hash);
        }
    }
    hash.finish()
}

/// Edits expressed in the names used by the metadata editor. A field with
/// no XML representation is intentionally absent and stays in a sidecar.
#[derive(Clone, Debug, Default)]
pub struct XmlChanges {
    pub album: BTreeMap<String, String>,
    pub tracks: BTreeMap<u8, BTreeMap<String, String>>,
}

pub fn album_field(name: &str) -> bool {
    matches!(name, "title" | "album_artist" | "date" | "genres" | "catalog_number"
        | "musicbrainz_release_id" | "musicbrainz_release_group_id")
}

pub fn track_field(name: &str) -> bool {
    matches!(name, "title" | "artist_credit" | "composers" | "musicbrainz_recording_id" | "musicbrainz_track_id")
}

struct Patch { start: usize, end: usize, text: String }

fn escaped(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

fn start_end(source: &str, node: Node<'_, '_>) -> Option<usize> {
    let mut quoted = None;
    for (offset, ch) in source[node.range().start..].char_indices() {
        if ch == '\'' || ch == '"' {
            if quoted == Some(ch) { quoted = None; } else if quoted.is_none() { quoted = Some(ch); }
        }
        if ch == '>' && quoted.is_none() { return Some(node.range().start + offset + 1); }
    }
    None
}

fn attribute_patch(source: &str, node: Node<'_, '_>, changed: &BTreeMap<String, String>) -> Option<Patch> {
    if changed.is_empty() { return None; }
    let end = start_end(source, node)?;
    let raw = &source[node.range().start..end];
    let mut attrs: BTreeMap<String, String> = node.attributes().map(|attr| (attr.name().to_owned(), attr.value().to_owned())).collect();
    attrs.extend(changed.iter().map(|(name, value)| (name.clone(), value.clone())));
    let closing = if raw.trim_end().ends_with("/>") { "/>" } else { ">" };
    let mut text = format!("<{}", node.tag_name().name());
    for (name, value) in attrs { text.push_str(&format!(" {name}=\"{}\"", escaped(&value))); }
    text.push_str(closing);
    Some(Patch { start: node.range().start, end, text })
}

fn element_text_patch(source: &str, parent: Node<'_, '_>, name: &str, value: &str) -> Option<Patch> {
    if let Some(node) = child(parent, name) {
        let start = start_end(source, node)?;
        let end = source[node.range().start..node.range().end].rfind(&format!("</{name}>")).map(|at| node.range().start + at)?;
        Some(Patch { start, end, text: escaped(value) })
    } else {
        let at = start_end(source, parent)?;
        Some(Patch { start: at, end: at, text: format!("<{name}>{}</{name}>", escaped(value)) })
    }
}

fn meta_patch(source: &str, track: Node<'_, '_>, values: &BTreeMap<String, String>) -> Option<Vec<Patch>> {
    let mut patches = Vec::new();
    let mut missing = values.clone();
    for node in track.children().filter(|node| node.is_element() && node.tag_name().name() == "meta") {
        let Some(name) = node.attribute("name") else { continue };
        let key = name.to_ascii_uppercase();
        let Some(value) = values.get(&key) else { continue };
        missing.remove(&key);
        let attrs = BTreeMap::from([("value".to_owned(), value.clone())]);
        patches.push(attribute_patch(source, node, &attrs)?);
    }
    if !missing.is_empty() {
        let at = source[track.range().start..track.range().end].rfind("</track>").map(|at| track.range().start + at)?;
        let text = missing.into_iter().map(|(name, value)| format!("<meta name=\"{}\" value=\"{}\"/>", escaped(&name), escaped(&value))).collect::<String>();
        patches.push(Patch { start: at, end: at, text });
    }
    Some(patches)
}

fn edited(source: &str, path: &Path, expected: &BTreeSet<u8>, changes: &XmlChanges) -> Option<String> {
    let doc = roxmltree::Document::parse(source).ok()?;
    let root = doc.root_element();
    if doc.descendants().any(|node| node.is_element() &&
        (node.tag_name().namespace().is_some() || node.attributes().any(|attr| attr.namespace().is_some()))) { return None; }
    let album = child(root, "Album")?;
    let disc = child(album, "Disc")?;
    let area = disc.children().find(|node| node.is_element() && node.tag_name().name() == "Area" && node.attribute("id") == Some("0"))?;
    let mut patches = Vec::new();
    let mut album_attrs = BTreeMap::new();
    let mut disc_attrs = BTreeMap::new();
    let mut all_track_meta = BTreeMap::new();
    for (field, value) in &changes.album {
        match field.as_str() {
            "title" => {
                patches.push(element_text_patch(source, album, "Title", value)?);
                patches.push(element_text_patch(source, disc, "Title", value)?);
                all_track_meta.insert("ALBUM".to_owned(), value.clone());
            }
            "album_artist" => {
                patches.push(element_text_patch(source, album, "Artist", value)?);
                patches.push(element_text_patch(source, disc, "Artist", value)?);
                all_track_meta.insert("ALBUM ARTIST".to_owned(), value.clone());
            }
            "date" => { disc_attrs.insert("creation_date".to_owned(), value.clone()); all_track_meta.insert("DATE".to_owned(), value.clone()); }
            "genres" => { album_attrs.insert("genre".to_owned(), value.clone()); disc_attrs.insert("genre".to_owned(), value.clone()); all_track_meta.insert("GENRE".to_owned(), value.clone()); }
            "catalog_number" => { album_attrs.insert("catalog_number".to_owned(), value.clone()); disc_attrs.insert("catalog_number".to_owned(), value.clone()); }
            "musicbrainz_release_id" => { all_track_meta.insert("MUSICBRAINZ_ALBUMID".to_owned(), value.clone()); }
            "musicbrainz_release_group_id" => { all_track_meta.insert("MUSICBRAINZ_RELEASEGROUPID".to_owned(), value.clone()); }
            _ => return None,
        }
    }
    if let Some(patch) = attribute_patch(source, album, &album_attrs) { patches.push(patch); }
    if let Some(patch) = attribute_patch(source, disc, &disc_attrs) { patches.push(patch); }
    let mut seen = BTreeSet::new();
    for track in area.children().filter(|node| node.is_element() && node.tag_name().name() == "track") {
        let number = track.attribute("id")?.parse::<u8>().ok()?;
        seen.insert(number);
        let mut values = all_track_meta.clone();
        if let Some(fields) = changes.tracks.get(&number) {
            for (field, value) in fields {
                let name = match field.as_str() {
                    "title" => "TITLE", "artist_credit" => "PERFORMER", "composers" => "COMPOSER",
                    "musicbrainz_recording_id" => "MUSICBRAINZ_RECORDINGID", "musicbrainz_track_id" => "MUSICBRAINZ_TRACKID",
                    _ => return None,
                };
                values.insert(name.to_owned(), value.clone());
            }
        }
        patches.extend(meta_patch(source, track, &values)?);
    }
    if seen != *expected || changes.tracks.keys().any(|number| !seen.contains(number)) { return None; }
    patches.sort_by_key(|patch| (std::cmp::Reverse(patch.start), std::cmp::Reverse(patch.end)));
    let mut out = source.to_owned();
    let mut previous = source.len();
    for patch in patches {
        if patch.end > previous { return None; }
        out.replace_range(patch.start..patch.end, &patch.text);
        previous = patch.start;
    }
    parse(path, out.as_bytes(), expected)?;
    Some(out)
}

pub struct StagedXml { temp: tempfile::NamedTempFile, path: PathBuf }

impl StagedXml {
    pub fn land(self) -> Result<(), String> {
        self.temp.persist(&self.path).map_err(|error| error.error.to_string())?;
        Ok(())
    }
}

/// Check the ISO pairing again before preparing an atomic replacement. The
/// temporary file is removed if another stage fails before `land`.
pub fn stage(image: &Path, path: &Path, changes: &XmlChanges) -> Result<Vec<StagedXml>, String> {
    let xml = super::sacd_tracks::xml_for(image).ok_or("the XML no longer matches the disc")?;
    if xml.path != path { return Err("the XML paired with the disc changed".into()); }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_XML { return Err("the XML is too large".into()); }
    let expected: BTreeSet<_> = xml.tracks.keys().copied().collect();
    let mut staged = Vec::new();
    // Exports sometimes leave two byte-identical XMLs in the same folder.
    // Update both, or the next read would see divergent candidates.
    for copy in xml_files(image) {
        if image_count(image) > 1 && copy != path { continue; }
        if copy != path && fs::read(&copy).ok().as_deref() != Some(bytes.as_slice()) { continue; }
        let source = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        let out = edited(source, &copy, &expected, changes).ok_or("the XML could not be safely edited")?;
        let parent = copy.parent().ok_or("the XML has no parent folder")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        let permissions = fs::metadata(&copy).map_err(|error| error.to_string())?.permissions();
        temp.as_file().set_permissions(permissions).map_err(|error| error.to_string())?;
        temp.write_all(out.as_bytes()).map_err(|error| error.to_string())?;
        temp.as_file().sync_all().map_err(|error| error.to_string())?;
        staged.push(StagedXml { temp, path: copy });
    }
    if staged.is_empty() { return Err("the XML disappeared".into()); }
    Ok(staged)
}

impl XmlMetadata {
    /// Put XML text over the ISO's text; an empty XML value leaves the ISO.
    pub fn overlay_tags(&self, number: u8, tags: &mut Tags) {
        let Some(track) = self.tracks.get(&number) else { return };
        if let Some(title) = track.get("TITLE") { tags.title = Some(title.to_owned()); }
        if let Some(artist) = track.get("PERFORMER") { tags.artists = vec![artist.to_owned()]; }
        if let Some(album) = track.get("ALBUM").or(self.title.as_deref()) { tags.album = Some(album.to_owned()); }
        if let Some(artist) = track.get("ALBUM ARTIST").or(self.artist.as_deref()) { tags.album_artist = Some(artist.to_owned()); }
        if let Some(genre) = track.get("GENRE").or(self.genre.as_deref()) { tags.genres = vec![genre.to_owned()]; }
        if let Some(composer) = track.get("COMPOSER").or_else(|| track.get("SONGWRITER")) { tags.composers = vec![composer.to_owned()]; }
        if let Some(date) = track.get("DATE").filter(|value| value.len() >= 4).or(self.date.as_deref()) { tags.date = Some(date.to_owned()); }
        if let Some(isrc) = track.get("ISRC") { tags.isrc = Some(isrc.to_owned()); }
        tags.disc_number = self.disc_number.or(tags.disc_number);
        tags.disc_total = self.disc_total.or(tags.disc_total);
    }

    /// The XML fields the metadata editor can show in addition to the scan's
    /// common tags. The sidecar is laid over this snapshot by the draft.
    pub fn overlay_snapshot(&self, number: u8, snapshot: &mut Snapshot) {
        let Some(track) = self.tracks.get(&number) else { return };
        snapshot.catalog_number = self.catalog_number.clone().or(snapshot.catalog_number.take());
        if let Some(composer) = track.get("COMPOSER").or_else(|| track.get("SONGWRITER")) { snapshot.composers = vec![composer.to_owned()]; }
        if let Some(performer) = track.get("PERFORMER") { snapshot.performers = vec![performer.to_owned()]; }
        if let Some(id) = track.get("MUSICBRAINZ_ALBUMID") { snapshot.musicbrainz_release_id = Some(id.to_owned()); }
        if let Some(id) = track.get("MUSICBRAINZ_RELEASEGROUPID") { snapshot.musicbrainz_release_group_id = Some(id.to_owned()); }
        if let Some(id) = track.get("MUSICBRAINZ_TRACKID") { snapshot.musicbrainz_track_id = Some(id.to_owned()); }
        if let Some(id) = track.get("MUSICBRAINZ_RECORDINGID") { snapshot.musicbrainz_recording_id = Some(id.to_owned()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AudioMetadata><!--keep this--><Album catalog_number="OLD" set_size="1" sequence_number="1" genre="Jazz"><Title>Old album</Title><Artist>Old artist</Artist><Disc creation_date="1970"><Title>Old album</Title><Artist>Old artist</Artist><Area id="0"><track id="1"><meta name="TITLE" value="Old &amp; title"/><meta name="ALBUM" value="Old album"/><meta name="PERFORMER" value="Old artist"/><meta name="EXTRA" value="keep"/></track><track id="2"><meta name="TITLE" value="Second"/></track></Area></Disc></Album></AudioMetadata>"#;

    #[test]
    fn edits_xml_text_without_losing_unknown_metadata_or_track_addresses() {
        let changes = XmlChanges {
            album: BTreeMap::from([
                ("title".into(), "New <album>".into()),
                ("catalog_number".into(), "CAT&2".into()),
                ("date".into(), "1992-01-02".into()),
            ]),
            tracks: BTreeMap::from([(2, BTreeMap::from([("title".into(), "A & B".into()),
                ("musicbrainz_recording_id".into(), "recording-id".into())]))]),
        };
        let expected = BTreeSet::from([1, 2]);
        let out = edited(SAMPLE, Path::new("disc.xml"), &expected, &changes).expect("valid edit");
        assert!(out.contains("<!--keep this-->"));
        assert!(out.contains("name=\"EXTRA\" value=\"keep\""));
        let xml = parse(Path::new("disc.xml"), out.as_bytes(), &expected).expect("valid XML");
        assert_eq!(xml.title.as_deref(), Some("New <album>"));
        assert_eq!(xml.catalog_number.as_deref(), Some("CAT&2"));
        assert_eq!(xml.date.as_deref(), Some("1992-01-02"));
        assert_eq!(xml.tracks[&1].get("ALBUM"), Some("New <album>"));
        assert_eq!(xml.tracks[&2].get("TITLE"), Some("A & B"));
        assert_eq!(xml.tracks[&2].get("MUSICBRAINZ_RECORDINGID"), Some("recording-id"));
    }

    #[test]
    fn accepts_identical_exports_and_rejects_a_mismatched_track_set() {
        let folder = tempfile::tempdir().unwrap();
        let image = folder.path().join("record.iso");
        fs::write(folder.path().join("record.xml"), SAMPLE).unwrap();
        fs::write(folder.path().join("copy.xml"), SAMPLE).unwrap();
        assert!(read_for(&image, &[1, 2]).is_some());
        assert!(read_for(&image, &[1, 3]).is_none());
        assert_ne!(stamp(&image), 0);
    }

    #[test]
    fn an_xml_in_a_folder_with_several_images_needs_a_name_match() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("one.iso");
        let second = folder.path().join("two.iso");
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();
        fs::write(folder.path().join("one.xml"), SAMPLE).unwrap();
        assert!(read_for(&first, &[1, 2]).is_some());
        assert!(read_for(&second, &[1, 2]).is_none());
    }
}
