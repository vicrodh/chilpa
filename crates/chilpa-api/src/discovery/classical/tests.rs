use super::*;

#[test]
fn composers_match_their_names_aliases_and_inverted_names() {
    let bach = composer("Johann Sebastian Bach").unwrap();
    assert_eq!((bach.period, bach.years().as_str()), ("baroque", "1685–1750"));
    assert_eq!(composer("Bach, Johann Sebastian").unwrap().name, "Johann Sebastian Bach");
    assert_eq!(composer("J.S. Bach").unwrap().name, "Johann Sebastian Bach");
    assert_eq!(composer("Händel").unwrap().name, "George Frideric Handel");
    assert_eq!(composer("Antonin Dvorak").unwrap().period, "romantic");
    assert_eq!(composer("Arvo Pärt").unwrap().years(), "1935–");
    assert!(composer("Bach").is_none(), "a surname shared by several composers names none");
    assert!(composer("Steve Harris").is_none());
}

#[test]
fn every_table_row_has_a_known_period_and_ordered_years() {
    let (list, _) = table();
    assert!(list.len() > 250 && list.len() < 512);
    for known in list {
        assert!(!period_label(known.period).is_empty(), "{}", known.name);
        if let (Some(born), Some(died)) = (known.born, known.died) { assert!(born < died, "{}", known.name); }
    }
}

#[test]
fn a_title_names_its_form() {
    assert_eq!(form_of("Symphony No. 5 in C minor, Op. 67"), Some("symphony"));
    assert_eq!(form_of("Cello Concerto in B minor, B. 191"), Some("concerto"));
    assert_eq!(form_of("Ja, mir hast du Arbeit gemacht, JLB 5"), None);
    assert_eq!(form_of("Die mit Tränen säen, cantata for 4 voices"), Some("cantata"));
    assert_eq!(form_of("Prelude and Fugue No. 18 in G-Sharp Minor, BWV 887"), Some("prelude_fugue"));
    assert_eq!(form_of("Dido and Aeneas, opera, Z. 626"), Some("opera"));
    assert_eq!(form_of("Ein Heldenleben, symphonic poem, Op. 40"), Some("tone_poem"));
    assert_eq!(form_of("Ellens Gesang III (\"Ave Maria\"), song for voice & piano"), Some("song"));
    assert_eq!(form_of("Präludium und Fuge"), Some("prelude_fugue"));
    assert_eq!(form_of("Song-cycle"), Some("song_cycle"), "a catalogue's work type names a form too");
    assert_eq!(form_of("Wildest Dreams"), None);
    assert_eq!(form_of("15 Sinfonias"), None, "a Baroque sinfonia is not a symphony");
    assert_eq!(form_of("4 Duettos"), Some("duet"));
}

#[test]
fn facts_say_where_they_came_from() {
    let found = facts("Symphony No. 9", &["Ludwig van Beethoven".into()]);
    assert_eq!(found.form.unwrap()["source"], "title");
    let period = found.period.unwrap();
    assert_eq!((period["value"].as_str(), period["source"].as_str()), (Some("classical"), Some("composers")));
    assert!(found.date.is_none() && found.instrumentation.is_none());
}

#[test]
fn a_work_is_classical_by_its_composer_or_by_a_classical_genre() {
    let mut classical = Classical::new(&["Baroque".into()], ["Nobuo Uematsu".to_string()]);
    assert!(classical.work(&["Wolfgang Amadeus Mozart".into()], false));
    assert!(classical.work(&["Nobuo Uematsu".into()], false), "a composer with classical-genre tracks");
    assert!(classical.work(&["Steve Harris".into()], true), "a work with a classical-genre track");
    assert!(!classical.work(&["Steve Harris".into()], false));
    assert!(classical.genre(&["baroque".into()]));
    classical.add_composers(&["Steve Harris".into()]);
    assert!(classical.composer("steve harris"));
}
