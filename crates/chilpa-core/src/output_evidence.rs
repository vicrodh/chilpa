//! I collect route facts and run explicit, silent opening checks.
pub use chilpa_audio::evidence::*;
pub use chilpa_audio::format::{SampleFormat, StreamFormat};
pub use chilpa_audio::negotiate::{Delivery, Transform};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
pub(crate) mod lease;
pub fn canonical(device:&str) -> String {
    crate::backends::resolve(device).and_then(|(backend,id)|backend.canonical_id(&id)
        .map(|id|chilpa_audio::backend::BackendOutput::full_id(backend.kind(),&id))).unwrap_or_else(||device.into())
}

pub fn inspect(device: &str) -> Inspection {
    let canonical = canonical(device);
    let device = canonical.as_str();
    let Some((backend, bare)) = crate::backends::resolve(device) else {
        let mut doc = Inspection::unavailable("", device, String::new());
        doc.state = State::Disconnected;
        return doc;
    };
    let mut doc = backend.inspect(&bare);
    doc.identity.id = device.into();
    use std::hash::Hasher;
    let mut context = std::collections::hash_map::DefaultHasher::new();
    static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    context.write(SESSION.get_or_init(||format!("{}:{:?}",std::process::id(),std::time::SystemTime::now())).as_bytes());
    context.write(doc.identity.generation.as_bytes());
    doc.identity.generation = format!("v1:{:016x}",context.finish());
    if !backend.outputs().iter().any(|o| o.id == bare) { doc.state = State::Disconnected; }
    doc.bound();
    doc
}

pub(crate) fn route(device: &str, delivery: &chilpa_audio::negotiate::Delivery, exclusive: bool) -> Route {
    static REVISION: AtomicU64 = AtomicU64::new(0);
    let mut delivered = delivery.clone();
    let planned_processing = delivered.transforms.iter().copied().filter(|t|matches!(t,Transform::DspApplied|Transform::VolumeApplied)).collect();
    delivered.transforms.retain(|t|!matches!(t,Transform::DspApplied|Transform::VolumeApplied));
    Route { revision: REVISION.fetch_add(1, Ordering::Relaxed) + 1, device: canonical(device),
        delivery: delivered, planned_processing, exclusive_requested: exclusive,
        observation: crate::backends::resolve(device).map(|(b, id)| b.observation(&id)).unwrap_or_default() }
}

/// I write no samples, select no output and never change the system default.
pub fn check(device: &str, requested: StreamFormat, exclusive: bool, cancel: &AtomicBool) -> Case {
    let before = inspect(device);
    let mut result = Case { identity: before.identity, requested, exclusive_requested: exclusive, opened: None,
        observation: Observation::default(), state: State::Untested, method: "silent_open".into(),
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        audible: State::Untested, physical_format:State::Unknown,error: String::new() };
    if cancel.load(Ordering::Acquire) { result.state = State::Cancelled; return result; }
    let Some(_lease) = lease::check() else { result.state = State::Busy; return result; };
    if before.state == State::Disconnected || !before.render_available {
        result.state = if before.render_available { State::Disconnected } else { State::Unsupported };
        return result;
    }
    if matches!(before.state,State::Busy | State::Permission) { result.state = before.state; return result; }
    let Some((backend, bare)) = crate::backends::resolve(device) else { result.state = State::Disconnected; return result; };
    match backend.check(&bare, requested, exclusive) {
        Ok((opened, observed)) => {
            result.opened = Some(opened); result.observation = observed;
            result.physical_format = physical_match(requested,&result.observation);
            result.state = if opened == requested && !(exclusive && result.observation.exclusive == Some(false)) { State::Available } else { State::Unsupported };
        }
        Err(error) => {
            result.state = match error { chilpa_audio::SinkError::Busy(_) => State::Busy,
                chilpa_audio::SinkError::Permission(_) => State::Permission,chilpa_audio::SinkError::Unsupported(_) => State::Unsupported,
                chilpa_audio::SinkError::DeviceLost => State::Disconnected,_ => State::Failed };
            result.error = error.to_string();
        }
    }
    if cancel.load(Ordering::Acquire) { result.state = State::Cancelled; }
    let after = inspect(device);
    if after.identity != result.identity { result.state = State::Stale; }
    result
}

/// I compare concrete readback tuples, independently of opening and audibility.
fn physical_match(requested:StreamFormat, observed:&Observation) -> State {
    if !observed.complete || observed.formats.is_empty() || observed.formats.iter().any(|f| !f.rate_min.is_finite() || f.rate_min <= 0.0
        || !f.rate_max.is_finite() || f.useful_bits == 0 || f.container_bits == 0 || f.channels == 0 || f.encoding == "unknown") { return State::Unknown; }
    let encoding = if requested.sample_format == SampleFormat::F32 {"float"} else if requested.sample_format.is_dsd() {"native_dsd"} else {"pcm"};
    if observed.formats.iter().all(|f|f.encoding == encoding && f.rate_min == requested.sample_rate_hz as f64
        && f.rate_max == requested.sample_rate_hz as f64 && f.channels == requested.channels
        && f.useful_bits == u16::from(requested.sample_format.valid_bits())
        && f.container_bits == (requested.sample_format.bytes()*8) as u16) { State::Available } else { State::Different }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepting_a_convertible_input_does_not_confirm_the_requested_physical_format() {
        let request = StreamFormat::new(SampleFormat::S24In32,192_000,2);
        assert_eq!(physical_match(request,&Observation::default()),State::Unknown);
        let mut observed = Observation { formats:vec![FormatRange { encoding:"pcm".into(),rate_min:48_000.0,rate_max:48_000.0,
            useful_bits:24,container_bits:32,channels:2,stream:"selected".into(),scope:Scope::Readback }],exclusive:Some(false),complete:true };
        assert_eq!(physical_match(request,&observed),State::Different);
        observed.formats[0].rate_min = 192_000.0; observed.formats[0].rate_max = 192_000.0;
        assert_eq!(physical_match(request,&observed),State::Available);
        observed.complete = false;
        assert_eq!(physical_match(request,&observed),State::Unknown);
        observed.complete = true;
        observed.formats[0].rate_min = 0.0;
        assert_eq!(physical_match(request,&observed),State::Unknown);
    }
    #[test]
    fn planned_processing_is_not_reported_as_consumed_processing() {
        let format = StreamFormat::new(SampleFormat::S16,44_100,2);
        let delivery = Delivery { requested:format,opened:format,
            transforms:vec![Transform::Widened {from_bits:16,to_bits:32},Transform::DspApplied,Transform::VolumeApplied] };
        let route = route("fixture:unavailable",&delivery,true);
        assert_eq!(route.delivery.transforms,vec![Transform::Widened {from_bits:16,to_bits:32}]);
        assert_eq!(route.planned_processing,vec![Transform::DspApplied,Transform::VolumeApplied]);
        assert!(route.observation.exclusive.is_none());
    }
    #[test]
    fn cancellation_before_opening_cannot_claim_a_tested_format() {
        let result = check("fixture:unavailable",StreamFormat::new(SampleFormat::S16,44_100,2),true,&AtomicBool::new(true));
        assert_eq!(result.state,State::Cancelled); assert!(result.opened.is_none());
        assert_eq!(result.audible,State::Untested);
    }
}
