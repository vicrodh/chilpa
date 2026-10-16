//! How long a stream is waited for.
//!
//! A server can accept the connection and never answer, or answer and then
//! go quiet with the socket open. Neither is an error the socket reports:
//! the read just never returns, and the thread that asked never comes back.
//! For the player that is every track after it waiting behind a dead
//! station.
//!
//! So the two waits are bounded here. Getting an answer has a time limit,
//! and once the body is flowing a silence longer than [`Patience::silence`]
//! ends it with an error. The body is read on a thread of its own, because
//! a blocked read cannot be given a deadline from the outside; whoever
//! consumes it waits on a channel, which can.

use std::io::{self, Read};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::time::Duration;

/// How long each part of opening and reading a stream is waited for.
#[derive(Clone, Copy, Debug)]
pub struct Patience {
    /// Reaching the server.
    pub connect: Duration,
    /// From the request to the headers of its reply.
    pub answer: Duration,
    /// Between two pieces of the body.
    pub silence: Duration,
}

impl Default for Patience {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            answer: Duration::from_secs(15),
            silence: Duration::from_secs(20),
        }
    }
}

impl Patience {
    /// A request that gives up when the server does not answer in time.
    pub(crate) fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_resolve(Some(self.connect))
            .timeout_connect(Some(self.connect))
            .timeout_recv_response(Some(self.answer))
            .build()
            .new_agent()
    }
}

/// How much is read ahead of whoever consumes: a few pieces of this size.
const PIECE: usize = 16 * 1024;
const AHEAD: usize = 16;

/// A body read on its own thread, so a silence can be told from a wait.
pub(crate) struct Watched {
    /// Behind a lock only so the reader can cross threads like any other;
    /// it is always reached through `&mut self`, so nothing ever waits on it.
    pieces: std::sync::Mutex<Receiver<io::Result<Vec<u8>>>>,
    current: Vec<u8>,
    at: usize,
    silence: Duration,
    ended: bool,
}

impl Watched {
    pub(crate) fn new(mut body: impl Read + Send + 'static, silence: Duration) -> Self {
        let (tx, pieces) = sync_channel::<io::Result<Vec<u8>>>(AHEAD);
        // The thread ends when the body does, or when nobody is left to
        // read it: the channel closes and the next send fails. While the
        // consumer is paused the channel is full and the send waits, which
        // holds the socket back the way not reading it did.
        let _ = std::thread::Builder::new().name("chilpa-stream-body".into()).spawn(move || loop {
            let mut piece = vec![0u8; PIECE];
            let read = body.read(&mut piece);
            let (message, last) = match read {
                Ok(0) => (Ok(Vec::new()), true),
                Ok(n) => {
                    piece.truncate(n);
                    (Ok(piece), false)
                }
                Err(e) => (Err(e), true),
            };
            if tx.send(message).is_err() || last {
                return;
            }
        });
        Self { pieces: std::sync::Mutex::new(pieces), current: Vec::new(), at: 0, silence, ended: false }
    }
}

impl Read for Watched {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at >= self.current.len() {
            if self.ended {
                return Ok(0);
            }
            let pieces = self.pieces.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner());
            match pieces.recv_timeout(self.silence) {
                Ok(Ok(piece)) if piece.is_empty() => {
                    self.ended = true;
                    return Ok(0);
                }
                Ok(Ok(piece)) => {
                    self.current = piece;
                    self.at = 0;
                }
                Ok(Err(e)) => {
                    self.ended = true;
                    return Err(e);
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.ended = true;
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        chilpa_i18n::t("the stream went quiet"),
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.ended = true;
                    return Ok(0);
                }
            }
        }
        let n = buf.len().min(self.current.len() - self.at);
        buf[..n].copy_from_slice(&self.current[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::time::Instant;

    fn short() -> Patience {
        Patience {
            connect: Duration::from_secs(2),
            answer: Duration::from_millis(600),
            silence: Duration::from_millis(600),
        }
    }

    /// A server that accepts, says what `speaks` makes it say, and then
    /// holds the socket open without another word.
    fn server(speaks: &'static [u8]) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for peer in listener.incoming().flatten() {
                let mut peer = peer;
                let mut asked = [0u8; 1024];
                let _ = peer.read(&mut asked);
                let _ = peer.write_all(speaks);
                let _ = peer.flush();
                held.push(peer);
            }
        });
        port
    }

    #[test]
    fn a_server_that_never_answers_is_given_up_on() {
        let port = server(b"");
        let started = Instant::now();
        let opened = crate::open_with(&format!("http://127.0.0.1:{port}/quiet"), short());
        assert!(opened.is_err(), "nothing came back, so nothing opened");
        assert!(started.elapsed() < Duration::from_secs(5), "it took {:?}", started.elapsed());
    }

    #[test]
    fn a_stream_that_goes_quiet_ends_with_an_error_instead_of_waiting_forever() {
        let port = server(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\n\r\nsome audio");
        let mut stream = crate::open_with(&format!("http://127.0.0.1:{port}/stalls"), short()).expect("it answers");
        let mut heard = Vec::new();
        let started = Instant::now();
        let mut buf = [0u8; 64];
        let ended = loop {
            match stream.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => heard.extend_from_slice(&buf[..n]),
                Err(e) => break Err(e),
            }
        };
        assert_eq!(heard, b"some audio", "what did arrive is handed over first");
        assert_eq!(ended.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5), "it took {:?}", started.elapsed());
    }

    #[test]
    fn a_body_that_ends_is_read_whole_and_then_ends() {
        let mut watched = Watched::new(io::Cursor::new(vec![7u8; 40_000]), Duration::from_secs(2));
        let mut all = Vec::new();
        watched.read_to_end(&mut all).unwrap();
        assert_eq!(all.len(), 40_000);
        assert_eq!(watched.read(&mut [0u8; 8]).unwrap(), 0, "and it stays ended");
    }
}
