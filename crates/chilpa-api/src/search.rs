//! La búsqueda, juntando lo local con lo que aporten los plugins.
//!
//! Las reglas vienen del spec CHD-0002 y no son adornos:
//!
//! - **Lo local sale primero y al instante.** Un plugin nunca hace esperar a
//!   lo que ya está en disco.
//! - **Cada source llega cuando contesta**, con su límite de tiempo. El que
//!   no contesta se omite y se marca; no detiene a los demás.
//! - **Chilpa ordena y agrupa.** Un plugin no puede reordenar ni esconder
//!   los resultados de otro: lo suyo se agrega al final, en el orden en que
//!   se registró.
//! - **Lo que es la misma canción se muestra una vez**, emparejado por ISRC
//!   o MBID, diciendo en dónde más está. Emparejar por nombre es no
//!   emparejar, así que no se hace.
//! - **Nunca una consulta por tecla.** Se espera a que el usuario deje de
//!   escribir, y cada source tiene un descanso mínimo entre consultas.

use super::*;
use chilpa_plugin::messages;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cuánto se espera a que el usuario deje de escribir antes de molestar a
/// un plugin.
pub const KEYSTROKE_DEBOUNCE: Duration = Duration::from_millis(350);

/// Lo menos que puede pasar entre dos consultas a los sources.
pub const SOURCE_COOLDOWN: Duration = Duration::from_millis(800);

/// Cuántos resultados se le piden a cada source.
const PER_SOURCE: usize = 50;

/// De dónde salió un resultado que ya estaba en disco.
const LOCAL: &str = "local";

/// Un resultado de búsqueda, venga de donde venga.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Hit {
    /// La ruta si es local, o el id externo si es de un source.
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    /// De dónde salió: `"local"` o el id del plugin.
    pub source: String,
    /// Cómo se llama ese source para el usuario.
    pub source_name: String,
    /// La calidad que ofrece, vacía si no se sabe.
    pub quality: String,
    /// Los demás sources donde está la misma canción.
    pub also_in: Vec<String>,
    /// Cierto si se puede poner a sonar hoy. Lo de un plugin todavía no.
    pub playable: bool,
}

impl Hit {
    fn local(t: LibraryTrack) -> Self {
        Self {
            id: t.path,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration_ms: t.duration_ms,
            source: LOCAL.into(),
            source_name: "Mi biblioteca".into(),
            quality: String::new(),
            also_in: Vec::new(),
            playable: t.present,
        }
    }

    fn remote(t: messages::RemoteTrack, source: &str, source_name: &str) -> Self {
        Self {
            id: t.external_id,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration_ms: t.duration_ms,
            source: source.to_string(),
            source_name: source_name.to_string(),
            quality: t.quality.describe(),
            also_in: Vec::new(),
            // Reproducir lo de un plugin necesita `chilpa_resolver` y la
            // ruta de stream; todavía no está, y decir que sí se puede
            // sería mentir en la interfaz.
            playable: false,
        }
    }
}

/// Lo que un source contestó a una consulta.
#[derive(Clone, Debug)]
pub struct SourceReply {
    pub source: String,
    pub source_name: String,
    pub results: messages::Results,
    /// Vacío si contestó bien.
    pub problem: String,
}

impl SourceReply {
    fn failed(source: String, source_name: String, problem: String) -> Self {
        Self { source, source_name, results: messages::Results::default(), problem }
    }
}

/// Las huellas de un track local: el ISRC y el MBID que traiga, ya
/// normalizados como los escribe un plugin.
fn fingerprints_of(isrc: Option<String>, mbid: Option<String>) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(f) = mbid.as_deref().and_then(messages::mbid_fingerprint) {
        v.push(f);
    }
    if let Some(f) = isrc.as_deref().and_then(messages::isrc_fingerprint) {
        v.push(f);
    }
    v
}

/// Junta lo local con lo de los plugins, sin que ninguno pise al otro.
///
/// `local_fingerprints` dice qué ISRC o MBID tiene cada resultado local (por
/// ruta), para no mostrar dos veces la misma canción.
pub fn merge(
    local: Vec<LibraryTrack>,
    local_fingerprints: &HashMap<String, Vec<String>>,
    replies: &[SourceReply],
) -> Vec<Hit> {
    let mut out: Vec<Hit> = local.into_iter().map(Hit::local).collect();

    // Dónde está cada huella dentro de lo que ya se va a mostrar.
    let mut at: HashMap<String, usize> = HashMap::new();
    for (i, hit) in out.iter().enumerate() {
        for fp in local_fingerprints.get(&hit.id).into_iter().flatten() {
            at.entry(fp.clone()).or_insert(i);
        }
    }

    for reply in replies {
        for track in &reply.results.tracks {
            match track.fingerprint().and_then(|f| at.get(&f).copied()) {
                // Ya estaba: no se repite, se anota dónde más vive.
                Some(i) => {
                    let name = reply.source_name.clone();
                    if !out[i].also_in.contains(&name) {
                        out[i].also_in.push(name);
                    }
                }
                None => {
                    let hit = Hit::remote(track.clone(), &reply.source, &reply.source_name);
                    if let Some(f) = track.fingerprint() {
                        at.insert(f, out.len());
                    }
                    out.push(hit);
                }
            }
        }
    }
    out
}

/// Una búsqueda en los plugins, corriendo en su propio hilo.
pub struct SearchJob {
    text: String,
    replies: std::sync::mpsc::Receiver<SourceReply>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SearchJob {
    /// Lanza la consulta a cada plugin que sepa buscar.
    pub fn start(plugins: Arc<Mutex<plugins::Plugins>>, text: String, limit: usize) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let query = messages::write(&messages::Query::new(&text, limit));
        let thread = std::thread::Builder::new()
            .name("chilpa-search".into())
            .spawn(move || {
                // Se toma la lista primero y se suelta el candado entre
                // plugins: uno lento no puede dejar la ventana esperando ni
                // bloquear al resto de la fachada.
                let Ok(who) = plugins.lock().map(|p| p.searchers()) else { return };
                for (id, name) in who {
                    let answer = match plugins.lock() {
                        Ok(mut p) => p.call_short(&id, messages::op::SEARCH, &query),
                        Err(_) => return,
                    };
                    let reply = match answer {
                        Ok(bytes) => match messages::read::<messages::Results>(&bytes) {
                            Ok(r) => SourceReply {
                                source: id,
                                source_name: name,
                                results: r.clean(),
                                problem: String::new(),
                            },
                            Err(e) => SourceReply::failed(id, name, e),
                        },
                        Err(e) => SourceReply::failed(id, name, e),
                    };
                    if tx.send(reply).is_err() {
                        // Ya no hay quien escuche: la búsqueda cambió.
                        return;
                    }
                }
            })
            .ok();
        Self { text, replies: rx, thread }
    }

    /// Para qué texto es esta búsqueda.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Lo que haya llegado, sin bloquear.
    pub fn collect(&self) -> Vec<SourceReply> {
        let mut v = Vec::new();
        while let Ok(r) = self.replies.try_recv() {
            v.push(r);
        }
        v
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|h| h.is_finished())
    }
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        // El hilo se acaba solo en cuanto el canal se cierra; no se espera,
        // porque una búsqueda vieja no debe frenar a la nueva.
        self.thread.take();
    }
}

/// Lo que la fachada guarda de la búsqueda en curso.
#[derive(Default)]
pub(crate) struct SearchState {
    /// Lo que el usuario tiene escrito ahora.
    pub text: String,
    /// Cuándo tecleó por última vez.
    pub keystroke: Option<Instant>,
    /// Cuándo se le preguntó a los sources por última vez.
    pub last_query: Option<Instant>,
    /// La consulta a los plugins, mientras corre.
    pub job: Option<SearchJob>,
    /// Lo que cada source contestó, uno por source.
    pub replies: Vec<SourceReply>,
    /// Lo que salió de la biblioteca, sin mezclar.
    pub local: Vec<LibraryTrack>,
    /// Las huellas de eso local, por ruta.
    pub fingerprints: HashMap<String, Vec<String>>,
    /// Lo local y lo remoto ya juntos, que es lo que la vista dibuja.
    pub hits: Vec<Hit>,
}

impl Chilpa {
    /// Busca en la biblioteca y, si hay plugins, se los pregunta también.
    ///
    /// Lo local sale de inmediato, en el valor que regresa. Lo de los
    /// plugins llega después por [`Chilpa::poll`] y se recoge con
    /// [`Chilpa::found`].
    pub fn search_all(&mut self, text: &str, limit: usize) -> Vec<Hit> {
        let text = text.trim().to_string();
        if text.is_empty() {
            self.search = SearchState::default();
            return Vec::new();
        }
        let local = self.search(&text, limit);
        let fingerprints = self.fingerprints_of(&local);
        // Una búsqueda nueva tira lo que contestaron los sources de la
        // anterior: mostrar resultados de otra consulta es peor que no
        // mostrar nada.
        self.search = SearchState {
            text,
            keystroke: Some(Instant::now()),
            last_query: self.search.last_query,
            job: None,
            replies: Vec::new(),
            local,
            fingerprints,
            hits: Vec::new(),
        };
        self.rebuild_hits();
        self.search.hits.clone()
    }

    /// Lo último que se juntó, para que la vista lo dibuje.
    pub fn found(&self) -> &[Hit] {
        &self.search.hits
    }

    /// Cierto mientras algún source siga sin contestar, para que la vista
    /// pueda decir que todavía falta algo en lugar de fingir que eso es
    /// todo lo que hay.
    pub fn search_running(&self) -> bool {
        self.search.job.as_ref().is_some_and(|j| !j.finished())
    }

    /// Los sources que contestaron mal, para decirlo sin esconderlo.
    pub fn search_problems(&self) -> Vec<String> {
        self.search
            .replies
            .iter()
            .filter(|r| !r.problem.is_empty())
            .map(|r| format!("{}: {}", r.source_name, r.problem))
            .collect()
    }

    /// Mueve la búsqueda: recoge lo que haya llegado y lanza la consulta
    /// cuando el usuario dejó de escribir. Lo llama `poll`.
    pub(crate) fn advance_search(&mut self) {
        // Lo que ya llegó, primero.
        let arrived: Vec<SourceReply> =
            self.search.job.as_ref().map(|j| j.collect()).unwrap_or_default();
        if !arrived.is_empty() {
            for r in arrived {
                self.search.replies.retain(|other| other.source != r.source);
                self.search.replies.push(r);
            }
            self.rebuild_hits();
        }

        if self.search.text.is_empty() {
            return;
        }
        // Todavía está escribiendo.
        if !self.search.keystroke.is_some_and(|t| t.elapsed() >= KEYSTROKE_DEBOUNCE) {
            return;
        }
        // Esta consulta ya se lanzó.
        if self.search.job.as_ref().is_some_and(|j| j.text() == self.search.text) {
            return;
        }
        // Se le acaba de preguntar a los sources: se espera.
        if self.search.last_query.is_some_and(|t| t.elapsed() < SOURCE_COOLDOWN) {
            return;
        }
        let Some(plugins) = self.plugins.clone() else { return };
        let any = plugins.lock().map(|p| !p.searchers().is_empty()).unwrap_or(false);
        if !any {
            return;
        }
        self.search.last_query = Some(Instant::now());
        self.search.job = Some(SearchJob::start(plugins, self.search.text.clone(), PER_SOURCE));
    }

    fn rebuild_hits(&mut self) {
        self.search.hits = merge(
            self.search.local.clone(),
            &self.search.fingerprints,
            &self.search.replies,
        );
    }

    /// Qué ISRC o MBID tiene cada resultado local, en una sola consulta.
    fn fingerprints_of(&mut self, local: &[LibraryTrack]) -> HashMap<String, Vec<String>> {
        if local.is_empty() {
            return HashMap::new();
        }
        let ids: Vec<i64> = local.iter().map(|t| t.id).collect();
        let by_id: HashMap<i64, String> = local.iter().map(|t| (t.id, t.path.clone())).collect();
        let Some(lib) = self.library() else { return HashMap::new() };
        let Ok(found) = lib.fingerprints_for(&ids) else { return HashMap::new() };
        found
            .into_iter()
            .filter_map(|(id, (isrc, mbid))| {
                let fps = fingerprints_of(isrc, mbid);
                if fps.is_empty() {
                    return None;
                }
                Some((by_id.get(&id)?.clone(), fps))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(id: i64, title: &str) -> LibraryTrack {
        LibraryTrack {
            id,
            path: format!("/musica/{title}.flac"),
            title: title.to_string(),
            artist: "Alguien".into(),
            album: "Un disco".into(),
            track_number: None,
            duration_ms: 200_000,
            present: true,
            analysis: String::new(),
        }
    }

    fn remote(id: &str, title: &str, isrc: Option<&str>) -> messages::RemoteTrack {
        messages::RemoteTrack {
            external_id: id.to_string(),
            title: title.to_string(),
            artist: "Alguien".into(),
            isrc: isrc.map(str::to_string),
            quality: messages::Quality {
                codec: Some("flac".into()),
                bit_depth: Some(24),
                sample_rate_hz: Some(96_000),
                channels: Some(2),
            },
            ..Default::default()
        }
    }

    fn reply(source: &str, tracks: Vec<messages::RemoteTrack>) -> SourceReply {
        SourceReply {
            source: source.to_string(),
            source_name: format!("Source {source}"),
            results: messages::Results { tracks, ..Default::default() },
            problem: String::new(),
        }
    }

    #[test]
    fn what_is_local_always_comes_first() {
        let mine = vec![local(1, "Una"), local(2, "Otra")];
        let replies = vec![reply("a", vec![remote("x", "De un plugin", None)])];
        let all = merge(mine, &HashMap::new(), &replies);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].source, LOCAL);
        assert_eq!(all[1].source, LOCAL);
        assert_eq!(all[2].source, "a", "lo del plugin va después");
    }

    #[test]
    fn the_same_song_shows_once_and_says_where_else_it_is() {
        let mine = vec![local(1, "Una")];
        let mut fps = HashMap::new();
        fps.insert("/musica/Una.flac".to_string(), vec!["isrc:USRC17607839".to_string()]);
        let replies = vec![
            reply("a", vec![remote("x", "Una", Some("USRC17607839"))]),
            reply("b", vec![remote("y", "Una", Some("usrc17607839"))]),
        ];
        let all = merge(mine, &fps, &replies);
        assert_eq!(all.len(), 1, "es la misma canción");
        assert_eq!(all[0].source, LOCAL);
        assert_eq!(all[0].also_in, vec!["Source a", "Source b"], "y se dice dónde más está");
    }

    #[test]
    fn without_a_fingerprint_nothing_is_matched() {
        // Mismo título y mismo artista, pero sin ISRC ni MBID: son dos
        // renglones, porque emparejar por nombre es adivinar.
        let mine = vec![local(1, "Una")];
        let replies = vec![reply("a", vec![remote("x", "Una", None)])];
        assert_eq!(merge(mine, &HashMap::new(), &replies).len(), 2);
    }

    #[test]
    fn two_plugins_with_a_song_that_is_not_local_show_it_once() {
        let replies = vec![
            reply("a", vec![remote("x", "Una", Some("USRC1"))]),
            reply("b", vec![remote("y", "Una", Some("USRC1"))]),
        ];
        let all = merge(Vec::new(), &HashMap::new(), &replies);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].source, "a", "se queda con el primero que contestó");
        assert_eq!(all[0].also_in, vec!["Source b"]);
    }

    #[test]
    fn a_source_that_did_not_answer_does_not_erase_the_others() {
        let broken = SourceReply::failed(
            "roto".into(),
            "Source roto".into(),
            "se pasó del cómputo permitido".into(),
        );
        let replies = vec![broken, reply("b", vec![remote("y", "Una", None)])];
        let all = merge(vec![local(1, "Local")], &HashMap::new(), &replies);
        assert_eq!(all.len(), 2, "lo local y lo del que sí contestó");
    }

    #[test]
    fn the_quality_of_a_source_shows_and_the_local_one_is_not_invented() {
        let replies = vec![reply("a", vec![remote("x", "Una", None)])];
        let all = merge(vec![local(1, "Local")], &HashMap::new(), &replies);
        assert_eq!(all[0].quality, "", "de lo local no se inventa una calidad aquí");
        assert_eq!(all[1].quality, "FLAC · 24 bits a 96000 Hz");
    }

    #[test]
    fn what_a_plugin_offers_cannot_be_played_yet_and_says_so() {
        let replies = vec![reply("a", vec![remote("x", "Una", None)])];
        let all = merge(Vec::new(), &HashMap::new(), &replies);
        assert!(!all[0].playable, "sin resolver el stream no se puede prometer");
    }

    #[test]
    fn the_mbid_wins_over_the_isrc_and_both_sides_write_it_the_same() {
        // La biblioteca local y un plugin tienen que producir exactamente la
        // misma cadena, o emparejar no empareja nada.
        let mbid = "B1A9C0E9-D987-4042-AE91-78D6A3267D69";
        let mut t = remote("x", "Una", Some("usrc1"));
        t.musicbrainz_recording_id = Some(mbid.to_string());
        let locales = fingerprints_of(Some("usrc1".into()), Some(mbid.to_ascii_lowercase()));
        assert_eq!(t.fingerprint().unwrap(), locales[0]);
        assert_eq!(locales.len(), 2, "lo local expone las dos, por si la otra orilla trae una");
    }
}
