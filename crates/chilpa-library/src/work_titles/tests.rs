use super::*;

fn names(list: &[&str]) -> Vec<String> { list.iter().map(|name| name.to_string()).collect() }

#[test]
fn a_part_title_names_its_whole_work() {
    assert_eq!(work_of_part("Symphony No. 5 in C minor, Op. 67: I. Allegro con brio"), Some("Symphony No. 5 in C minor, Op. 67"));
    assert_eq!(work_of_part("Prelude and Fugue No. 18 in G-Sharp Minor, BWV 887: II. Fugue"), Some("Prelude and Fugue No. 18 in G-Sharp Minor, BWV 887"));
    assert_eq!(work_of_part("Ja, mir hast du Arbeit gemacht, JLB 5: I. Ja, mir hast du Arbeit gemacht"), Some("Ja, mir hast du Arbeit gemacht, JLB 5"));
    assert_eq!(work_of_part("Nocturnes, Op. 9: No. 2 in E-Flat Major"), Some("Nocturnes, Op. 9"));
    assert_eq!(work_of_part("Die Walküre, WWV 86B: Act I: Vorspiel"), Some("Die Walküre, WWV 86B"));
    assert_eq!(work_of_part("Symphony No. 9 - IV. Presto"), Some("Symphony No. 9"));
}

#[test]
fn a_song_title_names_no_work() {
    for title in ["Wildest Dreams", "Run to the Hills - 2015 Remaster", "Lord of the Flies - Live", "22, Acacia Avenue",
        "Sonne (Live in Mexico City)", "Wagner: Das Rheingold, Scene 1: Weia! Waga!", "I Want You: She's So Heavy", "Part of Me: Reprise"] {
        assert_eq!(work_of_part(title), None, "{title}");
    }
}

#[test]
fn a_title_gives_a_work_only_with_a_composer_and_drops_a_printed_composer_name() {
    let title = "Wagner: Das Rheingold, WWV 86A: Scene 1. Weia! Waga!";
    assert_eq!(work_from_title(title, &names(&["Richard Wagner"])).as_deref(), Some("Das Rheingold, WWV 86A"));
    assert_eq!(work_from_title(title, &[]), None);
    assert_eq!(work_from_title("Ja, mir hast du Arbeit gemacht, JLB 5: II. Der Herr", &names(&["Johann Ludwig Bach"])).as_deref(), Some("Ja, mir hast du Arbeit gemacht, JLB 5"));
}

#[test]
fn a_lookup_does_not_propose_a_song_as_its_own_work() {
    assert_eq!(proposed_work("Wildest Dreams", "Wildest Dreams"), None);
    assert_eq!(proposed_work("Sonne", "Sonne (Live in Mexico City)"), None);
    assert_eq!(proposed_work("22 Acacia Avenue", "22, Acacia Avenue"), None);
    assert_eq!(proposed_work("Fear of the Dark", "Fear of the Dark - Live"), None);
    assert_eq!(proposed_work("", "Anything"), None);
}

#[test]
fn a_lookup_proposes_the_whole_work_of_a_part_and_keeps_numbered_works() {
    assert_eq!(proposed_work("Symphony no. 5 in C minor, op. 67: I. Allegro con brio", "Symphony No. 5: I. Allegro con brio").as_deref(),
        Some("Symphony no. 5 in C minor, op. 67"));
    assert_eq!(proposed_work("Ave verum corpus, K. 618", "Ave verum corpus, K. 618").as_deref(), Some("Ave verum corpus, K. 618"));
    assert_eq!(proposed_work("Gangland", "Total Eclipse").as_deref(), Some("Gangland"));
}

#[test]
fn catalogue_numbers_are_read_in_their_printed_forms() {
    for work in ["Symphony No. 5, Op. 67", "Op.67", "BWV 887", "Ave verum corpus, K. 618", "Cello Concerto in B minor, B. 191 (Op. 104)", "Duet, H. 598, Wq. 140"] {
        assert!(has_catalogue(work), "{work}");
    }
    for work in ["Wildest Dreams", "No More Lies", "Vol. 2", "Plan 9"] {
        assert!(!has_catalogue(work), "{work}");
    }
}
