//! El manifest de un plugin: quién es y qué pide.
//!
//! Se valida entero antes de cargar nada. Las reglas de aquí no son
//! formalidades: el `id` se vuelve un nombre de carpeta y los dominios se
//! vuelven la lista de a dónde puede salir. Un manifest que no se entiende
//! **no carga**, en vez de cargar con lo que sí se entendió.
//!
//! Se falla cerrado a propósito. Una capacidad desconocida no se ignora: si
//! este Chilpa no sabe qué quiere decir, no puede saber si es peligrosa.

use serde::{Deserialize, Serialize};

/// La versión de la API de plugins que entiende este Chilpa.
pub const API_VERSION: u32 = 1;

/// Para qué sirve un plugin.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Aporta catálogo: playlists, álbumes, artistas, tracks.
    Source,
    /// Completa metadata de lo que ya hay.
    Metadata,
    /// Trae letras.
    Lyrics,
    /// Reporta lo escuchado a un servicio.
    Scrobbler,
}

/// Dónde corre.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// Adentro, en sandbox. Lo normal (ADR-007).
    #[default]
    Wasm,
    /// Proceso aparte, para lo que necesite código nativo. Todavía no se
    /// implementa; un manifest que lo pida se rechaza diciéndolo.
    Process,
}

/// Lo que un plugin pide poder hacer.
///
/// El usuario las aprueba al instalarlo. Lo que no está aquí, no se puede:
/// no hay una capacidad "todo lo demás".
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Capability {
    /// Salir a la red, solo a estos dominios.
    Network { domains: Vec<String> },
    /// Guardar cosas en su propio almacén.
    Storage,
    /// Leer la biblioteca. Nunca escribirla.
    LibraryRead,
    /// Registrar una source con su catálogo.
    RegisterSource,
    /// Aportar un panel a la interfaz, como descriptor.
    RegisterPanel,
    /// Registrar un comando que el usuario puede disparar.
    RegisterCommand,
}

/// Lo que un plugin dice de sí mismo.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Identidad, en dominio al revés: `com.ejemplo.mi-plugin`.
    pub id: String,
    /// Cómo se llama para el usuario.
    pub name: String,
    /// Su versión, que es suya y no nos importa más que para mostrarla.
    pub version: String,
    /// Con qué versión de la API de Chilpa trabaja.
    pub api_version: u32,
    pub kind: Kind,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

/// Por qué no se aceptó un manifest.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ManifestError {
    /// No es JSON, o le falta algo que no tiene valor por omisión.
    Unreadable(String),
    /// El `id` no sirve como identidad ni como nombre de carpeta.
    BadId(String),
    /// Un campo obligatorio vino vacío.
    Empty(&'static str),
    /// Pide una versión de la API que este Chilpa no entiende.
    ApiIncompatible { wants: u32, have: u32 },
    /// Pide un runtime que todavía no existe.
    RuntimeNotImplemented(Runtime),
    /// Un dominio de la lista de red no es un dominio.
    BadDomain(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
    /// Lee un manifest y lo valida entero.
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
        Ok(())
    }

    /// Los dominios a los que puede salir. Vacío quiere decir a ninguno.
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

    /// Cierto si pidió esa capacidad.
    pub fn has(&self, which: &Capability) -> bool {
        self.capabilities.contains(which)
    }

    /// Cierto si puede guardar cosas.
    pub fn can_store(&self) -> bool {
        self.capabilities.iter().any(|c| matches!(c, Capability::Storage))
    }

    /// Cierto si puede salir a la red.
    pub fn can_go_out(&self) -> bool {
        !self.allowed_domains().is_empty()
    }
}

/// Un id sirve si se puede usar como nombre de carpeta sin pensarlo.
///
/// Nada de barras, nada de `..`, nada de mayúsculas —dos ids que solo se
/// distinguen por eso serían la misma carpeta en un sistema que no
/// distingue— y nada vacío entre puntos.
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

/// Un dominio sirve si es un nombre de máquina, no un comodín ni una URL.
fn valid_domain(d: &str) -> bool {
    if d.is_empty() || d.len() > 253 || d.contains('/') || d.contains(':') {
        return false;
    }
    // Sin comodines: "*" o "*.algo" es pedir permiso para todo o para un
    // árbol entero, y eso no se puede revisar de un vistazo.
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
                {"tipo": "network", "domains": ["api.ejemplo.com"]},
                {"tipo": "storage"}
            ]
        }"#
        .to_string()
    }

    #[test]
    fn un_manifest_bueno_se_lee_entero() {
        let m = Manifest::parse(&base()).expect("es válido");
        assert_eq!(m.id, "com.ejemplo.prueba");
        assert_eq!(m.kind, Kind::Source);
        assert_eq!(m.runtime, Runtime::Wasm, "el runtime por omisión es wasm");
        assert_eq!(m.allowed_domains(), vec!["api.ejemplo.com"]);
        assert!(m.can_store() && m.can_go_out());
    }

    #[test]
    fn sin_capacidades_no_puede_nada() {
        let json = base().replace(
            r#""capabilities": [
                {"tipo": "network", "domains": ["api.ejemplo.com"]},
                {"tipo": "storage"}
            ]"#,
            r#""capabilities": []"#,
        );
        let m = Manifest::parse(&json).unwrap();
        assert!(!m.can_go_out(), "sin red declarada no hay red");
        assert!(!m.can_store());
    }

    #[test]
    fn una_capacidad_desconocida_no_se_ignora() {
        // Fallar cerrado: si no se sabe qué pide, no se puede saber si es
        // peligroso.
        let json = base().replace(r#"{"tipo": "storage"}"#, r#"{"tipo": "leer_todo_el_disco"}"#);
        let e = Manifest::parse(&json).expect_err("no se acepta");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn una_version_de_api_distinta_no_carga() {
        let json = base().replace(r#""api_version": 1"#, r#""api_version": 99"#);
        let e = Manifest::parse(&json).expect_err("no se acepta");
        assert_eq!(e, ManifestError::ApiIncompatible { wants: 99, have: 1 });
        assert!(e.to_string().contains("no se carga"), "{e}");
    }

    #[test]
    fn un_id_que_no_sirve_de_carpeta_no_pasa() {
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
        // Y uno que sí.
        let json = base().replace("com.ejemplo.prueba", "org.mi-servicio.plugin2");
        assert!(Manifest::parse(&json).is_ok());
    }

    #[test]
    fn un_comodin_en_los_dominios_no_pasa() {
        for bad in ["*", "*.ejemplo.com", "https://api.ejemplo.com", "api.ejemplo.com/v1", "localhost"] {
            let json = base().replace("api.ejemplo.com", bad);
            assert!(
                matches!(Manifest::parse(&json), Err(ManifestError::BadDomain(_))),
                "\"{bad}\" no debería pasar"
            );
        }
    }

    #[test]
    fn el_runtime_de_proceso_se_rechaza_diciendo_por_que() {
        let json = base().replace(r#""kind": "source""#, r#""kind": "source", "runtime": "process""#);
        let e = Manifest::parse(&json).expect_err("todavía no existe");
        assert!(e.to_string().contains("todavía no existe"), "{e}");
    }

    #[test]
    fn un_manifest_que_no_es_json_se_rechaza_con_su_razon() {
        let e = Manifest::parse("esto no es json").expect_err("no es json");
        assert!(matches!(e, ManifestError::Unreadable(_)));
        let e = Manifest::parse(r#"{"id": "com.a.b"}"#).expect_err("le faltan campos");
        assert!(matches!(e, ManifestError::Unreadable(_)), "{e}");
    }

    #[test]
    fn un_nombre_o_una_version_vacios_no_pasan() {
        let json = base().replace(r#""name": "Prueba""#, r#""name": "   ""#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty("el nombre")));
        let json = base().replace(r#""version": "0.1.0""#, r#""version": """#);
        assert_eq!(Manifest::parse(&json), Err(ManifestError::Empty("la versión")));
    }
}
