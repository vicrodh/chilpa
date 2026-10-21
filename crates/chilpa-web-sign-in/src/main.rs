//! Opens a plugin's sign-in page in a window of its own and stops at the
//! address the plugin waits for.
//!
//! A provider's browser sign-in ends on a return address that only its own
//! application can receive. In the person's browser that page is a dead end
//! and its address has to be copied by hand. Here the navigation to that
//! address is cancelled and the address is printed on stdout instead.
//!
//! `chilpa-web-sign-in --url URL --until PREFIX [--user-agent AGENT] [--title TITLE]`
//!
//! Exit status: 0 with the address on stdout, 1 when the window was closed,
//! 2 for invalid arguments, 3 when this build has no web engine.

use std::process::ExitCode;

const CLOSED: u8 = 1;
const USAGE: u8 = 2;
const UNAVAILABLE: u8 = 3;

#[derive(Debug, PartialEq)]
struct Request {
    url: String,
    until: String,
    user_agent: Option<String>,
    title: String,
}

fn https(value: &str) -> bool {
    value.len() <= 8192 && value.starts_with("https://") && value.len() > "https://".len()
}

impl Request {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Option<Request> {
        let (mut url, mut until, mut user_agent, mut title) = (None, None, None, String::new());
        let mut arguments = arguments.into_iter();
        while let Some(name) = arguments.next() {
            let value = arguments.next()?;
            match name.as_str() {
                "--url" => url = Some(value),
                "--until" => until = Some(value),
                "--user-agent" => user_agent = Some(value).filter(|agent| !agent.is_empty() && agent.len() <= 512),
                "--title" => title = value,
                _ => return None,
            }
        }
        let (url, until) = (url.filter(|url| https(url))?, until.filter(|until| https(until))?);
        Some(Request { url, until, user_agent, title })
    }

    /// Whether a navigation reaches the return address: the prefix, then
    /// nothing, a query or a fragment.
    fn reaches(&self, address: &str) -> bool {
        address.strip_prefix(&self.until).is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '#']))
    }
}

fn main() -> ExitCode {
    let Some(request) = Request::parse(std::env::args().skip(1)) else {
        eprintln!("usage: chilpa-web-sign-in --url https://... --until https://... [--user-agent AGENT] [--title TITLE]");
        return ExitCode::from(USAGE);
    };
    match window::run(&request) {
        Some(Some(address)) => {
            println!("{address}");
            ExitCode::SUCCESS
        }
        Some(None) => ExitCode::from(CLOSED),
        None => ExitCode::from(UNAVAILABLE),
    }
}

#[cfg(all(target_os = "linux", feature = "webkit"))]
mod window {
    use super::Request;
    use gtk::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use webkit6::prelude::*;

    /// The captured address, or none when the window was closed first.
    pub fn run(request: &Request) -> Option<Option<String>> {
        let captured: Rc<RefCell<Option<String>>> = Rc::default();
        let application = gtk::Application::builder()
            .application_id("app.chilpa.Chilpa.SignIn")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let (until, url, agent, title) = (Rc::new(request.until.clone()), request.url.clone(), request.user_agent.clone(), request.title.clone());
        let found = captured.clone();
        application.connect_activate(move |application| {
            let settings = webkit6::Settings::new();
            if let Some(agent) = &agent {
                settings.set_user_agent(Some(agent.as_str()));
            }
            // Nothing of this sign-in outlives the window: no cookies, no cache.
            let session = webkit6::NetworkSession::new_ephemeral();
            let view = webkit6::WebView::builder().network_session(&session).settings(&settings).build();
            let window = gtk::ApplicationWindow::builder()
                .application(application)
                .title(title.as_str())
                .default_width(480)
                .default_height(760)
                .child(&view)
                .build();
            let (until, found, closing) = (until.clone(), found.clone(), window.downgrade());
            view.connect_decide_policy(move |_, decision, kind| {
                if !matches!(kind, webkit6::PolicyDecisionType::NavigationAction | webkit6::PolicyDecisionType::NewWindowAction) {
                    return false;
                }
                let Some(navigation) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else { return false };
                let address = navigation.navigation_action().and_then(|action| action.request()).and_then(|request| request.uri());
                let Some(address) = address else { return false };
                let reached = Request { url: String::new(), until: until.to_string(), user_agent: None, title: String::new() }.reaches(&address);
                if !reached {
                    return false;
                }
                decision.ignore();
                *found.borrow_mut() = Some(address.to_string());
                if let Some(window) = closing.upgrade() {
                    window.close();
                }
                true
            });
            view.load_uri(&url);
            window.present();
        });
        application.run_with_args::<&str>(&[]);
        let address = captured.borrow_mut().take();
        Some(address)
    }
}

#[cfg(not(all(target_os = "linux", feature = "webkit")))]
mod window {
    use super::Request;

    /// This build carries no web engine.
    pub fn run(_request: &Request) -> Option<Option<String>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_request_needs_two_secure_addresses() {
        let request = Request::parse(arguments(&["--url", "https://login.example/a?x=1", "--until", "https://example/return", "--title", "Source"])).unwrap();
        assert_eq!(request.url, "https://login.example/a?x=1");
        assert_eq!(request.title, "Source");
        assert!(Request::parse(arguments(&["--url", "http://login.example/a", "--until", "https://example/return"])).is_none());
        assert!(Request::parse(arguments(&["--url", "https://login.example/a"])).is_none());
        assert!(Request::parse(arguments(&["--url", "https://login.example/a", "--until", "https://example/r", "--other", "x"])).is_none());
    }

    #[test]
    fn only_the_return_address_itself_is_reached() {
        let request = Request::parse(arguments(&["--url", "https://login.example/a", "--until", "https://example/return"])).unwrap();
        assert!(request.reaches("https://example/return?code=1&state=2"));
        assert!(request.reaches("https://example/return"));
        assert!(!request.reaches("https://example/returned?code=1"));
        assert!(!request.reaches("https://login.example/a?redirect=https://example/return"));
    }
}
