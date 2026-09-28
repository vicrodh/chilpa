//! One reader for the reservation connection, including during method calls.
//!
//! A blocking D-Bus call beside `Connection::process` can lose its reply.
//! Here the caller pumps replies and incoming object calls itself; after
//! acquisition the entire connection moves to the reservation worker.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use dbus::blocking::{BlockingSender, Connection, Proxy};
use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::channel::Sender;
use dbus::{Error, Message, MessageType};
use dbus_crossroads::Crossroads;

pub(super) struct ReservationBus {
    pub connection: Connection,
    object: RefCell<Crossroads>,
}

impl ReservationBus {
    pub fn new(connection: Connection, card: u32) -> Self {
        let mut object = Crossroads::new();
        let reservation = object.register("org.freedesktop.ReserveDevice1", |interface| {
            interface.method("RequestRelease", ("priority",), ("result",),
                |_, _: &mut String, (_priority,): (i32,)| Ok((false,)));
            interface.property::<i32, _>("Priority").get(|_, _| Ok(super::PRIORITY));
            interface.property::<String, _>("ApplicationName").get(|_, _| Ok("Chilpa".to_string()));
            interface.property::<String, _>("ApplicationDeviceName").get(|_, device| Ok(device.clone()));
        });
        object.insert(format!("/org/freedesktop/ReserveDevice1/Audio{card}"),
            &[reservation], format!("ALSA card {card}"));
        Self { connection, object: RefCell::new(object) }
    }

    pub fn proxy<'a>(&'a self, name: &'a str, path: &'a str) -> Proxy<'a, &'a Self> {
        Proxy::new(name, path, super::PATIENCE, self)
    }

    pub fn request_name(&self, name: &str) -> Result<RequestNameReply, Error> {
        let (reply,): (u32,) = self.proxy("org.freedesktop.DBus", "/org/freedesktop/DBus")
            .method_call("org.freedesktop.DBus", "RequestName", (name, 0u32))?;
        use RequestNameReply::*;
        [PrimaryOwner, InQueue, Exists, AlreadyOwner].into_iter().find(|v| *v as u32 == reply)
            .ok_or_else(|| Error::new_failed("Invalid name request reply"))
    }

    fn dispatch(&self, message: Message) {
        if message.msg_type() == MessageType::MethodCall {
            let _ = self.object.borrow_mut().handle_message(message, &self.connection);
        }
    }

    pub fn process(&self, timeout: Duration) -> Result<(), Error> {
        if let Some(message) = self.connection.channel().blocking_pop_message(timeout)? {
            self.dispatch(message);
        }
        Ok(())
    }
}

impl BlockingSender for ReservationBus {
    fn send_with_reply_and_block(&self, message: Message, timeout: Duration) -> Result<Message, Error> {
        let serial = self.connection.send(message).map_err(|_| Error::new_failed("Could not send reservation request"))?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::new_custom("org.freedesktop.DBus.Error.NoReply", "Reservation request timed out"));
            }
            if let Some(mut reply) = self.connection.channel().blocking_pop_message(remaining)? {
                if reply.get_reply_serial() == Some(serial) {
                    reply.as_result()?;
                    return Ok(reply);
                }
                // A sound server may inspect our Priority before answering
                // RequestRelease. Answer it here, without a second reader.
                self.dispatch(reply);
            }
        }
    }
}
