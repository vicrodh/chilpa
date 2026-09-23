//! La tabla de contenido de un CD, y las cuentas que hay que hacerle.
//!
//! El TOC no trae duraciones: cada pista mide lo que hay hasta la siguiente,
//! y la última hasta el lead-out. Eso es correcto en un disco de puro audio y
//! **está mal en un CD-Extra**, donde después del audio viene una sesión de
//! datos: entre las dos sesiones hay un hueco de 11 400 sectores, unos 152
//! segundos, que no es música.
//!
//! La auditoría de QBZ encontró ese error: la última pista de audio llegaba
//! hasta el arranque de la pista de datos, así que el rip fallaba y el disco
//! daba identificadores distintos entre sistemas.

/// Sectores por segundo.
pub const SECTORS_PER_SEC: u32 = 75;
/// Hueco entre dos sesiones: lead-out de la primera más lead-in de la
/// segunda.
pub const SESSION_GAP_SECTORS: u32 = 11_400;
/// Bytes de un sector de audio: 588 frames estéreo de 16 bits.
pub const SECTOR_BYTES: usize = 2352;

/// Una entrada del TOC tal como la entrega la unidad.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TocEntry {
    pub number: u8,
    pub start_lsn: u32,
    pub is_audio: bool,
    /// En qué sesión vive. Un disco de puro audio trae todo en la 1.
    pub session: u8,
}

/// Una pista con su duración ya calculada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Track {
    pub number: u8,
    pub start_lsn: u32,
    pub sectors: u32,
    pub is_audio: bool,
    pub session: u8,
}

impl Track {
    pub fn seconds(&self) -> f64 {
        f64::from(self.sectors) / f64::from(SECTORS_PER_SEC)
    }

    pub fn end_lsn(&self) -> u32 {
        self.start_lsn + self.sectors
    }
}

/// El disco completo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Toc {
    pub tracks: Vec<Track>,
    pub leadout_lsn: u32,
}

impl Toc {
    pub fn audio_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.is_audio)
    }

    pub fn track(&self, number: u8) -> Option<&Track> {
        self.tracks.iter().find(|t| t.number == number)
    }

    /// Cierto si el disco mezcla audio y datos, o sea un CD-Extra o un disco
    /// de modo mixto.
    pub fn is_mixed(&self) -> bool {
        self.tracks.iter().any(|t| t.is_audio) && self.tracks.iter().any(|t| !t.is_audio)
    }
}

/// Arma el TOC calculando duraciones.
///
/// Las entradas vienen en orden. Cuando la pista siguiente está en otra
/// sesión, se descuenta el hueco entre sesiones en vez de contarlo como
/// música.
pub fn build(entries: &[TocEntry], leadout_lsn: u32) -> Toc {
    let mut tracks = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let next = entries.get(i + 1);
        let fin = match next {
            Some(s) if s.session != e.session => s.start_lsn.saturating_sub(SESSION_GAP_SECTORS),
            Some(s) => s.start_lsn,
            None => leadout_lsn,
        };
        tracks.push(Track {
            number: e.number,
            start_lsn: e.start_lsn,
            sectors: fin.saturating_sub(e.start_lsn),
            is_audio: e.is_audio,
            session: e.session,
        });
    }
    Toc { tracks, leadout_lsn }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(number: u8, start: u32, session: u8) -> TocEntry {
        TocEntry { number, start_lsn: start, is_audio: true, session }
    }

    fn data(number: u8, start: u32, session: u8) -> TocEntry {
        TocEntry { number, start_lsn: start, is_audio: false, session }
    }

    #[test]
    fn cada_pista_mide_hasta_la_siguiente_y_la_ultima_hasta_el_lead_out() {
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1)], 30_000);
        assert_eq!(toc.tracks[0].sectors, 15_000);
        assert_eq!(toc.tracks[1].sectors, 15_000);
        assert_eq!(toc.tracks[1].end_lsn(), 30_000);
        assert!(!toc.is_mixed());
    }

    #[test]
    fn en_un_cd_extra_la_ultima_de_audio_no_se_come_el_hueco_entre_sesiones() {
        // El hallazgo de la auditoría: sin descontar el hueco, la pista 2
        // medía 152 segundos de más y el rip fallaba.
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1), data(3, 41_400, 2)], 60_000);
        assert_eq!(toc.tracks[1].sectors, 41_400 - SESSION_GAP_SECTORS - 15_000);
        assert_eq!(toc.tracks[1].end_lsn(), 30_000);
        assert!(toc.is_mixed());
    }

    #[test]
    fn un_disco_de_modo_mixto_en_una_sola_sesion_no_descuenta_nada() {
        // Datos primero y audio después, todo en la sesión 1: aquí no hay
        // hueco que descontar.
        let toc = build(&[data(1, 0, 1), audio(2, 20_000, 1)], 40_000);
        assert_eq!(toc.tracks[0].sectors, 20_000);
        assert_eq!(toc.tracks[1].sectors, 20_000);
    }

    #[test]
    fn la_duracion_sale_en_segundos_a_75_sectores() {
        let toc = build(&[audio(1, 0, 1)], 750);
        assert_eq!(toc.tracks[0].seconds(), 10.0);
    }

    #[test]
    fn se_listan_solo_las_pistas_de_audio() {
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1), data(3, 41_400, 2)], 60_000);
        assert_eq!(toc.audio_tracks().count(), 2);
        assert_eq!(toc.track(3).unwrap().is_audio, false);
    }
}
