//! El formato LRC.
//!
//! Un archivo de texto donde cada renglón empieza con uno o varios sellos de
//! tiempo: `[01:23.45]Y así canta`. Lo que se cuida aquí:
//!
//! - **Un renglón puede traer varios sellos.** Un estribillo se escribe una
//!   vez con todas sus horas; tratar solo la primera pierde el resto de la
//!   canción.
//! - **Los sellos vienen de todo tipo**: `mm:ss`, `mm:ss.xx` (centésimas) y
//!   `mm:ss.xxx` (milésimas). Leer `.5` como 5 ms en vez de 500 deja la letra
//!   medio segundo adelantada.
//! - **Las etiquetas de cabecera** (`[ti:]`, `[ar:]`, `[offset:]`) no son
//!   renglones de letra y no deben aparecer en pantalla.
//! - **Un archivo sin un solo sello sigue siendo una letra**, nomás que sin
//!   sincronizar. Se muestra completa.

use crate::{Lyrics, Line};

/// Lee una letra en LRC, o en texto plano si no trae sellos.
pub fn parse(text: &str) -> Lyrics {
    let mut lines: Vec<Line> = Vec::new();
    let mut loose: Vec<String> = Vec::new();
    let mut offset_ms: i64 = 0;
    let mut title = None;
    let mut artist = None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let (labels, rest) = labels_of(line);
        if labels.is_empty() {
            // Puede ser una etiqueta de cabecera o texto sin sincronizar.
            match tag(line) {
                Some(("offset", v)) => {
                    offset_ms = v.trim().parse().unwrap_or(0);
                }
                Some(("ti", v)) => title = Some(v.trim().to_string()),
                Some(("ar", v)) => artist = Some(v.trim().to_string()),
                // Cualquier otra etiqueta conocida se calla; no es letra.
                Some(_) => {}
                None => {
                    if !line.trim().is_empty() {
                        loose.push(line.trim().to_string());
                    }
                }
            }
            continue;
        }
        let text = rest.trim().to_string();
        for at in labels {
            lines.push(Line { at_ms: at, text: text.clone() });
        }
    }

    if lines.is_empty() {
        return Lyrics {
            title,
            artist,
            lines: loose.into_iter().map(|t| Line { at_ms: 0, text: t }).collect(),
            synced: false,
        };
    }

    // El offset del archivo corre la letra entera. Es positivo cuando hay
    // que adelantarla, así que se resta.
    if offset_ms != 0 {
        for l in &mut lines {
            *l = Line {
                at_ms: (l.at_ms as i64 - offset_ms).max(0) as u64,
                text: std::mem::take(&mut l.text),
            };
        }
    }
    lines.sort_by_key(|l| l.at_ms);
    Lyrics { title, artist, lines: lines, synced: true }
}

/// Los sellos con los que empieza un renglón, y lo que queda después.
fn labels_of(line: &str) -> (Vec<u64>, &str) {
    let mut labels = Vec::new();
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start();
        let Some(close) = trimmed.find(']') else { break };
        if !trimmed.starts_with('[') {
            break;
        }
        let Some(ms) = time(&trimmed[1..close]) else { break };
        labels.push(ms);
        rest = &trimmed[close + 1..];
    }
    (labels, rest)
}

/// `mm:ss`, `mm:ss.xx` o `mm:ss.xxx` en milisegundos.
fn time(s: &str) -> Option<u64> {
    let (min, rest) = s.split_once(':')?;
    let minutes: u64 = min.trim().parse().ok()?;
    let (seg, fraction) = match rest.split_once(['.', ':']) {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let seconds: u64 = seg.trim().parse().ok()?;
    let millis = match fraction {
        None => 0,
        Some(f) => {
            let f = f.trim();
            if !f.chars().all(|c| c.is_ascii_digit()) || f.is_empty() {
                return None;
            }
            // Dos dígitos son centésimas, tres son milésimas. Leer ".5"
            // como 5 ms dejaría la letra medio segundo adelantada.
            let n: u64 = f.parse().ok()?;
            match f.len() {
                1 => n * 100,
                2 => n * 10,
                _ => n,
            }
        }
    };
    Some(minutes * 60_000 + seconds * 1_000 + millis)
}

/// `[clave:valor]` de cabecera, si el renglón es eso.
fn tag(line: &str) -> Option<(&str, &str)> {
    let l = line.trim();
    let inside = l.strip_prefix('[')?.strip_suffix(']')?;
    let (clave, valor) = inside.split_once(':')?;
    let clave = clave.trim();
    // Solo letras: así `[01:23]` no pasa por etiqueta.
    if clave.is_empty() || !clave.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((clave, valor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timestamps_are_read() {
        assert_eq!(time("01:02"), Some(62_000));
        assert_eq!(time("00:01.50"), Some(1_500), "dos dígitos son centésimas");
        assert_eq!(time("00:01.500"), Some(1_500), "tres son milésimas");
        assert_eq!(time("00:01.5"), Some(1_500), "uno son décimas");
        assert_eq!(time("ti:Algo"), None);
    }

    #[test]
    fn synced_lyrics_end_up_in_order() {
        let l = parse("[00:12.00]Uno\n[00:05.50]Cero\n");
        assert!(l.synced);
        assert_eq!(l.lines[0].text, "Cero");
        assert_eq!(l.lines[0].at_ms, 5_500);
        assert_eq!(l.lines[1].at_ms, 12_000);
    }

    #[test]
    fn a_line_with_several_stamps_comes_out_several_times() {
        let l = parse("[00:10.00][01:10.00][02:10.00]Estribillo\n");
        assert_eq!(l.lines.len(), 3, "el estribillo suena tres veces");
        assert_eq!(
            l.lines.iter().map(|x| x.at_ms).collect::<Vec<_>>(),
            vec![10_000, 70_000, 130_000]
        );
        assert!(l.lines.iter().all(|x| x.text == "Estribillo"));
    }

    #[test]
    fn headers_are_not_lyrics() {
        let l = parse("[ti:Canción]\n[ar:Alguien]\n[al:Disco]\n[00:01.00]Ahí va\n");
        assert_eq!(l.title.as_deref(), Some("Canción"));
        assert_eq!(l.artist.as_deref(), Some("Alguien"));
        assert_eq!(l.lines.len(), 1, "solo el renglón con sello");
        assert_eq!(l.lines[0].text, "Ahí va");
    }

    #[test]
    fn the_offset_shifts_the_whole_lyric() {
        let l = parse("[offset:500]\n[00:10.00]Tarde\n");
        assert_eq!(l.lines[0].at_ms, 9_500, "medio segundo antes");
        // Y no se va a negativo.
        let l = parse("[offset:99000]\n[00:10.00]Tarde\n");
        assert_eq!(l.lines[0].at_ms, 0);
    }

    #[test]
    fn text_without_stamps_is_still_lyrics() {
        let l = parse("Primera\n\nSegunda\n");
        assert!(!l.synced, "no está sincronizada");
        assert_eq!(l.lines.len(), 2, "y los renglones vacíos no cuentan");
        assert_eq!(l.lines[1].text, "Segunda");
    }

    #[test]
    fn a_line_without_text_stays_as_a_pause() {
        let l = parse("[00:01.00]Algo\n[00:05.00]\n[00:09.00]Más\n");
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[1].text, "", "el silencio también es parte de la letra");
    }
}
