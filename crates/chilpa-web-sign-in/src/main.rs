//! Opens a plugin's sign-in page in a window of its own and stops at the
//! address the plugin waits for.
//!
//! A provider's browser sign-in ends on a return address that only its own
//! application can receive. In the person's browser that page is a dead end
//! and its address has to be copied by hand. Here the navigation to that
//! address is cancelled and the address is printed on stdout instead.
//!
//! `chilpa-web-sign-in --url URL --until PREFIX [--title TITLE] [--user-agent AGENT]
//!  [--data-dir DIR --cache-dir DIR]`
//!
//! It follows what the provider's own desktop players do in their login
//! windows: the engine's own user agent unless one is declared, a website
//! data store that persists between sign-ins (a bot check passed once is not
//! asked again), and no camera, microphone, screen or sound, which a login
//! page does not need. The engine is Qt WebEngine, in this process and its
//! own helpers, so none of it counts toward Chilpa's memory.
//!
//! Exit status: 0 with the address on stdout, 1 when the window was closed,
//! 2 for invalid arguments, 3 when there is no web engine.

use chilpa_web_sign_in::{native, parent, Outcome, Request, USAGE};
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(request) = Request::parse(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(Outcome::Usage.code());
    };
    parent::follow();
    let outcome = native::open(&request);
    if let Outcome::Reached(address) = &outcome {
        println!("{address}");
    }
    ExitCode::from(outcome.code())
}
