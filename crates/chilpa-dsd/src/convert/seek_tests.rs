use super::*;
use crate::DsdInfo;

struct Source {
    info: DsdInfo,
    planar: Vec<Vec<u8>>,
    at: usize,
    alignment: usize,
    seekable: bool,
}

impl DsdSource for Source {
    fn info(&self) -> &DsdInfo {
        &self.info
    }
    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
        if self.at == self.planar[0].len() {
            return Ok(None);
        }
        let end = (self.at + 251).min(self.planar[0].len());
        let block = self
            .planar
            .iter()
            .map(|channel| channel[self.at..end].to_vec())
            .collect();
        self.at = end;
        Ok(Some(block))
    }
    fn seek(&mut self, sample: u64) -> Result<Option<u64>, DsdError> {
        if !self.seekable {
            return Ok(None);
        }
        self.at = (sample as usize / 8 / self.alignment * self.alignment).min(self.planar[0].len());
        Ok(Some(self.at as u64 * 8))
    }
}

fn converter(rate: u32, alignment: usize, seekable: bool) -> DsdPcmConverter {
    let bytes = 4096 * (rate / OUTPUT_RATE / 8) as usize;
    let source = Source {
        info: DsdInfo {
            dsd_rate_hz: rate,
            channels: 2,
            samples: bytes as u64 * 8,
        },
        planar: (0..2)
            .map(|channel| {
                (0..bytes)
                    .map(|i| ((i * 73 + channel * 131) ^ (i >> 3)) as u8)
                    .collect()
            })
            .collect(),
        at: 0,
        alignment,
        seekable,
    };
    DsdPcmConverter::new(Box::new(source), DEFAULT_GAIN_DB).unwrap()
}

fn remainder(converter: &mut DsdPcmConverter) -> Vec<f32> {
    let mut output = Vec::new();
    while let Some(block) = converter.next_block().unwrap() {
        output.extend(block);
    }
    output
}

#[test]
fn seek_preserves_the_continuous_filter_output_at_every_dsd_rate() {
    for rate in [2_822_400, 5_644_800, 11_289_600, 22_579_200] {
        let expected = remainder(&mut converter(rate, 1, true));
        // Exact bytes, DSF blocks, SACD frames and an off-grid landing.
        for alignment in [1, 4096, 4704, 31] {
            let mut converter = converter(rate, alignment, true);
            converter.next_block().unwrap();
            for frame in [1733, 19, 3999, 0, 4096, 9876, 2048] {
                let target = frame.min(4096);
                assert_eq!(converter.seek_to_frame(frame).unwrap(), Some(target));
                assert_eq!(
                    remainder(&mut converter),
                    expected[target as usize * 2..],
                    "rate={rate}, alignment={alignment}, frame={frame}"
                );
            }
        }
    }
}

#[test]
fn unsupported_seek_keeps_the_filter_history_and_remaining_audio() {
    let mut expected = converter(2_822_400, 1, false);
    let mut actual = converter(2_822_400, 1, false);
    assert_eq!(expected.next_block().unwrap(), actual.next_block().unwrap());
    assert_eq!(actual.seek_to_frame(2000).unwrap(), None);
    assert_eq!(remainder(&mut actual), remainder(&mut expected));
}
