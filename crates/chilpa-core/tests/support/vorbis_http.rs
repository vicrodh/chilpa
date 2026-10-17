//! A finite local Ogg asset, with the range contract used by native sources.

use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

pub struct Server {
    pub url: String,
    pub requests: Arc<AtomicUsize>,
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(file: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let Ok((mut peer, _)) = listener.accept() else {
                    break;
                };
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                peer.set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(peer.try_clone().unwrap());
                let mut range = (0, file.len() - 1);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (start, end) = value.trim().split_once('-').unwrap();
                        range = (
                            start.parse().unwrap(),
                            end.parse::<usize>()
                                .unwrap_or(file.len() - 1)
                                .min(file.len() - 1),
                        );
                    }
                }
                let (start, end) = range;
                assert!(start <= end && end < file.len(), "valid finite range");
                seen.fetch_add(1, Ordering::Relaxed);
                let header = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: audio/ogg\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    file.len(),
                    end + 1 - start
                );
                if peer.write_all(header.as_bytes()).is_ok() {
                    let _ = peer.write_all(&file[start..=end]);
                }
            }
        });
        Self {
            url: format!("http://{address}/asset"),
            requests,
            address,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(1));
        self.thread.take().unwrap().join().unwrap();
    }
}

pub struct Opener(pub String);

impl chilpa_core::remote::RemoteOpener for Opener {
    fn locate(
        &self,
        _: &chilpa_models::RemoteEntry,
    ) -> Result<chilpa_core::remote::Located, String> {
        Ok(chilpa_core::remote::Located {
            request: chilpa_core::RangedRequest {
                url: self.0.clone(),
                headers: Vec::new(),
            },
            extension: None,
            server: "Local test".into(),
            playback: None,
            normalization: None,
        })
    }
    fn describe(&self, _: &chilpa_models::RemoteEntry) -> chilpa_core::remote::Described {
        Default::default()
    }
}
