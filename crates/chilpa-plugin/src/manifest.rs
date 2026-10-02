//! A plugin's manifest: who it is and what it asks for.
//!
//! It is validated whole before anything is loaded. The rules here are not
//! formalities: the `id` becomes a folder name and the domains become the
//! list of where it may go. A manifest that is not understood **does not
//! load**, instead of loading with the part that was.
//!
//! It fails closed on purpose. An unknown capability is not ignored: if this
//! Chilpa does not know what it means, it cannot know whether it is
//! dangerous.

use serde::{Deserialize, Serialize};

/// The plugin API version this Chilpa understands.
pub const API_VERSION: u32 = 1;

/// What a plugin is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// It brings catalogue: playlists, albums, artists, tracks.
    Source,
    /// It fills in metadata for what is already there.
    Metadata,
    /// It fetches lyrics.
    Lyrics,
    /// It reports what was listened to, to a service.
    Scrobbler,
}

/// Where it runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// Inside, sandboxed. The usual (ADR-007).
    #[default]
    Wasm,
    /// A separate process, for anything that needs native code. Not
    /// implemented yet; a manifest asking for it is refused, saying so.
    Process,
    /// A host-owned compatibility worker with private storage and mediated
    /// network access. Only an explicitly selected adapter can load it.
    Bridge,
}

/// What a plugin asks to be able to do.
///
/// The user approves them on install. What is not here cannot be done: there
/// is no "everything else" capability.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Capability {
    /// Reach the network, only these domains.
    Network { domains: Vec<String> },
    /// Keep things in its own store.
    Storage,
    /// Read the library. Never write it.
    LibraryRead,
    /// Register a source with its catalogue.
    RegisterSource,
    /// Contribute a panel to the interface, as a descriptor.
    RegisterPanel,
    /// Register a command the user can trigger.
    RegisterCommand,
    /// The compatibility guest receives its own account credentials. They
    /// remain encrypted at rest and scoped to this installation.
    GuestCredentials,
    /// It handles its own account: it authenticates itself and keeps its
    /// credential wherever it likes. Chilpa does not see it, does not put
    /// it in its requests and cannot delete it.
    ///
    /// It exists because there are libraries that speak a service's own
    /// protocol and **authenticate themselves**: they need raw sockets and
    /// the credential in their hands. With those the normal invariant --
    /// the plugin uses the credential but never holds it -- cannot be
    /// upheld, and pretending it can would be worse than saying so.
    ///
    /// It only makes sense with [`Runtime::Process`]: a WASM plugin has
    /// neither sockets nor a disk of its own, so it could handle nothing.
    ///
    /// `keyring` says **where** it keeps it. If true, in the system keyring
    /// under its own name -- an entry of its own, separate from Chilpa's,
    /// never the same one. If false, wherever it decides, which may be a
    /// plaintext file.
    ///
    /// **It is a declaration, not a guarantee.** The other capabilities
    /// Chilpa enforces: a domain that is not on the list does not reach the
    /// network, full stop. This one it cannot check, because it is
    /// precisely about what happens beyond its reach. That is why the user
    /// is told what it is: what the plugin says about itself.
    OwnAccount {
        #[serde(default)]
        keyring: bool,
    },
}

impl Capability {
    /// What it asks for, said so someone can approve it or refuse it.
    ///
    /// It is in words and not JSON on purpose: a list of `{"type":
    /// "network"}` is not an informed decision, it is paperwork.
    pub fn describe(&self) -> String {
        match self {
            Capability::Network { domains } => {
                if domains.iter().any(|domain| domain == "*") {
                    chilpa_i18n::t("Network access, including authentication, artwork and audio servers discovered by the plugin")
                } else { chilpa_i18n::t_args("reach the internet, only {}", &[&domains.join(", ")]) }
            }
            Capability::Storage => chilpa_i18n::t("keep things of its own on disk"),
            Capability::LibraryRead => {
                chilpa_i18n::t("read your library, without being able to change it")
            }
            Capability::RegisterSource => {
                chilpa_i18n::t("bring catalogue: search for and fetch music")
            }
            Capability::RegisterPanel => chilpa_i18n::t("put a panel of its own in the window"),
            Capability::RegisterCommand => chilpa_i18n::t("add a command you trigger"),
            Capability::GuestCredentials => chilpa_i18n::t(
                "let this plugin handle its own sign-in secrets, encrypted on disk and separate from other plugins",
            ),
            // It is worded differently from the rest on purpose: it is
            // the only one that leaves Chilpa out, and approving it
            // should not feel like approving a panel. And it says "says
            // it", because that is the only honest wording: Chilpa
            // cannot check it.
            Capability::OwnAccount { keyring: true } => chilpa_i18n::t(
                "handle your account itself, outside Chilpa: it says it keeps your \
                 credential in the system keyring, under its own name. Chilpa does not \
                 see it, cannot delete it and cannot check any of this",
            ),
            Capability::OwnAccount { keyring: false } => chilpa_i18n::t(
                "handle your account itself, outside Chilpa: your credential ends up \
                 wherever it decides, which may be a plaintext file. Chilpa does not see \
                 it, cannot delete it and cannot check any of this",
            ),
        }
    }
}

/// What a plugin says about itself.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Identity, as a reversed domain: `com.example.my-plugin`.
    pub id: String,
    /// What it is called for the user.
    pub name: String,
    /// Its version, which is its own and matters to us only to show it.
    pub version: String,
    /// Which version of Chilpa's API it works with.
    pub api_version: u32,
    pub kind: Kind,
    #[serde(default)]
    pub runtime: Runtime,
    /// How an account is connected on that source, if it needs one.
    ///
    /// The plugin declares the endpoints, but Chilpa runs the flow: the
    /// token never passes through its hands (ADR-008).
    #[serde(default)]
    pub auth: Option<chilpa_auth::Auth>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// True if this source can deliver audio for Chilpa to play.
    ///
    /// There are large services whose API lets you search and browse but
    /// **never** hands audio bytes to a third party: they only control
    /// their own player. Such a source is declared here, and its results
    /// come out marked unplayable from the start instead of every click
    /// failing. It is not a limitation of ours and it is not hidden.
    #[serde(default = "yes")]
    pub delivers_audio: bool,
    /// What it is, in a sentence, for the list of installed plugins: where
    /// its catalogue comes from, what it needs.
    #[serde(default)]
    pub description: String,
    /// The name of the icon its entry in the window is drawn with, when it
    /// has one. It is a hint: a name the window does not know gets the
    /// generic icon.
    #[serde(default)]
    pub icon: String,
}

fn yes() -> bool {
    true
}

/// Why a manifest was not accepted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ManifestError {
    /// It is not JSON, or it is missing something with no default.
    Unreadable(String),
    /// The `id` works neither as an identity nor as a folder name.
    BadId(String),
    /// A required field came in empty.
    Empty(&'static str),
    /// It asks for an API version this Chilpa does not understand.
    ApiIncompatible { wants: u32, have: u32 },
    /// It asks for a runtime that does not exist yet.
    RuntimeNotImplemented(Runtime),
    /// A domain on the network list is not a domain.
    BadDomain(String),
    /// How it says the account connects does not work.
    BadAuth(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::BadAuth(e) => {
                write!(f, "{}", chilpa_i18n::t_args("how the account connects does not work: {}", &[&e.to_string()]))
            }
            ManifestError::Unreadable(e) => {
                write!(f, "{}", chilpa_i18n::t_args("the manifest is not understood: {}", &[e]))
            }
            ManifestError::BadId(id) => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "\"{}\" does not work as an id: it is used as a folder name, so it goes \
                     in lowercase, with dots, dashes or digits, and no slashes and no \
                     double dots",
                    &[id],
                )
            ),
            ManifestError::Empty(field) => {
                // The field is marked where the error is made.
                write!(f, "{}", chilpa_i18n::t_args("{} is missing", &[&chilpa_i18n::t(field)]))
            }
            ManifestError::ApiIncompatible { wants, have } => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "it asks for API version {} and this Chilpa understands {}; it does \
                     not load",
                    &[&wants.to_string(), &have.to_string()],
                )
            ),
            ManifestError::RuntimeNotImplemented(r) => {
                let runtime = format!("{r:?}");
                write!(
                    f,
                    "{}",
                    chilpa_i18n::t_args(
                        "it asks for the {} runtime, which does not exist yet",
                        &[&runtime],
                    )
                )
            }
            ManifestError::BadDomain(d) => {
                write!(
                    f,
                    "{}",
                    chilpa_i18n::t_args(
                        "\"{}\" is not a domain permission can be given to",
                        &[d],
                    )
                )
            }
        }
    }
}

impl Manifest {
    /// Reads a manifest and validates it whole.
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        Self::parse_for_runtime(json, Runtime::Wasm)
    }

    /// The host selects the available adapter before reading repository
    /// metadata. A manifest cannot choose an executable or enable a runtime.
    pub fn parse_for_runtime(json: &str, available: Runtime) -> Result<Self, ManifestError> {
        let m: Manifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Unreadable(e.to_string()))?;
        m.validate(available)?;
        Ok(m)
    }

    fn validate(&self, available: Runtime) -> Result<(), ManifestError> {
        if self.name.trim().is_empty() {
            return Err(ManifestError::Empty(chilpa_i18n::mark("the name")));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Empty(chilpa_i18n::mark("the version")));
        }
        if !valid_id(&self.id) {
            return Err(ManifestError::BadId(self.id.clone()));
        }
        if self.api_version != API_VERSION {
            return Err(ManifestError::ApiIncompatible {
                wants: self.api_version,
                have: API_VERSION,
            });
        }
        if self.runtime == Runtime::Process || self.runtime != available {
            return Err(ManifestError::RuntimeNotImplemented(self.runtime));
        }
        if self.has(&Capability::GuestCredentials) && (self.runtime != Runtime::Bridge || !self.can_store()) {
            return Err(ManifestError::BadAuth("guest credentials require an isolated bridge and private storage".into()));
        }
        for c in &self.capabilities {
            if let Capability::Network { domains } = c {
                for d in domains {
                    if !valid_domain(d) && !(self.runtime == Runtime::Bridge && d == "*") {
                        return Err(ManifestError::BadDomain(d.clone()));
                    }
                }
            }
        }
        // It is checked here and not at connect time: a plugin that would
        // send the user to type their password over http is not installed,
        // it does not fail halfway through the flow with the browser
        // already open.
        if let Some(a) = &self.auth {
            if let Some(e) = a.problem() {
                return Err(ManifestError::BadAuth(e));
            }
        }
        // A WASM plugin has neither sockets nor a disk of its own: it could
        // not handle an account even if it said it could. It is refused at
        // install, not ignored in silence.
        if self.runtime != Runtime::Process
            && self.capabilities.iter().any(|c| matches!(c, Capability::OwnAccount { .. }))
        {
            return Err(ManifestError::BadAuth(
                chilpa_i18n::mark(
                    "it says it handles its own account, but only a separate process \
                     plugin can do that: in WASM there are no sockets and no disk of its \
                     own",
                )
                    .into(),
            ));
        }
        Ok(())
    }

    /// The domains it may reach. Empty means none.
    pub fn allowed_domains(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .filter_map(|c| match c {
                Capability::Network { domains } => Some(domains.iter().map(String::as_str)),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// True if it asked for that capability.
    pub fn has(&self, which: &Capability) -> bool {
        self.capabilities.contains(which)
    }

    /// True if it can keep things.
    pub fn can_store(&self) -> bool {
        self.capabilities.iter().any(|c| matches!(c, Capability::Storage))
    }

    /// True if it can reach the network.
    pub fn can_go_out(&self) -> bool {
        !self.allowed_domains().is_empty()
    }

    /// What the user approved when they said yes (ADR-008).
    ///
    /// It is the plugin's kind and what it asks for, not the whole manifest:
    /// bumping a version without asking for anything new has no reason to ask
    /// again, but adding a domain does. It is compared as is, so a change can
    /// be shown by saying what is new.
    pub fn asks_for(&self) -> Asked {
        let mut capabilities = self.capabilities.clone();
        capabilities.sort_by_key(|c| c.describe());
        Asked { kind: self.kind, runtime: self.runtime, capabilities, auth: self.auth.clone() }
    }

    /// True if an account has to be connected before it is good for anything.
    pub fn needs_account(&self) -> bool {
        self.auth.is_some()
    }
}

/// What is approved of a plugin, and what is kept to know whether it still
/// holds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Asked {
    pub kind: Kind,
    /// Old approvals describe the native runtime. Moving to a compatibility
    /// worker changes the trust boundary even with identical capabilities.
    #[serde(default)]
    pub runtime: Runtime,
    pub capabilities: Vec<Capability>,
    /// How the account connects. It is part of what was approved because
    /// changing where the user is sent to type their password is the last
    /// thing that should happen without them seeing it again.
    #[serde(default)]
    pub auth: Option<chilpa_auth::Auth>,
}

impl Asked {
    /// What it asks for now that was not approved. Empty means the earlier
    /// approval is enough.
    pub fn beyond(&self, approved: &Asked) -> Vec<String> {
        if self.kind != approved.kind {
            return vec![chilpa_i18n::t_args("be a plugin of another kind: {}", &[&format!("{:?}", self.kind)])];
        }
        let mut out: Vec<String> = self
            .capabilities
            .iter()
            .filter(|c| !approved.capabilities.contains(c))
            .map(Capability::describe)
            .collect();
        if self.runtime != approved.runtime {
            out.push(chilpa_i18n::t("run through a different plugin runtime"));
        }
        if self.auth != approved.auth {
            out.push(match &self.auth {
                Some(a) if a.host().is_empty() => chilpa_i18n::t("keep a credential of yours"),
                Some(a) => {
                    chilpa_i18n::t_args("send you to grant permission at {}", &[&a.host()])
                }
                None => chilpa_i18n::t("stop using the account it had"),
            });
        }
        out
    }
}

/// An id works if it can be used as a folder name without thinking.
///
/// No slashes, no `..`, no uppercase -- two ids differing only in case
/// would be the same folder on a system that does not distinguish -- and
/// nothing empty between dots.
fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 {
        return false;
    }
    if !id.contains('.') {
        return false;
    }
    if id.starts_with('.') || id.ends_with('.') || id.contains("..") {
        return false;
    }
    id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

/// A domain works if it is a host name, not a wildcard and not a URL.
fn valid_domain(d: &str) -> bool {
    if d.is_empty() || d.len() > 253 || d.contains('/') || d.contains(':') {
        return false;
    }
    // No wildcards: "*" or "*.something" is asking permission for
    // everything or for a whole tree, and that cannot be reviewed at a
    // glance.
    if d.contains('*') {
        return false;
    }
    if !d.contains('.') || d.starts_with('.') || d.ends_with('.') || d.contains("..") {
        return false;
    }
    d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> String {
        r#"{
            "id": "com.example.test",
            "name": "Test",
            "version": "0.1.0",
            "api_version": 1,
            "kind": "source",
            "capabilities": [
                {"type": "network", "domains": ["api.example.com"]},
                {"type": "storage"}
            ]
        }"#
        .to_string()
    }

    #[test]
    fn a_good_manifest_is_read_in_full() {
        let m = Manifest::parse(&base()).expect("it is valid");
        assert_eq!(m.id, "com.example.test");
        assert_eq!(m.kind, Kind::Source);
        assert_eq!(m.runtime, Runtime::Wasm, "the default runtime is wasm");
        assert_eq!(m.allowed_domains(), vec!["api.example.com"]);
        assert!(m.can_store() && m.can_go_out());
    }

    #[test]
    fn with_no_capabilities_it_can_do_nothing() {
        let json = base().replace(
            r#""capabilities": [
                {"type": "network", "domains": ["api.example.com"]},
                {"type": "storage"}
            ]"#,
            r#""capabilities": []"#,
        );
        let m = Manifest::parse(&json).unwrap();
        assert!(!m.can_go_out(), "with no network declared there is no network");
        assert!(!m.can_store());
    }

    #[test]
    fn an_unknown_capability_is_not_ignored() {
        // Fail closed: if what it asks for is unknown, whether it is
        // dangerous cannot be known.
        let json = base().replace(r#"{"type": "storage"}"#, r#"{"type": "read_the_whole_disk"}"#);
        let e = Manifest::parse(&json).expect_err("it is not accepted");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn a_different_api_version_does_not_load() {
        let json = base().replace(r#""api_version": 1"#, r#""api_version": 99"#);
        let e = Manifest::parse(&json).expect_err("it is not accepted");
        assert_eq!(e, ManifestError::ApiIncompatible { wants: 99, have: 1 });
        assert!(e.to_string().contains("it does not load"), "{e}");
    }

    #[test]
    fn an_id_that_cannot_be_a_folder_does_not_pass() {
        for bad in [
            "no-dot",
            "../escape",
            "with/slash",
            "With.Uppercase",
            ".starts.with.a.dot",
            "ends.with.a.dot.",
            "double..dot",
            "",
        ] {
            let json = base().replace("com.example.test", bad);
            assert!(
                matches!(Manifest::parse(&json), Err(ManifestError::BadId(_))),
                "\"{bad}\" should not pass"
            );
        }
        // And one that does.
        let json = base().replace("com.example.test", "org.my-service.plugin2");
        assert!(Manifest::parse(&json).is_ok());
    }

    #[test]
    fn a_wildcard_in_the_domains_does_not_pass() {
        for bad in ["*", "*.example.com", "https://api.example.com", "api.example.com/v1", "localhost"] {
            let json = base().replace("api.example.com", bad);
            assert!(
                matches!(Manifest::parse(&json), Err(ManifestError::BadDomain(_))),
                "\"{bad}\" should not pass"
            );
        }
    }

    #[test]
    fn only_the_compatibility_runtime_accepts_an_explicit_full_network_grant() {
        let mut value: serde_json::Value = serde_json::from_str(&base()).unwrap();
        value["runtime"] = serde_json::json!("bridge");
        value["capabilities"] = serde_json::json!([{"type":"network","domains":["*"]}]);
        assert!(Manifest::parse_for_runtime(&value.to_string(), Runtime::Bridge).is_ok());
        assert!(Manifest::parse(&value.to_string()).is_err());
    }

    #[test]
    fn a_wasm_plugin_cannot_claim_it_manages_its_own_account() {
        // It has neither sockets nor a disk of its own: saying so would be
        // a lie, and it is refused at install instead of ignored.
        let e = Manifest::parse(
            r#"{"id":"com.a.b","name":"One","version":"1","api_version":1,"kind":"source",
                "capabilities":[{"type":"own_account"}]}"#,
        )
        .unwrap_err();
        assert!(e.to_string().contains("separate process"), "{e}");
    }

    #[test]
    fn managing_its_own_account_is_said_plainly_and_not_like_the_others() {
        // It is the only capability that leaves Chilpa out. Approving it
        // should not feel like approving a panel.
        // Keeping it in the system keyring is better news than keeping
        // it just anywhere, and it is worded differently.
        let in_keyring = Capability::OwnAccount { keyring: true }.describe();
        assert!(in_keyring.contains("system keyring"), "{in_keyring}");
        assert!(in_keyring.contains("its own name"), "separate from Chilpa's: {in_keyring}");

        let anywhere = Capability::OwnAccount { keyring: false }.describe();
        assert!(anywhere.contains("plaintext file"), "{anywhere}");

        // Both say the same thing underneath, including that it is not checked.
        for said in [in_keyring, anywhere] {
            assert!(said.contains("outside Chilpa"), "{said}");
            assert!(said.contains("does not see it"), "{said}");
            assert!(said.contains("delete"), "{said}");
            assert!(said.contains("check"), "it is a declaration, not a guarantee: {said}");
        }
    }

    #[test]
    fn the_process_runtime_is_refused_and_says_why() {
        let json = base().replace(r#""kind": "source""#, r#""kind": "source", "runtime": "process""#);
        let e = Manifest::parse(&json).expect_err("does not exist yet");
        assert!(e.to_string().contains("does not exist yet"), "{e}");
    }

    #[test]
    fn a_manifest_that_is_not_json_is_refused_with_its_reason() {
        let e = Manifest::parse("this is not json").expect_err("it is not json");
        assert!(matches!(e, ManifestError::Unreadable(_)));
        let e = Manifest::parse(r#"{"id": "com.a.b"}"#).expect_err("it is missing fields");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn an_empty_name_or_version_does_not_pass() {
        let json = base().replace(r#""name": "Test""#, r#""name": "   ""#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty(chilpa_i18n::mark("the name"))));
        let json = base().replace(r#""version": "0.1.0""#, r#""version": """#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty(chilpa_i18n::mark("the version"))));
    }
}
