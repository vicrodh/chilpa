use crate::format::{SampleFormat, StreamFormat};

/// I combine gains before rounding PCM once, preserving unity bit for bit.
/// The caller owns the ramp; this stage allocates nothing.
pub fn scale(bytes: &mut [u8], format: StreamFormat, mut gain: impl FnMut() -> f32) {
    if format.sample_format.is_dsd() { return; }
    for sample in bytes.chunks_exact_mut(format.sample_format.bytes()) {
        let gain = gain();
        if gain == 1.0 { continue; }
        if format.sample_format == SampleFormat::F32 {
            let value = f32::from_le_bytes(sample.try_into().unwrap()) * gain;
            sample.copy_from_slice(&value.to_le_bytes());
        } else {
            let value = super::wire::read(sample, format.sample_format) * f64::from(gain);
            super::wire::write(sample, format.sample_format, value);
        }
    }
}

/// I convert after gain, retaining float headroom and original integer precision.
pub(crate) fn convert_pcm(input: &[u8], source: SampleFormat, target: SampleFormat, output: &mut Vec<u8>) {
    if source == SampleFormat::F32 {
        output.resize(input.len() / 4 * target.bytes(), 0);
        for (input, output) in input.chunks_exact(4).zip(output.chunks_exact_mut(target.bytes())) {
            super::wire::write(output, target, f64::from(f32::from_le_bytes(input.try_into().unwrap())));
        }
    } else {
        output.clear();
        let full = (1u64 << (source.valid_bits() - 1)) as f64;
        for sample in input.chunks_exact(source.bytes()) {
            // I recover every integer bit in f64 and reuse the reference
            // packer's narrowing rule, including negative halfway samples.
            let sample = [(super::wire::read(sample, source) * full) as i32];
            crate::pack::pack_pcm_reference(&crate::pack::Samples::Int { data: &sample, bits: source.valid_bits() }, target, output)
                .expect("PCM conversion uses a negotiated PCM target");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_narrowing_keeps_the_reference_rounding_after_gain() {
        for source in [SampleFormat::S24Packed, SampleFormat::S24In32, SampleFormat::S32] {
            let shift = source.valid_bits() - 16;
            let sample = (300 << shift) + (1 << (shift - 1));
            let mut bytes = Vec::new();
            crate::pack::pack(&crate::pack::Samples::Int { data: &[sample, -sample], bits: source.valid_bits() }, source, &mut bytes).unwrap();
            let mut unity = Vec::new();
            convert_pcm(&bytes, source, SampleFormat::S16, &mut unity);
            assert_eq!(unity, [301i16.to_le_bytes(), (-300i16).to_le_bytes()].concat(), "unity retains the reference halfway rule");
            scale(&mut bytes, StreamFormat::new(source, 8_000, 1), || 0.5);
            let mut output = Vec::new();
            convert_pcm(&bytes, source, SampleFormat::S16, &mut output);
            assert_eq!(output, [150i16.to_le_bytes(), (-150i16).to_le_bytes()].concat());
        }
    }

    #[test]
    fn all_pcm_widths_scale_without_changing_their_layout() {
        for sample_format in [SampleFormat::S16, SampleFormat::S24Packed, SampleFormat::S24In32, SampleFormat::S32, SampleFormat::F32] {
            let format = StreamFormat::new(sample_format, 48_000, 1);
            let mut input = vec![0; sample_format.bytes()];
            super::super::wire::write(&mut input, sample_format, -0.5);
            let mut output = input.clone();
            scale(&mut output, format, || 0.5);
            assert_eq!(super::super::wire::read(&output, sample_format), -0.25);
            if sample_format == SampleFormat::S24In32 { assert_eq!(output[0], 0); }
            scale(&mut input, format, || 1.0);
            assert_eq!(super::super::wire::read(&input, sample_format), -0.5);
            scale(&mut output, format, || 0.0);
            assert_eq!(super::super::wire::read(&output, sample_format), 0.0);
        }
    }

    #[test]
    fn unity_preserves_every_bit_and_native_dsd_bypasses_scaling() {
        for sample_format in [SampleFormat::S32, SampleFormat::DsdU32Le, SampleFormat::DsdU32Be] {
            let format = StreamFormat::new(sample_format, 48_000, 1);
            let mut input = [1, 2, 3, 4];
            scale(&mut input, format, || 1.0);
            assert_eq!(input, [1, 2, 3, 4]);
            if sample_format.is_dsd() {
                scale(&mut input, format, || 0.0);
                assert_eq!(input, [1, 2, 3, 4]);
            }
        }
    }
}
