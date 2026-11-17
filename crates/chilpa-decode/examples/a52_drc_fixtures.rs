//! Generate synthetic coupled EAC3 controls; never connect an output device.
use oxideav_core::{AudioFrame, CodecId, CodecParameters, Error, Frame, SampleFormat};
use std::{io::Write, path::PathBuf, process::Command};
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::var_os("CHILPA_A52_FIXTURE_DIR").ok_or("CHILPA_A52_FIXTURE_DIR is required")?,
    );
    std::fs::create_dir_all(&output)?;
    for (frequency, name) in [(500.0f32, "low"), (8000.0, "high")] {
        for (word, gain) in [("0", "unity"), ("0xe0", "half")] {
            let mut params = CodecParameters::audio(CodecId::new("eac3"));
            params.sample_rate = Some(48000);
            params.channels = Some(2);
            params.sample_format = Some(SampleFormat::S16);
            params.bit_rate = Some(384000);
            for (key, value) in [
                ("cpl", "1"),
                ("cpl_begf", "0"),
                ("cpl_endf", "15"),
                ("dynrng", word),
            ] {
                params.options.insert(key, value);
            }
            let mut encoder = oxideav_ac3::eac3::make_encoder(&params)?;
            let mut signal = Vec::with_capacity(48000 * 4);
            for sample in 0..48000 {
                let value = (0.25
                    * (2.0 * std::f32::consts::PI * frequency * sample as f32 / 48000.0).sin()
                    * 32767.0) as i16;
                signal.extend(value.to_le_bytes());
                signal.extend(value.to_le_bytes());
            }
            encoder.send_frame(&Frame::Audio(AudioFrame {
                samples: 48000,
                pts: Some(0),
                data: vec![signal],
            }))?;
            encoder.flush()?;
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("synthetic.eac3");
            let mut raw = std::fs::File::create(&path)?;
            let mut packets = 0;
            loop {
                match encoder.receive_packet() {
                    Ok(packet) => {
                        assert!(oxideav_ac3::decoder::verify_packet_crc(&packet.data)?.all_ok());
                        raw.write_all(&packet.data)?;
                        packets += 1;
                    }
                    Err(Error::NeedMore | Error::Eof) => break,
                    Err(error) => return Err(error.into()),
                }
            }
            drop(raw);
            assert!(packets > 1);
            let target = output.join(format!("coupled-{name}-{gain}.m4a"));
            let result = Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-y", "-f", "eac3", "-i"])
                .arg(&path)
                .args(["-map_metadata", "-1", "-c:a", "copy", "-f", "mp4"])
                .arg(target)
                .output()?;
            assert!(
                result.status.success(),
                "remux: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()
}
