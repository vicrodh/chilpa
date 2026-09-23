//! Las tablas de contenido: qué pistas tiene el disco y dónde viven.

use crate::{be16, be32, timecode_frames, Encoding, SacdError, SECTOR};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Dónde vive la tabla maestra, con sus dos respaldos.
const MASTER_TOC_LSN: [u64; 3] = [510, 520, 530];
const MAX_AREA_TOC_SECTORS: u16 = 96;

/// Una pista del área estéreo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SacdTrack {
    pub number: u8,
    pub start_lsn: u32,
    pub length_lsn: u32,
    /// Duración en frames de 1/75 de segundo.
    pub duration_frames: u32,
    pub encoding: Encoding,
}

impl SacdTrack {
    pub fn duration_secs(&self) -> f64 {
        f64::from(self.duration_frames) / 75.0
    }
}

/// El área estéreo del disco.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SacdArea {
    pub tracks: Vec<SacdTrack>,
    pub encoding: Encoding,
    /// Primer y último sector del área de audio.
    pub audio_start_lsn: u32,
    pub audio_end_lsn: u32,
}

/// La imagen abierta, con su área estéreo ya leída.
pub struct SacdImage {
    pub(crate) file: BufReader<File>,
    pub(crate) sectors: u64,
    pub area: SacdArea,
}

impl SacdImage {
    pub(crate) fn read_sector(&mut self, lsn: u64, into: &mut [u8]) -> Result<(), SacdError> {
        if lsn >= self.sectors {
            return Err(SacdError::BadSector { lsn, why: "fuera de la imagen".into() });
        }
        self.file.seek(SeekFrom::Start(lsn * SECTOR as u64))?;
        self.file.read_exact(into)?;
        Ok(())
    }
}

/// Abre una imagen de SACD y lee su área estéreo.
pub fn open(path: &Path) -> Result<SacdImage, SacdError> {
    let file = File::open(path)?;
    let sectors = file.metadata()?.len() / SECTOR as u64;
    if sectors <= MASTER_TOC_LSN[0] {
        return Err(SacdError::MalformedToc("la imagen es más corta que su tabla maestra".into()));
    }
    let mut image = SacdImage {
        file: BufReader::new(file),
        sectors,
        area: SacdArea {
            tracks: Vec::new(),
            encoding: Encoding::Dsd3In14,
            audio_start_lsn: 0,
            audio_end_lsn: 0,
        },
    };

    let mut sector = vec![0u8; SECTOR];
    let master = MASTER_TOC_LSN
        .iter()
        .find_map(|&lsn| {
            image.read_sector(lsn, &mut sector).ok()?;
            (&sector[..8] == b"SACDMTOC").then(|| sector.clone())
        })
        .ok_or_else(|| SacdError::MalformedToc("no se encontró SACDMTOC".into()))?;

    let version = (master[8], master[9]);
    if version.0 != 1 {
        return Err(SacdError::Unsupported(format!("Master TOC versión {}.{}", version.0, version.1)));
    }

    // Dos ranuras de área: la estéreo y la multicanal. Se prueba cada copia.
    let ranuras = [
        (be32(&master, 0x40), be32(&master, 0x44), be16(&master, 0x54)),
        (be32(&master, 0x48), be32(&master, 0x4c), be16(&master, 0x56)),
    ];
    let mut area_toc = None;
    for (primaria, respaldo, tamaño) in ranuras {
        if tamaño == 0 || tamaño > MAX_AREA_TOC_SECTORS {
            continue;
        }
        for inicio in [primaria, respaldo].into_iter().filter(|s| *s != 0) {
            let mut toc = vec![0u8; SECTOR * tamaño as usize];
            let mut ok = true;
            for (i, trozo) in toc.chunks_mut(SECTOR).enumerate() {
                if image.read_sector(u64::from(inicio) + i as u64, trozo).is_err() {
                    ok = false;
                    break;
                }
            }
            if ok && &toc[..8] == b"TWOCHTOC" {
                area_toc = Some(toc);
                break;
            }
        }
        if area_toc.is_some() {
            break;
        }
    }
    let toc = area_toc
        .ok_or_else(|| SacdError::MalformedToc("no se encontró el área estéreo (TWOCHTOC)".into()))?;

    image.area = parse_area(&toc, image.sectors)?;
    Ok(image)
}

fn parse_area(toc: &[u8], sectors: u64) -> Result<SacdArea, SacdError> {
    let canales = toc[0x20];
    if canales != 2 || toc[0x21] & 0x1f != 0 {
        return Err(SacdError::Unsupported(format!("{canales} canales en el área estéreo")));
    }
    if toc[0x14] != 4 {
        return Err(SacdError::Unsupported(format!("frecuencia código {}", toc[0x14])));
    }
    let encoding = match toc[0x15] & 0x0f {
        0 => Encoding::Dst,
        2 => Encoding::Dsd3In14,
        3 => Encoding::Dsd3In16,
        otro => return Err(SacdError::Unsupported(format!("formato de frame {otro}"))),
    };
    let track_count = toc[0x45] as usize;
    if track_count == 0 {
        return Err(SacdError::MalformedToc("el área no declara pistas".into()));
    }
    let audio_start_lsn = be32(toc, 0x48);
    let audio_end_lsn = be32(toc, 0x4c);
    if audio_start_lsn > audio_end_lsn || u64::from(audio_end_lsn) >= sectors {
        return Err(SacdError::MalformedToc("el área de audio cae fuera de la imagen".into()));
    }

    let trl1 = find_block(toc, b"SACDTRL1")
        .ok_or_else(|| SacdError::MalformedToc("falta SACDTRL1".into()))?;
    let trl2 = find_block(toc, b"SACDTRL2")
        .ok_or_else(|| SacdError::MalformedToc("falta SACDTRL2".into()))?;
    // Cada bloque trae dos arreglos de 255 entradas de 4 bytes.
    const SEGUNDO_ARREGLO: usize = 8 + 255 * 4;

    let mut tracks = Vec::with_capacity(track_count);
    let mut anterior: Option<u32> = None;
    for i in 0..track_count {
        let start_lsn = be32(trl1, 8 + i * 4);
        let length_lsn = be32(trl1, SEGUNDO_ARREGLO + i * 4);
        let duration_frames = timecode_frames(trl2, SEGUNDO_ARREGLO + i * 4);
        if trl2[8 + i * 4 + 1] >= 60
            || trl2[8 + i * 4 + 2] >= 75
            || trl2[SEGUNDO_ARREGLO + i * 4 + 1] >= 60
            || trl2[SEGUNDO_ARREGLO + i * 4 + 2] >= 75
        {
            return Err(SacdError::MalformedToc(format!("código de tiempo inválido en la pista {}", i + 1)));
        }
        let fin = u64::from(start_lsn) + u64::from(length_lsn);
        if length_lsn == 0
            || start_lsn < audio_start_lsn
            || fin > u64::from(audio_end_lsn) + 1
            || anterior.is_some_and(|previo| start_lsn <= previo)
        {
            return Err(SacdError::MalformedToc(format!(
                "la pista {} cae fuera del área o desordenada",
                i + 1
            )));
        }
        anterior = Some(start_lsn);
        tracks.push(SacdTrack {
            number: (i + 1) as u8,
            start_lsn,
            length_lsn,
            duration_frames,
            encoding,
        });
    }

    Ok(SacdArea { tracks, encoding, audio_start_lsn, audio_end_lsn })
}

/// Busca un bloque con esa firma dentro de la tabla del área.
fn find_block<'a>(toc: &'a [u8], firma: &[u8; 8]) -> Option<&'a [u8]> {
    toc.chunks(SECTOR).find(|s| s.len() >= 8 && &s[..8] == firma)
}
