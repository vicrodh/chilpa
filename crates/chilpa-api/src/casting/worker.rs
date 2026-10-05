use super::{asset, end::End};
use chilpa_cast::{Device, Kind, Metadata, RemoteTransport, Status, media_server::MediaServer};
use std::{collections::VecDeque, sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc::{self, Receiver, Sender}}, thread::JoinHandle, time::{Duration, Instant}};

pub(super) enum Action { Pause(bool), Seek(f64), Volume(f64), Mute(bool), Stop }
pub(super) enum Order {
    Connect(Device, Option<chilpa_core::player::Player>), Load(String, f64, bool), Action(Action), Disconnect,
}
pub(super) enum Event { Connected, Loaded(Metadata, String, asset::Presentation), Status(Status), Ended, Failed(String), ControlFailed(String), Disconnected }
struct Request { generation: u64, sequence: u64, connection: u64, order: Order }
pub(super) struct Worker {
    orders: Sender<Request>, events: Receiver<(u64, u64, u64, Event)>, sequence: AtomicU64,
    latest: Arc<AtomicU64>, connection: Arc<AtomicU64>, thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn new() -> Self {
        let (tx, orders) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let connection = Arc::new(AtomicU64::new(0));
        let (generation, session) = (latest.clone(), connection.clone());
        let thread = std::thread::spawn(move || run(orders, events, generation, session));
        Self { orders: tx, events: rx, sequence: AtomicU64::new(0), latest, connection, thread: Some(thread) }
    }
    pub fn send(&self, generation: u64, order: Order) {
        self.latest.store(generation, Ordering::Release);
        if matches!(&order, Order::Connect(..) | Order::Disconnect) { self.connection.fetch_add(1, Ordering::AcqRel); }
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        let connection = self.connection.load(Ordering::Acquire);
        let _ = self.orders.send(Request { generation, sequence, connection, order });
    }
    pub fn poll(&self) -> Vec<(u64, Event)> {
        let serial = self.sequence.load(Ordering::Acquire);
        // Only snapshots expire with a control. A volume change must not swallow
        // a load failure or the one-shot event that advances the queue.
        let connection = self.connection.load(Ordering::Acquire);
        self.events.try_iter()
            .filter(|(session, _, stamp, event)| *session == connection && (*stamp == serial || !matches!(event, Event::Status(_))))
            .map(|(_, generation, _, event)| (generation, event)).collect()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.latest.fetch_add(1, Ordering::AcqRel);
        self.connection.fetch_add(1, Ordering::AcqRel);
        let (dummy, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.orders, dummy));
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}
fn run(orders: Receiver<Request>, events: Sender<(u64, u64, u64, Event)>, latest: Arc<AtomicU64>, session: Arc<AtomicU64>) {
    let mut remote: Option<Box<dyn RemoteTransport>> = None;
    let mut server: Option<MediaServer> = None;
    let mut end: Option<End> = None;
    let mut listening: Option<super::reporting::Listening> = None;
    let mut resume: Option<(Instant, f64)> = None;
    let mut active_connection = 0;
    let mut generation = 0;
    let mut sequence = 0;
    let mut last = Instant::now();
    let mut failures = 0;
    let mut pending = VecDeque::<Request>::new();
    loop {
        let request = match pending.pop_front() {
            Some(request) => Ok(request),
            None => orders.recv_timeout(Duration::from_millis(50)),
        };
        match request {
            Ok(Request { generation: stamp, sequence: serial, connection, order }) => {
                pending.extend(orders.try_iter());
                let stale_connection = connection != session.load(Ordering::Acquire);
                let stale_media = stamp != latest.load(Ordering::Acquire);
                if stale_connection || (stale_media && matches!(&order, Order::Action(_) | Order::Load(..))) {
                    // Even an abandoned connect must release the previous local DAC.
                    if let Order::Connect(_, Some(mut player)) = order { player.shutdown(); }
                    continue;
                }
                if matches!(&order, Order::Action(Action::Volume(_))) && pending.iter().any(|r| r.connection == connection && matches!(&r.order, Order::Action(Action::Volume(_)))) { continue; }
                active_connection = connection;
                generation = stamp;
                sequence = serial;
                let connecting = matches!(&order, Order::Connect(..));
                let disruptive = !matches!(&order, Order::Action(_)) || matches!(&order, Order::Action(Action::Stop));
                if disruptive { listening.take(); }
                let command = match &order {
                    Order::Connect(..) => "connect", Order::Load(..) => "load", Order::Disconnect => "disconnect",
                    Order::Action(action) => match action {
                        Action::Pause(true) => "pause", Action::Pause(false) => "play", Action::Seek(_) => "seek",
                        Action::Volume(_) => "volume", Action::Mute(_) => "mute", Action::Stop => "stop",
                    },
                };
                chilpa_core::trace_playback("cast", "command", format_args!("connection={connection} generation={stamp} sequence={serial} command={command}"));
                let result: Result<(), chilpa_cast::Error> = (|| {
                    match order {
                        Order::Connect(device, player) => {
                            end = None;
                            resume = None;
                            server.take();
                            if let Some(mut old) = remote.take() { let _ = old.disconnect(); }
                            if let Some(mut player) = player { player.shutdown(); }
                            if connection != session.load(Ordering::Acquire) { return Ok(()); }
                            let mut transport: Box<dyn RemoteTransport> = match device.kind {
                                Kind::Dlna => Box::new(chilpa_cast::dlna::Dlna::connect(chilpa_cast::dlna::describe(&device.description)?)),
                                Kind::Chromecast => Box::new(chilpa_cast::chromecast::Chromecast::connect(device.address)?),
                            };
                            if connection != session.load(Ordering::Acquire) { let _ = transport.disconnect(); return Ok(()); }
                            let _ = transport.stop();
                            server = Some(MediaServer::start_guarded(device.address, Some(latest.clone()))?);
                            remote = Some(transport);
                            if connection != session.load(Ordering::Acquire) { return Ok(()); }
                            failures = 0;
                            let _ = events.send((connection, latest.load(Ordering::Acquire), serial, Event::Connected));
                        }
                        Order::Load(path, position, paused) => {
                            end = None;
                            resume = None;
                            let server = server.as_ref().ok_or(chilpa_cast::Error::Network)?;
                            server.revoke();
                            let transport = remote.as_mut().ok_or(chilpa_cast::Error::Network)?;
                            // Buffered audio from the previous track must stop even
                            // when resolving the replacement fails.
                            let _ = transport.stop();
                            let (asset, metadata, mut presentation) = asset::resolve(&path)?;
                            if stamp != latest.load(Ordering::Acquire) { return Ok(()); }
                            let mime = asset.mime.clone();
                            chilpa_core::trace_playback("cast", "asset_ready", format_args!("generation={stamp} mime={mime} bytes={}", asset.length));
                            let url = server.publish(asset)?;
                            transport.load(&url, &mime, &metadata)?;
                            chilpa_core::trace_playback("cast", "load_accepted", format_args!("generation={stamp} mime={mime}"));
                            if stamp != latest.load(Ordering::Acquire) { server.revoke(); let _ = transport.stop(); return Ok(()); }
                            listening = presentation.playback.take().map(super::reporting::Listening::new);
                            end = Some(End::new(metadata.duration));
                            let _ = events.send((connection, stamp, serial, Event::Loaded(metadata, mime, presentation)));
                            // Like the reference player, I let the renderer open the
                            // media before seeking. Failure is a control error: the
                            // loaded track remains controllable and visible.
                            if position > 0.0 { resume = Some((Instant::now() + Duration::from_secs(2), position)); }
                            if paused {
                                if let Err(error) = transport.pause() { let _ = events.send((connection, stamp, serial, Event::ControlFailed(error.to_string()))); }
                            }
                        }
                        Order::Action(action) => {
                            let transport = remote.as_mut().ok_or(chilpa_cast::Error::Network)?;
                            match action {
                                Action::Pause(paused) => {
                                    if paused { transport.pause()?; } else { transport.play()?; }
                                }
                                Action::Seek(position) => {
                                    resume = None;
                                    transport.seek(position)?;
                                    if let Some(report) = &mut listening { report.seek(); }
                                    if let Some(end) = &mut end { end.seek(); }
                                }
                                Action::Volume(value) => transport.volume(value)?,
                                Action::Mute(value) => transport.mute(value)?,
                                Action::Stop => {
                                    end = None;
                                    resume = None;
                                    if let Some(server) = &server { server.revoke(); }
                                    transport.stop()?;
                                }
                            }
                        }
                        Order::Disconnect => {
                            end = None;
                            resume = None;
                            server.take();
                            if let Some(mut transport) = remote.take() { let _ = transport.disconnect(); }
                            let _ = events.send((connection, stamp, serial, Event::Disconnected));
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    if connection != session.load(Ordering::Acquire) { continue; }
                    chilpa_core::trace_playback("cast", "command_failed", format_args!("generation={stamp} command={command} cause={error:?} error={error}"));
                    if disruptive {
                        if let Some(server) = &server { server.revoke(); }
                        if let Some(transport) = &mut remote { let _ = transport.stop(); }
                        end = None;
                        resume = None;
                        let stamp = if connecting { latest.load(Ordering::Acquire) } else { stamp };
                        let _ = events.send((connection, stamp, serial, Event::Failed(error.to_string())));
                        if connecting { let _ = events.send((connection, stamp, serial, Event::Disconnected)); }
                    } else { let _ = events.send((connection, stamp, serial, Event::ControlFailed(error.to_string()))); }
                }
                last = Instant::now() - Duration::from_secs(1);
                // Controls already queued take priority over another network poll.
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if generation != latest.load(Ordering::Acquire) { continue; }
        if resume.is_some_and(|(when, _)| Instant::now() >= when) {
            let (_, position) = resume.take().unwrap();
            if let Some(transport) = remote.as_mut() {
                match transport.seek(position) {
                    Ok(()) => { if let Some(end) = &mut end { end.seek(); } if let Some(report) = &mut listening { report.seek(); } }
                    Err(error) => { let _ = events.send((active_connection, generation, sequence, Event::ControlFailed(error.to_string()))); }
                }
            }
        }
        if last.elapsed() < Duration::from_secs(1) { continue; }
        last = Instant::now();
        if let Some(transport) = remote.as_mut() {
            match transport.status() {
                Ok(status) => {
                    failures = 0;
                    if let Some(report) = &mut listening { report.tick(&status, Instant::now()); }
                    let finished = end.as_mut().is_some_and(|end| end.tick(&status));
                    let _ = events.send((active_connection, generation, sequence, Event::Status(status)));
                    if finished {
                        if let Some(mut report) = listening.take() { report.finish("finished"); }
                        end = None;
                        resume = None;
                        if let Some(server) = &server { server.revoke(); }
                        let _ = events.send((active_connection, generation, sequence, Event::Ended));
                    }
                }
                Err(error) => {
                    failures += 1;
                    if failures >= 5 {
                        if let Some(mut report) = listening.take() { report.finish("error"); }
                        end = None;
                        resume = None;
                        server.take();
                        if let Some(mut transport) = remote.take() { let _ = transport.disconnect(); }
                        let _ = events.send((active_connection, generation, sequence, Event::Failed(error.to_string())));
                        let _ = events.send((active_connection, generation, sequence, Event::Disconnected));
                    }
                }
            }
        }
    }
    server.take();
    if let Some(mut transport) = remote { let _ = transport.disconnect(); }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
