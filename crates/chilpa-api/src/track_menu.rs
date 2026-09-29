//! Track actions and their context rules, shared by thin frontends.
use chilpa_i18n as i18n;
use serde_json::{json, Value};

#[derive(Default)]
pub struct TrackMenuContext {
    pub count: usize,
    pub queued: bool,
    pub editable: bool,
    pub move_up: bool,
    pub move_down: bool,
    pub album: bool,
    pub artist: bool,
    pub folder: bool,
    pub metadata: bool,
    /// None of the chosen tracks can be played now (ADR-017).
    pub unavailable: bool,
}

pub fn track_menu(context: TrackMenuContext) -> Vec<Value> {
    if context.count == 0 { return Vec::new(); }
    // What cannot be played now offers nothing but leaving a list of mine,
    // and outside one, nothing at all (ADR-017).
    if context.unavailable {
        return if context.editable {
            vec![json!({"act": "remove", "text": i18n::t(i18n::mark("Remove from playlist"))})]
        } else {
            Vec::new()
        };
    }
    let mut entries = Vec::new();
    if context.count > 1 {
        entries.push(json!({"text": i18n::tf("{} track", "{} tracks", context.count as i64, &[&context.count.to_string()]), "note": true}));
    }
    let item = |act, label| json!({"act": act, "text": i18n::t(label)});
    entries.push(item("play", i18n::mark("Play now")));
    if context.queued {
        entries.push(item("dequeue", i18n::mark("Remove from queue")));
    } else {
        entries.push(item("queue", i18n::mark("Add to queue")));
        entries.push(item("next", i18n::mark("Play next")));
        entries.push(item("later", i18n::mark("Play later")));
    }
    entries.push(json!({"separator": true}));
    entries.push(item("pick", i18n::mark("Add to playlist…")));
    let mut navigation = Vec::new();
    if context.count == 1 {
        for (enabled, act, label) in [
            (context.album, "album", i18n::mark("Go to album")),
            (context.artist, "artist", i18n::mark("Go to artist")),
            (context.folder, "folder", i18n::mark("Show in folder")),
            (context.metadata, "metadata", i18n::mark("Edit metadata…")),
        ] {
            if enabled { navigation.push(item(act, label)); }
        }
    }
    if !navigation.is_empty() {
        entries.push(json!({"separator": true}));
        entries.extend(navigation);
    }
    if context.editable {
        entries.push(json!({"separator": true}));
        if context.count == 1 {
            if context.move_up { entries.push(json!({"act": "up", "text": i18n::t("Move up"), "hint": "Alt ↑"})); }
            if context.move_down { entries.push(json!({"act": "down", "text": i18n::t("Move down"), "hint": "Alt ↓"})); }
        }
        entries.push(item("remove", i18n::mark("Remove from playlist")));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_track_and_selection_offers_all_three_queue_placements() {
        for count in [1, 4] {
            let menu = track_menu(TrackMenuContext { count, album: true, artist: true, editable: true, ..Default::default() });
            let actions: Vec<_> = menu.iter().filter_map(|entry| entry["act"].as_str()).collect();
            assert_eq!(&actions[..5], &["play", "queue", "next", "later", "pick"]);
            assert!(actions.contains(&"remove"));
            assert_eq!(actions.contains(&"album"), count == 1);
            assert_eq!(actions.contains(&"artist"), count == 1);
            assert!(!menu.last().unwrap()["separator"].as_bool().unwrap_or(false));
        }
    }

    #[test]
    fn empty_targets_and_read_only_playlists_offer_no_mutation() {
        assert!(track_menu(TrackMenuContext::default()).is_empty());
        let menu = track_menu(TrackMenuContext { count: 1, queued: true, ..Default::default() });
        let actions: Vec<_> = menu.iter().filter_map(|entry| entry["act"].as_str()).collect();
        assert_eq!(actions, ["play", "dequeue", "pick"]);
    }
}

#[cfg(test)]
mod unavailable_tests {
    use super::*;

    #[test]
    fn an_unplayable_track_can_only_leave_a_playlist_of_mine() {
        let in_playlist = track_menu(TrackMenuContext { count: 1, editable: true, album: true, unavailable: true, ..Default::default() });
        assert_eq!(in_playlist.len(), 1);
        assert_eq!(in_playlist[0]["act"], "remove");
        assert!(track_menu(TrackMenuContext { count: 1, album: true, unavailable: true, ..Default::default() }).is_empty());
        assert!(track_menu(TrackMenuContext { count: 1, unavailable: false, ..Default::default() }).len() > 1);
    }
}
