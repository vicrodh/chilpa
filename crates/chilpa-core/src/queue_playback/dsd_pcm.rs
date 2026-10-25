//! Converted DSD in the negotiated PCM format, with seek and pause recovery.

use super::{
    DsdPcmConverter, FrameSource, PlayError, Samples, SeekSignal, SkipSignal, StreamFormat, pack,
};
use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicU64, Ordering},
};

pub(super) struct DsdPcmFrames<'a> {
    converter: &'a mut DsdPcmConverter,
    opened: StreamFormat,
    skip: SkipSignal,
    seek: SeekSignal,
    pub(super) skipped: bool,
    level: crate::loudness::level::Leveler,
    feed: Option<Arc<dyn chilpa_audio::seek_waveform::Feed>>,
    decoded_at: u64,
    pub(super) base: Arc<AtomicI64>,
    pub(super) delivered: Arc<AtomicU64>,
    moved: bool,
    moved_at: u64,
    rewind_to: Option<u64>,
}

impl<'a> DsdPcmFrames<'a> {
    pub(super) fn new(
        converter: &'a mut DsdPcmConverter,
        opened: StreamFormat,
        skip: SkipSignal,
        seek: SeekSignal,
        level: crate::loudness::level::Leveler,
        feed: Option<Arc<dyn chilpa_audio::seek_waveform::Feed>>,
    ) -> Self {
        Self {
            converter,
            opened,
            skip,
            seek,
            skipped: false,
            level,
            feed,
            decoded_at: 0,
            base: Arc::new(AtomicI64::new(0)),
            delivered: Arc::new(AtomicU64::new(0)),
            moved: false,
            moved_at: 0,
            rewind_to: None,
        }
    }
}

impl FrameSource for DsdPcmFrames<'_> {
    fn pcm_format(&self) -> Option<StreamFormat> { Some(self.opened) }
    fn ended_early(&self) -> bool {
        self.skipped
    }
    fn moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }
    fn heard_at_move(&self) -> Option<u64> {
        Some(self.moved_at)
    }
    fn position_at_move(&self) -> Option<u64> {
        Some((self.base.load(Ordering::Relaxed) + self.moved_at as i64).max(0) as u64)
    }
    fn rewind(&mut self) -> bool {
        self.rewind_to = Some(
            (self.delivered.load(Ordering::Relaxed) as i64 + self.base.load(Ordering::Relaxed))
                .max(0) as u64,
        );
        true
    }

    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.skip.take() {
            self.skipped = true;
            return Ok(None);
        }
        let asked = self.rewind_to.take().or_else(|| {
            self.seek
                .take()
                .map(|time| chilpa_audio::seek::frames_at(time, self.opened.sample_rate_hz))
        });
        if let Some(frame) = asked {
            let frame = frame.saturating_mul(u64::from(self.converter.output_rate()))
                / u64::from(self.opened.sample_rate_hz);
            if let Some(at) = self
                .converter
                .seek_to_frame(frame)
                .map_err(|e| e.to_string())?
            {
                self.moved_at = self.delivered.load(Ordering::Relaxed);
                let position = at.saturating_mul(u64::from(self.opened.sample_rate_hz))
                    / u64::from(self.converter.output_rate());
                self.base
                    .store(position as i64 - self.moved_at as i64, Ordering::Relaxed);
                self.moved = true;
                self.decoded_at = at;
                self.level.moved();
            }
        }
        let Some(mut samples) = self.converter.next_block().map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        if let Some(feed) = &self.feed {
            feed.feed_float(self.decoded_at, &samples);
        }
        self.decoded_at += (samples.len() / usize::from(self.converter.channels())) as u64;
        self.level.float(&mut samples);
        let mut bytes = Vec::with_capacity(samples.len() * self.opened.sample_format.bytes());
        if self.opened.sample_format == chilpa_audio::format::SampleFormat::F32 {
            bytes.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        } else {
            pack(&Samples::Float(&samples), self.opened.sample_format, &mut bytes)
                .map_err(|error| PlayError::Pack(error).to_string())?;
        }
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_dsd::{DsdError, DsdInfo, DsdSource};

    struct Source {
        info: DsdInfo,
        byte: u64,
    }
    impl DsdSource for Source {
        fn info(&self) -> &DsdInfo {
            &self.info
        }
        fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
            let bytes = (self.info.samples / 8).saturating_sub(self.byte).min(4704);
            if bytes == 0 {
                return Ok(None);
            }
            self.byte += bytes;
            Ok(Some(vec![vec![0x69; bytes as usize]; 2]))
        }
        fn seek(&mut self, sample: u64) -> Result<Option<u64>, DsdError> {
            self.byte = sample / 8 / 4704 * 4704;
            Ok(Some(self.byte * 8))
        }
    }

    #[test]
    fn converted_dsd_seeks_backwards_and_rewinds_after_read_ahead_ends() {
        let source = Source {
            info: DsdInfo {
                dsd_rate_hz: 2_822_400,
                channels: 2,
                samples: 2_822_400 * 2,
            },
            byte: 0,
        };
        let mut converter = DsdPcmConverter::new(Box::new(source), 0.0).unwrap();
        let seek = SeekSignal::new();
        let mut frames = DsdPcmFrames::new(
            &mut converter,
            StreamFormat::new(chilpa_audio::format::SampleFormat::F32, 88_200, 2),
            SkipSignal::new(),
            seek.clone(),
            crate::loudness::level::Leveler::volume_only(None),
            None,
        );
        seek.request(std::time::Duration::from_secs(1));
        assert!(frames.next_block().unwrap().is_some());
        assert!(frames.moved());
        assert_eq!(frames.position_at_move(), Some(88_200));
        frames.delivered.store(50, Ordering::Relaxed);
        seek.request(std::time::Duration::from_millis(250));
        assert!(frames.next_block().unwrap().is_some());
        assert!(frames.moved());
        assert_eq!(frames.position_at_move(), Some(22_050));
        assert_eq!(frames.heard_at_move(), Some(50));
        while frames.next_block().unwrap().is_some() {}
        frames.delivered.store(700, Ordering::Relaxed);
        assert!(frames.rewind());
        assert!(
            frames.next_block().unwrap().is_some(),
            "pause recovery reopens an exhausted converter"
        );
        assert!(frames.moved());
        assert_eq!(frames.position_at_move(), Some(22_700));
        assert_eq!(frames.heard_at_move(), Some(700));
    }
}
