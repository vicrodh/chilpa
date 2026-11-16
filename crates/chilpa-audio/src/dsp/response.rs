//! Response and headroom share the exact channel and tone routing of the renderer.
use super::{Channel, Filter, Preset, Shape, biquad::Biquad};

impl Preset {
    pub(super) fn routed_filters(&self, channel: usize) -> Vec<Filter> {
        let mut filters: Vec<_> = self.filters.iter().filter(|f| self.eq_enabled && f.affects_audio()
            && match f.channel { Channel::All => true, Channel::Left => channel == 0, Channel::Right => channel == 1 }).cloned().collect();
        if self.tone.enabled {
            for (shape, frequency, gain_db, q) in [
                (Shape::LowShelf, self.tone.bass_hz, self.tone.bass_db, self.tone.bass_q),
                (Shape::HighShelf, self.tone.treble_hz, self.tone.treble_db, self.tone.treble_q),
            ] {
                if gain_db != 0.0 { filters.push(Filter { shape, frequency, gain_db, q, ..Filter::default() }); }
            }
        }
        filters
    }

    pub fn effective_preamp_db(&self) -> f64 { self.preamp_for_rate(48_000) }

    /// Automatic headroom covers the worst channel, including independent tone.
    /// Dynamics gains remain intentional; the optional limiter handles their peaks.
    pub fn preamp_for_rate(&self, rate: u32) -> f64 { self.preamp_for_channels(rate, 2) }
    pub fn preamp_for_channels(&self, rate: u32, channels: u16) -> f64 {
        if !self.auto_headroom || self.is_identity() { return self.preamp_db; }
        let rate = rate.max(1000);
        let top = f64::from(rate) * 0.49;
        let mut boost = 0.0_f64;
        for channel in 0..usize::from(channels.clamp(1, 3)) {
            let settings = self.routed_filters(channel);
            let filters: Vec<_> = settings.iter().map(|f| Biquad::new(f, rate)).collect();
            let grid = (0..=2048).map(|i| top.powf(i as f64 / 2048.0));
            let centers = settings.iter().map(|f| f.frequency.min(top));
            boost = grid.chain(centers).map(|hz| filters.iter().map(|f| f.magnitude_db(hz, rate)).sum::<f64>()).fold(boost, f64::max);
        }
        self.preamp_db.min(-1.0 - boost)
    }

    /// The upper response of left and right, before preamp, stereo and dynamics.
    pub fn curve(&self, rate: u32) -> Vec<f64> {
        let grid: Vec<f64> = (0..=128).map(|i| 20.0 * 1000_f64.powf(i as f64 / 128.0)).collect();
        self.response_db(&grid, rate)
    }
    pub fn response_db(&self, frequencies: &[f64], rate: u32) -> Vec<f64> {
        let left = self.channel_response_db(frequencies, rate, 0);
        let right = self.channel_response_db(frequencies, rate, 1);
        left.into_iter().zip(right).map(|(l, r)| l.max(r)).collect()
    }
    pub fn channel_response_db(&self, frequencies: &[f64], rate: u32, channel: usize) -> Vec<f64> {
        let filters: Vec<_> = self.routed_filters(channel).iter().map(|f| Biquad::new(f, rate.max(1000))).collect();
        frequencies.iter().map(|&hz| if hz >= f64::from(rate) * 0.49 { 0.0 }
            else { filters.iter().map(|f| f.magnitude_db(hz, rate)).sum::<f64>() }).collect()
    }
}
