//! I inspect without selecting. One worker and one result are owned by this
//! facade; closing releases the view and cancels an active silent check.
mod history;
mod view;
#[cfg(test)] mod tests;
use chilpa_core::output_evidence::{self as engine, Case, Inspection, Route, State, StreamFormat};
use serde_json::{json, Value};
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};

struct ResultDoc {
    selection: String,
    inspection: Inspection,
    outputs: Vec<Value>,
    history: Vec<Case>,
    last: Option<Case>,
    error: String,
    hardware_generation: String,
}
#[derive(Default)]
pub(crate) struct AudioOutput {
    pub(crate) route: Option<Route>,
    shown: bool,
    requested: bool,
    selection: String,
    inspection: Option<Inspection>,
    outputs: Vec<Value>,
    history: Vec<Case>,
    last: Option<Case>,
    pending: Option<mpsc::Receiver<ResultDoc>>,
    checking: bool,
    next_inspection: Option<String>,
    cancel: Arc<AtomicBool>,
    revision: u64,
    key: String,
    error: String,
    hardware_generation: String,
    saved: bool,
}
impl Drop for AudioOutput { fn drop(&mut self) { self.cancel.store(true,Ordering::Release); } }

impl crate::Chilpa {
    pub(crate) fn migrate_output_identity(&mut self) {
        let mut settings = chilpa_core::settings::load();
        if settings.output.is_empty() { return; }
        let canonical = engine::canonical(&settings.output);
        if canonical != settings.output {
            settings.output = canonical;
            if chilpa_core::settings::save(&settings).is_err() { self.now.error = chilpa_i18n::t("Output identity could not be saved."); }
        }
    }
    pub(crate) fn audio_output_maintenance(&mut self, key: &str) -> Option<String> {
        if key != "audio.outputDetails" { return None; }
        self.audio_output.requested = true; Some(String::new())
    }
    pub fn take_audio_output_request(&mut self) -> bool { std::mem::take(&mut self.audio_output.requested) }
    pub(crate) fn advance_audio_output(&mut self) {
        let result = match self.audio_output.pending.as_ref().map(|r| r.try_recv()) {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.audio_output.pending = None;
                self.audio_output.checking = false;
                self.audio_output.error = chilpa_i18n::t("Audio output task stopped unexpectedly.");
                self.audio_output.revision += 1;
                return;
            }
            _ => return,
        };
        let state = &mut self.audio_output;
        state.pending = None;
        state.checking = false;
        if let Some(next) = state.next_inspection.take().filter(|_| state.shown) {
            let _ = self.inspect_audio_output(&next); return;
        }
        if !state.shown || state.selection != result.selection { return; }
        state.selection = result.inspection.identity.id.clone();
        state.hardware_generation = result.hardware_generation;
        state.inspection = Some(result.inspection); state.outputs = result.outputs;
        state.history = result.history; state.last = result.last;
        state.error = result.error; state.saved = state.last.is_some() && state.error.is_empty();
        state.revision += 1;
    }

    pub fn audio_output_command(&mut self, action: &str, argument: &str) -> bool {
        self.advance_audio_output();
        if action == "close" {
            let state = &mut self.audio_output;
            state.cancel.store(true,Ordering::Release); state.shown = false;
            state.selection.clear(); state.inspection = None; state.outputs.clear();
            state.next_inspection = None;
            state.history = Vec::new(); state.outputs = Vec::new(); state.last = None; state.error.clear(); state.key.clear();
            state.hardware_generation.clear();
            return true;
        }
        if action == "cancel" {
            self.audio_output.cancel.store(true,Ordering::Release); return true;
        }
        self.audio_output.shown = true;
        let selected = if action == "open" {
            self.casting.inspected_selection().map(remote_id).unwrap_or_else(|| {
                self.audio_output.route.as_ref().filter(|_| self.now.playing).map(|r|r.device.clone())
                    .unwrap_or_else(|| chilpa_core::settings::load().output)
            })
        } else if action == "inspect" { argument.to_string() } else { self.audio_output.selection.clone() };
        let result = match action {
            "open" | "inspect" | "refresh" => self.inspect_audio_output(&selected),
            "use" => {
                if argument != selected { Err(chilpa_i18n::t("Inspect an available output before selecting it.")) }
                else if self.audio_output.pending.is_some() { Err(chilpa_i18n::t("Wait for the audio check to finish.")) }
                else if selected.starts_with("cast:") { Err(chilpa_i18n::t("Choose the receiver in Play on to connect.")) }
                else if !self.audio_output.inspection.as_ref().is_some_and(|i| i.identity.id == selected && i.render_available && !matches!(i.state,State::Disconnected|State::Stale)) {
                    Err(chilpa_i18n::t("Inspect an available output before selecting it."))
                }
                else {
                    if self.casting.active() { self.cast_disconnect(); }
                    self.use_output_as_chosen(&selected); Ok(())
                }
            }
            "check" => self.check_audio_output(argument),
            _ => Err(chilpa_i18n::t("Unknown audio output action.")),
        };
        let accepted = result.is_ok();
        self.audio_output.error = result.err().unwrap_or_default();
        self.audio_output.revision += 1;
        accepted
    }

    fn inspect_audio_output(&mut self, selected: &str) -> Result<(),String> {
        if selected.len() > 2048 { return Err(chilpa_i18n::t("Invalid output identity.")); }
        if self.audio_output.pending.is_some() {
            self.audio_output.inspection = None; self.audio_output.history.clear(); self.audio_output.last = None;
            self.audio_output.next_inspection = Some(selected.into());
            self.audio_output.selection = selected.into();
            self.audio_output.cancel.store(true,Ordering::Release);
            return Ok(());
        }
        self.audio_output.selection = selected.into();
        self.audio_output.inspection = None; self.audio_output.last = None; self.audio_output.saved = false;
        self.audio_output.cancel = Arc::new(AtomicBool::new(false));
        let mut remotes: Vec<_> = self.casting.inspected_devices().iter().take(256).cloned().collect();
        if let Some(current) = self.casting.inspected_selection() {
            if !remotes.iter().any(|d| d.id == current.id && d.kind == current.kind) { remotes.push(current.clone()); }
        }
        let selected = selected.to_string();
        let path = chilpa_dirs::config().join("audio-output-checks.json");
        let (tx,rx) = mpsc::sync_channel(1);
        std::thread::Builder::new().name("chilpa-output-details".into()).spawn(move || {
            let before = chilpa_core::backends::generation();
            let outputs = listed(&remotes);
            let resolved = if selected.is_empty() { outputs.first().and_then(|o| o["id"].as_str()).unwrap_or_default() } else { &selected };
            let mut inspection = inspect(resolved,&remotes);
            let hardware_generation = chilpa_core::backends::generation();
            if before != hardware_generation { inspection.state = State::Stale; }
            let (history,error) = match history::load(&path) { Ok(rows) => (rows,String::new()), Err(e) => (Vec::new(),e) };
            let _ = tx.send(ResultDoc { selection:selected,inspection,outputs,history,last:None,error,hardware_generation });
        }).map_err(|_| chilpa_i18n::t("Audio output inspection could not start."))?;
        self.audio_output.checking = false; self.audio_output.pending = Some(rx); Ok(())
    }

    fn check_audio_output(&mut self, argument: &str) -> Result<(),String> {
        if argument.len() > 16*1024 { return Err(chilpa_i18n::t("Invalid audio check request.")); }
        if self.audio_output.pending.is_some() { return Err(chilpa_i18n::t("An audio check is already running.")); }
        // I leave transport, queue, track, volume and desktop defaults untouched.
        if self.now.playing || self.player.as_ref().is_some_and(|p| !p.idle()) {
            return Err(chilpa_i18n::t("Stop playback before checking an output. Inspection remains available while listening."));
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request { output:String,generation:String,format:StreamFormat,exclusive:bool }
        let request: Request = serde_json::from_str(argument)
            .map_err(|_| chilpa_i18n::t("Choose a PCM format, rate and channel count."))?;
        let requested = request.format;
        if requested.sample_format.is_dsd() || !(8_000..=768_000).contains(&requested.sample_rate_hz)
            || !(1..=8).contains(&requested.channels) {
            return Err(chilpa_i18n::t("Silent opening checks accept PCM only. Opening a PCM carrier does not check DoP."));
        }
        let inspection = self.audio_output.inspection.as_ref().ok_or_else(|| chilpa_i18n::t("Inspect an output before checking it."))?;
        if request.output != self.audio_output.selection || request.generation != inspection.identity.generation {
            return Err(chilpa_i18n::t("Equipment changed. Read its details again before checking a format."));
        }
        if inspection.identity.backend == "cast" || !inspection.render_available || matches!(inspection.state,State::Disconnected|State::Stale) {
            return Err(chilpa_i18n::t("Silent opening checks are unavailable for this output."));
        }
        let expected = inspection.identity.clone();
        let selected = self.audio_output.selection.clone();
        let exclusive = request.exclusive;
        let cancel = Arc::new(AtomicBool::new(false));
        let from_worker = cancel.clone();
        let path = chilpa_dirs::config().join("audio-output-checks.json");
        let (tx,rx) = mpsc::sync_channel(1);
        std::thread::Builder::new().name("chilpa-output-check".into()).spawn(move || {
            let mut case = if engine::inspect(&selected).identity == expected {
                engine::check(&selected,requested,exclusive,&from_worker)
            } else { Case { identity:expected,requested,exclusive_requested:exclusive,opened:None,
                observation:Default::default(),state:State::Stale,physical_format:State::Unknown,method:"silent_open".into(),
                timestamp:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),audible:State::Untested,error:String::new() } };
            if from_worker.load(Ordering::Acquire) { case.state = State::Cancelled; }
            let (history,error) = match history::append(&path,case.clone()) {
                Ok(rows) => (rows,String::new()), Err(e) => (history::load(&path).unwrap_or_default(),e),
            };
            let inspection = engine::inspect(&selected);
            let hardware_generation = chilpa_core::backends::generation();
            let _ = tx.send(ResultDoc { selection:selected,inspection,outputs:listed(&[]),history,last:Some(case),error,hardware_generation });
        }).map_err(|_| chilpa_i18n::t("Audio check could not start."))?;
        self.audio_output.cancel = cancel; self.audio_output.pending = Some(rx); self.audio_output.checking = true;
        self.audio_output.saved = false; Ok(())
    }
}

fn remote_id(d: &chilpa_cast::Device) -> String { format!("cast:{:?}:{}",d.kind,d.id) }
fn listed(remotes: &[chilpa_cast::Device]) -> Vec<Value> {
    let mut outputs: Vec<_> = chilpa_core::outputs().into_iter().take(256)
        .filter(|o| !o.id.starts_with("file:") || std::env::var_os("CHILPA_DEV").is_some_and(|v| v == "1"))
        .map(|o| json!({"id":o.id,"label":o.label,"detail":o.detail})).collect();
    outputs.extend(remotes.iter().take(256).map(|d| json!({"id":remote_id(d),"label":d.name,"detail":d.model})));
    outputs
}
fn inspect(selected: &str, remotes: &[chilpa_cast::Device]) -> Inspection {
    if let Some(d) = remotes.iter().find(|d| remote_id(d) == selected) {
        let mut doc = Inspection::unavailable("cast",selected,format!("{:?}:{}",d.kind,d.model));
        doc.identity.stable_id = d.id.clone(); doc.identity.strong = !d.id.is_empty();
        doc.identity.transport = format!("{:?}",d.kind); doc.render_available = false;
        doc.method = "renderer_discovery".into(); return doc;
    }
    engine::inspect(selected)
}
