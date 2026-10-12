//! Genre and tag normalization for MusicBrainz scene discovery
//!
//! Separates genres (primary signals) from tags (secondary signals),
//! filters noise, and normalizes equivalent names.

use std::collections::HashSet;

use super::{AffinitySeeds, Tag};

/// Tags that provide no useful genre/scene signal
const NOISY_TAGS: &[&str] = &[
    "favorites",
    "favorite",
    "favourite",
    "favourites",
    "awesome",
    "seen live",
    "cool",
    "good",
    "great",
    "love",
    "loved",
    "amazing",
    "best",
    "american",
    "british",
    "canadian",
    "australian",
    "german",
    "french",
    "japanese",
    "swedish",
    "norwegian",
    "finnish",
    "irish",
    "scottish",
    "korean",
    "mexican",
    "brazilian",
    "colombian",
    "argentinian",
    "chilean",
    "spanish",
    "italian",
    "dutch",
    "russian",
    "chinese",
    "indian",
    "african",
    "female vocalists",
    "male vocalists",
    "female vocalist",
    "male vocalist",
    "singer-songwriter",
    "bands i've seen live",
    "bands i have seen live",
    "check out",
    "under 2000 listeners",
];

/// Overly broad genre tags that return too much noise when used as search queries.
/// These are excluded from the search genres list but kept for affinity scoring.
/// Example: In Mexico everything is "latin", in UK everything is "rock" — searching
/// by these returns the entire country's catalog instead of scene-relevant artists.
const BROAD_TAGS: &[&str] = &[
    "rock",
    "pop",
    "latin",
    "latin music",
    "electronic",
    "electronica",
    "jazz",
    "metal",
    "hip hop",
    "hip-hop",
    "hiphop",
    "rap",
    "classical",
    "classical music",
    "folk",
    "folk music",
    "blues",
    "blues music",
    "country",
    "country music",
    "soul",
    "funk",
    "r&b",
    "rnb",
    "reggae",
    "world",
    "world music",
    "experimental",
    "dance",
    "dance music",
    "soundtrack",
    "instrumental",
];

/// Normalize genre/tag name to canonical form
pub fn normalize_genre(name: &str) -> String {
    let lower = name.to_lowercase().trim().to_string();

    match lower.as_str() {
        // Rock variants
        "alt rock" | "alt-rock" | "alternative" => "alternative rock".to_string(),
        "grunge rock" => "grunge".to_string(),
        "prog" | "prog rock" | "progressive" => "progressive rock".to_string(),
        "prog metal" | "progressive metal" => "progressive metal".to_string(),
        "post punk" => "post-punk".to_string(),
        "post rock" => "post-rock".to_string(),
        "post metal" => "post-metal".to_string(),
        "math rock" => "math rock".to_string(),
        "stoner" | "stoner rock" => "stoner rock".to_string(),
        "psychedelic" | "psych" | "psych rock" => "psychedelic rock".to_string(),
        "shoegaze" | "shoe gaze" => "shoegaze".to_string(),
        "noise rock" | "noise-rock" => "noise rock".to_string(),
        "hard rock" | "hard-rock" => "hard rock".to_string(),
        "indie" => "indie rock".to_string(),
        "punk" => "punk rock".to_string(),

        // Metal variants
        "death metal" | "death-metal" => "death metal".to_string(),
        "black metal" | "black-metal" => "black metal".to_string(),
        "doom" | "doom metal" | "doom-metal" => "doom metal".to_string(),
        "thrash" | "thrash metal" => "thrash metal".to_string(),
        "metalcore" | "metal core" => "metalcore".to_string(),
        "nu metal" | "nu-metal" | "nü-metal" => "nu metal".to_string(),
        "sludge" | "sludge metal" => "sludge metal".to_string(),

        // Electronic variants
        "electronic" | "electronica" => "electronic".to_string(),
        "idm" | "intelligent dance music" => "idm".to_string(),
        "edm" => "electronic dance music".to_string(),
        "dnb" | "drum n bass" | "drum & bass" | "drum'n'bass" => "drum and bass".to_string(),
        "ambient" | "ambient music" => "ambient".to_string(),
        "synth pop" | "synth-pop" | "synthpop" => "synthpop".to_string(),
        "trip hop" | "trip-hop" => "trip-hop".to_string(),
        "downtempo" | "down tempo" => "downtempo".to_string(),
        "techno" | "detroit techno" => "techno".to_string(),
        "house" | "house music" => "house".to_string(),

        // Hip-hop variants
        "hip hop" | "hip-hop" | "hiphop" => "hip hop".to_string(),
        "rap" | "rap music" => "hip hop".to_string(),
        "trap" | "trap music" => "trap".to_string(),

        // Jazz variants
        "jazz" | "contemporary jazz" => "jazz".to_string(),
        "jazz fusion" | "fusion" => "jazz fusion".to_string(),
        "free jazz" | "free-jazz" => "free jazz".to_string(),
        "acid jazz" | "acid-jazz" => "acid jazz".to_string(),

        // R&B / Soul
        "r&b" | "rnb" | "rhythm and blues" => "r&b".to_string(),
        "neo soul" | "neo-soul" => "neo-soul".to_string(),

        // Other
        "folk" | "folk music" => "folk".to_string(),
        "country" | "country music" => "country".to_string(),
        "blues" | "blues music" => "blues".to_string(),
        "classical" | "classical music" => "classical".to_string(),
        "world" | "world music" => "world music".to_string(),
        "reggae" | "reggae music" => "reggae".to_string(),
        "ska" | "ska music" => "ska".to_string(),
        "latin" | "latin music" => "latin".to_string(),

        // Default: return as-is (lowercased)
        _ => lower,
    }
}

/// Check if a tag is noisy (provides no genre/scene signal)
fn is_noisy_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    NOISY_TAGS.iter().any(|noisy| lower == *noisy)
}

/// Check if a normalized genre is too broad to use as a search query.
/// These tags are kept for affinity scoring but excluded from MB search queries.
pub fn is_broad_genre(normalized: &str) -> bool {
    BROAD_TAGS.iter().any(|broad| normalized == *broad)
}

/// Minimum vote count to consider a tag as a primary genre
const GENRE_MIN_VOTES: i32 = 1;

/// Maximum number of primary genres to extract
const MAX_GENRES: usize = 5;

/// Maximum number of secondary tags to keep
const MAX_TAGS: usize = 10;

/// Extract affinity seeds from MusicBrainz tags.
///
/// Genres (primary signal): top-voted tags with enough votes, normalized.
/// Tags (secondary signal): remaining useful tags after noise filtering.
pub fn extract_affinity_seeds(tags: &[Tag]) -> AffinitySeeds {
    if tags.is_empty() {
        return AffinitySeeds {
            genres: Vec::new(),
            tags: Vec::new(),
            normalized_seeds: Vec::new(),
        };
    }

    // Sort by vote count descending
    let mut sorted_tags: Vec<_> = tags.iter().collect();
    sorted_tags.sort_by(|a, b| b.count.unwrap_or(0).cmp(&a.count.unwrap_or(0)));

    let mut genres = Vec::new();
    let mut secondary_tags = Vec::new();
    let mut seen_normalized = HashSet::new();

    for tag in &sorted_tags {
        let count = tag.count.unwrap_or(0);
        if count < GENRE_MIN_VOTES {
            continue;
        }

        if is_noisy_tag(&tag.name) {
            continue;
        }

        let normalized = normalize_genre(&tag.name);

        if seen_normalized.contains(&normalized) {
            continue;
        }
        seen_normalized.insert(normalized.clone());

        if genres.len() < MAX_GENRES {
            genres.push(normalized);
        } else if secondary_tags.len() < MAX_TAGS {
            secondary_tags.push(normalized);
        }
    }

    let normalized_seeds: Vec<String> = genres
        .iter()
        .chain(secondary_tags.iter())
        .cloned()
        .collect();

    AffinitySeeds {
        genres,
        tags: secondary_tags,
        normalized_seeds,
    }
}

/// Compute the genre summary string for display (e.g., "grunge / alternative rock")
pub fn genre_summary(seeds: &AffinitySeeds) -> String {
    if seeds.genres.is_empty() {
        return String::new();
    }
    seeds
        .genres
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_genre() {
        assert_eq!(normalize_genre("alt rock"), "alternative rock");
        assert_eq!(normalize_genre("Alt Rock"), "alternative rock");
        assert_eq!(normalize_genre("grunge rock"), "grunge");
        assert_eq!(normalize_genre("prog"), "progressive rock");
        assert_eq!(normalize_genre("hip hop"), "hip hop");
        assert_eq!(normalize_genre("hip-hop"), "hip hop");
        assert_eq!(normalize_genre("unknown genre"), "unknown genre");
    }

    #[test]
    fn test_noisy_tags_filtered() {
        let tags = vec![
            Tag {
                name: "rock".to_string(),
                count: Some(10),
            },
            Tag {
                name: "seen live".to_string(),
                count: Some(8),
            },
            Tag {
                name: "awesome".to_string(),
                count: Some(5),
            },
            Tag {
                name: "grunge".to_string(),
                count: Some(4),
            },
        ];

        let seeds = extract_affinity_seeds(&tags);
        assert_eq!(seeds.genres, vec!["rock", "grunge"]);
        assert!(seeds.tags.is_empty());
    }

    #[test]
    fn test_empty_tags() {
        let seeds = extract_affinity_seeds(&[]);
        assert!(seeds.genres.is_empty());
        assert!(seeds.tags.is_empty());
        assert!(seeds.normalized_seeds.is_empty());
    }
}
