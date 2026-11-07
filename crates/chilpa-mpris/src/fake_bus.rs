//! A session bus of its own and a stand-in service on it, for the tests of
//! the portals that run without a desktop. The service records every method
//! call it receives and answers through the closure it was given.

use crate::portal::Bus;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::message::{Message, Type};

/// The system's dbus-daemon on a socket in a folder of its own, with no
/// service files: a name nobody owns is not started on demand.
pub(crate) struct PrivateBus {
    daemon: Child,
    address: String,
    dir: PathBuf,
}

impl PrivateBus {
    pub(crate) fn start() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("chilpa-mpris-bus-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("bus.conf");
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
"#, dir.join("bus").display())).unwrap();
        let mut daemon = Command::new("/usr/bin/dbus-daemon").arg("--config-file").arg(&config)
            .args(["--nofork", "--nopidfile", "--print-address"])
            .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("these tests start a private dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut address).unwrap();
        Self { daemon, address: address.trim().to_string(), dir }
    }

    pub(crate) fn bus(&self) -> Bus {
        Bus::Address(self.address.clone())
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A service that owns one name on a private bus.
pub(crate) struct FakeService {
    conn: Connection,
    calls: Arc<Mutex<Vec<Message>>>,
}

impl FakeService {
    /// Owns `name` and hands each method call to `answer`, which replies.
    pub(crate) fn serve(bus: &PrivateBus, name: &'static str, answer: impl Fn(&Connection, &Message) + Send + 'static) -> Self {
        let conn = Builder::address(bus.address.as_str()).unwrap().build().unwrap();
        // Read from before the name is taken, so no call is missed.
        let messages = MessageIterator::from(&conn);
        conn.request_name(name).unwrap();
        let calls = Arc::<Mutex<Vec<Message>>>::default();
        let (server, kept) = (conn.clone(), calls.clone());
        std::thread::spawn(move || {
            for message in messages {
                let Ok(message) = message else { break };
                if message.message_type() != Type::MethodCall { continue; }
                kept.lock().unwrap().push(message.clone());
                answer(&server, &message);
            }
        });
        Self { conn, calls }
    }

    /// The method calls received so far, by member name.
    pub(crate) fn members(&self) -> Vec<String> {
        self.calls().iter().filter_map(|call| call.header().member().map(|member| member.to_string())).collect()
    }

    pub(crate) fn calls(&self) -> Vec<Message> {
        self.calls.lock().unwrap().clone()
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }
}

impl Drop for FakeService {
    fn drop(&mut self) {
        let _ = self.conn.clone().close();
    }
}
