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
                format!("salir a internet, solo a {}", domains.join(", "))
            }
            Capability::Storage => "guardar sus propias cosas en disco".into(),
            Capability::LibraryRead => "leer tu biblioteca, sin poder cambiarla".into(),
            Capability::RegisterSource => "aportar catálogo: buscar y traer música".into(),
            Capability::RegisterPanel => "poner un panel suyo en la ventana".into(),
            Capability::RegisterCommand => "agregar un comando que tú disparas".into(),
            // It is worded differently from the rest on purpose: it is
            // the only one that leaves Chilpa out, and approving it
            // should not feel like approving a panel. And it says "says
            // it", because that is the only honest wording: Chilpa
            // cannot check it.
            Capability::OwnAccount { keyring: true } => "manejar tu cuenta él mismo, fuera \
                de Chilpa: dice que guarda tu credencial en el llavero del sistema, bajo su \
                propio nombre. Chilpa no la ve, no la puede borrar y no puede comprobarlo"
                .into(),
            Capability::OwnAccount { keyring: false } => "manejar tu cuenta él mismo, fuera \
                de Chilpa: tu credencial queda donde él decida, que puede ser un archivo en \
                claro. Chilpa no la ve, no la puede borrar y no puede comprobarlo"
                .into(),
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
            ManifestError::BadAuth(e) => write!(f, "cómo conecta la cuenta no sirve: {e}"),
            ManifestError::Unreadable(e) => write!(f, "el manifest no se entiende: {e}"),
            ManifestError::BadId(id) => write!(
                f,
                "\"{id}\" no sirve como id: se usa como nombre de carpeta, así que va en \
                 minúsculas, con puntos, guiones o dígitos, y sin barras ni puntos dobles"
            ),
            ManifestError::Empty(field) => write!(f, "falta {field}"),
            ManifestError::ApiIncompatible { wants, have } => write!(
                f,
                "pide la versión {wants} de la API y este Chilpa entiende la {have}; \
                 no se carga"
            ),
            ManifestError::RuntimeNotImplemented(r) => {
                write!(f, "pide el runtime {r:?}, que todavía no existe")
            }
            ManifestError::BadDomain(d) => {
                write!(f, "\"{d}\" no es un dominio al que se pueda dar permiso")
            }
        }
    }
}

impl Manifest {
    /// Reads a manifest and validates it whole.
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let m: Manifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Unreadable(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.name.trim().is_empty() {
            return Err(ManifestError::Empty("el nombre"));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Empty("la versión"));
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
        if self.runtime == Runtime::Process {
            return Err(ManifestError::RuntimeNotImplemented(self.runtime));
        }
        for c in &self.capabilities {
            if let Capability::Network { domains } = c {
                for d in domains {
                    if !valid_domain(d) {
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
                "dice manejar su propia cuenta, pero eso solo puede hacerlo un plugin de \
                 proceso aparte: en WASM no hay sockets ni disco suyo"
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
        Asked { kind: self.kind, capabilities, auth: self.auth.clone() }
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
            return vec![format!("ser un plugin de otro tipo: {:?}", self.kind)];
        }
        let mut out: Vec<String> = self
            .capabilities
            .iter()
            .filter(|c| !approved.capabilities.contains(c))
            .map(Capability::describe)
            .collect();
        if self.auth != approved.auth {
            out.push(match &self.auth {
                Some(a) if a.host().is_empty() => "guardar una credencial tuya".to_string(),
                Some(a) => format!("mandarte a dar permiso en {}", a.host()),
                None => "dejar de usar la cuenta que tenía".to_string(),
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
            "id": "com.ejemplo.prueba",
            "name": "Prueba",
            "version": "0.1.0",
            "api_version": 1,
            "kind": "source",
            "capabilities": [
                {"type": "network", "domains": ["api.ejemplo.com"]},
                {"type": "storage"}
            ]
        }"#
        .to_string()
    }

    #[test]
    fn a_good_manifest_is_read_in_full() {
        let m = Manifest::parse(&base()).expect("es válido");
        assert_eq!(m.id, "com.ejemplo.prueba");
        assert_eq!(m.kind, Kind::Source);
        assert_eq!(m.runtime, Runtime::Wasm, "el runtime por omisión es wasm");
        assert_eq!(m.allowed_domains(), vec!["api.ejemplo.com"]);
        assert!(m.can_store() && m.can_go_out());
    }

    #[test]
    fn with_no_capabilities_it_can_do_nothing() {
        let json = base().replace(
            r#""capabilities": [
                {"type": "network", "domains": ["api.ejemplo.com"]},
                {"type": "storage"}
            ]"#,
            r#""capabilities": []"#,
        );
        let m = Manifest::parse(&json).unwrap();
        assert!(!m.can_go_out(), "sin red declarada no hay red");
        assert!(!m.can_store());
    }

    #[test]
    fn an_unknown_capability_is_not_ignored() {
        // Fail closed: if what it asks for is unknown, whether it is
        // dangerous cannot be known.
        let json = base().replace(r#"{"type": "storage"}"#, r#"{"type": "leer_todo_el_disco"}"#);
        let e = Manifest::parse(&json).expect_err("no se acepta");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn a_different_api_version_does_not_load() {
        let json = base().replace(r#""api_version": 1"#, r#""api_version": 99"#);
        let e = Manifest::parse(&json).expect_err("no se acepta");
        assert_eq!(e, ManifestError::ApiIncompatible { wants: 99, have: 1 });
        assert!(e.to_string().contains("no se carga"), "{e}");
    }

    #[test]
    fn an_id_that_cannot_be_a_folder_does_not_pass() {
        for bad in [
            "sin-punto",
            "../escapar",
            "con/barra",
            "Con.Mayusculas",
            ".empieza.en.punto",
            "termina.en.punto.",
            "doble..punto",
            "",
        ] {
            let json = base().replace("com.ejemplo.prueba", bad);
            assert!(
                matches!(Manifest::parse(&json), Err(ManifestError::BadId(_))),
                "\"{bad}\" no debería pasar"
            );
        }
        // And one that does.
        let json = base().replace("com.ejemplo.prueba", "org.mi-servicio.plugin2");
        assert!(Manifest::parse(&json).is_ok());
    }

    #[test]
    fn a_wildcard_in_the_domains_does_not_pass() {
        for bad in ["*", "*.ejemplo.com", "https://api.ejemplo.com", "api.ejemplo.com/v1", "localhost"] {
            let json = base().replace("api.ejemplo.com", bad);
            assert!(
                matches!(Manifest::parse(&json), Err(ManifestError::BadDomain(_))),
                "\"{bad}\" no debería pasar"
            );
        }
    }

    #[test]
    fn a_wasm_plugin_cannot_claim_it_manages_its_own_account() {
        // It has neither sockets nor a disk of its own: saying so would be
        // a lie, and it is refused at install instead of ignored.
        let e = Manifest::parse(
            r#"{"id":"com.a.b","name":"Uno","version":"1","api_version":1,"kind":"source",
                "capabilities":[{"type":"own_account"}]}"#,
        )
        .unwrap_err();
        assert!(e.to_string().contains("proceso aparte"), "{e}");
    }

    #[test]
    fn managing_its_own_account_is_said_plainly_and_not_like_the_others() {
        // It is the only capability that leaves Chilpa out. Approving it
        // should not feel like approving a panel.
        // Keeping it in the system keyring is better news than keeping
        // it just anywhere, and it is worded differently.
        let in_keyring = Capability::OwnAccount { keyring: true }.describe();
        assert!(in_keyring.contains("llavero del sistema"), "{in_keyring}");
        assert!(in_keyring.contains("su propio nombre"), "separate from Chilpa's: {in_keyring}");

        let anywhere = Capability::OwnAccount { keyring: false }.describe();
        assert!(anywhere.contains("archivo en claro"), "{anywhere}");

        // Both say the same thing underneath, including that it is not checked.
        for said in [in_keyring, anywhere] {
            assert!(said.contains("fuera de Chilpa"), "{said}");
            assert!(said.contains("no la ve"), "{said}");
            assert!(said.contains("borrar"), "{said}");
            assert!(said.contains("comprobarlo"), "es una declaración, no una garantía: {said}");
        }
    }

    #[test]
    fn the_process_runtime_is_refused_and_says_why() {
        let json = base().replace(r#""kind": "source""#, r#""kind": "source", "runtime": "process""#);
        let e = Manifest::parse(&json).expect_err("todavía no existe");
        assert!(e.to_string().contains("todavía no existe"), "{e}");
    }

    #[test]
    fn a_manifest_that_is_not_json_is_refused_with_its_reason() {
        let e = Manifest::parse("esto no es json").expect_err("no es json");
        assert!(matches!(e, ManifestError::Unreadable(_)));
        let e = Manifest::parse(r#"{"id": "com.a.b"}"#).expect_err("le faltan campos");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn an_empty_name_or_version_does_not_pass() {
        let json = base().replace(r#""name": "Prueba""#, r#""name": "   ""#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty("el nombre")));
        let json = base().replace(r#""version": "0.1.0""#, r#""version": """#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty("la versión")));
    }
}
