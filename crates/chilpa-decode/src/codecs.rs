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
