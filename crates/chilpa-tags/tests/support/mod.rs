/// ID3v2.4 with the two frames used by disc subtitles and works. DSF and
/// DFF only read ID3; their tests must not depend on lofty writing those
/// container formats.
pub fn id3_disc_work() -> Vec<u8> {
    fn syncsafe(size: usize) -> [u8; 4] {
        let size = size as u32;
        [((size >> 21) & 0x7f) as u8, ((size >> 14) & 0x7f) as u8, ((size >> 7) & 0x7f) as u8, (size & 0x7f) as u8]
    }
    fn frame(id: &[u8; 4], value: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&syncsafe(value.len()));
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(value);
        out
    }
    let mut frames = frame(b"TSST", b"\x03Scene One");
    frames.extend(frame(b"TXXX", b"\x03WORK\0Das Rheingold"));
    let mut tag = b"ID3\x04\0\0".to_vec();
    tag.extend_from_slice(&syncsafe(frames.len()));
    tag.extend(frames);
    tag
}
