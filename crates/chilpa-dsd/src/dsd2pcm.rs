//! Streaming DSD-to-PCM FIR decimator.
//!
//! This is a Rust port of Sebastian Gesemann's `dsd2pcm` conversion engine,
//! distributed under the BSD-2-Clause license. The coefficient table and
//! byte-folding algorithm follow the upstream implementation. See
//! `licenses/dsd2pcm-BSD-2-Clause.txt`.

use std::sync::OnceLock;

const HALF_TAPS: usize = 48;
const FIFO_SIZE: usize = 16;
const FIFO_MASK: usize = FIFO_SIZE - 1;
const TABLES: usize = 6;

#[rustfmt::skip]
const TAPS: [f64; HALF_TAPS] = [
    0.09950731974056658, 0.09562845727714668, 0.08819647126516944,
    0.07782552527068175, 0.06534876523171299, 0.05172629311427257,
    0.0379429484910187, 0.02490921351762261, 0.0133774746265897,
    0.003883043418804416, -0.003284703416210726, -0.008080250212687497,
    -0.01067241812471033, -0.01139427235000863, -0.0106813877974587,
    -0.009007905078766049, -0.006828859761015335, -0.004535184322001496,
    -0.002425035959059578, -0.0006922187080790708, 0.0005700762132016592,
    0.001353838005269448, 0.001713709690937, 0.001742046839472948,
    0.001545601648013235, 0.001226696225277855, 0.0008704322683580222,
    0.0005381636200535649, 0.000266446345425276, 0.00007002968740383528,
    -0.000052794070532407, -0.0001140625650874684, -0.0001304796361231895,
    -0.0001189970287491285, -0.0000939624715507305, -0.000065776341883272,
    -0.0000407492895835035, -0.0000217407957554587, -0.00000916305893159272,
    -0.0000020174601450322016, 0.000001249721855219005,
    0.0000021666551905373926, 0.0000019305208929910826,
    0.000001319400334374195, 0.0000007410039764049091,
    0.0000003423230509967409, 0.0000001244182214744588,
    0.00000003130441005359396,
];

struct Lookup {
    reverse: [u8; 256],
    sums: [[f32; 256]; TABLES],
}

fn lookup() -> &'static Lookup {
    static LOOKUP: OnceLock<Lookup> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        let mut reverse = [0u8; 256];
        let mut value = 0u32;
        for reversed in &mut reverse {
            *reversed = value as u8;
            let mut bit = 128;
            while bit != 0 {
                value ^= bit;
                if value & bit != 0 {
                    break;
                }
                bit >>= 1;
            }
        }

        let mut sums = [[0.0f32; 256]; TABLES];
        for table in 0..TABLES {
            let count = (HALF_TAPS - table * 8).min(8);
            for byte in 0..256usize {
                let mut sum = 0.0;
                for bit in 0..count {
                    let sign = (((byte >> (7 - bit)) & 1) as i32 * 2 - 1) as f64;
                    sum += sign * TAPS[table * 8 + bit];
                }
                sums[TABLES - 1 - table][byte] = sum as f32;
            }
        }
        Lookup { reverse, sums }
    })
}

/// One channel of DSD, decimated eight to one.
pub(crate) struct Dsd2Pcm {
    fifo: [u8; FIFO_SIZE],
    position: usize,
}

impl Dsd2Pcm {
    pub(crate) fn new() -> Self {
        // A DC-balanced DSD idle pattern avoids a transient while the filter
        // history fills.
        Self {
            fifo: [0x69; FIFO_SIZE],
            position: 0,
        }
    }

    pub(crate) fn translate(&mut self, source: &[u8], output: &mut Vec<f32>) {
        let lookup = lookup();
        let mut position = self.position;
        output.reserve(source.len());
        for &byte in source {
            self.fifo[position] = byte;
            let mirrored = position.wrapping_sub(TABLES) & FIFO_MASK;
            self.fifo[mirrored] = lookup.reverse[self.fifo[mirrored] as usize];

            let mut sum = 0.0f32;
            for table in 0..TABLES {
                let front = self.fifo[position.wrapping_sub(table) & FIFO_MASK] as usize;
                let back = self.fifo
                    [position.wrapping_sub(TABLES * 2 - 1).wrapping_add(table) & FIFO_MASK]
                    as usize;
                sum += lookup.sums[table][front] + lookup.sums[table][back];
            }
            output.push(sum);
            position = (position + 1) & FIFO_MASK;
        }
        self.position = position;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_dsd_becomes_silence() {
        let mut converter = Dsd2Pcm::new();
        let mut output = Vec::new();
        converter.translate(&[0x69; 4096], &mut output);
        assert_eq!(output.len(), 4096);
        assert!(output[64..].iter().all(|sample| sample.abs() < 1e-3));
    }
}
