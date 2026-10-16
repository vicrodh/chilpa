//! The whole listening history written out as CSV, newest first, for a
//! spreadsheet: one row a listen, with the names it was heard under.

use chilpa_i18n as i18n;
use chilpa_library::store::listening::ListenFilter;
use std::io::Write;
use std::path::Path;

/// How many listens are read from the log at a time.
const CHUNK: usize = 1000;

/// A cell as CSV carries it: quoted when it holds a comma, a quote or a
/// line break. A cell a spreadsheet would read as a formula starts with a
/// quote mark, so a track called "=1+1" stays a name.
fn cell(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{text}") } else { text.to_string() };
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn line(cells: &[String]) -> String {
    let mut out = cells.iter().map(|c| cell(c)).collect::<Vec<_>>().join(",");
    out.push_str("\r\n");
    out
}

impl crate::Chilpa {
    /// Writes every listen to `to` and says how many went; on failure the
    /// window's error says why.
    pub fn export_listening_history(&mut self, to: &Path) -> Option<String> {
        let Some(count) = self.write_history(to) else {
            self.now.error = i18n::t("The listening history could not be saved");
            return None;
        };
        Some(i18n::tf("{} listen saved", "{} listens saved", count as i64, &[&count.to_string()]))
    }

    /// Where the stats screen leaves its picture before asking where it
    /// goes.
    pub fn stats_picture_path(&self) -> std::path::PathBuf {
        chilpa_dirs::cache().join("listening-stats.png")
    }

    fn write_history(&mut self, to: &Path) -> Option<u64> {
        let lib = self.library()?;
        let yes = i18n::t("Yes");
        let no = i18n::t("No");
        let mut file = std::io::BufWriter::new(std::fs::File::create(to).ok()?);
        let head = [
            i18n::mark("Date"), i18n::mark("Time"), i18n::mark("Track"), i18n::mark("Artist"), i18n::mark("Album"),
            i18n::mark("Heard"), i18n::mark("Length"), i18n::mark("Skipped"), i18n::mark("Quality"), i18n::mark("Bit-perfect"),
        ];
        file.write_all(line(&head.map(i18n::t)).as_bytes()).ok()?;
        let mut count = 0;
        loop {
            let (rows, _) = lib.listening_history(&ListenFilter::default(), "", count as usize, CHUNK).ok()?;
            for e in &rows {
                let cells = [
                    e.local_time.get(..10).unwrap_or_default().to_string(),
                    e.local_time.get(11..16).unwrap_or_default().to_string(),
                    e.title.clone(),
                    e.artist.clone(),
                    e.album.clone(),
                    crate::format::clock(e.played_ms),
                    if e.duration_ms > 0 { crate::format::clock(e.duration_ms) } else { String::new() },
                    if e.skipped { yes.clone() } else { no.clone() },
                    crate::listening_stats::quality(e.bit_depth, e.sample_rate_hz),
                    if e.bit_perfect { yes.clone() } else { no.clone() },
                ];
                file.write_all(line(&cells).as_bytes()).ok()?;
            }
            count += rows.len() as u64;
            if rows.len() < CHUNK {
                break;
            }
        }
        file.flush().ok()?;
        Some(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_quoted_when_it_must_be_and_never_reads_as_a_formula() {
        assert_eq!(cell("One"), "One");
        assert_eq!(cell("Live, 1977"), "\"Live, 1977\"");
        assert_eq!(cell("The \"Best\""), "\"The \"\"Best\"\"\"");
        assert_eq!(cell("=1+1"), "'=1+1");
        assert_eq!(cell("-ism"), "'-ism");
        assert_eq!(line(&["a".into(), "b".into()]), "a,b\r\n");
    }
}
