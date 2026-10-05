//! The Tracks screen: every track of the library in one list, found by
//! what is typed, in an order, and grouped or not.

use crate::{Filter, Reader, Snapshot, TrackOrder, TrackRow};

/// What the screen asks for, as it travels in one string: the order, the
/// grouping and what was typed, a unit separator apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TracksQuery {
    pub order: String,
    /// "off", "album", "artist" or "name".
    pub group: String,
    pub text: String,
}

impl TracksQuery {
    pub fn parse(key: &str) -> Self {
        let mut parts = key.splitn(3, '\u{1f}');
        let mut next = || parts.next().unwrap_or_default().to_string();
        Self { order: next(), group: next(), text: next() }
    }

    /// The order inside each group, or of the whole list when it has none:
    /// the one asked for, and the order of the records when none was.
    pub fn order(&self) -> TrackOrder {
        TrackOrder::from_name(if self.order.is_empty() { "album" } else { &self.order })
    }

    pub fn grouped(&self) -> bool {
        matches!(self.group.as_str(), "album" | "artist" | "name")
    }
}

/// What a query leaves: the tracks, and where each letter starts among
/// them as `[["A", 12], ["B", 40]]` when they are grouped.
#[derive(Clone, Debug, Default)]
pub struct FoundTracks {
    pub snapshot: Snapshot,
    pub letters: String,
    pub group: String,
}

/// The group a row belongs to, which is what its header says.
pub fn group_of(group: &str, row: &TrackRow) -> String {
    match group {
        "album" => row.album.clone(),
        "artist" => row.artist.clone(),
        "name" => chilpa_library::letter_of(&row.title),
        _ => String::new(),
    }
}

impl Reader {
    /// The tracks of a query. The sources, qualities and formats left out
    /// come from `extras`, which the Library screen shares.
    pub fn tracks(&self, query: &str, extras: &Filter) -> FoundTracks {
        let query = TracksQuery::parse(query);
        let filter = Filter {
            text: query.text.clone(),
            excluded_sources: extras.excluded_sources.clone(),
            qualities: extras.qualities.clone(),
            formats: extras.formats.clone(),
            grouped_by: if query.grouped() { query.group.clone() } else { String::new() },
            ..Filter::default()
        };
        let snapshot = self.snapshot(&filter, query.order());
        if let Some(found) = self.with_streaming_tracks(&query, extras, &snapshot) { return found; }
        let letters = if query.grouped() {
            let by = if query.group == "name" { "title" } else { query.group.as_str() };
            let found = self.letters(&snapshot.ids, by);
            serde_json::to_string(&found).unwrap_or_default()
        } else {
            String::new()
        };
        FoundTracks { snapshot, letters, group: query.group }
    }
}

impl Reader {
    fn with_streaming_tracks(&self, query: &TracksQuery, filter: &Filter, snapshot: &Snapshot) -> Option<FoundTracks> {
        let catalog = self.catalog.as_ref()?;
        let natives = catalog.natives()?;
        let root = crate::Chilpa::plugins_dir();
        let words: Vec<_> = query.text.to_lowercase().split_whitespace().map(str::to_owned).collect();
        let mut remote = Vec::new();
        let mut dates = std::collections::HashMap::new();
        for plugin in natives.ids() {
            if !crate::source_integration::read(&root, &plugin).integrate_tracks || filter.excluded_sources.contains(&format!("plugin:{plugin}")) { continue; }
            for (track, art) in catalog.shelf_tracks(&format!("{plugin} section:tracks")) {
                let row = crate::source_catalog::view::row_of(&track, art);
                let text = format!("{} {} {}", row.title, row.artist, row.album).to_lowercase();
                let tier = match row.quality.tier { crate::quality::Tier::Lossy => "lossy", crate::quality::Tier::Lossless => "lossless", crate::quality::Tier::HiRes => "hires", crate::quality::Tier::Dsd => "dsd", _ => "unknown" };
                if !words.iter().all(|w| text.contains(w)) || (!filter.qualities.is_empty() && !filter.qualities.iter().any(|q| q == tier)) || (!filter.formats.is_empty() && !filter.formats.iter().any(|f| f.eq_ignore_ascii_case(&row.quality.format))) { continue; }
                dates.insert(row.id, (track.metadata["year"].as_i64().unwrap_or(0) as i32, track.metadata["added_at"].as_i64().unwrap_or(0)));
                remote.push(row);
            }
        }
        if remote.is_empty() { return None; }
        dates.extend(self.lib.track_dates(&snapshot.ids).unwrap_or_default());
        let mut rows: Vec<_> = snapshot.ids.chunks(500).flat_map(|ids| self.lib.tracks_by_ids(ids).unwrap_or_default()).map(crate::browse::row_of).collect();
        rows.extend(remote);
        Some(arrange_tracks(rows, query, &dates))
    }
}

fn arrange_tracks(mut rows: Vec<TrackRow>, query: &TracksQuery, dates: &std::collections::HashMap<i64,(i32,i64)>) -> FoundTracks {
    use std::cmp::Ordering;
    rows.sort_by(|a,b| {
        let text = |a: &str,b: &str| a.trim().to_lowercase().cmp(&b.trim().to_lowercase());
        let grouped = if query.grouped() { text(&group_of(&query.group,a), &group_of(&query.group,b)) } else { Ordering::Equal };
        let date = |id: i64| dates.get(&id).copied().unwrap_or_default();
        let order = match query.order() {
            TrackOrder::Title => text(&a.title,&b.title), TrackOrder::TitleDescending => text(&b.title,&a.title),
            TrackOrder::Artist | TrackOrder::Performer => text(&a.artist,&b.artist), TrackOrder::ArtistDescending => text(&b.artist,&a.artist),
            TrackOrder::Year => date(a.id).0.cmp(&date(b.id).0), TrackOrder::YearDescending => date(b.id).0.cmp(&date(a.id).0),
            TrackOrder::Added => date(b.id).1.cmp(&date(a.id).1),
            TrackOrder::Duration => a.duration_ms.cmp(&b.duration_ms), TrackOrder::DurationDescending => b.duration_ms.cmp(&a.duration_ms),
            TrackOrder::Plays => b.plays.cmp(&a.plays), _ => Ordering::Equal,
        };
        grouped.then(order).then_with(|| text(&a.album,&b.album)).then(a.disc.cmp(&b.disc)).then(a.number.cmp(&b.number)).then_with(|| text(&a.title,&b.title)).then(a.id.cmp(&b.id))
    });
    let mut seen = std::collections::HashSet::new();
    rows.retain(|r| seen.insert(r.id));
    let mut letters = Vec::new();
    let mut previous = String::new();
    if query.grouped() {
        for (index,row) in rows.iter().enumerate() {
            let label = chilpa_library::letter_of(&group_of(&query.group,row));
            if index == 0 || label != previous { letters.push((label.clone(),index)); previous = label; }
        }
    }
    let mut album_ids = Vec::new(); let mut albums = std::collections::HashSet::new();
    for row in &rows { if row.album_id != 0 && albums.insert(row.album_id) { album_ids.push(row.album_id); } }
    let snapshot = Snapshot { ids: rows.iter().map(|r| r.id).collect(), totals: crate::Totals { tracks:rows.len(),albums:album_ids.len(),duration_ms:rows.iter().map(|r| r.duration_ms).sum() }, album_ids };
    FoundTracks { snapshot, letters:serde_json::to_string(&letters).unwrap_or_default(), group:query.group.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, title: &str, artist: &str, album: i64) -> TrackRow {
        let mut row = crate::browse::file_row(title);
        row.id = id; row.title = title.into(); row.artist = artist.into();
        row.album_id = album; row.duration_ms = 1000;
        row
    }

    #[test]
    fn mixed_tracks_share_sort_groups_and_totals() {
        let rows = vec![row(-2,"Second","B",-10),row(1,"First","A",10),row(-3,"Third","A",-10)];
        let query = TracksQuery::parse("title-desc\u{1f}artist\u{1f}");
        let found = arrange_tracks(rows,&query,&Default::default());
        assert_eq!(found.snapshot.ids,vec![-3,1,-2]);
        assert_eq!(found.letters,r#"[["A",0],["B",2]]"#);
        assert_eq!(found.snapshot.totals.duration_ms,3000);
        assert_eq!(found.snapshot.totals.albums,2);
    }

    #[test]
    fn mixed_tracks_use_dates_instead_of_source_order() {
        let rows = vec![row(-2,"Remote","B",-10),row(1,"Local","A",10)];
        let dates = [(1,(2024,30)),(-2,(2025,20))].into_iter().collect();
        let found = arrange_tracks(rows.clone(),&TracksQuery::parse("added\u{1f}off\u{1f}"),&dates);
        assert_eq!(found.snapshot.ids,vec![1,-2]);
        let found = arrange_tracks(rows,&TracksQuery::parse("year-desc\u{1f}off\u{1f}"),&dates);
        assert_eq!(found.snapshot.ids,vec![-2,1]);
    }

    #[test]
    fn the_order_asked_for_stands_whatever_the_grouping() {
        let asked = |key: &str| TracksQuery::parse(key).order();
        assert_eq!(asked("title-desc\u{1f}off\u{1f}"), TrackOrder::TitleDescending);
        assert_eq!(asked("title-desc\u{1f}artist\u{1f}"), TrackOrder::TitleDescending, "a grouping orders the groups, not what is in them");
        assert_eq!(asked(""), TrackOrder::Album, "nothing asked is the order of the records");
        assert_eq!(TracksQuery::parse("added\u{1f}album\u{1f}two words").text, "two words");
    }
}

impl crate::Chilpa {
    /// A track's origin, resolved from memory even when its source is disconnected.
    pub fn track_source_name(&self, path: &str) -> String {
        if let Some((id, _)) = crate::source_opener::of_plugin(path) {
            let name = self.source_name(&id);
            return if name.is_empty() { id } else { name };
        }
        chilpa_i18n::t(if crate::on_a_server(path) { "Server" } else { "Local" })
    }
}
