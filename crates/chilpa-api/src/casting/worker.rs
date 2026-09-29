use super::{asset, end::End};
use chilpa_cast::{Device, Kind, Metadata, RemoteTransport, Status, media_server::MediaServer};
use std::{sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc::{self, Receiver, Sender}}, thread::JoinHandle, time::{Duration, Instant}};

pub(super) enum Action { Pause(bool), Seek(f64), Volume(f64), Mute(bool), Stop }
pub(super) enum Order {
    Connect(Device, Option<chilpa_core::player::Player>), Load(String, f64, bool), Action(Action), Disconnect,
}
pub(super) enum Event { Connected, Loaded(Metadata, String), Status(Status), Ended, Failed(String), ControlFailed(String), Disconnected }
pub(super) struct Worker {
    orders: Sender<(u64, u64, Order)>, events: Receiver<(u64, u64, Event)>, sequence: AtomicU64, latest: Arc<AtomicU64>, thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn new() -> Self {
        let (tx, orders) = mpsc::channel(); let (events, rx) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0)); let generation = latest.clone();
        let thread = std::thread::spawn(move || run(orders, events, generation));
        Self { orders: tx, events: rx, sequence: AtomicU64::new(0), latest, thread: Some(thread) }
    }
    pub fn send(&self, generation: u64, order: Order) { self.latest.store(generation, Ordering::Release); let serial = self.sequence.fetch_add(1, Ordering::AcqRel) + 1; let _ = self.orders.send((generation, serial, order)); }
    pub fn poll(&self) -> Vec<(u64, Event)> {
        let serial = self.sequence.load(Ordering::Acquire);
        self.events.try_iter().filter(|(_, stamp, event)| *stamp == serial || matches!(event, Event::Loaded(..) | Event::Connected | Event::Disconnected)).map(|(generation, _, event)| (generation, event)).collect()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.latest.fetch_add(1, Ordering::AcqRel);
        // Closing the sender tells the worker to revoke and disconnect.
        let (dummy, _) = mpsc::channel(); drop(std::mem::replace(&mut self.orders, dummy));
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}
fn run(orders: Receiver<(u64, u64, Order)>, events: Sender<(u64, u64, Event)>, latest: Arc<AtomicU64>) {
    let mut remote: Option<Box<dyn RemoteTransport>> = None;
    let mut server: Option<MediaServer> = None;
    let mut end: Option<End> = None;
    let mut generation = 0; let mut sequence = 0; let mut last = Instant::now(); let mut failures = 0;
    loop {
        match orders.recv_timeout(Duration::from_millis(100)) {
            Ok((stamp, serial, order)) => {
                if stamp != latest.load(Ordering::Acquire) && matches!(order, Order::Action(_) | Order::Load(..)) { continue; }
                generation = stamp; sequence = serial;
                let disruptive = !matches!(&order, Order::Action(_)) || matches!(&order, Order::Action(Action::Stop));
                let result: Result<(), chilpa_cast::Error> = (|| {
                    match order {
                        Order::Connect(device, player) => {
                            end = None; server.take();
                            if let Some(mut old) = remote.take() { let _ = old.disconnect(); }
                            if let Some(mut player) = player { player.shutdown(); }
                            let mut transport: Box<dyn RemoteTransport> = match device.kind {
                                Kind::Dlna => Box::new(chilpa_cast::dlna::Dlna::connect(chilpa_cast::dlna::describe(&device.description)?)),
                                Kind::Chromecast => Box::new(chilpa_cast::chromecast::Chromecast::connect(device.address)?),
                            };
                            let _ = transport.stop();
                            server = Some(MediaServer::start_guarded(device.address, Some(latest.clone()))?); remote = Some(transport);
                            let _ = events.send((stamp, serial, Event::Connected));
                        }
                        Order::Load(path, position, paused) => {
                            end = None;
                            let server = server.as_ref().ok_or(chilpa_cast::Error::Network)?;
                            server.revoke();
                            if stamp != latest.load(Ordering::Acquire) { return Ok(()); }
                            let transport = remote.as_mut().ok_or(chilpa_cast::Error::Network)?;
                            let (asset, metadata) = asset::resolve(&path)?;
                            if stamp != latest.load(Ordering::Acquire) { return Ok(()); }
                            let mime = asset.mime.clone(); let url = server.publish(asset)?;
                            transport.load(&url, &mime, &metadata)?;
                            if position > 0.0 { transport.seek(position)?; }
                            if paused { transport.pause()?; }
                            end = Some(End::new(metadata.duration));
                            let _ = events.send((stamp, serial, Event::Loaded(metadata, mime)));
                        }
                        Order::Action(action) => {
                            let transport = remote.as_mut().ok_or(chilpa_cast::Error::Network)?;
                            match action {
                                Action::Pause(true) => transport.pause()?, Action::Pause(false) => transport.play()?,
                                Action::Seek(position) => { transport.seek(position)?; end = Some(End::new(0.0)); }
                                Action::Volume(value) => transport.volume(value)?, Action::Mute(value) => transport.mute(value)?,
                                Action::Stop => { end = None; if let Some(server) = &server { server.revoke(); } transport.stop()?; }
                            }
                        }
                        Order::Disconnect => {
                            end = None; server.take();
                            if let Some(mut transport) = remote.take() { let _ = transport.disconnect(); }
                            let _ = events.send((stamp, serial, Event::Disconnected));
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    if disruptive {
                        if let Some(server) = &server { server.revoke(); }
                        end = None;
                        let _ = events.send((stamp, serial, Event::Failed(error.to_string())));
                    } else { let _ = events.send((stamp, serial, Event::ControlFailed(error.to_string()))); }
                }
                last = Instant::now() - Duration::from_secs(1);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if generation != latest.load(Ordering::Acquire) || last.elapsed() < Duration::from_secs(1) { continue; }
        last = Instant::now();
        if let Some(transport) = remote.as_mut() {
            match transport.status() {
                Ok(status) => {
                    failures = 0;
                    let finished = end.as_mut().is_some_and(|end| end.tick(&status));
                    let _ = events.send((generation, sequence, Event::Status(status)));
                    if finished { end = None; if let Some(server) = &server { server.revoke(); } let _ = events.send((generation, sequence, Event::Ended)); }
                }
                Err(error) => {
                    failures += 1;
                    if failures >= 3 {
                        end = None; server.take(); remote.take();
                        let _ = events.send((generation, sequence, Event::Failed(error.to_string())));
                    }
                }
            }
        }
    }
    server.take();
    if let Some(mut transport) = remote { let _ = transport.disconnect(); }
}
