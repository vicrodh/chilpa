//! One requested install or probe; no worker exists while idle.
mod process;
use super::packages;
use chilpa_meta::beets;
use std::{path::Path, process::Child, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread::JoinHandle, time::{Duration, Instant}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure { Cancelled, Offline, Unavailable, Download, Verify, Archive, Install, Version, Timeout, OutputLimit, DiskLimit, Storage, Busy }
impl Failure {
    fn message(self) -> String {
        match self {
            Self::Cancelled => chilpa_i18n::t("The beets task was cancelled."),
            Self::Offline => chilpa_core::network::blocked(),
            Self::Unavailable => chilpa_i18n::t("Automatic beets installation is not available on this platform."),
            Self::Download => chilpa_i18n::t("The beets installer could not be downloaded. Try again."),
            Self::Verify => chilpa_i18n::t("The installer did not match its pinned checksum."),
            Self::Archive => chilpa_i18n::t("The installer archive could not be opened safely."),
            Self::Install => chilpa_i18n::t("The private beets environment could not be installed. The previous runtime is unchanged."),
            Self::Version => chilpa_i18n::t("The selected environment does not provide beets 2.14.1."),
            Self::Timeout => chilpa_i18n::t("The beets task reached its time limit."),
            Self::OutputLimit => chilpa_i18n::t("The installer exceeded its output limit."),
            Self::DiskLimit => chilpa_i18n::t("The private beets environment exceeded its size limit."),
            Self::Storage => chilpa_i18n::t("The private beets environment could not be saved."),
            Self::Busy => chilpa_i18n::t("The previous beets task is still stopping."),
        }
    }
}
#[derive(Clone, Copy)]
enum Phase { Idle, Waiting, Bootstrap, Python, Packages, Checking, Ready, Cancelling, Failed(Failure) }
struct Status { phase: Phase, progress: f64, installing: bool }
pub(super) struct View { pub message: String, pub progress: f64, pub installing: bool }
struct Outcome { python: String, installed: bool }
pub(super) struct Shared {
    status: Mutex<Status>,
    cancel: AtomicBool,
    child: Mutex<Option<Child>>,
    result: Mutex<Option<Result<Outcome, Failure>>>,
}
impl Shared {
    fn phase(&self, phase: Phase, progress: f64) {
        let mut status = self.status.lock().unwrap_or_else(|error| error.into_inner());
        status.phase = phase; status.progress = progress;
    }
    fn check(&self, online: bool) -> Result<(), Failure> {
        if self.cancel.load(Ordering::Acquire) { Err(Failure::Cancelled) }
        else if online && chilpa_core::network::current().offline { Err(Failure::Offline) }
        else { Ok(()) }
    }
    fn kill(&self) {
        if let Some(mut child) = self.child.lock().unwrap_or_else(|error| error.into_inner()).take() {
            let _ = child.kill(); let _ = child.wait();
        }
    }
}
pub(crate) struct Managed {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    checked: Option<String>,
}
impl Drop for Managed {
    fn drop(&mut self) { self.shared.cancel.store(true,Ordering::Release); self.shared.kill(); }
}
impl Managed {
    pub(super) fn uninitialized() -> Self {
        Self { shared:Arc::new(Shared { status:Mutex::new(Status { phase:Phase::Idle,progress:0.0,installing:false }),
            cancel:AtomicBool::new(false), child:Mutex::new(None), result:Mutex::new(None) }), worker:None, checked:None }
    }
    pub(super) fn check_configured(&mut self) {
        self.start(false,move |shared| {
            shared.check(false)?;
            let settings = chilpa_core::settings::load();
            let python = settings.window.get(super::PYTHON).filter(|value| !value.is_empty()).cloned()
                .or_else(|| packages::installed().map(|item| item.python().to_string_lossy().into_owned()))
                .ok_or(Failure::Unavailable)?;
            shared.phase(Phase::Checking,0.0);
            let cache = workspace()?;
            probe(&shared,Path::new(&python),cache.path())?;
            Ok(Outcome { python,installed:false })
        });
    }
    pub(super) fn new() -> Self {
        let installed = packages::installed().map(|item| item.python().to_string_lossy().into_owned());
        let phase = if installed.is_some() { Phase::Ready } else { Phase::Idle };
        Self { shared:Arc::new(Shared { status:Mutex::new(Status { phase,progress:0.0,installing:false }),
            cancel:AtomicBool::new(false), child:Mutex::new(None), result:Mutex::new(None) }), worker:None, checked:installed }
    }
    pub(super) fn verified_runtime(&self) -> Option<String> { self.ready().then(|| self.checked.clone()).flatten() }
    pub(super) fn ready(&self) -> bool {
        !self.busy() && self.checked.is_some()
            && matches!(self.shared.status.lock().unwrap_or_else(|error| error.into_inner()).phase, Phase::Ready)
    }
    pub(super) fn forget_other(&mut self, runtime: &str) { if self.checked.as_deref() != Some(runtime) { self.checked = None; } }
    pub(super) fn busy(&self) -> bool { self.worker.is_some() }
    pub(super) fn cancel(&self) {
        self.shared.cancel.store(true,Ordering::Release);
        let progress = self.shared.status.lock().unwrap_or_else(|error| error.into_inner()).progress;
        self.shared.phase(Phase::Cancelling,progress);
    }
    fn start(&mut self, installing: bool, task: impl FnOnce(Arc<Shared>) -> Result<Outcome,Failure> + Send + 'static) {
        if self.busy() { return; }
        self.shared.cancel.store(false,Ordering::Release);
        *self.shared.status.lock().unwrap_or_else(|error| error.into_inner()) = Status { phase:Phase::Waiting,progress:0.0,installing };
        let shared = self.shared.clone();
        let worker = std::thread::Builder::new().name("beets-setup".into()).spawn(move || {
            let result = task(shared.clone());
            match &result {
                Ok(_) => shared.phase(Phase::Ready,1.0),
                Err(error) => shared.phase(Phase::Failed(*error),0.0),
            }
            *shared.result.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        });
        match worker {
            Ok(worker) => self.worker = Some(worker),
            Err(_) => self.shared.phase(Phase::Failed(Failure::Install),0.0),
        }
    }
    pub(super) fn install(&mut self, previous: String, lease: Arc<AtomicBool>) {
        self.start(true,move |shared| install(&shared,&previous,&lease));
    }
    pub(super) fn check(&mut self, python: String) {
        self.start(false,move |shared| {
            shared.check(false)?;
            shared.phase(Phase::Checking,0.0);
            let cache = workspace()?;
            probe(&shared,Path::new(&python),cache.path())?;
            Ok(Outcome { python,installed:false })
        });
    }
    pub(super) fn poll(&mut self) -> Option<bool> {
        if !self.worker.as_ref().is_some_and(JoinHandle::is_finished) { return None; }
        if self.worker.take()?.join().is_err() {
            self.shared.phase(Phase::Failed(Failure::Install),0.0);
            self.checked = None;
            return Some(false);
        }
        let result = self.shared.result.lock().unwrap_or_else(|error| error.into_inner()).take()?;
        match result {
            Ok(outcome) => { self.shared.phase(Phase::Ready,1.0); self.checked = Some(outcome.python); Some(outcome.installed) }
            Err(error) => { self.shared.phase(Phase::Failed(error),0.0); self.checked = None; Some(false) }
        }
    }
    pub(super) fn view(&self) -> View {
        let status = self.shared.status.lock().unwrap_or_else(|error| error.into_inner());
        let message = match status.phase {
            Phase::Idle => if packages::supported() { chilpa_i18n::t("Download a private copy managed by Chilpa.") } else { Failure::Unavailable.message() },
            Phase::Waiting => chilpa_i18n::t("Preparing beets…"),
            Phase::Bootstrap => chilpa_i18n::t("Downloading the verified installer…"),
            Phase::Python => chilpa_i18n::t("Installing private Python…"),
            Phase::Packages => chilpa_i18n::t("Installing beets and its pinned packages…"),
            Phase::Checking => chilpa_i18n::t("Checking beets…"),
            Phase::Ready => chilpa_i18n::t("beets 2.14.1 is ready."),
            Phase::Cancelling => chilpa_i18n::t("Cancelling the beets task…"),
            Phase::Failed(error) => error.message(),
        };
        View { message,progress:status.progress,installing:status.installing }
    }
    pub(super) fn description(&self, python: &str) -> String {
        if self.busy() { return self.view().message; }
        if python.is_empty() { return chilpa_i18n::t("Not installed"); }
        if self.checked.as_deref() == Some(python) { self.view().message }
        else { chilpa_i18n::t("Not checked") }
    }
}
fn workspace() -> Result<tempfile::TempDir, Failure> {
    let cache = chilpa_dirs::cache().join("beets");
    std::fs::create_dir_all(&cache).map_err(|_| Failure::Storage)?;
    tempfile::Builder::new().prefix("setup-").tempdir_in(cache).map_err(|_| Failure::Storage)
}
fn probe(shared: &Shared, python: &Path, cache: &Path) -> Result<(),Failure> {
    shared.check(false)?;
    let response = beets::lookup(python,cache,&beets::Request::probe(),&shared.cancel).map_err(|error| match error {
        beets::Error::Cancelled => Failure::Cancelled,
        beets::Error::Timeout => Failure::Timeout,
        _ => Failure::Version,
    })?;
    if response.beets_version != packages::BEETS { return Err(Failure::Version); }
    shared.check(false)
}
fn install(shared: &Arc<Shared>, previous: &str, lease: &AtomicBool) -> Result<Outcome,Failure> {
    shared.check(true)?;
    if !packages::supported() { return Err(Failure::Unavailable); }
    let deadline = Instant::now()+Duration::from_secs(15*60);
    let stopping = Instant::now();
    while lease.load(Ordering::Acquire) {
        shared.check(true)?;
        if stopping.elapsed() > Duration::from_secs(2) { return Err(Failure::Busy); }
        std::thread::sleep(Duration::from_millis(20));
    }
    let cache = workspace()?;
    shared.phase(Phase::Bootstrap,0.1);
    let bootstrap = packages::bootstrap(cache.path())?;
    shared.check(true)?;
    let directory = packages::generation(previous)?;
    let environment = directory.path().join("environment");
    shared.phase(Phase::Python,0.3);
    process::run(shared,&bootstrap.executable,&[
        "--no-config".into(),"--no-progress".into(),"--system-certs".into(),"venv".into(),"--no-project".into(),
        "--managed-python".into(),"--python".into(),packages::PYTHON.into(),environment.as_os_str().to_owned(),
    ],directory.path(),cache.path(),deadline)?;
    packages::check_size(directory.path())?;
    let python = chilpa_dirs::python_environment(&environment);
    let requirements = cache.path().join("requirements.txt");
    std::fs::write(&requirements,packages::REQUIREMENTS).map_err(|_| Failure::Storage)?;
    shared.phase(Phase::Packages,0.65);
    process::run(shared,&bootstrap.executable,&[
        "--no-config".into(),"--no-progress".into(),"--system-certs".into(),"pip".into(),"install".into(),
        "--python".into(),python.as_os_str().to_owned(),"--require-hashes".into(),"--only-binary".into(),":all:".into(),
        "--link-mode".into(),"copy".into(),"--index-url".into(),"https://pypi.org/simple".into(),
        "--requirements".into(),requirements.as_os_str().to_owned(),
    ],directory.path(),cache.path(),deadline)?;
    packages::check_size(directory.path())?;
    shared.check(true)?;
    shared.phase(Phase::Checking,0.9);
    let check = cache.path().join("check");
    std::fs::create_dir(&check).map_err(|_| Failure::Storage)?;
    probe(shared,&python,&check)?;
    packages::check_size(directory.path())?;
    shared.check(true)?;
    let python = python.to_string_lossy().into_owned();
    packages::activate(directory,previous)?;
    Ok(Outcome { python,installed:true })
}

#[cfg(test)]
mod tests;
