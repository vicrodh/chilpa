//! Framed pipes with deadline-bound host waits. Sandbox launch stays outside
//! the protocol crate; the supplied revoker must terminate the whole worker.
//!
//! Stopping waits [`STOP_WAIT`] for the two pipe threads after the revoker
//! returns. A thread still blocked on a pipe then, because the worker could
//! not be ended and keeps its end open, is reported and left to finish when
//! that pipe closes.

use crate::bridge::{BridgeError, WorkerTransport, MAX_FRAME};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long stopping waits for the pipe threads once the worker was revoked.
const STOP_WAIT: Duration = Duration::from_secs(2);

type WriteJob = (Vec<u8>, SyncSender<Result<(), BridgeError>>);

pub struct PipeTransport {
    writes: Option<SyncSender<WriteJob>>,
    reads: Option<Receiver<Result<Vec<u8>, BridgeError>>>,
    revoker: Option<Box<dyn FnOnce() + Send>>,
    threads: Vec<JoinHandle<()>>,
    /// Each pipe thread's place in `threads`, sent as it ends.
    finished: Receiver<usize>,
}

/// Tells the transport its thread ended, however it ended.
struct Finished(Sender<usize>, usize);

impl Drop for Finished {
    fn drop(&mut self) {
        let _ = self.0.send(self.1);
    }
}

impl PipeTransport {
    /// I accept only pipes from an already isolated worker. Ordinary child
    /// creation is deliberately not part of this constructor.
    pub fn isolated(
        mut input: impl Write + Send + 'static,
        mut output: impl Read + Send + 'static,
        revoke: impl FnOnce() + Send + 'static,
    ) -> Self {
        let (writes, jobs) = mpsc::sync_channel::<WriteJob>(1);
        let (ended, finished) = mpsc::channel();
        let writer_ended = Finished(ended.clone(), 0);
        let writer = std::thread::spawn(move || {
            let _ended = writer_ended;
            while let Ok((bytes, finished)) = jobs.recv() {
                let result = input.write_all(&(bytes.len() as u32).to_be_bytes())
                    .and_then(|()| input.write_all(&bytes)).and_then(|()| input.flush())
                    .map_err(|_| BridgeError::Disconnected);
                let failed = result.is_err();
                let _ = finished.send(result);
                if failed { break; }
            }
        });
        let (replies, reads) = mpsc::sync_channel(1);
        let reader_ended = Finished(ended, 1);
        let reader = std::thread::spawn(move || {
            let _ended = reader_ended;
            loop {
                let result = (|| {
                    let mut prefix = [0; 4];
                    output.read_exact(&mut prefix).map_err(|_| BridgeError::Disconnected)?;
                    let length = u32::from_be_bytes(prefix) as usize;
                    if length == 0 || length > MAX_FRAME { return Err(BridgeError::TooLarge); }
                    let mut bytes = vec![0; length];
                    output.read_exact(&mut bytes).map_err(|_| BridgeError::Disconnected)?;
                    Ok(bytes)
                })();
                let failed = result.is_err();
                if replies.send(result).is_err() || failed { break; }
            }
        });
        Self { writes: Some(writes), reads: Some(reads), revoker: Some(Box::new(revoke)), threads: vec![writer, reader], finished }
    }
}

impl WorkerTransport for PipeTransport {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> Result<(), BridgeError> {
        if bytes.len() > MAX_FRAME { return Err(BridgeError::TooLarge); }
        if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
        let writes = self.writes.as_ref().ok_or(BridgeError::Disconnected)?;
        let (finished, result) = mpsc::sync_channel(1);
        writes.try_send((bytes.to_vec(), finished)).map_err(|_| BridgeError::Disconnected)?;
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(BridgeError::Timeout)?;
        result.recv_timeout(remaining).map_err(|_| BridgeError::Timeout)?
    }

    fn receive(&mut self, deadline: Instant, maximum: usize) -> Result<Vec<u8>, BridgeError> {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(BridgeError::Timeout)?;
        let result = self.reads.as_ref().ok_or(BridgeError::Disconnected)?
            .recv_timeout(remaining).map_err(|_| BridgeError::Timeout)??;
        if result.len() > maximum { return Err(BridgeError::TooLarge); }
        Ok(result)
    }

    fn stop(&mut self) {
        self.writes.take();
        self.reads.take();
        if let Some(revoke) = self.revoker.take() { revoke(); }
        if self.threads.is_empty() { return; }
        let deadline = Instant::now() + STOP_WAIT;
        let mut ended = Vec::new();
        while ended.len() < self.threads.len() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else { break };
            match self.finished.recv_timeout(remaining) {
                Ok(index) => ended.push(index),
                Err(_) => break,
            }
        }
        let mut blocked = Vec::new();
        for (index, thread) in self.threads.drain(..).enumerate() {
            if ended.contains(&index) { let _ = thread.join(); } else { blocked.push(thread); }
        }
        if !blocked.is_empty() {
            // The worker keeps its end of a pipe: the revoker could not end it.
            eprintln!("worker: {} pipe thread(s) still blocked {} s after the worker was revoked; left to end with the pipe",
                blocked.len(), STOP_WAIT.as_secs());
        }
    }
}

impl Drop for PipeTransport {
    fn drop(&mut self) { self.stop(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stopping_comes_back_even_when_the_worker_cannot_be_ended() {
        // The far ends stay open here, as a worker that outlived its revoker
        // keeps them: the reader never sees the pipe close.
        let (worker_input, input) = std::io::pipe().unwrap();
        let (output, worker_output) = std::io::pipe().unwrap();
        let mut transport = PipeTransport::isolated(input, output, || {});
        let (stopped, came_back) = mpsc::channel();
        std::thread::spawn(move || {
            transport.stop();
            let _ = stopped.send(());
        });
        assert!(came_back.recv_timeout(Duration::from_secs(10)).is_ok(), "stop waited a bounded time for its threads");
        drop((worker_input, worker_output));
    }
}
