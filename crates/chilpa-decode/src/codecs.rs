//! One bounded registry shared by playback and full-file verification.
use std::sync::OnceLock;
use symphonia::core::codecs::CodecRegistry;

pub(crate) fn registry() -> &'static CodecRegistry {
    static CODECS: OnceLock<CodecRegistry> = OnceLock::new();
    CODECS.get_or_init(|| {
        let mut codecs = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut codecs);
        codecs.register_all::<super::opus::OpusDecoder>();
        codecs
    })
}

/// A/52 needs the following compressed access unit for enhanced coupling.
/// Other codecs retain their existing single-packet registry path.
pub(crate) enum Engine {
    Symphonia(Box<dyn symphonia::core::codecs::Decoder>),
    A52(Box<super::a52::A52Decoder>),
}
impl Engine {
    pub(crate) fn new(
        params: &symphonia::core::codecs::CodecParameters,
        options: &symphonia::core::codecs::DecoderOptions,
    ) -> symphonia::core::errors::Result<Self> {
        if params.codec == symphonia::core::codecs::CODEC_TYPE_EAC3 {
            Ok(Self::A52(Box::new(super::a52::A52Decoder::new(params)?)))
        } else {
            registry().make(params, options).map(Self::Symphonia)
        }
    }
    pub(crate) fn lookahead(&self) -> bool {
        matches!(self, Self::A52(_))
    }
    pub(crate) fn name(&self, fallback: &'static str) -> &'static str {
        match self {
            Self::A52(decoder) => decoder.name(),
            _ => fallback,
        }
    }
    pub(crate) fn reset(&mut self) {
        match self {
            Self::A52(decoder) => decoder.reset(),
            Self::Symphonia(decoder) => decoder.reset(),
        }
    }
    pub(crate) fn symphonia(&mut self) -> &mut dyn symphonia::core::codecs::Decoder {
        match self {
            Self::Symphonia(decoder) => &mut **decoder,
            Self::A52(_) => unreachable!("A/52 has its own preroll"),
        }
    }
    pub(crate) fn decode(
        &mut self,
        packet: &symphonia::core::formats::Packet,
        next: Option<&symphonia::core::formats::Packet>,
    ) -> symphonia::core::errors::Result<symphonia::core::audio::AudioBufferRef<'_>> {
        match self {
            Self::A52(decoder) => decoder.decode(packet, next),
            Self::Symphonia(decoder) => decoder.decode(packet),
        }
    }
    pub(crate) fn finalize(&mut self) -> symphonia::core::codecs::FinalizeResult {
        match self {
            Self::A52(_) => Default::default(),
            Self::Symphonia(decoder) => decoder.finalize(),
        }
    }
}
