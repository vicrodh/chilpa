use crate::format::SampleFormat;

pub(super) fn read(bytes: &[u8], format: SampleFormat) -> f64 {
    let value = match format {
        SampleFormat::S16 => f64::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
        SampleFormat::S24Packed => f64::from(i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]])) / 2147483648.0,
        SampleFormat::S24In32 | SampleFormat::S32 => f64::from(i32::from_le_bytes(bytes.try_into().unwrap())) / 2147483648.0,
        SampleFormat::F32 => f64::from(f32::from_le_bytes(bytes.try_into().unwrap())),
        _ => 0.0,
    };
    if value.is_finite() { value } else { 0.0 }
}

pub(super) fn write(bytes: &mut [u8], format: SampleFormat, value: f64) {
    let value = if value.is_finite() { value.clamp(-1.0, 1.0) } else { 0.0 };
    match format {
        SampleFormat::S16 => bytes.copy_from_slice(&((value*32768.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes()),
        SampleFormat::S24Packed => {
            let word = ((value*8388608.0).round().clamp(-8388608.0, 8388607.0) as i32).to_le_bytes();
            bytes.copy_from_slice(&word[..3]);
        }
        SampleFormat::S24In32 => {
            let word = ((value*8388608.0).round().clamp(-8388608.0, 8388607.0) as i32) << 8;
            bytes.copy_from_slice(&word.to_le_bytes());
        }
        SampleFormat::S32 => bytes.copy_from_slice(&((value*2147483648.0).round().clamp(-2147483648.0, 2147483647.0) as i32).to_le_bytes()),
        SampleFormat::F32 => bytes.copy_from_slice(&(value as f32).to_le_bytes()),
        _ => {}
    }
}
