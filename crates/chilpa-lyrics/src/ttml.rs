//! TTML lyrics: lines, words and syllables in XML.
//!
//! The W3C timed-text format, as lyrics databases write it: one `<p>` per
//! line, one `<span>` per word or syllable, singers declared in the head,
//! backing vocals as a span with the role `x-bg`, and translations either in
//! the head (keyed by line) or as spans with the role `x-translation`.
//!
//! Attributes are matched by local name: producers disagree on prefixes and
//! namespaces, and a lyric is not worth refusing over a namespace.
//!
//! What is looked after:
//!
//! - **Whitespace between spans is the space between words.** Two spans
//!   with nothing between them are one word split in syllables.
//! - **A file without a single time is still lyrics**, unsynced. Reading it
//!   as empty loses lyrics that are right there.
//! - **Every time expression**: `12.5`, `12.5s`, `1500ms`, `1:02.5` and
//!   `00:01:02.5`.

use crate::model::{Agent, AgentKind, Credit, CreditRole, Document, Layer, LayerKind, LayerLine, Line, Segment};
use roxmltree::{Node, NodeType};
use std::collections::HashMap;

/// Reads TTML lyrics. `None` when it is not XML or has no `<tt>` root.
pub fn parse(text: &str) -> Option<Document> {
    let xml = roxmltree::Document::parse(text).ok()?;
    let root = xml.root_element();
    if root.tag_name().name() != "tt" {
        return None;
    }
    let mut doc = Document::new();
    doc.language = attribute(root, "lang").map(str::to_string);

    let mut agents: HashMap<String, u32> = HashMap::new();
    for node in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "agent") {
        let Some(id) = attribute(node, "id") else { continue };
        let kind = match attribute(node, "type") {
            Some("group") => AgentKind::Group,
            Some("other") => AgentKind::Other,
            _ => AgentKind::Person,
        };
        let name = node
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "name")
            .map(|n| text_of(n).trim().to_string())
            .filter(|n| !n.is_empty());
        agents.insert(id.to_string(), doc.agents.len() as u32);
        doc.agents.push(Agent { kind, name });
    }
    for node in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "songwriter") {
        let name = text_of(node).trim().to_string();
        if !name.is_empty() {
            doc.credits.push(Credit { name, role: Some(CreditRole::Songwriter) });
        }
    }

    let Some(body) = root.children().find(|n| n.is_element() && n.tag_name().name() == "body") else {
        return Some(doc.finish());
    };
    doc.duration_ms = attribute(body, "dur").and_then(time);

    let mut keys: HashMap<String, u32> = HashMap::new();
    let mut inline_layers: Vec<Layer> = Vec::new();
    for p in body.descendants().filter(|n| n.is_element() && n.tag_name().name() == "p") {
        let index = doc.lines.len() as u32;
        let mut line = Line {
            start_ms: attribute(p, "begin").and_then(time),
            end_ms: attribute(p, "end").and_then(time),
            agent: attribute(p, "agent").and_then(|a| agents.get(a).copied()),
            section: section_of(p),
            ..Line::default()
        };
        let mut layers: Vec<(LayerKind, String, String)> = Vec::new();
        collect(p, false, &mut line.segments, &mut layers);
        if line.segments.is_empty() {
            line.text = plain_text(p);
        }
        if let Some(last) = line.segments.last_mut() {
            last.space_after = false;
        }
        if line.text.trim().is_empty() && line.segments.is_empty() {
            continue;
        }
        if let Some(key) = attribute(p, "key") {
            keys.insert(key.to_string(), index);
        }
        for (kind, language, text) in layers {
            let at = inline_layers.iter().position(|l| l.kind == kind && l.language == language);
            let layer = match at {
                Some(i) => &mut inline_layers[i],
                None => {
                    inline_layers.push(Layer { kind, language, provider: None, lines: Vec::new() });
                    inline_layers.last_mut().expect("just pushed")
                }
            };
            layer.lines.push(LayerLine { line: index, text, segments: Vec::new() });
        }
        doc.lines.push(line);
    }
    doc.layers = inline_layers;
    doc.layers.extend(keyed_layers(root, &keys));
    Some(doc.finish())
}

/// The timed pieces of a line, in order, and the translations it carries.
fn collect(node: Node, background: bool, segments: &mut Vec<Segment>, layers: &mut Vec<(LayerKind, String, String)>) {
    for child in node.children() {
        match child.node_type() {
            NodeType::Text => {
                let text = child.text().unwrap_or("");
                if text.chars().any(char::is_whitespace) {
                    if let Some(last) = segments.last_mut() {
                        last.space_after = true;
                    }
                }
            }
            NodeType::Element if child.tag_name().name() == "span" => {
                let role = attribute(child, "role").unwrap_or("");
                let language = attribute(child, "lang").unwrap_or("und").to_string();
                match role {
                    "x-translation" => {
                        layers.push((LayerKind::Translation, language, plain_text(child)));
                        continue;
                    }
                    "x-roman" => {
                        layers.push((LayerKind::Romanization, language, plain_text(child)));
                        continue;
                    }
                    _ => {}
                }
                let background = background || role == "x-bg";
                match attribute(child, "begin").and_then(time) {
                    Some(start) if !has_timed_span(child) => {
                        let raw = text_of(child);
                        let words = raw.trim();
                        if words.is_empty() {
                            continue;
                        }
                        if let Some(last) = segments.last_mut() {
                            last.space_after |= raw.starts_with(char::is_whitespace);
                        }
                        segments.push(Segment {
                            text: words.to_string(),
                            start_ms: start,
                            end_ms: attribute(child, "end").and_then(time),
                            space_after: raw.ends_with(char::is_whitespace),
                            background,
                        });
                    }
                    _ => collect(child, background, segments, layers),
                }
            }
            _ => {}
        }
    }
}

fn has_timed_span(node: Node) -> bool {
    node.descendants().skip(1).any(|n| n.is_element() && n.tag_name().name() == "span" && attribute(n, "begin").is_some())
}

/// Translations and transliterations that live in the head, keyed by line.
fn keyed_layers(root: Node, keys: &HashMap<String, u32>) -> Vec<Layer> {
    let mut layers = Vec::new();
    for (group, kind) in [("translation", LayerKind::Translation), ("transliteration", LayerKind::Romanization)] {
        for node in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == group) {
            let language = attribute(node, "lang").unwrap_or("und").to_string();
            let mut lines = Vec::new();
            for text in node.children().filter(|n| n.is_element() && n.tag_name().name() == "text") {
                let Some(line) = attribute(text, "for").and_then(|k| keys.get(k)) else { continue };
                let mut segments = Vec::new();
                let mut ignored = Vec::new();
                collect(text, false, &mut segments, &mut ignored);
                if let Some(last) = segments.last_mut() {
                    last.space_after = false;
                }
                let words = plain_text(text);
                if !words.is_empty() {
                    lines.push(LayerLine { line: *line, text: words, segments });
                }
            }
            if !lines.is_empty() {
                layers.push(Layer { kind, language, provider: None, lines });
            }
        }
    }
    layers
}

/// The part of the song a line belongs to, from it or from its `<div>`.
fn section_of(p: Node) -> Option<String> {
    p.ancestors()
        .filter(|n| n.is_element())
        .find_map(|n| attribute(n, "songPart").or_else(|| attribute(n, "song-part")))
        .map(str::to_string)
}

/// The lead text of an element: its words without translations or backing
/// vocals, with runs of whitespace made one space.
fn plain_text(node: Node) -> String {
    let mut out = String::new();
    gather(node, &mut out);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn gather(node: Node, out: &mut String) {
    for child in node.children() {
        match child.node_type() {
            NodeType::Text => out.push_str(child.text().unwrap_or("")),
            NodeType::Element => {
                if matches!(attribute(child, "role"), Some("x-translation" | "x-roman" | "x-bg")) {
                    continue;
                }
                if child.tag_name().name() == "br" {
                    out.push(' ');
                } else {
                    gather(child, out);
                }
            }
            _ => {}
        }
    }
}

/// All the text under an element, as written.
fn text_of(node: Node) -> String {
    node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

/// An attribute by its local name, whatever its namespace.
fn attribute<'a, 'input>(node: Node<'a, 'input>, local: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == local).map(|a| a.value())
}

/// A TTML time expression in milliseconds.
pub(crate) fn time(s: &str) -> Option<u32> {
    let s = s.trim();
    let seconds = |v: &str| -> Option<f64> { v.parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0) };
    let total = if let Some(v) = s.strip_suffix("ms") {
        seconds(v)? / 1000.0
    } else if let Some(v) = s.strip_suffix('s') {
        seconds(v)?
    } else if let Some(v) = s.strip_suffix('m') {
        seconds(v)? * 60.0
    } else if let Some(v) = s.strip_suffix('h') {
        seconds(v)? * 3600.0
    } else {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.as_slice() {
            [sec] => seconds(sec)?,
            [min, sec] => min.parse::<u32>().ok()? as f64 * 60.0 + seconds(sec)?,
            [hour, min, sec] => hour.parse::<u32>().ok()? as f64 * 3600.0 + min.parse::<u32>().ok()? as f64 * 60.0 + seconds(sec)?,
            _ => return None,
        }
    };
    let ms = (total * 1000.0).round();
    (ms <= u32::MAX as f64).then_some(ms as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timing;

    const SYLLABLES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttm="http://www.w3.org/ns/ttml#metadata"
    xmlns:itunes="http://music.apple.com/lyric-ttml-internal" itunes:timing="Word" xml:lang="en">
  <head>
    <metadata>
      <ttm:agent type="person" xml:id="v1"><ttm:name type="full">First Singer</ttm:name></ttm:agent>
      <ttm:agent type="group" xml:id="v2"/>
      <iTunesMetadata xmlns="http://music.apple.com/lyric-ttml-internal">
        <translations>
          <translation type="replacement" xml:lang="fr">
            <text for="L1">Bonjour le monde</text>
          </translation>
        </translations>
        <songwriters><songwriter>Some Writer</songwriter></songwriters>
      </iTunesMetadata>
    </metadata>
  </head>
  <body dur="3:34.000">
    <div begin="12.34" end="20.0" itunes:songPart="Verse">
      <p begin="12.34" end="15.02" itunes:key="L1" ttm:agent="v1"><span begin="12.34" end="12.9">Hel</span><span begin="12.9" end="13.3">lo</span> <span begin="13.3" end="15.02">world</span><span ttm:role="x-bg"><span begin="14.0" end="14.6">(oh)</span></span></p>
      <p begin="00:00:16.000" end="00:00:18.500" itunes:key="L2" ttm:agent="v2"><span begin="16s" end="17000ms">All</span> <span begin="17.0" end="18.5">together</span></p>
    </div>
  </body>
</tt>"#;

    #[test]
    fn syllables_words_and_voices_are_read() {
        let doc = parse(SYLLABLES).expect("it is TTML");
        assert_eq!(doc.timing, Timing::Syllable);
        assert_eq!(doc.language.as_deref(), Some("en"));
        assert_eq!(doc.duration_ms, Some(214_000));
        assert_eq!(doc.lines.len(), 2);

        let first = &doc.lines[0];
        assert_eq!(first.text, "Hello world", "two spans with nothing between are one word");
        assert_eq!(first.background.as_deref(), Some("(oh)"));
        assert_eq!((first.start_ms, first.end_ms), (Some(12_340), Some(15_020)));
        assert_eq!(first.section.as_deref(), Some("Verse"));
        assert_eq!(first.agent, Some(0));
        assert_eq!(doc.agents[0].name.as_deref(), Some("First Singer"));
        assert_eq!(doc.agents[1].kind, AgentKind::Group);

        let second = &doc.lines[1];
        assert_eq!(second.text, "All together");
        assert_eq!(second.segments[0].start_ms, 16_000, "seconds with a unit");
        assert_eq!(second.segments[0].end_ms, Some(17_000), "milliseconds with a unit");
        assert_eq!(second.start_ms, Some(16_000), "clock time");
    }

    #[test]
    fn translations_in_the_head_land_on_their_line() {
        let doc = parse(SYLLABLES).unwrap();
        let layer = &doc.layers[0];
        assert_eq!((layer.kind, layer.language.as_str()), (LayerKind::Translation, "fr"));
        assert_eq!(layer.lines[0].line, 0);
        assert_eq!(layer.lines[0].text, "Bonjour le monde");
        assert_eq!(doc.credits[0].name, "Some Writer");
    }

    #[test]
    fn line_timing_is_read_without_spans() {
        let doc = parse(
            r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div>
                <p begin="1.0" end="3.0">First line</p>
                <p begin="4.0" end="6.0">Second <br/>line</p>
            </div></body></tt>"#,
        )
        .unwrap();
        assert_eq!(doc.timing, Timing::Line);
        assert_eq!(doc.lines[1].text, "Second line");
    }

    #[test]
    fn a_file_without_times_is_unsynced_not_empty() {
        let doc = parse(r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div><p>Only</p><p>words</p></div></body></tt>"#).unwrap();
        assert_eq!(doc.timing, Timing::Unsynced);
        assert_eq!(doc.plain(), "Only\nwords");
    }

    #[test]
    fn inline_translations_and_romanizations_become_layers() {
        let doc = parse(
            r#"<tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttm="http://www.w3.org/ns/ttml#metadata"><body><div>
                <p begin="1.0" end="2.0"><span begin="1.0" end="1.5">君</span><span begin="1.5" end="2.0">の</span><span ttm:role="x-translation" xml:lang="en">of you</span><span ttm:role="x-roman" xml:lang="ja-Latn">kimi no</span></p>
            </div></body></tt>"#,
        )
        .unwrap();
        assert_eq!(doc.lines[0].text, "君の", "the translation is not part of the line");
        assert_eq!(doc.timing, Timing::Word);
        let kinds: Vec<(LayerKind, &str, &str)> =
            doc.layers.iter().map(|l| (l.kind, l.language.as_str(), l.lines[0].text.as_str())).collect();
        assert_eq!(kinds, vec![(LayerKind::Translation, "en", "of you"), (LayerKind::Romanization, "ja-Latn", "kimi no")]);
    }

    #[test]
    fn what_is_not_ttml_is_refused() {
        assert!(parse("[00:01.00]LRC").is_none());
        assert!(parse("<html><body/></html>").is_none());
    }

    #[test]
    fn time_expressions() {
        assert_eq!(time("12.5"), Some(12_500));
        assert_eq!(time("1:02.5"), Some(62_500));
        assert_eq!(time("01:00:00.000"), Some(3_600_000));
        assert_eq!(time("250ms"), Some(250));
        assert_eq!(time("-1"), None);
        assert_eq!(time("abc"), None);
    }
}
