//! Smart lists: rules over the library, not a list made by hand.
//!
//! A smart list does not store tracks, it stores conditions. It is resolved
//! again every time it is looked at, so a new track that matches joins on
//! its own and one that stopped matching leaves on its own.
//!
//! Today's rules come from what the library already knows: when a track
//! arrived, how often it played, at what format and how long it runs. Mood
//! and rhythm will be further rules over `audio_features`, which is already
//! in the schema and versioned per analyzer; that is why this is built as a
//! list of rules and not as six hand-written queries.
//!
//! Everything that comes from the user travels as a parameter. The only
//! thing interpolated into the SQL are the templates in this file.

use crate::store::{Library, StoredTrack};
use rusqlite::types::Value;

/// A condition a track has to meet.
#[derive(Clone, PartialEq, Debug)]
pub enum Rule {
    /// It has never played all the way through.
    NeverPlayed,
    /// It has played all the way through at least that many times.
    PlayedAtLeast(usize),
    /// It arrived in the library within that many days.
    AddedWithinDays(u32),
    /// It played within that many days.
    PlayedWithinDays(u32),
    /// More than 16 bits or more than 48 kHz.
    HiRes,
    /// It runs longer than this.
    LongerThanMs(u64),
    /// It runs shorter than this.
    ShorterThanMs(u64),
    /// By that artist, ignoring case.
    ArtistIs(String),
    /// In that genre.
    GenreIs(String),
    /// With an analyzed tempo inside that range.
    BpmBetween(f32, f32),
}

/// How what comes out is ordered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Order {
    /// Artist and title, like a shelf.
    #[default]
    Artist,
    /// The latest arrivals first.
    NewestFirst,
    /// The most played first.
    MostPlayed,
    /// At random, different every time it is looked at.
    Random,
}

/// A smart list: a name and its conditions.
#[derive(Clone, PartialEq, Debug)]
pub struct SmartList {
    pub name: String,
    /// All of them have to hold. An "or" is built with two lists.
    pub rules: Vec<Rule>,
    pub order: Order,
    pub limit: usize,
}

impl SmartList {
    pub fn new(name: &str, rules: Vec<Rule>, order: Order, limit: usize) -> Self {
        Self { name: name.to_string(), rules, order, limit }
    }
}

/// The lists Chilpa ships with.
///
/// These are the ones that can be worked out without audio analysis. Mood
/// and rhythm join them as soon as there is `audio_features`.
pub fn builtins() -> Vec<SmartList> {
    vec![
        SmartList::new(chilpa_i18n::mark("Never played"), vec![Rule::NeverPlayed], Order::Random, 200),
        SmartList::new(
            chilpa_i18n::mark("Added this month"),
            vec![Rule::AddedWithinDays(30)],
            Order::NewestFirst,
            200,
        ),
        SmartList::new(
            chilpa_i18n::mark("What you play most"),
            vec![Rule::PlayedAtLeast(2)],
            Order::MostPlayed,
            100,
        ),
        SmartList::new(chilpa_i18n::mark("High resolution"), vec![Rule::HiRes], Order::Artist, 500),
        SmartList::new(
            chilpa_i18n::mark("From this week"),
            vec![Rule::PlayedWithinDays(7)],
            Order::MostPlayed,
            100,
        ),
        SmartList::new(
            chilpa_i18n::mark("For a long while"),
            vec![Rule::LongerThanMs(8 * 60 * 1000)],
            Order::Artist,
            200,
        ),
        // These two need the analysis; without it they come out empty, which
        // is the honest answer: there is nothing to fill them with yet.
        SmartList::new(chilpa_i18n::mark("Calm"), vec![Rule::BpmBetween(50.0, 95.0)], Order::Random, 200),
        SmartList::new(
            chilpa_i18n::mark("To move to"),
            vec![Rule::BpmBetween(118.0, 145.0)],
            Order::Random,
            200,
        ),
    ]
}

/// True if the rule can only match a track that has been played.
///
/// It decides how the rollup is joined, and that decides whether a list
/// that matches nothing costs a look or a walk. With a `LEFT JOIN` the
/// plays are a column of every track, so `plays >= 2` has to be asked of
/// two hundred thousand rows. With an inner join the rollup can go first,
/// and `plays >= 2` over an index that has no such row answers in the time
/// it takes to look.
///
/// Measured at two hundred thousand tracks, none of them played twice:
/// 120 ms to find that out, against microseconds.
fn needs_a_play(rule: &Rule) -> bool {
    matches!(rule, Rule::PlayedAtLeast(_) | Rule::PlayedWithinDays(_))
}

/// What a rule needs joined in, if anything.
///
/// A rule that filters on another table has two shapes to choose from, and
/// the choice decides which table the query is driven from.
///
/// `EXISTS (SELECT 1 FROM other WHERE other.track_id = t.id AND ...)` is
/// correlated: it can only be answered one way, by walking `tracks` and
/// looking each one up. So a tempo rule walked two hundred thousand tracks
/// to find the few thousand with that tempo -- or, in a library whose
/// analysis has not run yet, to find none at all.
///
/// A join can be answered from either side and the planner picks the cheaper
/// one, which is the side with the index that narrows: `audio_features(bpm)`,
/// `track_genres(genre_id)`. Measured on two hundred thousand tracks,
/// counting "Calm": 35 ms to 2 ms analyzed, 18 ms to 0.07 ms unanalyzed.
///
/// A rule may only do this if its join cannot give one track two rows, or
/// the track would be counted twice and fill the list's limit with itself.
/// Both of these are keyed so that it cannot: `audio_features` since
/// `MIGRATION_V9`, `track_genres` by its primary key.
///
/// The alias carries the rule's position so that two rules over the same
/// table do not collide.
fn join(rule: &Rule, i: usize) -> Option<String> {
    match rule {
        Rule::BpmBetween(..) => {
            Some(format!("JOIN audio_features f{i} ON f{i}.track_id = t.id"))
        }
        Rule::GenreIs(_) => Some(format!(
            "JOIN track_genres tg{i} ON tg{i}.track_id = t.id
             JOIN genres g{i} ON g{i}.id = tg{i}.genre_id"
        )),
        _ => None,
    }
}

/// A rule's piece of the `WHERE`, with its parameters.
fn condition(rule: &Rule, i: usize, now: i64) -> (String, Vec<Value>) {
    let days_ago = |d: u32| Value::Integer(now - i64::from(d) * 86_400);
    match rule {
        // Never played is the other side of the same coin, and it is the
        // one case that wants the outer join: what it looks for is the
        // absence of a row.
        Rule::NeverPlayed => ("COALESCE(e.plays, 0) = 0".into(), vec![]),
        // No COALESCE: the join is inner when this rule is present, so the
        // column is there, and a bare comparison is what an index can serve.
        Rule::PlayedAtLeast(n) => ("e.plays >= ?".into(), vec![Value::Integer(*n as i64)]),
        Rule::AddedWithinDays(d) => ("t.added_at >= ?".into(), vec![days_ago(*d)]),
        Rule::PlayedWithinDays(d) => ("e.last_played >= ?".into(), vec![days_ago(*d)]),
        // High resolution is anything above CD.
        Rule::HiRes => (
            "(t.bit_depth > 16 OR t.sample_rate_hz > 48000)".into(),
            vec![],
        ),
        Rule::LongerThanMs(ms) => (
            "t.duration_ms > ?".into(),
            vec![Value::Integer(*ms as i64)],
        ),
        Rule::ShorterThanMs(ms) => (
            // A duration of zero means it is unknown; it does not count as
            // "short".
            "(t.duration_ms > 0 AND t.duration_ms < ?)".into(),
            vec![Value::Integer(*ms as i64)],
        ),
        Rule::ArtistIs(name) => (
            "(t.artist = ? COLLATE NOCASE OR EXISTS (SELECT 1 FROM credit_parts p WHERE p.credit = trim(t.artist) AND p.name = ?))".into(),
            vec![Value::Text(name.clone()), Value::Text(name.clone())],
        ),
        // Whichever analyzer ran last: the list says what matches today.
        // See `join` for why this is a join and not an `EXISTS`.
        Rule::BpmBetween(min, max) => (
            format!("f{i}.bpm BETWEEN ? AND ?"),
            vec![Value::Real(f64::from(*min)), Value::Real(f64::from(*max))],
        ),
        Rule::GenreIs(name) => (
            format!("g{i}.name = ?"),
            vec![Value::Text(name.clone())],
        ),
    }
}

/// Everything a rule list needs joined in, the rollup included.
///
/// The rollup is joined inner when some rule requires a play, because then
/// a track with no row cannot match and the planner is free to drive from
/// `track_rollup`'s index instead of from `tracks`. With no such rule it has
/// to be outer: `NeverPlayed` is looking for the absence of a row.
fn joins(rules: &[Rule]) -> String {
    let rollup = if rules.iter().any(needs_a_play) { "JOIN" } else { "LEFT JOIN" };
    let mut sql = format!("{rollup} track_rollup e ON e.track_id = t.id");
    for (i, rule) in rules.iter().enumerate() {
        if let Some(extra) = join(rule, i) {
            sql.push_str("\n             ");
            sql.push_str(&extra);
        }
    }
    sql
}

fn sql_order(order: Order) -> &'static str {
    match order {
        Order::Artist => "t.artist COLLATE NOCASE, t.title COLLATE NOCASE",
        Order::NewestFirst => "t.added_at DESC, t.title COLLATE NOCASE",
        Order::MostPlayed => "COALESCE(e.plays, 0) DESC, t.title COLLATE NOCASE",
        Order::Random => "RANDOM()",
    }
}

impl Library {
    /// What a rule list comes down to: the `WHERE` and its parameters.
    ///
    /// Shared so that counting a list and resolving it can never drift into
    /// two different answers to the same question.
    fn smart_where(&self, list: &SmartList) -> (String, Vec<Value>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Present tracks only: a list offering what cannot be played is no
        // use at all.
        let mut conditions = vec!["t.present = 1".to_string()];
        let mut values: Vec<Value> = Vec::new();
        for (i, rule) in list.rules.iter().enumerate() {
            let (sql, params) = condition(rule, i, now);
            conditions.push(sql);
            values.extend(params);
        }
        (conditions.join(" AND "), values)
    }

    /// How many tracks a list would give, without building it.
    ///
    /// The panel shows eight of these side by side, and it wants a number,
    /// not the tracks. Counting inside a `LIMIT` is what makes the
    /// difference: with the ordering gone there is nothing to sort, and a
    /// list with plenty of matches stops as soon as it has enough instead of
    /// walking the library to sort what it was going to throw away.
    ///
    /// It is capped at the list's limit for the same reason the list is:
    /// the number is what you would get, not how many exist.
    pub fn smart_count(&self, list: &SmartList) -> rusqlite::Result<usize> {
        let (where_sql, mut values) = self.smart_where(list);
        values.push(Value::Integer(list.limit.max(1) as i64));
        let joins = joins(&list.rules);
        let sql = format!(
            "SELECT count(*) FROM (
                 SELECT 1
                 FROM tracks t
                 {joins}
                 WHERE {where_sql}
                 LIMIT ?
             )"
        );
        let n: i64 = self.db.query_row(&sql, rusqlite::params_from_iter(values), |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Resolves a smart list against the library.
    ///
    /// Present tracks only: a list offering what cannot be played is no use at
    /// all.
    pub fn smart_tracks(&self, list: &SmartList) -> rusqlite::Result<Vec<StoredTrack>> {
        let (where_sql, mut values) = self.smart_where(list);
        values.push(Value::Integer(list.limit.max(1) as i64));

        let sql = format!(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1)
             FROM tracks t
             LEFT JOIN albums a ON a.id = t.album_id
             {}
             WHERE {}
             ORDER BY {}
             LIMIT ?",
            joins(&list.rules),
            where_sql,
            sql_order(list.order)
        );

        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), crate::store::row_to_track)?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Four tracks with different histories.
    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // (id, title, artist, duration, bits, rate, added_at)
        let rows = [
            (1, "Old", "One", 200_000i64, 16, 44_100, now - 400 * 86_400),
            (2, "New", "Two", 200_000, 24, 96_000, now - 2 * 86_400),
            (3, "Long", "One", 10 * 60_000, 16, 44_100, now - 2 * 86_400),
            (4, "Missing", "Three", 100_000, 16, 44_100, now - 2 * 86_400),
        ];
        for (id, title, artist, dur, bits, rate, high) in rows {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, duration_ms,
                                         bit_depth, sample_rate_hz, added_at, present)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        source,
                        format!("/{title}.flac"),
                        title,
                        artist,
                        dur,
                        bits,
                        rate,
                        high,
                        i64::from(id != 4)
                    ],
                )
                .unwrap();
        }
        // The old one played three times, long ago; the new one twice, yesterday.
        for i in 0..3 {
            lib.log_play(Some(1), Some(source), now - 300 * 86_400 + i, 200_000, 200_000, false, None, None, true)
                .unwrap();
        }
        for i in 0..2 {
            lib.log_play(Some(2), Some(source), now - 86_400 + i, 200_000, 200_000, false, None, None, true)
                .unwrap();
        }
        lib
    }

    fn titles(lib: &Library, list: &SmartList) -> Vec<String> {
        lib.smart_tracks(list).unwrap().into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn never_played_means_the_ones_that_never_played() {
        let lib = library();
        let mut come_out = titles(
            &lib,
            &SmartList::new("x", vec![Rule::NeverPlayed], Order::Artist, 50),
        );
        come_out.sort();
        assert_eq!(come_out, ["Long"], "the missing one does not count even though it never played");
    }

    #[test]
    fn recently_added_leaves_the_old_ones_out() {
        let lib = library();
        let mut come_out = titles(
            &lib,
            &SmartList::new("x", vec![Rule::AddedWithinDays(30)], Order::NewestFirst, 50),
        );
        come_out.sort();
        assert_eq!(come_out, ["Long", "New"]);
    }

    #[test]
    fn this_week_looks_at_when_it_played_not_when_it_arrived() {
        let lib = library();
        assert_eq!(
            titles(&lib, &SmartList::new("x", vec![Rule::PlayedWithinDays(7)], Order::MostPlayed, 50)),
            ["New"],
            "the old one played three hundred days ago"
        );
    }

    #[test]
    fn high_resolution_is_whatever_goes_beyond_cd() {
        let lib = library();
        assert_eq!(
            titles(&lib, &SmartList::new("x", vec![Rule::HiRes], Order::Artist, 50)),
            ["New"]
        );
    }

    #[test]
    fn every_rule_adds_up() {
        let lib = library();
        let list = SmartList::new(
            "x",
            vec![Rule::AddedWithinDays(30), Rule::LongerThanMs(8 * 60_000)],
            Order::Artist,
            50,
        );
        assert_eq!(titles(&lib, &list), ["Long"], "new and long at the same time");
    }

    #[test]
    fn the_limit_is_honored() {
        let lib = library();
        let list = SmartList::new("x", vec![], Order::Artist, 2);
        assert_eq!(lib.smart_tracks(&list).unwrap().len(), 2);
    }

    #[test]
    fn bpm_filters_on_what_the_analysis_found() {
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        lib.save_features(2, "chilpa", 1, Some(72.0), None, None, None).unwrap();
        // Number 3 was analyzed and no beat was found.
        lib.save_features(3, "chilpa", 1, None, None, None, None).unwrap();

        let fast = SmartList::new("x", vec![Rule::BpmBetween(118.0, 145.0)], Order::Artist, 50);
        assert_eq!(titles(&lib, &fast), ["Old"]);

        let slow = SmartList::new("x", vec![Rule::BpmBetween(50.0, 95.0)], Order::Artist, 50);
        assert_eq!(titles(&lib, &slow), ["New"]);

        // And without analysis the list comes out empty instead of bringing everything.
        let clean = Library::in_memory().unwrap();
        assert!(clean.smart_tracks(&fast).unwrap().is_empty());
    }

    #[test]
    fn a_better_analyzer_replaces_what_the_old_one_said() {
        // What the table's old key allowed: the first analyzer heard 128,
        // the second heard 72, and both rows stayed. Asking "any row in the
        // band" then put the same track in two lists that are opposites, and
        // no amount of re-analyzing could take it out of the wrong one.
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        lib.save_features(1, "chilpa", 2, Some(72.0), None, None, None).unwrap();

        let fast = SmartList::new("x", vec![Rule::BpmBetween(118.0, 145.0)], Order::Artist, 50);
        let slow = SmartList::new("x", vec![Rule::BpmBetween(50.0, 95.0)], Order::Artist, 50);
        assert!(titles(&lib, &fast).is_empty(), "the old measurement is gone");
        assert_eq!(titles(&lib, &slow), ["Old"], "and the new one is what counts");
        assert_eq!(lib.track_bpm(1, "chilpa").unwrap(), Some(72.0));

        // And a track cannot fill a list with copies of itself.
        assert_eq!(lib.smart_count(&slow).unwrap(), 1);
    }

    #[test]
    fn two_analyzers_do_not_get_in_each_others_way() {
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        lib.save_features(1, "other", 1, Some(72.0), None, None, None).unwrap();
        assert_eq!(lib.track_bpm(1, "chilpa").unwrap(), Some(128.0));
        assert_eq!(lib.track_bpm(1, "other").unwrap(), Some(72.0));
    }

    #[test]
    fn a_genre_list_brings_each_track_once() {
        let lib = library();
        // Two genres on the same track: the rule asks for one of them.
        lib.db.execute("INSERT INTO genres (id, name) VALUES (1, 'Rock'), (2, 'Jazz')", []).unwrap();
        lib.db
            .execute("INSERT INTO track_genres (track_id, genre_id) VALUES (1, 1), (1, 2)", [])
            .unwrap();
        let rock = SmartList::new("x", vec![Rule::GenreIs("Rock".into())], Order::Artist, 50);
        assert_eq!(titles(&lib, &rock), ["Old"]);
        assert_eq!(lib.smart_count(&rock).unwrap(), 1);
    }

    #[test]
    fn analyzing_again_with_the_same_version_replaces_its_row() {
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(100.0), None, None, None).unwrap();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        assert_eq!(lib.track_bpm(1, "chilpa").unwrap(), Some(128.0));
        assert_eq!(lib.analyzed_count("chilpa", 1).unwrap(), 1, "one row, not two");
    }

    #[test]
    fn pending_means_whatever_lacks_that_version() {
        let lib = library();
        assert_eq!(lib.tracks_pending_analysis("chilpa", 1, 50).unwrap().len(), 3, "without the missing one");
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        assert_eq!(lib.tracks_pending_analysis("chilpa", 1, 50).unwrap().len(), 2);
        // A new analyzer asks for all of them again.
        assert_eq!(lib.tracks_pending_analysis("chilpa", 2, 50).unwrap().len(), 3);
    }

    #[test]
    fn an_odd_artist_name_is_just_a_name() {
        let lib = library();
        let odd = "One'; DROP TABLE tracks; --";
        let list = SmartList::new("x", vec![Rule::ArtistIs(odd.into())], Order::Artist, 50);
        assert!(lib.smart_tracks(&list).unwrap().is_empty());
        assert_eq!(lib.track_count(true).unwrap(), 4, "the table is still there");
    }

    #[test]
    fn the_built_in_ones_all_run() {
        let lib = library();
        for list in builtins() {
            lib.smart_tracks(&list)
                .unwrap_or_else(|e| panic!("the list \"{}\" did not run: {e}", list.name));
        }
    }

    #[test]
    fn counting_a_list_and_building_it_answer_the_same() {
        // They are two queries over one set of rules, and the panel shows
        // the first while the button hands you the second. A list that says
        // fourteen and gives nine is worse than a slow one.
        let lib = library();
        for list in builtins() {
            let built = lib.smart_tracks(&list).unwrap().len();
            let counted = lib.smart_count(&list).unwrap();
            assert_eq!(built, counted, "the list \"{}\"", list.name);
        }
        let compound = SmartList::new(
            "compound",
            vec![Rule::HiRes, Rule::LongerThanMs(1)],
            Order::Artist,
            200,
        );
        assert_eq!(lib.smart_tracks(&compound).unwrap().len(), lib.smart_count(&compound).unwrap());
    }
}
