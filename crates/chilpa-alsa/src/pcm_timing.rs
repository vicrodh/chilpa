//! PCM writer cadence independent of the retained output buffer.

use alsa::pcm::Frames;
use chilpa_audio::format::{SampleFormat, StreamFormat};

/// Use rate-dependent direct-device periods: 12.5 ms
/// below 96 kHz, 25 ms below 192 kHz, and 50 ms at higher rates. The hardware
/// selects its nearest supported period; total buffering stays at 500 ms.
/// Native DSD retains its existing four-period negotiation unchanged.
pub(crate) fn period_frames(stream: StreamFormat) -> Option<Frames> {
    if stream.sample_format.is_dsd() { return None; }
    let divisor = if stream.sample_rate_hz >= 192_000 { 20 }
        else if stream.sample_rate_hz >= 96_000 { 40 } else { 80 };
    // Dividing u32 by at least 20 fits a signed 32-bit ALSA frame count too.
    Some((stream.sample_rate_hz / divisor).max(1) as Frames)
}

/// PCM still starts with three quarters of the actual negotiated buffer.
/// A shorter period must not move startup toward a nearly full queue.
/// Raw DSD continues to use its actual buffer-minus-period threshold.
pub(crate) fn start_threshold(format: SampleFormat, buffer: Frames, period: Frames) -> Frames {
    if format.is_dsd() { (buffer - period).max(1) }
    else { (buffer - buffer / 4).max(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_period_replaces_the_old_125_ms_delivery_cadence() {
        for format in [SampleFormat::S16, SampleFormat::S24Packed, SampleFormat::S24In32,
            SampleFormat::S32, SampleFormat::F32] {
            for (rate, expected) in [(44_100, 551), (48_000, 600), (88_200, 1102),
                (96_000, 2400), (176_400, 4410), (192_000, 9600), (384_000, 19200), (768_000, 38400)] {
                let period = period_frames(StreamFormat::new(format, rate, 2)).unwrap();
                assert_eq!(period, expected);
                assert!(period < (rate / 8) as Frames, "PCM cannot remain at the old 125 ms delivery cadence");
                assert!(period as i64 * 1000 <= i64::from(rate) * 50, "no PCM rate requests a period above 50 ms");
                assert!(period > 0);
            }
        }
    }

    #[test]
    fn period_requests_fit_32_bit_alsa_frame_counts_even_for_invalid_rates() {
        assert_eq!(period_frames(StreamFormat::new(SampleFormat::F32, 0, 2)), Some(1));
        let largest = period_frames(StreamFormat::new(SampleFormat::F32, u32::MAX, 2)).unwrap();
        assert!(largest > 0);
        assert!(largest as u64 <= i32::MAX as u64);
    }

    #[test]
    fn shorter_periods_preserve_the_original_pcm_startup_headroom() {
        for buffer in [1, 4, 22050, 24000, 48000, 96000, 384000] {
            let old_period = (buffer / 4).max(1);
            let expected = (buffer - old_period).max(1);
            for actual_period in [1, (buffer / 40).max(1), (buffer / 20).max(1), old_period] {
                assert_eq!(start_threshold(SampleFormat::S16, buffer, actual_period), expected);
            }
        }
        assert_eq!(start_threshold(SampleFormat::S16, 24000, 600), 18000);
        assert_ne!(start_threshold(SampleFormat::S16, 24000, 600), 24000 - 600,
            "a shorter period must not raise startup latency to 487.5 ms");
    }

    #[test]
    fn native_dsd_keeps_the_original_period_and_actual_device_threshold() {
        for format in [SampleFormat::DsdU32Be, SampleFormat::DsdU32Le] {
            for rate in [44_100, 88_200, 176_400, 352_800, 705_600] {
                assert_eq!(period_frames(StreamFormat::new(format, rate, 2)), None);
            }
            for (buffer, period) in [(24000, 6000), (43232, 10808), (131072, 32768), (8000, 3000)] {
                assert_eq!(start_threshold(format, buffer, period), (buffer - period).max(1));
            }
        }
    }

    #[test]
    fn dop_carrier_rates_keep_half_second_queue_startup_margin() {
        // DoP is negotiated as integer PCM; its bytes and marker handling stay
        // in the existing renderer. Only the output wakeup cadence changes.
        for (rate, expected_period) in [(176_400, 4410), (352_800, 17640), (705_600, 35280)] {
            let format = StreamFormat::new(SampleFormat::S24In32, rate, 2);
            assert_eq!(period_frames(format), Some(expected_period));
            let buffer = (rate / 2) as Frames;
            assert_eq!(start_threshold(format.sample_format, buffer, expected_period), buffer - buffer / 4);
        }
    }
}
