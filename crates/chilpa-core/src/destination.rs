//! Where a play's sound goes: the output asked for, or, when that is the
//! standing output and it will not open, the first route that does.
//!
//! A play on the output Chilpa starts on by itself always sounds, bit-perfect
//! or not (ADR-011). What it reports -- the route, whether the path can be
//! exact -- is about where it went, never about where it was sent.

use crate::player::{OpenSink, PlayerEvent};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};
use std::cell::RefCell;

/// The output a play was asked to use and the one it is on now.
pub(crate) struct Destination<'a> {
    asked: &'a str,
    current: RefCell<String>,
    events: &'a crate::player::Events,
}

impl<'a> Destination<'a> {
    pub(crate) fn new(asked: &'a str, events: &'a crate::player::Events) -> Self {
        Self { asked, current: RefCell::new(asked.to_string()), events }
    }

    /// The output the play is on: what its route and verdict describe.
    pub(crate) fn device(&self) -> String {
        self.current.borrow().clone()
    }

    /// Opens the output the play is on. When it will not open, the one asked
    /// for and then each of its fallbacks are tried, once each, and the first
    /// that opens is where the play stays. The error is the first one: it is
    /// the answer about the output somebody would look at.
    pub(crate) fn open(
        &self,
        open: &mut OpenSink,
        format: StreamFormat,
        exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        let current = self.device();
        let first = match open(&current, format, exclusive) {
            Ok(sink) => return Ok(sink),
            Err(error) => error,
        };
        let fallbacks = crate::backends::standing_fallbacks(self.asked);
        if fallbacks.is_empty() {
            return Err(first);
        }
        let mut routes = vec![(self.asked.to_string(), String::new())];
        routes.extend(fallbacks);
        for (route, label) in routes.into_iter().filter(|(route, _)| *route != current) {
            match open(&route, format, exclusive) {
                Ok(sink) => {
                    chilpa_audio::diagnostics::trace(
                        "player",
                        "route_fell_back",
                        format_args!("asked={:?} from={current:?} to={route:?} because={first}", self.asked),
                    );
                    if route != self.asked {
                        let _ = self.events.send(PlayerEvent::FellBack {
                            message: chilpa_i18n::t_args(
                                "The system default output would not open; playing through {}.",
                                &[&label],
                            ),
                        });
                    }
                    *self.current.borrow_mut() = route;
                    return Ok(sink);
                }
                Err(error) => chilpa_audio::diagnostics::trace(
                    "player",
                    "route_fallback_refused",
                    format_args!("route={route:?} error={error}"),
                ),
            }
        }
        Err(first)
    }
}
