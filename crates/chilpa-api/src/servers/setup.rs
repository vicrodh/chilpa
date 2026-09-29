//! Adding a server, one step at a time (ADR-017).
//!
//! Every step that goes to the network runs on a thread of its own and is
//! picked up by `poll`, which the window calls four times a second. The
//! window reads `ServerSetup` and shows the step it is at; nothing here
//! blocks it.
//!
//! The steps:
//!
//! - **OpenSubsonic, Jellyfin:** the address, then signing in (a password,
//!   an API key, or Jellyfin's Quick Connect), then which libraries.
//! - **Plex:** a PIN confirmed at plex.tv/link, then which server when the
//!   account reaches more than one, then which music sections.

use super::net::{client, Network};
use chilpa_servers::{jellyfin, plex, subsonic, Credential, Details, Failure, Kind, Library, Probe};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// What the window shows of the setup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerSetup {
    /// `idle`, `address`, `signin`, `code`, `servers`, `libraries`, `done`.
    pub step: String,
    /// The provider being added: `subsonic`, `jellyfin`, `plex`.
    pub kind: String,
    pub address: String,
    /// What the server said it is.
    pub product: String,
    /// OpenSubsonic: it takes an API key instead of a password.
    pub api_key: bool,
    /// Jellyfin: Quick Connect is on.
    pub quick_connect: bool,
    /// The code to type elsewhere: Jellyfin's Quick Connect, Plex's PIN.
    pub code: String,
    /// Where to type it: plex.tv/link. Empty for Jellyfin, whose code is
    /// typed in a Jellyfin already signed in.
    pub link: String,
    /// The servers or the libraries to choose from: (id, name, detail).
    pub choices: Vec<(String, String, String)>,
    pub busy: bool,
    pub error: String,
}

/// What a finished setup hands to the library.
#[derive(Clone, Debug)]
pub(crate) struct Ready {
    pub(crate) kind: Kind,
    pub(crate) instance: String,
    pub(crate) label: String,
    pub(crate) address: String,
    pub(crate) account: String,
    pub(crate) identity: String,
    pub(crate) product: String,
    pub(crate) libraries: Vec<String>,
    pub(crate) details: Details,
    pub(crate) key: Credential,
}

enum Outcome {
    Probed(Result<Probe, Failure>),
    Signed(Result<Signed, Failure>),
    QuickStarted(Result<(String, String), Failure>),
    QuickReady(Result<bool, Failure>),
    Pin(Result<plex::Pin, Failure>),
    PinToken(Result<Option<String>, Failure>),
    PlexServers(Result<Vec<plex::PlexServer>, Failure>),
    Reached(Result<(plex::PlexServer, String), Failure>),
    Libraries(Result<Vec<Library>, Failure>),
}

struct Signed {
    key: Credential,
    account: String,
    identity: String,
    product: String,
    details: Details,
}

#[derive(Default)]
pub(crate) struct Setup {
    pub(crate) view: ServerSetup,
    kind: Option<Kind>,
    instance: String,
    label: String,
    probe: Probe,
    signed: Option<Signed>,
    quick_secret: String,
    pin: u64,
    account_token: String,
    plex: Vec<plex::PlexServer>,
    task: Option<(u64, Receiver<Outcome>)>,
    generation: u64,
    /// When to ask again whether the code was accepted.
    again: Option<Instant>,
}

/// How often a code is checked while the user types it elsewhere.
const CHECK_EVERY: Duration = Duration::from_secs(2);

impl Setup {
    fn run(&mut self, work: impl FnOnce() -> Outcome + Send + 'static) {
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("chilpa-server-setup".into()).spawn(move || {
            let _ = tx.send(work());
        });
        self.task = Some((self.generation, rx));
        self.view.busy = true;
        self.view.error.clear();
    }

    fn fail(&mut self, failure: Failure) {
        self.view.busy = false;
        self.view.error = failure.to_string();
    }

    /// Starts adding a server of that kind.
    pub(crate) fn begin(&mut self, provider: &str) {
        let Some(kind) = Kind::from_provider(provider) else { return };
        self.generation += 1;
        *self = Setup { generation: self.generation, ..Setup::default() };
        self.kind = Some(kind);
        self.instance = super::keys::mint();
        self.view.kind = provider.to_string();
        match kind {
            Kind::Plex => {
                self.view.step = "code".into();
                self.view.link = plex::LINK.into();
                let who = client(&self.instance);
                self.run(move || Outcome::Pin(plex::pin_start(&Network, &who)));
            }
            _ => self.view.step = "address".into(),
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.generation += 1;
        *self = Setup { generation: self.generation, ..Setup::default() };
        self.view.step = "idle".into();
    }

    /// The address was typed: see what answers there.
    pub(crate) fn address(&mut self, text: &str) {
        let (Some(kind), false) = (self.kind, self.view.busy) else { return };
        let address = match chilpa_servers::normalize(text) {
            Ok(a) => a,
            Err(f) => return self.fail(f),
        };
        self.view.address = address.clone();
        self.run(move || {
            Outcome::Probed(match kind {
                Kind::Subsonic => subsonic::probe(&Network, &address),
                _ => jellyfin::probe(&Network, &address),
            })
        });
    }

    /// Signs in with a password, or with an API key when `as_key`.
    pub(crate) fn sign_in(&mut self, user: &str, secret: &str, as_key: bool) {
        let (Some(kind), false) = (self.kind, self.view.busy) else { return };
        let (address, user, secret, who) = (self.view.address.clone(), user.to_string(), secret.to_string(), client(&self.instance));
        self.run(move || {
            Outcome::Signed(match kind {
                Kind::Subsonic => subsonic::sign_in(&Network, &address, &user, &secret, as_key).map(|s| Signed {
                    key: s.key,
                    account: s.account,
                    identity: s.identity,
                    product: s.product,
                    details: Details { extensions: s.extensions, ..Details::default() },
                }),
                _ => jellyfin::sign_in(&Network, &address, &who, &user, &secret).map(|s| Signed {
                    key: s.key,
                    account: s.account,
                    identity: String::new(),
                    product: String::new(),
                    details: Details { user_id: s.user_id, ..Details::default() },
                }),
            })
        });
    }

    /// Jellyfin: signs in with a code typed in a Jellyfin already signed in.
    pub(crate) fn quick_connect(&mut self) {
        if self.kind != Some(Kind::Jellyfin) || self.view.busy {
            return;
        }
        let (address, who) = (self.view.address.clone(), client(&self.instance));
        self.run(move || Outcome::QuickStarted(jellyfin::quick_connect_start(&Network, &address, &who)));
    }

    /// Plex: an account's token obtained some other way than this PIN, as
    /// if the PIN had given it. For the lab tests, which cannot confirm a
    /// PIN at plex.tv.
    pub(crate) fn plex_account(&mut self, token: &str) {
        if self.kind != Some(Kind::Plex) {
            return;
        }
        self.generation += 1;
        self.task = None;
        self.again = None;
        self.take(Outcome::PinToken(Ok(Some(token.to_string()))));
    }

    /// Plex: the server chosen among those the account reaches.
    pub(crate) fn pick_server(&mut self, identity: &str) {
        let Some(chosen) = self.plex.iter().find(|s| s.identity == identity).cloned() else { return };
        let who = client(&self.instance);
        self.run(move || {
            Outcome::Reached(plex::reach(&Network, &who, &chosen.identity, &chosen.connections).map(|address| (chosen, address)))
        });
    }

    /// Which libraries come in. Returns what the library needs to add the
    /// server; the setup is done.
    pub(crate) fn finish(&mut self, chosen: &[String]) -> Option<Ready> {
        let kind = self.kind?;
        let signed = self.signed.as_ref()?;
        if self.view.step != "libraries" {
            return None;
        }
        let all: Vec<String> = self.view.choices.iter().map(|(id, _, _)| id.clone()).collect();
        let chosen: Vec<String> = all.iter().filter(|id| chosen.contains(id)).cloned().collect();
        // None chosen is not "all": the list below keeps "all" as empty.
        if chosen.is_empty() {
            self.view.error = chilpa_i18n::t("choose at least one library");
            return None;
        }
        // All of them chosen is kept as "all": a library added later on the
        // server comes in by itself.
        let libraries = if chosen.len() == all.len() { Vec::new() } else { chosen };
        Some(Ready {
            kind,
            instance: self.instance.clone(),
            label: if self.label.is_empty() { host_of(&self.view.address) } else { self.label.clone() },
            address: self.view.address.clone(),
            account: signed.account.clone(),
            identity: if signed.identity.is_empty() { self.probe.identity.clone() } else { signed.identity.clone() },
            product: if signed.product.is_empty() { self.view.product.clone() } else { signed.product.clone() },
            libraries,
            details: signed.details.clone(),
            key: signed.key.clone(),
        })
    }

    pub(crate) fn done(&mut self, name: &str) {
        self.view = ServerSetup { step: "done".into(), kind: self.view.kind.clone(), product: name.to_string(), ..ServerSetup::default() };
    }

    pub(crate) fn failed(&mut self, why: String) {
        self.view.busy = false;
        self.view.error = why;
    }

    /// Picks up what a step brought back. Returns whether the view changed.
    pub(crate) fn poll(&mut self) -> bool {
        if let Some(at) = self.again {
            if Instant::now() >= at && self.task.is_none() {
                self.again = None;
                self.check_code();
            }
        }
        let Some((generation, rx)) = self.task.as_ref() else { return false };
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.task = None;
                self.view.busy = false;
                return true;
            }
        };
        let stale = *generation != self.generation;
        self.task = None;
        if stale {
            return false;
        }
        self.view.busy = false;
        self.take(outcome);
        true
    }

    fn check_code(&mut self) {
        match self.kind {
            Some(Kind::Plex) => {
                let (pin, who) = (self.pin, client(&self.instance));
                self.run(move || Outcome::PinToken(plex::pin_token(&Network, &who, pin)));
            }
            Some(Kind::Jellyfin) => {
                let (address, secret) = (self.view.address.clone(), self.quick_secret.clone());
                self.run(move || Outcome::QuickReady(jellyfin::quick_connect_ready(&Network, &address, &secret)));
            }
            _ => {}
        }
        // Waiting for the person is not the window being busy.
        self.view.busy = false;
    }

    fn libraries(&mut self) {
        let (Some(kind), Some(signed)) = (self.kind, self.signed.as_ref()) else { return };
        let server = chilpa_servers::Server {
            kind,
            address: self.view.address.clone(),
            client: client(&self.instance),
            libraries: Vec::new(),
            details: signed.details.clone(),
        };
        let key = signed.key.clone();
        self.run(move || Outcome::Libraries(chilpa_servers::libraries(&Network, &server, &key)));
    }

    fn take(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Probed(Ok(probe)) => {
                self.view.product = probe.product.clone();
                self.view.api_key = probe.api_key;
                self.view.quick_connect = probe.quick_connect;
                self.label = probe.name.clone();
                self.probe = probe;
                self.view.step = "signin".into();
            }
            Outcome::Signed(Ok(signed)) => {
                if !signed.product.is_empty() {
                    self.view.product = signed.product.clone();
                }
                self.signed = Some(signed);
                self.libraries();
            }
            Outcome::QuickStarted(Ok((secret, code))) => {
                self.quick_secret = secret;
                self.view.code = code;
                self.view.step = "code".into();
                self.again = Some(Instant::now() + CHECK_EVERY);
            }
            Outcome::QuickReady(Ok(false)) | Outcome::PinToken(Ok(None)) => self.again = Some(Instant::now() + CHECK_EVERY),
            Outcome::QuickReady(Ok(true)) => {
                let (address, secret, who) = (self.view.address.clone(), self.quick_secret.clone(), client(&self.instance));
                self.run(move || {
                    Outcome::Signed(jellyfin::quick_connect_finish(&Network, &address, &who, &secret).map(|s| Signed {
                        key: s.key,
                        account: s.account,
                        identity: String::new(),
                        product: String::new(),
                        details: Details { user_id: s.user_id, ..Details::default() },
                    }))
                });
            }
            Outcome::Pin(Ok(pin)) => {
                self.pin = pin.id;
                self.view.code = pin.code;
                self.again = Some(Instant::now() + CHECK_EVERY);
            }
            Outcome::PinToken(Ok(Some(token))) => {
                self.account_token = token.clone();
                let who = client(&self.instance);
                self.run(move || Outcome::PlexServers(plex::servers(&Network, &who, &token)));
            }
            Outcome::PlexServers(Ok(servers)) => {
                if servers.is_empty() {
                    return self.fail(Failure::Unexpected(chilpa_i18n::t("that Plex account reaches no server")));
                }
                self.view.choices = servers
                    .iter()
                    .map(|s| (s.identity.clone(), s.name.clone(), if s.owned { s.product.clone() } else { chilpa_i18n::t("shared with you") }))
                    .collect();
                self.plex = servers;
                if self.plex.len() == 1 {
                    let only = self.plex[0].identity.clone();
                    self.pick_server(&only);
                } else {
                    self.view.step = "servers".into();
                }
            }
            Outcome::Reached(Ok((server, address))) => {
                self.view.address = address;
                self.view.product = server.product.clone();
                self.label = server.name.clone();
                self.probe.identity = server.identity.clone();
                self.signed = Some(Signed {
                    key: Credential::Plex { account: self.account_token.clone(), server: server.token.clone() },
                    account: String::new(),
                    identity: server.identity.clone(),
                    product: server.product.clone(),
                    details: Details { connections: server.connections.clone(), ..Details::default() },
                });
                self.libraries();
            }
            Outcome::Libraries(Ok(found)) => {
                if found.is_empty() {
                    return self.fail(Failure::Unexpected(chilpa_i18n::t("that server has no music library")));
                }
                self.view.choices = found.into_iter().map(|l| (l.id, l.name, String::new())).collect();
                self.view.step = "libraries".into();
            }
            Outcome::Probed(Err(f))
            | Outcome::Signed(Err(f))
            | Outcome::QuickStarted(Err(f))
            | Outcome::QuickReady(Err(f))
            | Outcome::Pin(Err(f))
            | Outcome::PinToken(Err(f))
            | Outcome::PlexServers(Err(f))
            | Outcome::Reached(Err(f))
            | Outcome::Libraries(Err(f)) => self.fail(f),
        }
    }
}

/// The host of an address, to name a server that has no name of its own.
fn host_of(address: &str) -> String {
    address.split("://").nth(1).unwrap_or(address).split(['/', ':']).next().unwrap_or(address).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_without_a_name_is_called_by_its_host() {
        assert_eq!(host_of("http://music.lan:4533"), "music.lan");
        assert_eq!(host_of("https://jf.example.org/jellyfin"), "jf.example.org");
    }

    #[test]
    fn a_plex_setup_starts_with_a_pin_and_the_others_with_an_address() {
        let mut setup = Setup::default();
        setup.begin("subsonic");
        assert_eq!((setup.view.step.as_str(), setup.view.busy), ("address", false));
        assert_eq!(setup.instance.len(), 16);
        setup.begin("nothing");
        assert_eq!(setup.view.kind, "subsonic", "an unknown kind changes nothing");
        setup.cancel();
        assert_eq!(setup.view.step, "idle");
    }

    #[test]
    fn a_bad_address_is_said_before_anything_goes_out() {
        let mut setup = Setup::default();
        setup.begin("jellyfin");
        setup.address("   ");
        assert!(!setup.view.error.is_empty());
        assert!(!setup.view.busy);
    }

    #[test]
    fn nothing_is_ready_before_the_libraries_are_chosen() {
        let mut setup = Setup::default();
        setup.begin("subsonic");
        assert!(setup.finish(&[]).is_none());
    }

    #[test]
    fn no_library_chosen_is_not_all_of_them() {
        let mut setup = Setup::default();
        setup.begin("subsonic");
        setup.signed = Some(Signed {
            key: Credential::ApiKey { key: "k".into() },
            account: "lab".into(),
            identity: "id".into(),
            product: "Navidrome".into(),
            details: Details::default(),
        });
        setup.view.step = "libraries".into();
        setup.view.choices = vec![("1".into(), "Music".into(), String::new()), ("2".into(), "Audiobooks".into(), String::new())];
        assert!(setup.finish(&[]).is_none(), "nothing chosen adds nothing");
        assert!(!setup.view.error.is_empty());
        assert!(setup.finish(&["9".into()]).is_none(), "an id it never offered is not a choice");
        assert_eq!(setup.finish(&["2".into()]).unwrap().libraries, vec!["2".to_string()]);
        assert!(setup.finish(&["1".into(), "2".into()]).unwrap().libraries.is_empty(), "all of them is kept as all");
    }
}
