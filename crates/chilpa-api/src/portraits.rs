//! Visible portraits share two workers and a queue of at most 32 jobs.
//! Each mounted image owns a cancellation handle; there is no polling.
mod cache;
use chilpa_meta::{Http, Reply};
use std::sync::{Arc, Mutex, OnceLock, Condvar, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

#[derive(Default)]
pub struct PortraitImage { pub path: String, pub info: chilpa_meta::portrait::Portrait }

pub struct PortraitRequest(Arc<AtomicBool>);
impl Drop for PortraitRequest { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

struct Job {
    name: String, id: String, alive: Arc<AtomicBool>,
    done: Box<dyn FnOnce(PortraitImage) + Send>,
}

fn allowed() -> bool {
    let policy = chilpa_core::network::current();
    !policy.offline && policy.musicbrainz
}

struct Network { alive: Arc<AtomicBool>, http: crate::metadata::worker::Network }
impl Http for Network {
    fn get(&self, url: &str, accept: &str, timeout: Duration, max_bytes: usize) -> Result<Reply, String> {
        if !self.alive.load(Ordering::Acquire) || !allowed() { return Err("portrait request cancelled".into()); }
        self.http.get(url, accept, timeout, max_bytes)
    }
}

fn resolve(job: &Job) -> PortraitImage {
    let key = format!("{}:{}", job.id, job.name.trim().to_lowercase());
    let cached = cache::get(&key, !allowed());
    let info = match cached {
        Some(info) => info,
        None if allowed() => {
            let http = Network { alive:job.alive.clone(), http:crate::metadata::worker::Network::new() };
            let result = chilpa_meta::portrait::lookup(&chilpa_meta::Net::musicbrainz(&http), &job.name, Some(&job.id));
            if !job.alive.load(Ordering::Acquire) || !allowed() { return PortraitImage::default(); }
            match result {
                Ok(info) => { cache::put(key, info.clone(), false); info }
                Err(_) => { cache::put(key, None, true); None }
            }
        }
        None => None,
    };
    let Some(info) = info else { return PortraitImage::default(); };
    if !job.alive.load(Ordering::Acquire) { return PortraitImage::default(); }
    let path = crate::pictures::kept(&info.image).or_else(|| {
        allowed().then(|| crate::pictures::keep_approved(&info.image,
            &["upload.wikimedia.org".into(),"thumb.wikimedia.org".into()])).flatten()
    }).unwrap_or_default();
    PortraitImage { path, info }
}

struct Queue { jobs:Mutex<std::collections::VecDeque<Job>>, ready:Condvar }
fn queue() -> &'static Arc<Queue> {
    static JOBS: OnceLock<Arc<Queue>> = OnceLock::new();
    JOBS.get_or_init(|| {
        let queue = Arc::new(Queue { jobs:Mutex::new(std::collections::VecDeque::new()), ready:Condvar::new() });
        for index in 0..2 {
            let queue = queue.clone();
            let _ = std::thread::Builder::new().name(format!("chilpa-portraits-{index}")).spawn(move || loop {
                let job = {
                    let mut jobs = queue.jobs.lock().unwrap_or_else(|error| error.into_inner());
                    while jobs.is_empty() { jobs = queue.ready.wait(jobs).unwrap_or_else(|error| error.into_inner()); }
                    jobs.pop_front().unwrap()
                };
                if !job.alive.load(Ordering::Acquire) { continue; }
                let result = resolve(&job);
                if job.alive.load(Ordering::Acquire) { (job.done)(result); }
            });
        }
        queue
    })
}

pub fn request_artist_portrait(name: &str, id: &str, done: impl FnOnce(PortraitImage) + Send + 'static) -> Option<PortraitRequest> {
    if name.trim().is_empty() || name.len() > 512 || id.len() > 36 { return None; }
    let alive = Arc::new(AtomicBool::new(true));
    let job = Job { name:name.trim().into(), id:id.into(), alive:alive.clone(), done:Box::new(done) };
    let queue = queue();
    if !push(queue, job) { return None; }
    Some(PortraitRequest(alive))
}

fn push(queue: &Queue, job: Job) -> bool {
    let mut jobs = queue.jobs.lock().unwrap_or_else(|error| error.into_inner());
    jobs.retain(|job| job.alive.load(Ordering::Acquire));
    if jobs.len() == 32 { return false; }
    jobs.push_back(job);
    queue.ready.notify_one();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portrait_queue_is_bounded_and_cancelled_rows_leave_room_for_the_new_viewport() {
        let queue = Queue { jobs:Mutex::new(std::collections::VecDeque::new()),ready:Condvar::new() };
        let job = || {
            let alive = Arc::new(AtomicBool::new(true));
            (Job { name:"Artist".into(),id:String::new(),alive:alive.clone(),done:Box::new(|_| panic!("no worker in this test")) },PortraitRequest(alive))
        };
        let mut handles = Vec::new();
        for _ in 0..32 { let (job,handle) = job(); assert!(push(&queue,job)); handles.push(handle); }
        let (overflow,_) = job();
        assert!(!push(&queue,overflow));
        drop(handles);
        let (next,handle) = job();
        assert!(push(&queue,next));
        assert_eq!(queue.jobs.lock().unwrap().len(),1);
        drop(handle);
        assert!(!queue.jobs.lock().unwrap()[0].alive.load(Ordering::Acquire));
    }
}
