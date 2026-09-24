//! Lo que Chilpa y un plugin se dicen.
//!
//! Son los mensajes de la API v1, en JSON (ADR-007). Viven aquí y no en la
//! fachada para que un plugin de primera parte y uno de tercera hablen
//! exactamente el mismo idioma: si la API no aguanta nuestras propias
//! features, no sirve (ADR-003).
//!
//! Dos reglas que se ven en los tipos:
//!
//! 1. **Nada nombra a un servicio** (ADR-004). Un plugin describe lo suyo
//!    con datos —id externo, calidad, disponibilidad—, nunca con conceptos
//!    de una plataforma.
//! 2. **Todo lo que entra se limpia.** Un plugin puede contestar cualquier
//!    cosa; lo que pase de aquí para adentro ya viene recortado, sin campos
//!    vacíos y sin listas infinitas.

use serde::{Deserialize, Serialize};

/// Las operaciones que un plugin puede exportar.
///
/// Un plugin exporta las que sepa hacer; Chilpa le pregunta solo por las que
/// su manifest declaró.
pub mod op {
    /// Buscar. Entra [`super::Query`], sale [`super::Results`].
    pub const SEARCH: &str = "chilpa_search";
    /// Las listas del usuario en ese source.
    pub const PLAYLISTS: &str = "chilpa_playlists";
    /// Las pistas de una de esas listas.
    pub const PLAYLIST_TRACKS: &str = "chilpa_playlist_tracks";
    /// De un id externo a algo que se pueda reproducir.
    pub const RESOLVER: &str = "chilpa_resolver";
    /// "Olvida lo que tengas mío." Se le pide antes de desinstalarlo.
    ///
    /// Es la única operación que Chilpa manda sin que nadie la pida desde
    /// la ventana, y la única cuya respuesta no se usa para nada: lo que
    /// importa es haberla mandado. Un plugin que maneja su propia cuenta
    /// (`own_account`) es el que tiene algo que borrar aquí; para los demás
    /// es no hacer nada.
    pub const REVOKE: &str = "chilpa_revoke";
}

/// Cuánto se acepta de una sola respuesta, por tipo de entidad.
///
/// Un plugin que contesta cien mil tracks no está ayudando: está llenando la
/// memoria de Chilpa. Se recorta y se sigue.
pub const MAX_PER_REPLY: usize = 500;

/// Lo que se le pregunta a un plugin al buscar.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    /// Cuántos resultados se quieren, como mucho.
    pub limit: usize,
}

impl Query {
    pub fn new(text: &str, limit: usize) -> Self {
        Self { text: text.to_string(), limit: limit.min(MAX_PER_REPLY) }
    }
}

/// La calidad con la que un source ofrece algo.
///
/// Todo es opcional porque no todos los sources lo saben, y decir cero
/// cuando no se sabe es peor que no decir nada.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Quality {
    pub codec: Option<String>,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u16>,
}

impl Quality {
    /// Cómo se dice, para mostrarla. Vacío si no se sabe nada.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(c) = &self.codec {
            parts.push(c.to_ascii_uppercase());
        }
        match (self.bit_depth, self.sample_rate_hz) {
            (Some(b), Some(r)) => parts.push(format!("{b} bits a {r} Hz")),
            (None, Some(r)) => parts.push(format!("{r} Hz")),
            (Some(b), None) => parts.push(format!("{b} bits")),
            _ => {}
        }
        parts.join(" · ")
    }
}

/// Una pista que vive en un source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemoteTrack {
    /// El id que le da su source. Es la mitad de su identidad; la otra es el
    /// source mismo, y esa la pone Chilpa.
    pub external_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration_ms: u64,
    /// Para emparejar con lo local sin depender del nombre.
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub musicbrainz_recording_id: Option<String>,
    #[serde(default)]
    pub quality: Quality,
    /// La portada, si el source la ofrece por URL.
    #[serde(default)]
    pub artwork_url: Option<String>,
}

impl RemoteTrack {
    /// Cierto si tiene lo mínimo para mostrarse y volverse a pedir.
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.title.trim().is_empty()
    }

    /// Con qué se puede emparejar contra lo local. `None` si no trae nada
    /// firme: emparejar por nombre es como no emparejar.
    ///
    /// Se prefiere el MBID porque identifica la grabación; el ISRC puede
    /// repetirse entre reediciones del mismo master.
    pub fn fingerprint(&self) -> Option<String> {
        self.musicbrainz_recording_id
            .as_deref()
            .and_then(mbid_fingerprint)
            .or_else(|| self.isrc.as_deref().and_then(isrc_fingerprint))
    }
}

/// La huella de un MBID, normalizada. Vive suelta porque la biblioteca
/// local tiene que escribir exactamente la misma cadena para que emparejar
/// signifique algo; dos normalizaciones parecidas no emparejan nada.
pub fn mbid_fingerprint(mbid: &str) -> Option<String> {
    let m = mbid.trim();
    (!m.is_empty()).then(|| format!("mbid:{}", m.to_ascii_lowercase()))
}

/// Lo mismo para un ISRC, que por norma se escribe en mayúsculas.
pub fn isrc_fingerprint(isrc: &str) -> Option<String> {
    let i = isrc.trim();
    (!i.is_empty()).then(|| format!("isrc:{}", i.to_ascii_uppercase()))
}

/// Cómo viene lo que un source entrega para reproducir.
///
/// Medido en las APIs de dos servicios grandes: lo normal es una URL sola
/// ya firmada, pero también se entregan manifiestos segmentados. Se declara
/// en lugar de adivinarlo por la extensión, porque una URL firmada no tiene
/// extensión que mirar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Una sola URL que se abre y se lee de corrido. Es lo que Chilpa sabe
    /// reproducir hoy.
    #[default]
    Progressive,
    /// Un manifiesto DASH: el audio viene en segmentos.
    Dash,
    /// Un manifiesto HLS, por las mismas.
    Hls,
}

impl Delivery {
    /// Cierto si Chilpa sabe reproducir eso hoy.
    pub fn playable(&self) -> bool {
        matches!(self, Delivery::Progressive)
    }

    /// Cómo se le dice al usuario cuando no se puede.
    pub fn describe(&self) -> &'static str {
        match self {
            Delivery::Progressive => "una URL directa",
            Delivery::Dash => "un manifiesto DASH, que Chilpa todavía no sabe leer",
            Delivery::Hls => "un manifiesto HLS, que Chilpa todavía no sabe leer",
        }
    }
}

/// Lo que se le pide al source para poder reproducir algo suyo.
///
/// Entra por [`op::RESOLVER`] y sale un [`Stream`]. Va aparte de la
/// búsqueda porque resolver cuesta: muchos sources firman una URL por
/// llamada, y pedirla para cada resultado de una lista sería regalarles
/// trabajo y quemar el límite de la cuenta.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Resolve {
    /// El id que el mismo source dio al buscar.
    pub external_id: String,
}

impl Resolve {
    pub fn new(external_id: &str) -> Self {
        Self { external_id: external_id.to_string() }
    }
}

/// Un álbum que vive en un source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemoteAlbum {
    pub external_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub track_count: usize,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub musicbrainz_release_id: Option<String>,
}

impl RemoteAlbum {
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.title.trim().is_empty()
    }
}

/// Una lista que vive en un source.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RemotePlaylist {
    pub external_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub track_count: usize,
    #[serde(default)]
    pub artwork_url: Option<String>,
}

impl RemotePlaylist {
    pub fn usable(&self) -> bool {
        !self.external_id.trim().is_empty() && !self.name.trim().is_empty()
    }
}

/// Lo que un plugin contesta a una búsqueda.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Results {
    #[serde(default)]
    pub tracks: Vec<RemoteTrack>,
    #[serde(default)]
    pub albums: Vec<RemoteAlbum>,
    #[serde(default)]
    pub playlists: Vec<RemotePlaylist>,
}

impl Results {
    /// Quita lo que no sirve y recorta lo que sobra.
    ///
    /// Se hace de este lado, siempre, antes de que nada toque una vista. Un
    /// plugin puede contestar lo que quiera; lo que pase de aquí ya está
    /// limpio.
    pub fn clean(mut self) -> Self {
        self.tracks.retain(RemoteTrack::usable);
        self.albums.retain(RemoteAlbum::usable);
        self.playlists.retain(RemotePlaylist::usable);
        self.tracks.truncate(MAX_PER_REPLY);
        self.albums.truncate(MAX_PER_REPLY);
        self.playlists.truncate(MAX_PER_REPLY);
        self
    }

    pub fn empty(&self) -> bool {
        self.tracks.is_empty() && self.albums.is_empty() && self.playlists.is_empty()
    }

    /// Cuántas cosas trae en total.
    pub fn count(&self) -> usize {
        self.tracks.len() + self.albums.len() + self.playlists.len()
    }
}

/// Lo que un plugin contesta cuando se le pide resolver una pista.
///
/// Chilpa decodifica y entrega; el plugin no toca el audio ni el dispositivo
/// (ADR-003). Por eso aquí solo hay de dónde leer, no cómo sonar.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Stream {
    /// De dónde se lee. Tiene que ser `http` o `https`.
    ///
    /// Medido en las APIs de dos servicios grandes: la URL viene **ya
    /// firmada en la query**, así que no lleva cabecera de autenticación.
    /// Lo que sí va firmado es la llamada a la API que la entrega, y eso el
    /// plugin ya lo hace con `http` (ADR-008).
    #[serde(default)]
    pub url: String,
    /// Cuando una pista viene partida en varias URLs que se leen en orden.
    ///
    /// No es lo mismo que un manifiesto segmentado: son archivos completos,
    /// uno tras otro. **El plugin decodifica el manifiesto de su servicio y
    /// entrega la lista ya lista**; Chilpa no sabe de formatos de
    /// manifiesto, que es lo que dice el ADR-004.
    #[serde(default)]
    pub urls: Vec<String>,
    /// Lo que el source dice que es, para elegir el decodificador.
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub quality: Quality,
    /// Hasta cuándo sirve esa URL, en segundos unix. Muchos sources dan
    /// enlaces que caducan.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Cómo viene. Por omisión, una URL directa, que es lo más común.
    #[serde(default)]
    pub delivery: Delivery,
}

impl Stream {
    /// De dónde se lee, en orden. `url` primero si viene, luego `urls`.
    pub fn parts(&self) -> Vec<&str> {
        std::iter::once(self.url.as_str())
            .filter(|u| !u.is_empty())
            .chain(self.urls.iter().map(String::as_str))
            .collect()
    }

    /// Cierto si se puede intentar abrir: hay al menos una parte y todas
    /// son http.
    pub fn usable(&self) -> bool {
        let parts = self.parts();
        !parts.is_empty()
            && parts.iter().all(|u| u.starts_with("https://") || u.starts_with("http://"))
    }

    /// Cierto si ya caducó según el reloj que se le pase.
    pub fn expired(&self, now_unix: i64) -> bool {
        self.expires_at.is_some_and(|t| t <= now_unix)
    }
}

/// Lee lo que contestó un plugin, sin confiar en nada.
pub fn read<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("contestó algo que no se entiende: {e}"))
}

/// Arma lo que se le va a mandar.
pub fn write<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_never_asks_for_more_than_fits() {
        assert_eq!(Query::new("algo", 10).limit, 10);
        assert_eq!(Query::new("algo", 100_000).limit, MAX_PER_REPLY);
    }

    #[test]
    fn what_is_useless_is_dropped_and_what_is_extra_is_trimmed() {
        let mut r = Results::default();
        r.tracks.push(RemoteTrack { external_id: "a".into(), title: "Una".into(), ..Default::default() });
        // Sin id no se puede volver a pedir; sin título no se puede mostrar.
        r.tracks.push(RemoteTrack { external_id: "".into(), title: "Otra".into(), ..Default::default() });
        r.tracks.push(RemoteTrack { external_id: "b".into(), title: "   ".into(), ..Default::default() });
        for i in 0..1000 {
            r.albums.push(RemoteAlbum {
                external_id: i.to_string(),
                title: "Disco".into(),
                ..Default::default()
            });
        }
        let r = r.clean();
        assert_eq!(r.tracks.len(), 1, "solo la que sirve");
        assert_eq!(r.albums.len(), MAX_PER_REPLY, "lo demás se recorta");
        assert_eq!(r.count(), 1 + MAX_PER_REPLY);
    }

    #[test]
    fn the_fingerprint_prefers_the_mbid_and_normalizes() {
        let t = RemoteTrack {
            isrc: Some(" usrc17607839 ".into()),
            musicbrainz_recording_id: Some(" ABC-123 ".into()),
            ..Default::default()
        };
        assert_eq!(t.fingerprint().as_deref(), Some("mbid:abc-123"));

        let solo_isrc = RemoteTrack { isrc: Some("usrc17607839".into()), ..Default::default() };
        assert_eq!(solo_isrc.fingerprint().as_deref(), Some("isrc:USRC17607839"));

        // Sin nada firme no hay huella: emparejar por nombre es no emparejar.
        assert_eq!(RemoteTrack::default().fingerprint(), None);
        let blank = RemoteTrack {
            isrc: Some("  ".into()),
            musicbrainz_recording_id: Some("".into()),
            ..Default::default()
        };
        assert_eq!(blank.fingerprint(), None);
    }

    #[test]
    fn a_half_known_quality_is_described_as_half_known() {
        assert_eq!(Quality::default().describe(), "");
        let c = Quality { codec: Some("flac".into()), bit_depth: Some(24), sample_rate_hz: Some(96_000), channels: Some(2) };
        assert_eq!(c.describe(), "FLAC · 24 bits a 96000 Hz");
        let solo_rate = Quality { sample_rate_hz: Some(44_100), ..Default::default() };
        assert_eq!(solo_rate.describe(), "44100 Hz");
    }

    #[test]
    fn a_track_can_come_in_several_files_read_in_order() {
        // Es el caso normal en hi-res: el manifiesto del servicio trae
        // varias URLs completas. El plugin lo decodifica y entrega la
        // lista; Chilpa no sabe de manifiestos.
        let s: Stream = read(
            br#"{"urls":["https://a.b/1.flac","https://a.b/2.flac"],"codec":"flac"}"#,
        )
        .unwrap();
        assert_eq!(s.parts(), ["https://a.b/1.flac", "https://a.b/2.flac"]);
        assert!(s.usable());

        // Una sola sigue escribiéndose como siempre.
        let one: Stream = read(br#"{"url":"https://a.b/x.flac"}"#).unwrap();
        assert_eq!(one.parts(), ["https://a.b/x.flac"]);
    }

    #[test]
    fn one_bad_part_spoils_the_whole_stream() {
        // Basta con que una no sea http para no intentar ninguna: leer la
        // mitad de una pista y fallar es peor que no empezar.
        let s: Stream =
            read(br#"{"urls":["https://a.b/1.flac","file:///etc/passwd"]}"#).unwrap();
        assert!(!s.usable());
        assert!(read::<Stream>(br#"{"codec":"flac"}"#).unwrap().parts().is_empty());
    }

    #[test]
    fn a_stream_that_is_not_http_is_not_attempted() {
        let bad = Stream { url: "file:///etc/passwd".into(), ..Default::default() };
        assert!(!bad.usable());
        let good = Stream { url: "https://a.b/c".into(), ..Default::default() };
        assert!(good.usable());
    }

    #[test]
    fn a_link_with_a_date_expires() {
        let s = Stream {
            url: "https://a.b/c".into(),
            expires_at: Some(1000),
            ..Default::default()
        };
        assert!(!s.expired(999));
        assert!(s.expired(1000), "justo al filo ya no sirve");
        assert!(s.expired(1001));
        // Sin fecha no caduca.
        let undated = Stream { expires_at: None, ..s };
        assert!(!undated.expired(i64::MAX));
    }

    #[test]
    fn what_a_plugin_answers_is_read_or_it_says_why_not() {
        let r: Results = read(br#"{"tracks":[{"external_id":"a","title":"Una"}]}"#).unwrap();
        assert_eq!(r.tracks.len(), 1);
        assert_eq!(r.tracks[0].artist, "", "lo que falta toma su valor por omisión");

        let e = read::<Results>(b"esto no es json").expect_err("no se entiende");
        assert!(e.contains("no se entiende"), "{e}");
    }

    #[test]
    fn one_extra_field_does_not_break_the_read() {
        // Un plugin de una versión más nueva puede mandar campos que este
        // Chilpa no conoce; se ignoran en vez de rechazar todo.
        let r: Results =
            read(br#"{"tracks":[{"external_id":"a","title":"Una","algo_nuevo":42}]}"#).unwrap();
        assert_eq!(r.tracks.len(), 1);
    }
}
