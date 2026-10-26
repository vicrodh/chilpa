//! I publish only changed documents while the shared panel is visible.
use super::*;
use chilpa_core::output_evidence::{Scope, SampleFormat};

impl crate::Chilpa {
    pub fn audio_output_doc(&mut self, seen: u64) -> Option<(u64,String)> {
        self.advance_audio_output();
        if !self.audio_output.shown { return None; }
        let generation = chilpa_core::backends::generation();
        if !self.audio_output.hardware_generation.is_empty() && self.audio_output.hardware_generation != generation {
            if let Some(inspection) = &mut self.audio_output.inspection { inspection.state = State::Stale; }
            self.audio_output.error = chilpa_i18n::t("Equipment changed. Read its details again before checking a format.");
            self.audio_output.revision += 1;
        }
        self.audio_output.hardware_generation = generation;
        let dsp = self.now.playing && !self.casting.active() && self.player.as_ref().is_some_and(|p| p.dsp().active());
        let gain = self.now.playing && !self.casting.active() && self.player.as_ref().is_some_and(|p| p.gain_active());
        let key = format!("{}:{}:{dsp}:{gain}:{}:{}:{}:{:?}:{}",self.audio_output.route.as_ref().map_or(0,|r| r.revision),
            self.now.playing,self.now.paused,self.cast_destination().unwrap_or_default(),chilpa_i18n::current_language(),
            self.casting.inspected_status().state,self.casting.inspected_error());
        if self.audio_output.key != key { self.audio_output.key = key; self.audio_output.revision += 1; }
        let state = &self.audio_output;
        if seen == state.revision { return None; }
        let inspection = state.inspection.as_ref();
        let selection_name = state.outputs.iter().find(|o| o["id"] == state.selection)
            .and_then(|o| o["label"].as_str()).unwrap_or(&state.selection);
        let history: Vec<_> = state.history.iter().rev().filter(|c| inspection.is_some_and(|i|
            c.identity.backend == i.identity.backend && c.identity.stable_id == i.identity.stable_id)).take(32)
            .map(|c| json!({"case":c,"requested":format_name(c.requested),"status":status(c.state),
                "sameContext":inspection.is_some_and(|i| i.state != State::Stale && i.identity.strong && i.identity == c.identity),
                "opened":c.opened.map(format_name).unwrap_or_default(),"physical":status(c.physical_format),"audible":status(c.audible)})).collect();
        let formats: Vec<_> = inspection.into_iter().flat_map(|i| &i.formats).map(|f| json!({"format":f,
            "description":range_name(f),"scope":scope(f.scope)})).collect();
        let attributes: Vec<_> = inspection.into_iter().flat_map(|i| &i.attributes).map(|a| json!({
            "name":attribute(&a.name),"value":a.value,"scope":scope(a.scope)})).collect();
        let current = if self.casting.active() { self.casting.inspected_selection().map(remote_id).unwrap_or_default() }
            else if self.now.playing { state.route.as_ref().map(|r|r.device.clone()).unwrap_or_default() }
            else { let kept = chilpa_core::settings::load().output;
                if kept.is_empty() { state.outputs.first().and_then(|o|o["id"].as_str()).unwrap_or_default().into() } else { kept } };
        Some((state.revision,json!({"revision":state.revision,"selection":state.selection,"name":selection_name,
            "current":current,"outputs":state.outputs,"inspection":inspection,"formats":formats,"attributes":attributes,
            "status":inspection.map(|i|if i.state == State::Available { chilpa_i18n::t("Details available") } else {status(i.state)})
                .unwrap_or_else(|| if state.pending.is_some() {chilpa_i18n::t("Loading…")} else {status(if state.error.is_empty() {State::Unknown} else {State::Failed})}),
            "history":history,"last":state.last,"lastStatus":state.last.as_ref().map(|c|status(c.state)),"saved":state.saved,"busy":state.pending.is_some(),"checking":state.checking,"error":state.error,
            "lastRequested":state.last.as_ref().map(|c|format_name(c.requested)),
            "lastOpened":state.last.as_ref().and_then(|c|c.opened).map(format_name),
            "lastPhysicalStatus":state.last.as_ref().map(|c|status(c.physical_format)),
            "lastReadback":state.last.as_ref().map(|c|c.observation.formats.iter().map(range_name).collect::<Vec<_>>().join("\n")),
            "linuxDiagnostics":cfg!(target_os="linux"),
            "canCheck":inspection.is_some_and(|i| i.render_available && !matches!(i.state,State::Disconnected|State::Stale|State::Busy|State::Permission) && i.identity.backend != "cast"),
            "canUse":inspection.is_some_and(|i|i.render_available && !matches!(i.state,State::Disconnected|State::Stale) && i.identity.backend != "cast"),
            "playing":self.now.playing,"exclusiveRequested":self.exclusive(),"now":self.audio_now(dsp,gain),
        }).to_string()))
    }

    fn audio_now(&self, dsp: bool, gain: bool) -> Value {
        if !self.now.playing { return json!({"available":false,"summary":chilpa_i18n::t("Nothing is playing. Equipment details remain available.")}); }
        if self.casting.active() {
            return json!({"available":true,"cast":true,"summary":chilpa_i18n::t("Chilpa sends the original audio to the receiver."),
                "destination":self.cast_destination(),"source":format!("{} · {} Hz",self.now.quality.format,self.now.quality.rate_hz.unwrap_or(0)),
                "rendererStatus":renderer_status(&self.casting.inspected_status().state),"rendererError":self.casting.inspected_error(),
                "processing":chilpa_i18n::t("Local DSP and software volume are bypassed."),
                "output":chilpa_i18n::t("Original audio over the selected network transport"),
                "readback":chilpa_i18n::t("The receiver does not report its physical DAC format.")});
        }
        let Some(route) = &self.audio_output.route else {
            return json!({"available":false,"summary":chilpa_i18n::t("Waiting for the engine's opening report…")});
        };
        // DSP and gain flags at negotiation describe intent. I show consumed
        // block observations for those stages, including live bypass changes.
        let transforms: Vec<_> = route.delivery.transforms.iter().filter(|t| !matches!(t,
            chilpa_core::output_evidence::Transform::DspApplied | chilpa_core::output_evidence::Transform::VolumeApplied))
            .map(|t| transform(*t)).collect();
        let converted = route.delivery.transforms.iter().any(|t| !t.preserves_samples() && !matches!(t,
            chilpa_core::output_evidence::Transform::DspApplied | chilpa_core::output_evidence::Transform::VolumeApplied));
        let summary = if dsp || gain || converted { chilpa_i18n::t("Chilpa applies the processing shown below.") }
            else if self.now.bit_perfect { chilpa_i18n::t("The engine reports unchanged samples on this route.") }
            else { chilpa_i18n::t("No sample changes are reported in Chilpa; the downstream path is unverified.") };
        let mut processing = transforms;
        if dsp { processing.push(chilpa_i18n::t("DSP applied to consumed PCM blocks")); }
        if gain { processing.push(chilpa_i18n::t("Gain applied to consumed PCM blocks")); }
        if processing.is_empty() { processing.push(chilpa_i18n::t("No sample processing reported in Chilpa")); }
        let readback: Vec<_> = route.observation.formats.iter().map(range_name).collect();
        let bits = self.now.quality.bits.map(|b|chilpa_i18n::t_args("{} musical bits",&[&b.to_string()]))
            .unwrap_or_else(|| if self.now.quality.format == "DSD" {chilpa_i18n::t("One-bit DSD")} else {chilpa_i18n::t("Bit depth not stated")});
        let source = chilpa_i18n::t_args("{} · {} Hz · {} · {} channels",&[&self.now.quality.format,
            &self.now.quality.rate_hz.unwrap_or(0).to_string(),&bits,&route.delivery.requested.channels.to_string()]);
        let output = if self.dsp.passthrough && !route.delivery.opened.sample_format.is_dsd() {
            chilpa_i18n::t_args("DoP carrier · {}",&[&format_name(route.delivery.opened)])
        } else { format_name(route.delivery.opened) };
        json!({"available":true,"summary":summary,"destination":route.device,"route":route,
            "source":source,"samplesChanged":dsp || gain || converted,
            "processing":processing.join("\n"),"output":output,
            "readback":if readback.is_empty() { chilpa_i18n::t("No physical readback is available for this route.") } else { readback.join("\n") },
            "readbackScope":if !route.observation.formats.is_empty() && !route.observation.complete {
                chilpa_i18n::t("Some endpoint streams could not be read.")
            } else {chilpa_i18n::t("Read at opening on the selected endpoint. This does not measure audible output.")},
            "exclusiveRequested":route.exclusive_requested,"exclusiveObserved":route.observation.exclusive,
            "dspActive":dsp,"gainActive":gain})
    }
}

pub(super) fn format_name(f: StreamFormat) -> String {
    chilpa_i18n::t_args("{} Hz · {} · {} channels",&[&f.sample_rate_hz.to_string(),sample_name(f.sample_format),&f.channels.to_string()])
}
fn sample_name(f: SampleFormat) -> &'static str {
    match f { SampleFormat::S16 => "PCM 16",SampleFormat::S24Packed => "PCM 24",SampleFormat::S24In32 => "PCM 24/32",
        SampleFormat::S32 => "PCM 32",SampleFormat::F32 => "Float 32",SampleFormat::DsdU32Be => "DSD U32 BE",SampleFormat::DsdU32Le => "DSD U32 LE" }
}
fn range_name(f: &engine::FormatRange) -> String {
    let rate = if f.rate_min == f.rate_max { format!("{}",f.rate_min) } else { format!("{}–{}",f.rate_min,f.rate_max) };
    chilpa_i18n::t_args("{} · {} Hz · {}/{} bits · {} channels",&[&f.encoding,&rate,&f.useful_bits.to_string(),&f.container_bits.to_string(),&f.channels.to_string()])
}
fn status(s: State) -> String {
    chilpa_i18n::t(match s { State::Unknown => chilpa_i18n::mark("Unknown"),State::Untested => chilpa_i18n::mark("Untested"),
        State::Available => chilpa_i18n::mark("Accepted"),State::Different => chilpa_i18n::mark("Different format"),State::Busy => chilpa_i18n::mark("Busy"),State::Permission => chilpa_i18n::mark("Permission required"),
        State::Disconnected => chilpa_i18n::mark("Disconnected"),State::Unsupported => chilpa_i18n::mark("Unsupported"),State::Stale => chilpa_i18n::mark("Out of date"),
        State::Cancelled => chilpa_i18n::mark("Cancelled"),State::Failed => chilpa_i18n::mark("Failed") })
}
fn scope(s: Scope) -> String {
    chilpa_i18n::t(match s { Scope::HardwareDeclaration => chilpa_i18n::mark("Declared by the hardware driver"),
        Scope::ConvertibleInput => chilpa_i18n::mark("Convertible system input; physical DAC capabilities are unknown"),
        Scope::DriverAcceptance => chilpa_i18n::mark("Driver query; playback has not been tested"),Scope::Opened => chilpa_i18n::mark("Opened by Chilpa"),
        Scope::Readback => chilpa_i18n::mark("Read from the selected endpoint"),Scope::Audibility => chilpa_i18n::mark("Auditory confirmation") })
}
fn attribute(name: &str) -> String {
    chilpa_i18n::t(match name { "sample_rates" => chilpa_i18n::mark("Input rates (Hz)"),"sample_formats" => chilpa_i18n::mark("Input sample formats"),
        "channels" => chilpa_i18n::mark("Input channels"),_ => chilpa_i18n::mark("Reported property") })
}
fn transform(t: engine::Transform) -> String {
    use engine::Transform::*;
    match t { Widened { from_bits,to_bits } => chilpa_i18n::t_args("Word widened from {} to {} bits without sample loss",&[&from_bits.to_string(),&to_bits.to_string()]),
        Narrowed { from_bits,to_bits } => chilpa_i18n::t_args("Resolution reduced from {} to {} bits",&[&from_bits.to_string(),&to_bits.to_string()]),
        Quantized { to_bits } => chilpa_i18n::t_args("Decoded floating samples quantized to {} bits",&[&to_bits.to_string()]),
        Duplicated => chilpa_i18n::t("Mono duplicated without changing sample values"),Floated => chilpa_i18n::t("Samples converted to floating point"),
        Mixed => chilpa_i18n::t("Mixed output"),DsdToPcm => chilpa_i18n::t("DSD converted to PCM"),
        VolumeApplied | DspApplied => String::new() }
}
fn renderer_status(state: &chilpa_cast::State) -> String {
    chilpa_i18n::t(match state { chilpa_cast::State::Playing=>chilpa_i18n::mark("Playing"),chilpa_cast::State::Paused=>chilpa_i18n::mark("Paused"),
        chilpa_cast::State::Buffering=>chilpa_i18n::mark("Buffering…"),chilpa_cast::State::Stopped=>chilpa_i18n::mark("Stopped"),chilpa_cast::State::Finished=>chilpa_i18n::mark("Finished") })
}
