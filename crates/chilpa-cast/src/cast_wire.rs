//! The Cast V2 protobuf envelope has five scalar fields and a UTF-8 payload.
use crate::{Error, Result};
use std::io::{Read, Write};

fn varint(out: &mut Vec<u8>, mut n: u64) { while n >= 128 { out.push((n as u8) | 128); n >>= 7; } out.push(n as u8); }
fn string(out: &mut Vec<u8>, field: u8, value: &str) { out.push(field * 8 + 2); varint(out, value.len() as u64); out.extend(value.as_bytes()); }
fn number(bytes: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let b = *bytes.get(*pos).ok_or(Error::Protocol)?; *pos += 1;
        if shift == 63 && b > 1 { return Err(Error::Protocol); }
        value |= ((b & 127) as u64) << shift;
        if b < 128 { return Ok(value); }
    }
    Err(Error::Protocol)
}
pub fn send(stream: &mut impl Write, source: &str, destination: &str, namespace: &str, payload: &serde_json::Value) -> Result<()> {
    let mut bytes = vec![8, 0];
    string(&mut bytes, 2, source); string(&mut bytes, 3, destination); string(&mut bytes, 4, namespace);
    bytes.extend([40, 0]); string(&mut bytes, 6, &payload.to_string());
    stream.write_all(&(bytes.len() as u32).to_be_bytes()).and_then(|_| stream.write_all(&bytes)).and_then(|_| stream.flush()).map_err(|_| Error::Network)
}
pub fn receive(stream: &mut impl Read) -> Result<(String, String, serde_json::Value)> {
    let mut len = [0; 4]; stream.read_exact(&mut len).map_err(|_| Error::Network)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > 1024 * 1024 { return Err(Error::Protocol); }
    let mut bytes = vec![0; len]; stream.read_exact(&mut bytes).map_err(|_| Error::Network)?;
    let (mut pos, mut source, mut namespace, mut payload) = (0, String::new(), String::new(), None);
    while pos < len {
        let key = number(&bytes, &mut pos)?;
        match key & 7 {
            0 => { number(&bytes, &mut pos)?; }
            2 => {
                let count = usize::try_from(number(&bytes, &mut pos)?).map_err(|_| Error::Protocol)?;
                let end = pos.checked_add(count).filter(|n| *n <= len).ok_or(Error::Protocol)?;
                if matches!(key >> 3, 2 | 4 | 6) {
                    let text = std::str::from_utf8(&bytes[pos..end]).map_err(|_| Error::Protocol)?;
                    match key >> 3 { 2 => source = text.into(), 4 => namespace = text.into(), 6 => payload = Some(serde_json::from_str(text).map_err(|_| Error::Protocol)?), _ => {} }
                }
                pos = end;
            }
            _ => return Err(Error::Protocol),
        }
    }
    Ok((source, namespace, payload.ok_or(Error::Protocol)?))
}
