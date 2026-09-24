//! The limits a plugin runs under.
//!
//! They are not warnings or good intentions: they belong to wasmtime's
//! store, and the engine enforces them whether the plugin likes it or not. A
//! plugin that goes over does not "misbehave": it is stopped.
//!
//! The three are different and all three are needed:
//!
//! - **Fuel** for the one that hangs computing.
//! - **Memory** for the one that grows without stopping.
//! - **Clock** for the one that waits on something that never arrives, which
//!   burns no fuel because it is not executing anything.

use std::time::Duration;

/// How much a plugin can spend per call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Limits {
    /// Instructions, roughly. What it is given per call.
    pub fuel: u64,
    /// How much linear memory it can hold, in bytes.
    pub memory_bytes: usize,
    /// How long a call can take in wall clock time.
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        // A billion units of fuel is on the order of a second of compute; a
        // plugin parsing a reply uses a fraction of it. Thirty two megabytes
        // is enough to hold a large reply and its result in memory. Five
        // seconds of wall clock is more than any reasonable call needs,
        // given that the host is the one waiting on the network, not the
        // plugin.
        Self {
            fuel: 1_000_000_000,
            memory_bytes: 32 * 1024 * 1024,
            timeout: Duration::from_secs(5),
        }
    }
}

impl Limits {
    /// Those of a call expected to be short, like resolving a lyric.
    pub fn short() -> Self {
        Self {
            fuel: 50_000_000,
            memory_bytes: 8 * 1024 * 1024,
            timeout: Duration::from_secs(2),
        }
    }

    /// Those of something that can take a while, like fetching a whole catalogue.
    pub fn long() -> Self {
        Self {
            fuel: 10_000_000_000,
            memory_bytes: 128 * 1024 * 1024,
            timeout: Duration::from_secs(60),
        }
    }

    /// The wasm pages that correspond to the allowed memory.
    ///
    /// A page is 64 KiB; it is the unit a guest's memory grows in, so the limit
    /// is rounded down to whole pages.
    pub fn max_pages(&self) -> usize {
        self.memory_bytes / (64 * 1024)
    }
}

/// Why a call to a plugin was stopped.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Halt {
    /// The fuel ran out: it was computing too much.
    OutOfFuel,
    /// It asked for more memory than it is allowed.
    OutOfMemory,
    /// It went over the clock.
    Timeout(Duration),
}

impl std::fmt::Display for Halt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Halt::OutOfFuel => {
                write!(f, "se pasó del cómputo que tenía permitido y se detuvo")
            }
            Halt::OutOfMemory => write!(f, "pidió más memoria de la permitida"),
            Halt::Timeout(d) => write!(f, "no contestó en {d:?} y se detuvo"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_limits_are_reasonable() {
        let l = Limits::default();
        assert!(l.fuel > 0 && l.memory_bytes > 0 && !l.timeout.is_zero());
        assert_eq!(l.max_pages(), 512, "32 MiB son 512 páginas de 64 KiB");
    }

    #[test]
    fn the_short_limits_are_tighter_than_the_long_ones() {
        let (c, d, l) = (Limits::short(), Limits::default(), Limits::long());
        assert!(c.fuel < d.fuel && d.fuel < l.fuel);
        assert!(c.memory_bytes < d.memory_bytes && d.memory_bytes < l.memory_bytes);
        assert!(c.timeout < d.timeout && d.timeout < l.timeout);
    }

    #[test]
    fn memory_is_rounded_up_to_whole_pages() {
        let l = Limits { memory_bytes: 100_000, ..Limits::default() };
        assert_eq!(l.max_pages(), 1, "100 KB no alcanzan para dos páginas");
        let l = Limits { memory_bytes: 1000, ..Limits::default() };
        assert_eq!(l.max_pages(), 0, "menos de una página es ninguna");
    }

    #[test]
    fn every_halt_explains_itself_in_plain_words() {
        for d in [
            Halt::OutOfFuel,
            Halt::OutOfMemory,
            Halt::Timeout(Duration::from_secs(5)),
        ] {
            let text = d.to_string();
            assert!(text.len() > 10, "{text}");
            assert!(!text.contains("Error"), "nada de jerga: {text}");
        }
    }
}
