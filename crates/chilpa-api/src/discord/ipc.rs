//! Discord's local RPC, spoken as Fooyin's client speaks it
//! (`src/plugins/discord/discordipcclient.cpp`).
//!
//! A frame is a little-endian opcode and length, then that many bytes of
//! JSON. The handshake has to be answered `DISPATCH`/`READY`, and every
//! `SET_ACTIVITY` waits for its own answer, up to five seconds: Discord
//! says there when it refuses a status, and an answer nobody reads piles up
//! in the socket. Discord's pipes are looked for where Fooyin looks, plus
//! the two clients `discord-rich-presence` knew.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Discord numbers its pipes from 0; Fooyin tries ten.
const PIPES: usize = 10;
/// How long an answer may take (Fooyin's `ReplyTimeout`).
const ANSWER_WAIT: Duration = Duration::from_secs(5);
/// No answer Discord gives is near this; past it the stream is not Discord.
const LARGEST_FRAME: u32 = 64 * 1024;
/// Frames read while looking for the answer to one command.
const FRAMES_READ: usize = 8;

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;

/// Where a client of Discord's leaves its pipe, under a socket folder.
const CLIENT_FOLDERS: [&str; 4] = ["app/com.discordapp.Discord", "app/dev.vencord.Vesktop", "snap.discord", "snap.discord-canary"];

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

/// Why a command did not take.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The connection is gone or speaks something else: drop it.
    Broken(String),
    /// Discord answered, and refused. The connection is fine.
    Refused(String),
}

pub(crate) struct Connection {
    pipe: Box<dyn Pipe>,
    sent: u64,
}

/// The folders to look in: each socket folder, then Discord's own folders
/// under it and every Flatpak's `xdg-run`, as they are there.
fn pipe_folders(bases: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut add = |folder: PathBuf| {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    };
    for base in bases {
        add(base.clone());
    }
    for base in bases {
        for client in CLIENT_FOLDERS {
            let folder = base.join(client);
            if folder.is_dir() {
                add(folder);
            }
        }
        let mut sandboxes: Vec<PathBuf> = std::fs::read_dir(base.join(".flatpak")).into_iter().flatten().flatten()
            .map(|entry| entry.path().join("xdg-run")).filter(|folder| folder.is_dir()).collect();
        sandboxes.sort();
        sandboxes.into_iter().for_each(&mut add);
    }
    folders
}

/// Every pipe that is there, in the order Fooyin tries them.
fn pipes_in(bases: &[PathBuf]) -> Vec<PathBuf> {
    pipe_folders(bases).iter()
        .flat_map(|folder| (0..PIPES).map(move |n| folder.join(format!("discord-ipc-{n}"))))
        .filter(|pipe| pipe.exists())
        .collect()
}

pub(crate) fn pipes() -> Vec<PathBuf> {
    if cfg!(windows) {
        return (0..PIPES).map(|n| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{n}"))).collect();
    }
    pipes_in(&chilpa_dirs::socket_dirs())
}

#[cfg(unix)]
fn open(path: &Path) -> Option<Box<dyn Pipe>> {
    let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(ANSWER_WAIT)).ok()?;
    stream.set_write_timeout(Some(ANSWER_WAIT)).ok()?;
    Some(Box::new(stream))
}

/// A named pipe takes no time limit through the standard library: an
/// answer that never comes holds this thread, and only it.
#[cfg(windows)]
fn open(path: &Path) -> Option<Box<dyn Pipe>> {
    let pipe = std::fs::OpenOptions::new().read(true).write(true).open(path).ok()?;
    Some(Box::new(pipe))
}

#[cfg(not(any(unix, windows)))]
fn open(_: &Path) -> Option<Box<dyn Pipe>> {
    None
}

fn broken(error: impl std::fmt::Display) -> Failure {
    Failure::Broken(error.to_string())
}

impl Connection {
    /// The first pipe that answers the handshake for `client_id`.
    pub(crate) fn open(pipes: &[PathBuf], client_id: &str) -> Result<Connection, Failure> {
        for path in pipes {
            let Some(pipe) = open(path) else { continue };
            let mut connection = Connection { pipe, sent: 0 };
            if connection.handshake(client_id).is_ok() {
                return Ok(connection);
            }
        }
        Err(Failure::Broken("no Discord pipe answered".into()))
    }

    fn handshake(&mut self, client_id: &str) -> Result<(), Failure> {
        self.send(HANDSHAKE, &json!({"v": 1, "client_id": client_id}))?;
        let (_, ready) = self.receive()?;
        if ready["cmd"] == "DISPATCH" && ready["evt"] == "READY" {
            Ok(())
        } else {
            Err(broken("the handshake was not answered READY"))
        }
    }

    fn send(&mut self, opcode: u32, payload: &Value) -> Result<(), Failure> {
        let body = payload.to_string();
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body.as_bytes());
        self.pipe.write_all(&frame).and_then(|()| self.pipe.flush()).map_err(broken)
    }

    fn receive(&mut self) -> Result<(u32, Value), Failure> {
        let mut header = [0u8; 8];
        self.pipe.read_exact(&mut header).map_err(broken)?;
        let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if length > LARGEST_FRAME {
            return Err(broken(format!("a frame of {length} bytes")));
        }
        let mut body = vec![0u8; length as usize];
        self.pipe.read_exact(&mut body).map_err(broken)?;
        Ok((opcode, serde_json::from_slice(&body).map_err(broken)?))
    }

    /// Sets the status, or clears it with `Value::Null`, and reads Discord's
    /// answer.
    pub(crate) fn set_activity(&mut self, activity: Value) -> Result<(), Failure> {
        self.sent += 1;
        let nonce = format!("{}-{}", std::process::id(), self.sent);
        let command = json!({"cmd": "SET_ACTIVITY", "args": {"pid": std::process::id(), "activity": activity}, "nonce": nonce});
        self.send(FRAME, &command)?;
        for _ in 0..FRAMES_READ {
            let (opcode, answer) = self.receive()?;
            match opcode {
                PING => self.send(PONG, &answer)?,
                CLOSE => return Err(broken(format!("Discord closed the connection: {answer}"))),
                _ if answer["nonce"] != nonce.as_str() => {}
                // Discord answers a refusal as `evt: ERROR`; Fooyin looks for
                // `cmd: ERROR`. Either is a refusal.
                _ if answer["evt"] == "ERROR" || answer["cmd"] == "ERROR" => {
                    let message = answer["data"]["message"].as_str().unwrap_or_default();
                    return Err(Failure::Refused(format!("{}: {message}", answer["data"]["code"])));
                }
                _ if answer["cmd"] == "SET_ACTIVITY" => return Ok(()),
                _ => return Err(Failure::Refused(format!("an unexpected answer: {}", answer["cmd"]))),
            }
        }
        Err(broken("no answer to the status"))
    }

    pub(crate) fn close(mut self) {
        let _ = self.send(CLOSE, &json!({}));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn frame(stream: &mut impl Read) -> Option<(u32, Value)> {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).ok()?;
        let length = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let mut body = vec![0u8; length as usize];
        stream.read_exact(&mut body).ok()?;
        Some((u32::from_le_bytes(header[0..4].try_into().unwrap()), serde_json::from_slice(&body).unwrap()))
    }

    fn write(stream: &mut impl Write, opcode: u32, payload: &Value) {
        let body = payload.to_string();
        let mut out = opcode.to_le_bytes().to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body.as_bytes());
        stream.write_all(&out).unwrap();
    }

    /// A Discord that answers the handshake as `ready` says and each status
    /// as `answer` says, and hands back what it was sent.
    fn discord(folder: &Path, ready: Value, answer: fn(&Value) -> Value) -> (PathBuf, std::thread::JoinHandle<Vec<(u32, Value)>>) {
        let path = folder.join("discord-ipc-0");
        let listener = UnixListener::bind(&path).unwrap();
        let heard = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut heard = Vec::new();
            while let Some((opcode, payload)) = frame(&mut peer) {
                match opcode {
                    HANDSHAKE => write(&mut peer, FRAME, &ready),
                    FRAME => write(&mut peer, FRAME, &answer(&payload)),
                    _ => {}
                }
                heard.push((opcode, payload));
                if opcode == CLOSE {
                    break;
                }
            }
            heard
        });
        (path, heard)
    }

    fn ready() -> Value {
        json!({"cmd": "DISPATCH", "evt": "READY", "data": {"v": 1, "user": {"username": "listener"}}})
    }

    fn agreed(command: &Value) -> Value {
        json!({"cmd": "SET_ACTIVITY", "data": command["args"]["activity"], "evt": null, "nonce": command["nonce"]})
    }

    #[test]
    fn a_status_goes_after_the_handshake_and_each_answer_is_read() {
        let folder = tempfile::tempdir().unwrap();
        let (pipe, heard) = discord(folder.path(), ready(), agreed);
        let mut connection = Connection::open(&[pipe], "1562851627461910589").unwrap();
        connection.set_activity(json!({"type": 2, "details": "Time"})).unwrap();
        connection.set_activity(Value::Null).unwrap();
        connection.close();
        let heard = heard.join().unwrap();
        assert_eq!(heard[0], (HANDSHAKE, json!({"v": 1, "client_id": "1562851627461910589"})));
        assert_eq!(heard[1].1["cmd"], "SET_ACTIVITY");
        assert_eq!(heard[1].1["args"]["pid"], std::process::id());
        assert_eq!(heard[1].1["args"]["activity"]["details"], "Time");
        assert_ne!(heard[1].1["nonce"], heard[2].1["nonce"], "each command has its own nonce");
        assert_eq!(heard[2].1["args"]["activity"], Value::Null, "a cleared status is a null activity");
        assert_eq!(heard[3].0, CLOSE);
    }

    #[test]
    fn a_refused_status_is_said_and_the_connection_stays() {
        let folder = tempfile::tempdir().unwrap();
        let refuse = |command: &Value| json!({"cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": command["nonce"],
            "data": {"code": 4000, "message": "child \"activity\" fails"}});
        let (pipe, heard) = discord(folder.path(), ready(), refuse);
        let mut connection = Connection::open(&[pipe], "1").unwrap();
        let refused = connection.set_activity(json!({"details": "x"}));
        assert_eq!(refused, Err(Failure::Refused("4000: child \"activity\" fails".into())));
        assert!(matches!(connection.set_activity(json!({"details": "y"})), Err(Failure::Refused(_))), "still answered");
        connection.close();
        assert_eq!(heard.join().unwrap().len(), 4);
    }

    #[test]
    fn a_pipe_that_does_not_answer_ready_is_no_connection() {
        let folder = tempfile::tempdir().unwrap();
        let (pipe, heard) = discord(folder.path(), json!({"cmd": "DISPATCH", "evt": "ERROR"}), agreed);
        assert!(Connection::open(&[folder.path().join("discord-ipc-9"), pipe], "1").is_err());
        assert_eq!(heard.join().unwrap().len(), 1, "only the handshake was heard");
    }

    #[test]
    fn a_frame_past_the_budget_breaks_the_connection() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("discord-ipc-0");
        let listener = UnixListener::bind(&path).unwrap();
        let talker = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            frame(&mut peer);
            let mut huge = 1u32.to_le_bytes().to_vec();
            huge.extend_from_slice(&(LARGEST_FRAME + 1).to_le_bytes());
            peer.write_all(&huge).unwrap();
        });
        assert!(Connection::open(&[path], "1").is_err());
        talker.join().unwrap();
    }

    #[test]
    fn pipes_are_looked_for_in_each_folder_then_in_each_clients_folder() {
        let base = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let client = base.path().join("app/com.discordapp.Discord");
        let sandbox = base.path().join(".flatpak/dev.vencord.Vesktop/xdg-run");
        for folder in [&client, &sandbox] {
            std::fs::create_dir_all(folder).unwrap();
        }
        std::fs::create_dir_all(base.path().join(".flatpak/org.other.App")).unwrap();
        for pipe in [base.path().join("discord-ipc-1"), other.path().join("discord-ipc-0"), client.join("discord-ipc-0"), sandbox.join("discord-ipc-3")] {
            std::fs::write(pipe, b"").unwrap();
        }
        let found = pipes_in(&[base.path().to_path_buf(), other.path().to_path_buf()]);
        assert_eq!(found, [base.path().join("discord-ipc-1"), other.path().join("discord-ipc-0"), client.join("discord-ipc-0"), sandbox.join("discord-ipc-3")]);
    }
}
