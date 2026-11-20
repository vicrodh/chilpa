//! What a composition is, read without the network: its form from the
//! printed title and its composer's period and years from a bundled table.
//! Each fact names where it came from, so values from external catalogues
//! (dates, instrumentation, a form stated by a catalogue) can join under the
//! same fields with their own source. The table is parsed once, on the first
//! composer or composition page, and holds under 512 entries.
use chilpa_library::store::query::fold;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const COMPOSERS: &str = include_str!("../../data/classical-composers.tsv");

pub(crate) struct Composer {
    pub name: &'static str,
    pub born: Option<i32>,
    pub died: Option<i32>,
    pub period: &'static str,
}

impl Composer {
    /// "1685–1750", "1935–" for a living composer, "–1201" with no birth year.
    pub fn years(&self) -> String {
        match (self.born, self.died) {
            (None, None) => String::new(),
            (born, died) => format!("{}–{}", born.map(|y| y.to_string()).unwrap_or_default(), died.map(|y| y.to_string()).unwrap_or_default()),
        }
    }
}

fn key(name: &str) -> String { fold(name).chars().filter(|c| c.is_alphanumeric()).collect() }

fn table() -> &'static (Vec<Composer>, HashMap<String, usize>) {
    static TABLE: OnceLock<(Vec<Composer>, HashMap<String, usize>)> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut list = Vec::new();
        let mut keys = HashMap::new();
        for line in COMPOSERS.lines().filter(|line| !line.starts_with('#') && !line.trim().is_empty()) {
            let mut cells = line.split('\t');
            let (Some(name), Some(born), Some(died), Some(period)) = (cells.next(), cells.next(), cells.next(), cells.next()) else { continue };
            let at = list.len();
            for alias in std::iter::once(name).chain(cells.next().unwrap_or("").split('|')).filter(|alias| !alias.is_empty()) {
                keys.entry(key(alias)).or_insert(at);
            }
            list.push(Composer { name, born: born.parse().ok(), died: died.parse().ok(), period });
        }
        (list, keys)
    })
}

/// The table's composer for a credited name, also when printed "Last, First".
pub(crate) fn composer(name: &str) -> Option<&'static Composer> {
    let (list, keys) = table();
    keys.get(&key(name))
        .or_else(|| name.split_once(',').and_then(|(last, first)| keys.get(&key(&format!("{} {}", first.trim(), last.trim())))))
        .map(|at| &list[*at])
}

pub(crate) fn period_label(period: &str) -> String {
    match period {
        "medieval" => chilpa_i18n::t("Medieval"),
        "renaissance" => chilpa_i18n::t("Renaissance"),
        "baroque" => chilpa_i18n::t("Baroque"),
        "classical" => chilpa_i18n::t("Classical period"),
        "romantic" => chilpa_i18n::t("Romantic"),
        "modern" => chilpa_i18n::t("Modern"),
        "contemporary" => chilpa_i18n::t("Contemporary"),
        _ => String::new(),
    }
}

/// Forms and the words that name them in titles and in catalogues' work
/// types. Compound names are searched first; then the earliest single word.
const COMPOUND: &[(&str, &[&str])] = &[
    ("tone_poem", &["symphonic poem", "tone poem", "poeme symphonique", "sinfonische dichtung"]),
    ("song_cycle", &["song cycle", "song-cycle", "liederkreis", "liederzyklus"]),
    ("prelude_fugue", &["prelude and fugue", "praludium und fuge", "prelude et fugue"]),
    ("concerto", &["concerto grosso", "sinfonia concertante", "symphonie concertante"]),
];
const SINGLE: &[(&str, &[&str])] = &[
    ("symphony", &["symphony", "symphonies", "sinfonie", "symphonie", "sinfonia", "sinfonias"]),
    ("concerto", &["concerto", "concertos", "concerti", "konzert", "concierto", "concertino"]),
    ("sonata", &["sonata", "sonatas", "sonate", "sonaten", "sonatina", "sonatine"]),
    ("quartet", &["quartet", "quartets", "quartett", "quatuor", "cuarteto", "quartetto"]),
    ("quintet", &["quintet", "quintett", "quintette", "quintetto"]),
    ("trio", &["trio", "trios"]),
    ("suite", &["suite", "suites"]),
    ("partita", &["partita", "partitas"]),
    ("prelude", &["prelude", "preludes", "praludium", "praeludium", "preludio"]),
    ("fugue", &["fugue", "fuge", "fuga"]),
    ("toccata", &["toccata"]),
    ("fantasia", &["fantasia", "fantasie", "fantaisie"]),
    ("cantata", &["cantata", "cantatas", "kantate", "cantate"]),
    ("oratorio", &["oratorio", "oratorium"]),
    ("mass", &["mass", "messe", "missa", "misa"]),
    ("requiem", &["requiem"]),
    ("passion", &["passion", "passio"]),
    ("motet", &["motet", "motets", "motette", "mottetto"]),
    ("opera", &["opera", "oper", "singspiel", "zarzuela"]),
    ("operetta", &["operetta", "operette"]),
    ("aria", &["aria", "arie"]),
    ("song", &["song", "songs", "lied", "lieder", "chanson", "cancion", "canzone"]),
    ("madrigal", &["madrigal", "madrigals", "madrigale"]),
    ("chorale", &["chorale"]),
    ("overture", &["overture", "ouverture", "obertura"]),
    ("variations", &["variations", "variationen", "variazioni", "variaciones"]),
    ("etude", &["etude", "etudes", "study"]),
    ("nocturne", &["nocturne", "nocturnes", "notturno", "nocturno"]),
    ("waltz", &["waltz", "waltzes", "walzer", "valse", "vals"]),
    ("mazurka", &["mazurka", "mazurkas"]),
    ("polonaise", &["polonaise", "polonaises"]),
    ("ballade", &["ballade", "ballades"]),
    ("impromptu", &["impromptu", "impromptus"]),
    ("scherzo", &["scherzo"]),
    ("rondo", &["rondo"]),
    ("minuet", &["minuet", "menuett", "menuet", "minuetto"]),
    ("march", &["march", "marsch", "marche"]),
    ("serenade", &["serenade", "serenata", "standchen"]),
    ("divertimento", &["divertimento"]),
    ("rhapsody", &["rhapsody", "rhapsodie", "rapsodia"]),
    ("ballet", &["ballet", "ballett", "balletto"]),
    ("canon", &["canon", "kanon"]),
    ("gigue", &["gigue", "giga"]),
    ("chaconne", &["chaconne", "ciaccona"]),
    ("passacaglia", &["passacaglia"]),
    ("invention", &["invention", "inventions", "inventio"]),
    ("intermezzo", &["intermezzo"]),
    ("capriccio", &["capriccio", "caprice"]),
];

fn words(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

fn found_at(haystack: &[String], phrase: &str) -> Option<usize> {
    let needle: Vec<&str> = phrase.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    (needle.len() <= haystack.len()).then(|| (0..=haystack.len() - needle.len())
        .find(|at| needle.iter().enumerate().all(|(n, word)| haystack[at + n] == *word))).flatten()
}

/// The form a title or a catalogue's work type names, as a stable key.
pub(crate) fn form_of(text: &str) -> Option<&'static str> {
    let haystack = words(text);
    if let Some((form, _)) = COMPOUND.iter().find(|(_, names)| names.iter().any(|name| found_at(&haystack, name).is_some())) { return Some(form); }
    SINGLE.iter().filter_map(|(form, names)| names.iter().filter_map(|name| found_at(&haystack, name)).min().map(|at| (at, *form)))
        .min().map(|(_, form)| form)
}

pub(crate) fn form_label(form: &str) -> String {
    let text = match form {
        "tone_poem" => chilpa_i18n::mark("Tone poem"), "song_cycle" => chilpa_i18n::mark("Song cycle"), "prelude_fugue" => chilpa_i18n::mark("Prelude and fugue"), "symphony" => chilpa_i18n::mark("Symphony"),
        "concerto" => chilpa_i18n::mark("Concerto"), "sonata" => chilpa_i18n::mark("Sonata"), "quartet" => chilpa_i18n::mark("Quartet"), "quintet" => chilpa_i18n::mark("Quintet"), "trio" => chilpa_i18n::mark("Trio"),
        "suite" => chilpa_i18n::mark("Suite"), "partita" => chilpa_i18n::mark("Partita"), "prelude" => chilpa_i18n::mark("Prelude"), "fugue" => chilpa_i18n::mark("Fugue"), "toccata" => chilpa_i18n::mark("Toccata"),
        "fantasia" => chilpa_i18n::mark("Fantasia"), "cantata" => chilpa_i18n::mark("Cantata"), "oratorio" => chilpa_i18n::mark("Oratorio"), "mass" => chilpa_i18n::mark("Mass"), "requiem" => chilpa_i18n::mark("Requiem"),
        "passion" => chilpa_i18n::mark("Passion"), "motet" => chilpa_i18n::mark("Motet"), "opera" => chilpa_i18n::mark("Opera"), "operetta" => chilpa_i18n::mark("Operetta"), "aria" => chilpa_i18n::mark("Aria"), "song" => chilpa_i18n::mark("Song"),
        "madrigal" => chilpa_i18n::mark("Madrigal"), "chorale" => chilpa_i18n::mark("Chorale"), "overture" => chilpa_i18n::mark("Overture"), "variations" => chilpa_i18n::mark("Variations"), "etude" => chilpa_i18n::mark("Étude"),
        "nocturne" => chilpa_i18n::mark("Nocturne"), "waltz" => chilpa_i18n::mark("Waltz"), "mazurka" => chilpa_i18n::mark("Mazurka"), "polonaise" => chilpa_i18n::mark("Polonaise"), "ballade" => chilpa_i18n::mark("Ballade"),
        "impromptu" => chilpa_i18n::mark("Impromptu"), "scherzo" => chilpa_i18n::mark("Scherzo"), "rondo" => chilpa_i18n::mark("Rondo"), "minuet" => chilpa_i18n::mark("Minuet"), "march" => chilpa_i18n::mark("March"),
        "serenade" => chilpa_i18n::mark("Serenade"), "divertimento" => chilpa_i18n::mark("Divertimento"), "rhapsody" => chilpa_i18n::mark("Rhapsody"), "ballet" => chilpa_i18n::mark("Ballet"), "canon" => chilpa_i18n::mark("Canon"),
        "gigue" => chilpa_i18n::mark("Gigue"), "chaconne" => chilpa_i18n::mark("Chaconne"), "passacaglia" => chilpa_i18n::mark("Passacaglia"), "invention" => chilpa_i18n::mark("Invention"),
        "intermezzo" => chilpa_i18n::mark("Intermezzo"), "capriccio" => chilpa_i18n::mark("Capriccio"),
        _ => return String::new(),
    };
    chilpa_i18n::t(text)
}

/// One fact and where it came from: "title" (the printed work title) or
/// "composers" (the bundled composer table).
fn fact(value: &str, label: String, source: &str) -> Value {
    json!({"value": value, "label": label, "source": source})
}

/// The facts of a composition, for the table and its detail.
pub(crate) struct Facts {
    pub form: Option<Value>,
    pub period: Option<Value>,
    pub date: Option<Value>,
    pub instrumentation: Option<Value>,
}

pub(crate) fn facts(title: &str, composers: &[String]) -> Facts {
    let form = form_of(title).map(|form| fact(form, form_label(form), "title"));
    let period = composers.iter().find_map(|name| composer(name)).map(|known| fact(known.period, period_label(known.period), "composers"));
    Facts { form, period, date: None, instrumentation: None }
}

/// Which compositions belong to the classical tradition: a composer in the
/// table, a composer with a track in a classical genre, or a work whose own
/// tracks are in one.
#[derive(Default)]
pub(crate) struct Classical {
    genres: HashSet<String>,
    composers: HashSet<String>,
}

impl Classical {
    pub fn new(genres: &[String], composers: impl IntoIterator<Item = String>) -> Self {
        Self { genres: genres.iter().map(|genre| genre.to_lowercase()).collect(), composers: composers.into_iter().map(|name| key(&name)).collect() }
    }
    pub fn genre(&self, names: &[String]) -> bool { names.iter().any(|name| self.genres.contains(&name.to_lowercase())) }
    pub fn add_composers(&mut self, names: &[String]) { self.composers.extend(names.iter().map(|name| key(name))); }
    pub fn composer(&self, name: &str) -> bool { composer(name).is_some() || self.composers.contains(&key(name)) }
    pub fn work(&self, composers: &[String], genre: bool) -> bool { genre || composers.iter().any(|name| self.composer(name)) }
}

#[cfg(test)]
mod tests;
