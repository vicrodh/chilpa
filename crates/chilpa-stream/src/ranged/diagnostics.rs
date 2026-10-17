//! Numeric buffer evidence, enabled only for a memory investigation.
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub(super) struct MemoryTrace(u64);

impl MemoryTrace {
    pub(super) fn open() -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let enabled = *ENABLED.get_or_init(|| std::env::var("CHILPA_STREAM_MEMORY_DIAGNOSTICS").is_ok_and(|v| v == "1"));
        Self(if enabled { NEXT.fetch_add(1, Ordering::Relaxed) } else { 0 })
    }

    pub(super) fn response(self, status: u16, offset: u64, requested: u64, total: Option<u64>, len: usize, capacity: usize) {
        if self.0 == 0 { return; }
        // I accept numbers only: no address, headers, error text or audio.
        let _ = write_response(&mut io::stderr().lock(), self.0, status, offset, requested, total, len, capacity);
    }

    pub(super) fn close(self, len: usize, capacity: usize, ahead_pending: bool) {
        if self.0 == 0 { return; }
        // A pending receiver may hold another window or an unfinished request.
        // I report its presence, not an unmeasured byte count.
        let _ = writeln!(io::stderr().lock(), "stream-memory {}", serde_json::json!({
            "event": "close", "stream": self.0, "bytes": len, "capacity": capacity,
            "ahead_pending": ahead_pending,
        }));
    }
}

fn write_response(
    out: &mut impl Write, stream: u64, status: u16, offset: u64, requested: u64,
    total: Option<u64>, len: usize, capacity: usize,
) -> io::Result<()> {
    writeln!(out, "stream-memory {}", serde_json::json!({
        "event": "response", "stream": stream, "status": status, "offset": offset,
        "requested": requested, "total": total, "bytes": len, "capacity": capacity,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_distinguishes_reserved_memory_from_received_bytes() {
        let mut out = Vec::new();
        write_response(&mut out, 7, 200, 0, 524_288, Some(900_000), 900_000, 1_048_576).unwrap();
        let line = String::from_utf8(out).unwrap();
        let record: serde_json::Value = serde_json::from_str(line.strip_prefix("stream-memory ").unwrap()).unwrap();
        assert_eq!(record["status"], 200);
        assert_eq!(record["bytes"], 900_000);
        assert_eq!(record["capacity"], 1_048_576);
        assert_eq!(record["requested"], 524_288);
        assert_eq!(record["stream"], 7);
        assert!(record.as_object().unwrap().iter().all(|(key, value)| key == "event" || value.is_number()));
    }

}
