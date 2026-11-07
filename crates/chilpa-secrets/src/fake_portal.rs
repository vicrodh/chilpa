//! A stand-in for the desktop portal on a private bus, for the tests that
//! run without Flatpak, after chilpa-bwrap's. It answers the version and
//! `RetrieveSecret` the way each test asks, writes the value into the
//! descriptor it was handed, and keeps what each request asked for.

use crate::portal::{Bus, INTERFACE, NAME, PATH, REQUEST};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::{Fd, ObjectPath, OwnedValue, Value};

/// A session bus of its own, from the system's dbus-daemon, with no service
/// files: a name nobody owns is not started on demand.
pub(crate) struct PrivateBus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    pub(crate) fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(&config, format!(r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#, dir.path().join("bus").display())).unwrap();
        let mut daemon = Command::new("/usr/bin/dbus-daemon").arg("--config-file").arg(&config)
            .args(["--nofork", "--nopidfile", "--print-address"])
            .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("these tests start a private dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut address).unwrap();
        Self { daemon, address: address.trim().to_string(), _dir: dir }
    }

    pub(crate) fn bus(&self) -> Bus {
        Bus::Address(self.address.clone())
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// How the stand-in answers.
#[derive(Clone)]
pub(crate) enum Answer {
    /// Writes the value, closes its end and answers 0.
    Value(Vec<u8>),
    /// Writes the value and answers 0, but keeps its end open.
    ValueKeptOpen(Vec<u8>),
    /// Writes nothing and answers with this response.
    Ended(u32),
    /// Hands back the request and never answers it.
    Never,
    /// Serves no such interface: the version cannot be read.
    NoInterface,
}

#[derive(Default)]
struct Record {
    /// The `handle_token` of each request.
    names: Vec<String>,
    /// Ends kept open on purpose.
    kept: Vec<std::fs::File>,
}

pub(crate) struct FakePortal {
    record: Arc<Mutex<Record>>,
    _conn: Connection,
}

impl FakePortal {
    /// Owns the portal's name on `bus` and answers as `answer` says.
    pub(crate) fn serve(bus: &PrivateBus, answer: Answer) -> Self {
        let conn = Builder::address(bus.address.as_str()).unwrap().build().unwrap();
        // Read from before the name is taken, so no call is missed.
        let messages = MessageIterator::from(&conn);
        conn.request_name(NAME).unwrap();
        let record = Arc::<Mutex<Record>>::default();
        let (server, kept) = (conn.clone(), record.clone());
        std::thread::spawn(move || {
            for message in messages {
                let Ok(message) = message else { break };
                respond(&server, &message, &answer, &kept);
            }
        });
        Self { record, _conn: conn }
    }

    /// The name each request gave its request object, in order.
    pub(crate) fn requests(&self) -> Vec<String> {
        self.record.lock().unwrap().names.clone()
    }
}

fn respond(conn: &Connection, message: &Message, answer: &Answer, record: &Arc<Mutex<Record>>) {
    if message.message_type() != Type::MethodCall {
        return;
    }
    let header = message.header();
    let _ = match header.member().map(|member| member.as_str()) {
        Some("Get") => match answer {
            Answer::NoInterface => conn.reply_dbus_error(&header, zbus::fdo::Error::UnknownInterface(INTERFACE.into())),
            _ => conn.reply(&header, &Value::U32(1)),
        },
        Some("RetrieveSecret") => retrieve(conn, message, answer, record),
        _ => Ok(()),
    };
}

fn retrieve(conn: &Connection, message: &Message, answer: &Answer, record: &Arc<Mutex<Record>>) -> zbus::Result<()> {
    let header = message.header();
    let body = message.body();
    let (fd, options): (Fd<'_>, HashMap<String, Value<'_>>) = body.deserialize()?;
    let name = match options.get("handle_token") {
        Some(Value::Str(name)) => name.to_string(),
        _ => "none".to_string(),
    };
    let caller = header.sender().map(|sender| sender.to_string()).unwrap_or_default();
    // The request object's path, as the portal builds it from the caller.
    let path = format!("{PATH}/request/{}/{name}", caller.trim_start_matches(':').replace('.', "_"));
    let mut end = std::fs::File::from(fd.as_fd().try_clone_to_owned().map_err(|e| zbus::Error::Failure(e.to_string()))?);
    let response = match answer {
        Answer::Value(value) => {
            end.write_all(value).map_err(|e| zbus::Error::Failure(e.to_string()))?;
            drop(end);
            Some(0)
        }
        Answer::ValueKeptOpen(value) => {
            end.write_all(value).map_err(|e| zbus::Error::Failure(e.to_string()))?;
            record.lock().unwrap().kept.push(end);
            Some(0)
        }
        Answer::Ended(response) => Some(*response),
        Answer::Never | Answer::NoInterface => None,
    };
    record.lock().unwrap().names.push(name);
    conn.reply(&header, &ObjectPath::try_from(path.as_str())?)?;
    match response {
        Some(response) => conn.emit_signal(
            Some(caller.as_str()),
            path.as_str(),
            REQUEST,
            "Response",
            &(response, HashMap::<String, OwnedValue>::new()),
        ),
        None => Ok(()),
    }
}
