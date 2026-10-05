//! A small disc with a complete stereo table and playable audio.

const SECTOR: usize = 2048;
const AREA: u32 = 540;
const AUDIO_START: u32 = 550;
const AUDIO_END: u32 = 569;

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn sector(img: &mut [u8], lsn: u32) -> &mut [u8] {
    let at = lsn as usize * SECTOR;
    &mut img[at..at + SECTOR]
}

/// A two-track SACD image with the text a pressed disc carries, its track
/// titles in the second sector of the SACDTTxt block as on real discs.
pub fn image(path: &std::path::Path) {
    let mut img = vec![0u8; 600 * SECTOR];
    {
        let m = sector(&mut img, 510);
        m[..8].copy_from_slice(b"SACDMTOC");
        m[8] = 1;
        m[9] = 20;
        m[0x40..0x44].copy_from_slice(&be32(AREA));
        m[0x54..0x56].copy_from_slice(&5u16.to_be_bytes());
        m[0x80] = 1;
        m[0x8a] = 2;
    }
    {
        let t = sector(&mut img, 511);
        t[..8].copy_from_slice(b"SACDText");
        t[0x10..0x12].copy_from_slice(&0x40u16.to_be_bytes());
        t[0x12..0x14].copy_from_slice(&0x80u16.to_be_bytes());
        t[0x40..0x40 + 18].copy_from_slice(b"Agents Of Fortune\0");
        t[0x80..0x80 + 17].copy_from_slice(b"Blue \xD6yster Cult\0");
    }
    {
        let a = sector(&mut img, AREA);
        a[..8].copy_from_slice(b"TWOCHTOC");
        a[0x14] = 4;
        a[0x15] = 2;
        a[0x20] = 2;
        a[0x41] = 2;
        a[0x45] = 2;
        a[0x48..0x4c].copy_from_slice(&be32(AUDIO_START));
        a[0x4c..0x50].copy_from_slice(&be32(AUDIO_END));
        a[0x5a] = 2;
        a[0x80..0x82].copy_from_slice(&3u16.to_be_bytes());
    }
    const SECOND: usize = 8 + 255 * 4;
    {
        let t1 = sector(&mut img, AREA + 1);
        t1[..8].copy_from_slice(b"SACDTRL1");
        t1[8..12].copy_from_slice(&be32(AUDIO_START));
        t1[12..16].copy_from_slice(&be32(AUDIO_START + 10));
        t1[SECOND..SECOND + 4].copy_from_slice(&be32(10));
        t1[SECOND + 4..SECOND + 8].copy_from_slice(&be32(10));
    }
    {
        let t2 = sector(&mut img, AREA + 2);
        t2[..8].copy_from_slice(b"SACDTRL2");
        t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
        // Two frames each: what ten sectors of 2,042 bytes of audio hold.
        t2[SECOND..SECOND + 4].copy_from_slice(&[0, 0, 2, 0]);
        t2[SECOND + 4..SECOND + 8].copy_from_slice(&[0, 0, 2, 0]);
    }
    {
        let b = sector(&mut img, AREA + 3);
        b[..8].copy_from_slice(b"SACDTTxt");
        b[8..10].copy_from_slice(&(SECTOR as u16 + 0x20).to_be_bytes());
        b[10..12].copy_from_slice(&(SECTOR as u16 + 0x80).to_be_bytes());
    }
    {
        let b = sector(&mut img, AREA + 4);
        b[0x20] = 1;
        b[0x24] = 1;
        b[0x26..0x26 + 30].copy_from_slice(b"This Ain't The Summer Of Love\0");
        b[0x80] = 1;
        b[0x84] = 1;
        b[0x86..0x86 + 17].copy_from_slice(b"True Confessions\0");
    }
    for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
        let s = sector(&mut img, lsn);
        s[0] = (1 << 5) | (1 << 2);
        let header = 1 + 2 + 3;
        let len = SECTOR - header;
        let w: u16 = 0x8000 | (2 << 11) | len as u16;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        s[4] = (i / 75) as u8;
        s[5] = (i % 75) as u8;
        for b in &mut s[header..] {
            *b = 0x69;
        }
    }
    std::fs::write(&path, img).unwrap();
}

