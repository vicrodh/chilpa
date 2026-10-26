//! I preserve the physical stream ranges and read the selected device only.
use crate::{ffi::*, read};
use chilpa_audio::evidence::*;
fn range(f: AudioStreamBasicDescription, lo: f64, hi: f64, stream: u32, scope: Scope) -> FormatRange {
    FormatRange { encoding: if f.mFormatID != kAudioFormatLinearPCM { "unknown" }
        else if f.mFormatFlags & kAudioFormatFlagIsFloat != 0 { "float" } else { "pcm" }.into(),
        rate_min: lo, rate_max: hi, useful_bits: f.mBitsPerChannel as u16,
        container_bits: if f.mFormatFlags & (1 << 5) != 0 { (f.mBytesPerFrame * 8) as u16 }
            else if f.mChannelsPerFrame > 0 { (f.mBytesPerFrame * 8 / f.mChannelsPerFrame) as u16 } else { 0 },
        channels: f.mChannelsPerFrame as u16, stream: stream.to_string(), scope }
}
pub fn inspect(id: &str) -> Inspection {
    let mut doc = Inspection::unavailable("coreaudio", id, String::new());
    let Some(device) = crate::find(id) else { doc.state = State::Disconnected; return doc };
    let uid = read::uid(device);
    doc.identity.strong = !uid.is_empty();
    doc.identity.stable_id = if uid.is_empty() {crate::device_key(device)} else {uid};
    doc.identity.transport = format!("{:08x}",read::transport(device));
    // AudioObjectID changes on reconnection; available ranges bind configuration.
    doc.identity.generation = device.to_string();
    doc.method = "coreaudio_physical_stream_ranges".into();
    for stream in read::output_streams(device).into_iter().take(32) {
        for r in read::ranged_physical_formats(stream).into_iter().take(256 - doc.formats.len()) {
            doc.formats.push(range(r.mFormat,r.mSampleRateRange.mMinimum,r.mSampleRateRange.mMaximum,stream,Scope::HardwareDeclaration));
        }
    }
    doc.identity.generation.push_str(&format!(":{:?}", doc.formats));
    doc.state = State::Available;
    doc.observation = observation(id);
    doc.identity.generation.push_str(&format!(":{}:{:?}",read::current_rate(device),doc.observation.formats));
    let holder = read::hogged_by(device);
    if holder >= 0 && holder != std::process::id() as i32 { doc.state = State::Busy; }
    doc.bound(); doc
}
pub fn observation(id: &str) -> Observation {
    let Some(device) = crate::find(id) else { return Observation::default() };
    let streams = read::output_streams(device);
    let count = streams.len();
    let formats:Vec<_> = streams.into_iter().take(32).filter_map(|stream| {
        let f = read::format(stream,kAudioStreamPropertyPhysicalFormat)?;
        Some(range(f,f.mSampleRate,f.mSampleRate,stream,Scope::Readback))
    }).collect();
    Observation { exclusive: read::hog_holder(device).and_then(|holder|
        if holder == std::process::id() as i32 {Some(true)} else if holder == -1 {Some(false)} else {None}),
        complete:count > 0 && formats.len() == count,formats }
}
