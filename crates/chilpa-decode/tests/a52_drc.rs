//! Dynamic-range gain reaches coded and coupling-reconstructed frequencies.
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};
fn fixture(band: &str, gain: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/a52-drc/coupled-{band}-{gain}.m4a"))
}
fn rms(band: &str, gain: &str) -> f64 {
    let path = fixture(band, gain);
    let source = symphonia::core::io::MediaSourceStream::new(
        Box::new(std::fs::File::open(&path).unwrap()),
        Default::default(),
    );
    let mut format = symphonia::default::get_probe()
        .format(
            &symphonia::core::probe::Hint::new(),
            source,
            &Default::default(),
            &Default::default(),
        )
        .unwrap()
        .format;
    let packet = format.next_packet().unwrap();
    assert!(
        oxideav_ac3::decoder::verify_packet_crc(&packet.data)
            .unwrap()
            .all_ok()
    );
    let mut bits = oxideav_core::bits::BitReader::new(&packet.data[2..]);
    let info = oxideav_ac3::eac3::bsi::parse_with(&mut bits).unwrap();
    let header = oxideav_ac3::eac3::audfrm::parse_with(&mut bits, &info).unwrap();
    assert_eq!(info.num_blocks, 6);
    assert!(
        header.cplinu_blk.iter().all(|active| *active),
        "the fixture must exercise coupling"
    );
    assert!(!header.ahte && !header.spxattene);
    let mut decoder = Decoded::open(&path).unwrap();
    let mut skip = 1536 * 2;
    let mut energy = 0.0f64;
    let mut count = 0;
    while let Some(block) = decoder.next_block().unwrap() {
        let Block::Float(data) = block else {
            panic!("lossy PCM must remain float")
        };
        for value in data {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            energy += f64::from(value).powi(2);
            count += 1;
        }
    }
    assert!(count > 48000);
    (energy / count as f64).sqrt()
}
fn check(band: &str) {
    let unity = rms(band, "unity");
    let half = rms(band, "half");
    assert!(unity > 0.05);
    let ratio = half / unity;
    assert!(
        (ratio - 0.5).abs() < 0.005,
        "{band}: DRC 0xe0 must scale the complete signal by 0.5, observed {ratio:.9}"
    );
}
#[test]
fn coded_low_band_follows_dynamic_range_gain() {
    check("low")
}
#[test]
fn reconstructed_coupling_band_follows_dynamic_range_gain() {
    check("high")
}
