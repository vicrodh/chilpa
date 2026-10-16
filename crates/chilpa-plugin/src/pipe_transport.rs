//! Framed pipes with deadline-bound host waits. Sandbox launch stays outside
//! the protocol crate; the supplied revoker must terminate the whole worker.

use crate::bridge::{BridgeError, WorkerTransport, MAX_FRAME};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;
use std::time::Instant;

type WriteJob = (Vec<u8>, SyncSender<Result<(), BridgeError>>);

pub struct PipeTransport {
    writes: Option<SyncSender<WriteJob>>,
    reads: Option<Receiver<Result<Vec<u8>, BridgeError>>>,
    revoker: Option<Box<dyn FnOnce() + Send>>,
    threads: Vec<JoinHandle<()>>,
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
        let writer = std::thread::spawn(move || {
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
        let reader = std::thread::spawn(move || loop {
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
        });
        Self { writes: Some(writes), reads: Some(reads), revoker: Some(Box::new(revoke)), threads: vec![writer, reader] }
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
        for thread in self.threads.drain(..) { let _ = thread.join(); }
    }
}

impl Drop for PipeTransport {
    fn drop(&mut self) { self.stop(); }
}
