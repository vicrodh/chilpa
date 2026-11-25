//! Where a play's sound goes: the output asked for, or, when that is the
//! system default and it will not open, the first route that does.
//!
//! A play on the system default always sounds, bit-perfect or not (ADR-011).
//! What it reports -- the route, whether the path can be exact -- is about
//! where it went, never about where it was sent.

use crate::player::{OpenSink, PlayerEvent};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};
use std::cell::RefCell;

/// The output a play was asked to use and the one it is on now, with
/// whether the road past it can be exact.
pub(crate) struct Destination<'a> {
    asked: &'a str,
    exclusive: bool,
    /// The output and whether its path can be exact, judged once when the
    /// play lands on it: judging asks the stack, and a server's stack asks
    /// `pactl`, which must not run between two tracks of a gapless play.
    current: RefCell<(String, bool)>,
    events: &'a crate::player::Events,
}

impl<'a> Destination<'a> {
    /// `exact` is `path_is_exact(asked, exclusive)`, already known to the caller.
    pub(crate) fn new(asked: &'a str, exact: bool, exclusive: bool, events: &'a crate::player::Events) -> Self {
        Self { asked, exclusive, current: RefCell::new((asked.to_string(), exact)), events }
    }

    /// The output the play is on, and whether its path can be exact: what
    /// its route and verdict describe.
    pub(crate) fn route(&self) -> (String, bool) {
        self.current.borrow().clone()
    }

    /// Opens the output the play is on. When it will not open and the play
    /// was asked for the system default, the default and then each of its
    /// fallbacks are tried, once each, and the first that opens is where the
    /// play stays. When none opens, the error is the default's own: it is
    /// the answer about the output somebody would look at.
    pub(crate) fn open(
        &self,
        open: &mut OpenSink,
        format: StreamFormat,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        let exclusive = self.exclusive;
        let current = self.route().0;
        let failed = match open(&current, format, exclusive) {
            Ok(sink) => return Ok(sink),
            Err(error) => error,
        };
        let fallbacks = crate::backends::standing_fallbacks(self.asked);
        if fallbacks.is_empty() {
            return Err(failed);
        }
        // The default's answer, once it is known: now when the play was on
        // it, or when it is tried again below after a fallback stopped opening.
        let (mut asked_failed, mut other_failed) =
            if current == self.asked { (Some(failed), None) } else { (None, Some(failed)) };
        let mut routes = vec![(self.asked.to_string(), String::new())];
        routes.extend(fallbacks);
        for (route, label) in routes.into_iter().filter(|(route, _)| *route != current) {
            match open(&route, format, exclusive) {
                Ok(sink) => {
                    chilpa_audio::diagnostics::trace(
                        "player",
                        "route_fell_back",
                        format_args!("asked={:?} from={current:?} to={route:?}", self.asked),
                    );
                    if route != self.asked {
                        let _ = self.events.send(PlayerEvent::FellBack {
                            message: chilpa_i18n::t_args(
                                "The system default output would not open; playing through {}.",
                                &[&label],
                            ),
                        });
                    }
                    let exact = crate::path_is_exact(&route, exclusive);
                    *self.current.borrow_mut() = (route, exact);
                    return Ok(sink);
                }
                Err(error) => {
                    chilpa_audio::diagnostics::trace(
                        "player",
                        "route_fallback_refused",
                        format_args!("route={route:?} error={error}"),
                    );
                    if route == self.asked {
                        asked_failed = Some(error);
                    } else {
                        other_failed.get_or_insert(error);
                    }
                }
            }
        }
        Err(asked_failed.or(other_failed).unwrap_or(SinkError::DeviceLost))
    }
}
