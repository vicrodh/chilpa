//! A SACD image built by hand: master table, stereo area with two tracks,
//! and audio sectors with their real packing.

use chilpa_dsd::DsdSource;
use chilpa_sacd::{open, Encoding, SacdError, SacdTrackReader, DATA_TYPE_AUDIO, SECTOR};
use std::path::PathBuf;

const AREA_TOC_LSN: u32 = 540;
const AUDIO_START: u32 = 550;
const AUDIO_END: u32 = 569;
const SECTORS: usize = 600;

struct TestImage(Vec<u8>);

impl TestImage {
    fn new() -> Self {
        Self(vec![0u8; SECTORS * SECTOR])
    }

    fn sector(&mut self, lsn: u32) -> &mut [u8] {
        let at = lsn as usize * SECTOR;
        &mut self.0[at..at + SECTOR]
    }

    fn write(&self, name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-sacd-{name}.iso"));
        std::fs::write(&p, &self.0).unwrap();
        p
    }
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// Builds an image with two tracks; `dst` marks the area as compressed.
fn build(dst: bool) -> TestImage {
    let mut img = TestImage::new();

    {
        let m = img.sector(510);
        m[..8].copy_from_slice(b"SACDMTOC");
        m[8] = 1; // version 1.20
        m[9] = 20;
        // The stereo area slot: primary, backup and size in sectors.
        m[0x40..0x44].copy_from_slice(&be32(AREA_TOC_LSN));
        m[0x44..0x48].copy_from_slice(&be32(0));
        m[0x54..0x56].copy_from_slice(&3u16.to_be_bytes());
    }

    {
        let a = img.sector(AREA_TOC_LSN);
        a[..8].copy_from_slice(b"TWOCHTOC");
        a[0x14] = 4; // DSD64
        a[0x15] = if dst { 0 } else { 2 };
        a[0x20] = 2; // two channels
        a[0x21] = 0;
        a[0x40] = 0; // area duration: 0:02:00
        a[0x41] = 2;
        a[0x42] = 0;
        a[0x45] = 2; // two tracks
        a[0x48..0x4c].copy_from_slice(&be32(AUDIO_START));
        a[0x4c..0x50].copy_from_slice(&be32(AUDIO_END));
    }

    const SECOND_ARRAY: usize = 8 + 255 * 4;
    {
        let t1 = img.sector(AREA_TOC_LSN + 1);
        t1[..8].copy_from_slice(b"SACDTRL1");
        // Track 1: sectors 550..559. Track 2: 560..569.
        t1[8..12].copy_from_slice(&be32(AUDIO_START));
        t1[12..16].copy_from_slice(&be32(AUDIO_START + 10));
        t1[SECOND_ARRAY..SECOND_ARRAY + 4].copy_from_slice(&be32(10));
        t1[SECOND_ARRAY + 4..SECOND_ARRAY + 8].copy_from_slice(&be32(10));
    }
    {
        let t2 = img.sector(AREA_TOC_LSN + 2);
        t2[..8].copy_from_slice(b"SACDTRL2");
        // Starts at 0:00:00 and 0:01:00; one second each.
        t2[8..12].copy_from_slice(&[0, 0, 0, 0]);
        t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND_ARRAY..SECOND_ARRAY + 4].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND_ARRAY + 4..SECOND_ARRAY + 4 + 4].copy_from_slice(&[0, 1, 0, 0]);
    }

    // Audio sectors: one per sector, with its frame-info record.
    for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
        let s = img.sector(lsn);
        s[0] = (1 << 5) | (1 << 2) | u8::from(dst);
        let info = if dst { 4 } else { 3 };
        let header_len = 1 + 2 + info;
        let len = SECTOR - header_len;
        let w: u16 = 0x8000 | (DATA_TYPE_AUDIO << 11) | len as u16;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        s[3] = 0;
        s[4] = (i / 75) as u8;
        s[5] = (i % 75) as u8;
        // The payload: byte n of the left channel is n and the right one its
        // complement, interleaved byte by byte.
        for (k, b) in s[header_len..].iter_mut().enumerate() {
            let sample = ((i * len + k) / 2 % 251) as u8;
            *b = if k % 2 == 0 { sample } else { !sample };
        }
    }
    img
}

#[test]
fn both_tracks_of_the_stereo_area_are_read() {
    let path = build(false).write("two-tracks");
    let img = open(&path).unwrap();
    assert_eq!(img.area.encoding, Encoding::Dsd3In14);
    assert_eq!(img.area.tracks.len(), 2);
    assert_eq!(img.area.tracks[0].number, 1);
    assert_eq!(img.area.tracks[0].start_lsn, AUDIO_START);
    assert_eq!(img.area.tracks[1].start_lsn, AUDIO_START + 10);
    assert_eq!(img.area.tracks[0].duration_secs(), 1.0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_track_delivers_stereo_dsd64_with_the_channels_split() {
    let path = build(false).write("audio");
    let img = open(&path).unwrap();
    let track = img.area.tracks[0].clone();
    let mut reader = SacdTrackReader::open(img, &track).unwrap();

    assert_eq!(reader.info().dsd_rate_hz, 2_822_400, "DSD64");
    assert_eq!(reader.info().channels, 2);

    let block = reader.next_block().unwrap().expect("there is audio");
    assert_eq!(block.len(), 2);
    assert_eq!(block[0].len(), 9408 / 2, "half a frame per channel");
    // The left one carries the sample and the right one its complement.
    for i in 0..16 {
        assert_eq!(block[1][i], !block[0][i], "the channels were not scrambled");
    }
    let _ = std::fs::remove_file(path);
}

/// A DST frame that is not coded: a first byte of zero says so, and the
/// 9408 bytes after it are the frame as it plays. It says what a DST track
/// must sound like without a DST encoder at hand. The left channel is
/// `left` throughout and the right one its complement.
fn plain_dst_frame(left: u8) -> Vec<u8> {
    let mut frame = vec![0u8];
    for _ in 0..9408 / 2 {
        frame.extend_from_slice(&[left, !left]);
    }
    frame
}

/// Lays `frames` out from the area's first sector as a DST disc does: each
/// frame begins with a packet whose record gives its timecode and how many
/// packets it spans, a sector packs as many packets as fit, and what is left
/// after its last packet is zeros. Gives back the sector each frame starts
/// in and the last sector written.
fn lay_dst(img: &mut TestImage, frames: &[(u32, Vec<u8>)]) -> (Vec<u32>, u32) {
    // Where every packet goes first: the record of a frame's first packet
    // says how many packets the whole frame took.
    let mut sectors: Vec<Vec<(usize, std::ops::Range<usize>)>> = Vec::new();
    let (mut f, mut off) = (0, 0);
    while f < frames.len() {
        let mut packets = Vec::new();
        let mut used = 1;
        while f < frames.len() && packets.len() < 7 {
            let cost = if off == 0 { 2 + 4 } else { 2 };
            if used + cost >= SECTOR {
                break;
            }
            let take = (frames[f].1.len() - off).min(SECTOR - used - cost);
            packets.push((f, off..off + take));
            used += cost + take;
            off += take;
            if off == frames[f].1.len() {
                f += 1;
                off = 0;
            }
        }
        sectors.push(packets);
    }
    let spans: Vec<u8> =
        (0..frames.len()).map(|f| sectors.iter().flatten().filter(|(g, _)| *g == f).count() as u8).collect();

    let mut starts = Vec::new();
    for (i, packets) in sectors.iter().enumerate() {
        let lsn = AUDIO_START + i as u32;
        let s = img.sector(lsn);
        s.fill(0);
        let begun: Vec<usize> = packets.iter().filter(|(_, r)| r.start == 0).map(|(f, _)| *f).collect();
        s[0] = ((packets.len() as u8) << 5) | ((begun.len() as u8) << 2) | 1;
        for (j, f) in begun.iter().enumerate() {
            let t = frames[*f].0;
            let r = 1 + 2 * packets.len() + 4 * j;
            s[r..r + 4].copy_from_slice(&[(t / 75 / 60) as u8, (t / 75 % 60) as u8, (t % 75) as u8, spans[*f] << 2]);
            starts.push(lsn);
        }
        let mut at = 1 + 2 * packets.len() + 4 * begun.len();
        for (k, (f, range)) in packets.iter().enumerate() {
            let begins: u16 = if range.start == 0 { 0x8000 } else { 0 };
            let w = begins | (DATA_TYPE_AUDIO << 11) | range.len() as u16;
            s[1 + 2 * k..3 + 2 * k].copy_from_slice(&w.to_be_bytes());
            s[at..at + range.len()].copy_from_slice(&frames[*f].1[range.clone()]);
            at += range.len();
        }
    }
    (starts, AUDIO_START + sectors.len() as u32 - 1)
}

/// Sets the area's two tracks as TRL1 and TRL2 carry them: first sector,
/// how many sectors, first frame and how many frames.
fn set_tracks(img: &mut TestImage, tracks: [(u32, u32, u32, u32); 2]) {
    const SECOND_ARRAY: usize = 8 + 255 * 4;
    let timecode = |f: u32| [(f / 75 / 60) as u8, (f / 75 % 60) as u8, (f % 75) as u8, 0];
    for (i, (lsn, sectors, first, frames)) in tracks.into_iter().enumerate() {
        let t1 = img.sector(AREA_TOC_LSN + 1);
        t1[8 + i * 4..12 + i * 4].copy_from_slice(&be32(lsn));
        t1[SECOND_ARRAY + i * 4..SECOND_ARRAY + 4 + i * 4].copy_from_slice(&be32(sectors));
        let t2 = img.sector(AREA_TOC_LSN + 2);
        t2[8 + i * 4..12 + i * 4].copy_from_slice(&timecode(first));
        t2[SECOND_ARRAY + i * 4..SECOND_ARRAY + 4 + i * 4].copy_from_slice(&timecode(frames));
    }
}

/// Four DST frames at timecodes 0 to 3, the left channel of frame `k`
/// being `0x10 + k`.
fn four_frames() -> Vec<(u32, Vec<u8>)> {
    (0..4u8).map(|k| (u32::from(k), plain_dst_frame(0x10 + k))).collect()
}

/// A DST image of two tracks of two frames each. The second track's
/// sectors begin where frame `second_from` starts.
fn dst_image(frames: &[(u32, Vec<u8>)], second_from: usize) -> TestImage {
    let mut img = build(true);
    let (starts, last) = lay_dst(&mut img, frames);
    let second = starts[second_from];
    set_tracks(&mut img, [(starts[0], starts[2] - starts[0] + 1, 0, 2), (second, last - second + 1, 2, 2)]);
    img
}

/// Why a track stopped, when it failed.
fn failure(played: Result<Vec<u8>, chilpa_dsd::DsdError>) -> String {
    match played {
        Err(chilpa_dsd::DsdError::Corrupt(why)) => why,
        other => panic!("the track should have failed: {other:?}"),
    }
}

/// Plays track `number` and gives back the left byte of every frame, after
/// checking each frame came whole: that byte throughout on the left and its
/// complement on the right.
fn lefts(img: &TestImage, name: &str, number: usize) -> Result<Vec<u8>, chilpa_dsd::DsdError> {
    let path = img.write(name);
    let disc = open(&path).unwrap();
    assert_eq!(disc.area.encoding, Encoding::Dst);
    let track = disc.area.tracks[number - 1].clone();
    let mut reader = SacdTrackReader::open(disc, &track).expect("a DST track opens");
    let mut lefts = Vec::new();
    let played = loop {
        match reader.next_block() {
            Ok(Some(block)) => {
                let left = block[0][0];
                assert_eq!(block[0].len(), 9408 / 2, "half a frame per channel");
                assert!(
                    block[0].iter().all(|b| *b == left) && block[1].iter().all(|b| *b == !left),
                    "frame {left:#04x} came out whole and as it went in"
                );
                lefts.push(left);
            }
            Ok(None) => break Ok(lefts),
            Err(e) => break Err(e),
        }
    };
    let _ = std::fs::remove_file(path);
    played
}

#[test]
fn a_dst_track_plays_the_dsd_its_frames_carry() {
    let img = dst_image(&four_frames(), 2);
    assert_eq!(lefts(&img, "dst-first", 1).unwrap(), vec![0x10, 0x11]);
    assert_eq!(lefts(&img, "dst-second", 2).unwrap(), vec![0x12, 0x13]);
}

#[test]
fn a_dst_track_starts_at_its_first_frame_by_timecode() {
    // DST frames are short, so the sector a track starts in can hold the
    // start of the last frame of the track before. Its timecode says whose
    // it is; the first frame to start in the sector does not.
    let img = dst_image(&four_frames(), 1);
    assert_eq!(lefts(&img, "dst-shared", 2).unwrap(), vec![0x12, 0x13]);
}

#[test]
fn a_dst_frame_cut_short_of_its_packets_is_refused() {
    let mut img = dst_image(&four_frames(), 2);
    // The first frame's record claims one packet more than it has, so the
    // next frame starts while it is still open.
    let s = img.sector(AUDIO_START);
    let record = 1 + 2 * usize::from(s[0] >> 5);
    s[record + 3] += 1 << 2;
    let why = failure(lefts(&img, "dst-short", 1));
    assert!(why.contains("before all its packets"), "{why}");
}

#[test]
fn a_dst_frame_that_does_not_decode_is_an_error_and_not_silence() {
    // A stuffing bit set where the format wants zeros: the decoder refuses
    // it, and would hand over 0x55 silence in its place.
    let mut frames = four_frames();
    frames[0].1[0] = 0x01;
    let why = failure(lefts(&dst_image(&frames, 2), "dst-corrupt", 1));
    assert!(why.contains("does not decode"), "{why}");
}

#[test]
fn a_dst_frame_missing_from_a_track_is_an_error_and_not_a_jump() {
    // Timecodes 0, 1, 3, 4: the second track, which starts at 2, has lost
    // its first frame.
    let mut frames = four_frames();
    frames[2].0 = 3;
    frames[3].0 = 4;
    let why = failure(lefts(&dst_image(&frames, 2), "dst-gap", 2));
    assert!(why.contains("jump from 2 to 3"), "{why}");
}

#[test]
fn an_image_that_is_not_a_sacd_is_refused() {
    let img = TestImage::new();
    let path = img.write("empty");
    assert!(matches!(open(&path), Err(SacdError::MalformedToc(_))));
    let _ = std::fs::remove_file(path);
}

/// The same image dumped with its physical sectors whole: 2064 bytes each, a
/// 12-byte header and a 4-byte trailer around the 2048 that carry data. The
/// header and trailer are not zeros, so a reader that ignores them reads
/// garbage rather than silence.
fn physical(img: &TestImage) -> TestImage {
    let mut out = Vec::with_capacity(SECTORS * 2064);
    for (lsn, sector) in img.0.chunks(SECTOR).enumerate() {
        out.extend_from_slice(&(lsn as u32).to_be_bytes());
        out.extend_from_slice(&[0xAA; 8]);
        out.extend_from_slice(sector);
        out.extend_from_slice(&[0x55; 4]);
    }
    TestImage(out)
}

#[test]
fn an_image_of_2064_byte_sectors_plays_the_same_dsd() {
    // 11 of the 39 SACD images in one collection were dumped this way, and
    // every one was taken for something that is not a SACD.
    let logical = build(false);
    let physical_path = physical(&logical).write("physical");
    let logical_path = logical.write("logical-twin");

    let img = open(&physical_path).expect("a dump of 2064-byte sectors is a SACD image");
    assert_eq!(img.area.tracks.len(), 2);
    let track = img.area.tracks[0].clone();
    let from_physical = SacdTrackReader::open(img, &track).unwrap().next_block().unwrap();

    let img = open(&logical_path).unwrap();
    let track = img.area.tracks[0].clone();
    let from_logical = SacdTrackReader::open(img, &track).unwrap().next_block().unwrap();

    assert_eq!(from_physical, from_logical, "the same DSD, byte for byte");
    let _ = std::fs::remove_file(physical_path);
    let _ = std::fs::remove_file(logical_path);
}

/// The two-track image with the text a pressed disc carries: the album's
/// title and artist in the master table's text sector, and a title for each
/// track -- and a performer for the first -- in the area's SACDTTxt block,
/// all in ISO-8859-1 so the umlaut has to be decoded rather than passed.
///
/// The block spans two sectors and its records sit in the second, past byte
/// 2048 of the block, the way they do on the discs this was checked against:
/// on "Das Rheingold" the first record is 4096 bytes in. A block cut to one
/// sector read every real disc's titles as missing.
fn with_text() -> TestImage {
    let mut img = build(false);
    {
        let m = img.sector(510);
        m[0x54..0x56].copy_from_slice(&5u16.to_be_bytes()); // two more TOC sectors
        m[0x80] = 1; // one text channel
        m[0x8a] = 2; // ISO-8859-1
    }
    {
        let t = img.sector(511);
        t[..8].copy_from_slice(b"SACDText");
        t[0x10..0x12].copy_from_slice(&0x40u16.to_be_bytes());
        t[0x12..0x14].copy_from_slice(&0x80u16.to_be_bytes());
        t[0x40..0x40 + 18].copy_from_slice(b"Agents Of Fortune\0");
        t[0x80..0x80 + 17].copy_from_slice(b"Blue \xD6yster Cult\0");
    }
    {
        let a = img.sector(AREA_TOC_LSN);
        a[0x5a] = 2; // ISO-8859-1
        a[0x80..0x82].copy_from_slice(&3u16.to_be_bytes()); // SACDTTxt is the fourth sector
    }
    {
        let b = img.sector(AREA_TOC_LSN + 3);
        b[..8].copy_from_slice(b"SACDTTxt");
        // Pointers relative to the block, into its second sector.
        b[8..10].copy_from_slice(&(SECTOR as u16 + 0x20).to_be_bytes());
        b[10..12].copy_from_slice(&(SECTOR as u16 + 0x80).to_be_bytes());
    }
    {
        let b = img.sector(AREA_TOC_LSN + 4);
        // Track 1: two items, a title and a performer, each a kind byte, a
        // spare byte and a string, padded with zeros.
        b[0x20] = 2;
        let title = b"This Ain't The Summer Of Love\0";
        b[0x24] = 1;
        b[0x26..0x26 + title.len()].copy_from_slice(title);
        let at = 0x26 + title.len() + 2;
        b[at] = 2;
        b[at + 2..at + 2 + 17].copy_from_slice(b"Blue \xD6yster Cult\0");
        // Track 2: a title and nothing else.
        b[0x80] = 1;
        b[0x84] = 1;
        b[0x86..0x86 + 17].copy_from_slice(b"True Confessions\0");
    }
    img
}

#[test]
fn a_disc_names_itself_and_its_tracks() {
    let path = with_text().write("text");
    let img = open(&path).unwrap();
    assert_eq!(img.area.album.as_deref(), Some("Agents Of Fortune"));
    assert_eq!(img.area.artist.as_deref(), Some("Blue \u{d6}yster Cult"), "decoded, not passed through");
    assert_eq!(img.area.tracks[0].title.as_deref(), Some("This Ain't The Summer Of Love"));
    assert_eq!(img.area.tracks[0].performer.as_deref(), Some("Blue \u{d6}yster Cult"));
    assert_eq!(img.area.tracks[1].title.as_deref(), Some("True Confessions"));
    assert_eq!(img.area.tracks[1].performer, None);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_disc_without_text_still_plays_and_says_nothing() {
    let path = build(false).write("no-text");
    let img = open(&path).unwrap();
    assert_eq!(img.area.album, None);
    assert!(img.area.tracks.iter().all(|t| t.title.is_none()));
    let _ = std::fs::remove_file(path);
}

/// The two-track image with the second track's first sector laid out as on
/// real discs: 672 bytes of the first track's last frame, then the second
/// track's first frame, the only packet marked as a frame start. On "Wish
/// You Were Here", "Das Rheingold" and "Led Zeppelin IV" most tracks after
/// the first begin 672 or 1344 bytes before their first frame.
fn misaligned() -> TestImage {
    let mut img = build(false);
    let s = img.sector(AUDIO_START + 10);
    s.fill(0);
    s[0] = (2 << 5) | (1 << 2); // two packets, one frame-info record
    let header = 1 + 2 * 2 + 3;
    let tail = 672usize;
    let rest = SECTOR - header - tail;
    let before: u16 = (DATA_TYPE_AUDIO << 11) | tail as u16;
    let first: u16 = 0x8000 | (DATA_TYPE_AUDIO << 11) | rest as u16;
    s[1..3].copy_from_slice(&before.to_be_bytes());
    s[3..5].copy_from_slice(&first.to_be_bytes());
    for b in &mut s[header..header + tail] {
        *b = 0xEE;
    }
    for (k, b) in s[header + tail..].iter_mut().enumerate() {
        *b = if k % 2 == 0 { 0x11 } else { 0x22 };
    }
    img
}

#[test]
fn a_track_starts_at_its_first_frame_and_not_at_the_last_one_s_tail() {
    // Read from the first byte, every 9408-byte window straddles two real
    // frames; QBZ measured that as noise with the music behind it.
    let path = misaligned().write("misaligned");
    let img = open(&path).unwrap();
    let track = img.area.tracks[1].clone();
    let block = SacdTrackReader::open(img, &track).unwrap().next_block().unwrap().expect("there is audio");
    assert_eq!(
        (block[0][0], block[1][0]),
        (0x11, 0x22),
        "the second track begins at its own first frame, not at 0xEE left over from the first"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_track_ends_where_the_disc_says_and_not_where_its_sectors_do() {
    // The sectors of a track can hold a whole frame more than its length,
    // and that frame is the next track's. The disc says the first track
    // runs one frame here.
    let mut img = build(false);
    let t2 = img.sector(AREA_TOC_LSN + 2);
    const SECOND_ARRAY: usize = 8 + 255 * 4;
    t2[SECOND_ARRAY..SECOND_ARRAY + 4].copy_from_slice(&[0, 0, 1, 0]); // 0:00, frame 1
    let path = img.write("one-frame");
    let img = open(&path).unwrap();
    let track = img.area.tracks[0].clone();
    let mut reader = SacdTrackReader::open(img, &track).unwrap();
    let mut frames = 0;
    while reader.next_block().unwrap().is_some() {
        frames += 1;
    }
    assert_eq!(frames, 1, "one frame, though its ten sectors hold two");
    let _ = std::fs::remove_file(path);
}

/// Opens track `number` of an image written as `name`.
fn reader(img: &TestImage, name: &str, number: usize) -> (SacdTrackReader, PathBuf) {
    let path = img.write(name);
    let disc = open(&path).unwrap();
    let track = disc.area.tracks[number - 1].clone();
    (SacdTrackReader::open(disc, &track).unwrap(), path)
}

/// One-bit samples of a channel in a frame.
const PER_FRAME: u64 = 2_822_400 / 75;

#[test]
fn a_dst_track_seeks_by_the_timecode_of_its_frames() {
    let img = dst_image(&four_frames(), 2);
    let (mut second, path) = reader(&img, "dst-seek", 2);
    assert_eq!(second.seek(PER_FRAME + 5).unwrap(), Some(PER_FRAME), "to the frame the sample is in");
    assert_eq!(second.next_block().unwrap().unwrap()[0][0], 0x13);
    assert_eq!(second.seek(0).unwrap(), Some(0), "and back");
    assert_eq!(second.next_block().unwrap().unwrap()[0][0], 0x12);
    assert_eq!(second.next_block().unwrap().unwrap()[0][0], 0x13);
    assert!(second.next_block().unwrap().is_none(), "two frames, as before the seek");
    let _ = std::fs::remove_file(path);

    let (mut first, path) = reader(&img, "dst-seek-first", 1);
    assert_eq!(first.seek(PER_FRAME).unwrap(), Some(PER_FRAME));
    assert_eq!(first.next_block().unwrap().unwrap()[0][0], 0x11);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dst_seek_to_the_start_skips_the_last_frame_of_the_track_before() {
    // The second track's sectors begin where the first track's last frame
    // starts, so the search lands among frames that are not the track's.
    let img = dst_image(&four_frames(), 1);
    let (mut second, path) = reader(&img, "dst-seek-shared", 2);
    assert_eq!(second.next_block().unwrap().unwrap()[0][0], 0x12);
    assert_eq!(second.seek(0).unwrap(), Some(0));
    assert_eq!(second.next_block().unwrap().unwrap()[0][0], 0x12);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_seek_past_the_end_of_a_track_ends_it() {
    let img = dst_image(&four_frames(), 2);
    let (mut first, path) = reader(&img, "dst-seek-past", 1);
    assert_eq!(first.seek(10 * PER_FRAME).unwrap(), Some(2 * PER_FRAME), "its two frames and no more");
    assert!(first.next_block().unwrap().is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_uncompressed_track_seeks_to_the_sector_its_frame_starts_in() {
    // In the image `build` writes, a frame starts in every sector and the
    // timecode is the sector's place in the area, so frame 3 of the first
    // track starts in its fourth sector.
    let img = build(false);
    let (mut first, path) = reader(&img, "dsd-seek", 1);
    assert_eq!(first.seek(3 * PER_FRAME).unwrap(), Some(3 * PER_FRAME));
    let block = first.next_block().unwrap().expect("there is audio after it");
    let header_len = 1 + 2 + 3;
    let len = SECTOR - header_len;
    let expected = ((3 * len) / 2 % 251) as u8;
    assert_eq!((block[0][0], block[1][0]), (expected, !expected), "the first byte of the fourth sector");
    let _ = std::fs::remove_file(path);
}
