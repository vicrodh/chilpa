//! One compressed access unit of lookahead, released after seek or drop.
//! Non-A/52 decoders do not prefetch. A deferred read error never discards
//! the last complete access unit, including the final packet at EOF.
use symphonia::core::{
    errors::{Error, Result},
    formats::{FormatReader, Packet},
};

#[derive(Default)]
pub(crate) struct Packets {
    pending: Option<Packet>,
    deferred: Option<Error>,
}
impl Packets {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn lookahead(&self) -> Option<&Packet> {
        self.pending.as_ref()
    }
    pub(crate) fn next(
        &mut self,
        format: &mut dyn FormatReader,
        id: u32,
        prefetch: bool,
    ) -> Result<Packet> {
        let current = if let Some(packet) = self.pending.take() {
            packet
        } else if let Some(error) = self.deferred.take() {
            return Err(error);
        } else {
            read(format, id)?
        };
        if prefetch {
            match read(format, id) {
                Ok(packet) => self.pending = Some(packet),
                Err(error) => self.deferred = Some(error),
            }
        }
        Ok(current)
    }
}
fn read(format: &mut dyn FormatReader, id: u32) -> Result<Packet> {
    loop {
        let packet = format.next_packet()?;
        if packet.track_id() == id {
            return Ok(packet);
        }
    }
}
