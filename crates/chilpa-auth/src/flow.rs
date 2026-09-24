//! El flujo: a dónde se manda al usuario, qué contesta el navegador, y cómo
//! eso se vuelve tokens.

use crate::pkce::{nonce, Pkce};
use crate::{secure, Auth, AuthError, Tokens};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

/// Cuánto se espera a que el usuario termine en el navegador.
const PATIENCE: Duration = Duration::from_secs(300);

/// La dirección a la que el navegador regresa.
///
/// Es loopback con puerto efímero: el RFC 8252 lo prefiere sobre un esquema
/// propio (`chilpa://`) porque un esquema lo puede registrar cualquier otra
/// aplicación instalada, y entonces el código de autorización llega a manos
/// ajenas.
pub struct Loopback {
    listener: TcpListener,
    state: String,
}

impl Loopback {
    pub fn bind() -> Result<Self, AuthError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(false)?;
        Ok(Self { listener, state: nonce() })
    }

    /// A dónde le decimos al servicio que mande de vuelta al navegador.
    pub fn redirect_uri(&self) -> String {
        let port = self.listener.local_addr().map(|a| a.port()).unwrap_or(0);
        format!("http://127.0.0.1:{port}/callback")
    }

    pub fn state(&self) -> &str {
        &self.state
    }
}

/// La URL a la que hay que mandar al usuario.
///
/// No la abre: quien llama decide cómo se abre un navegador, que es cosa
/// del sistema y no de este crate.
pub fn authorize_url(auth: &Auth, pkce: &Pkce, back: &Loopback) -> Option<String> {
    let Auth::OAuth2 { authorize_url, client_id, scopes, .. } = auth else {
        return None;
    };
    let joiner = if authorize_url.contains('?') { '&' } else { '?' };
    let mut url = format!(
        "{authorize_url}{joiner}response_type=code&client_id={}&redirect_uri={}&state={}\
         &code_challenge={}&code_challenge_method={}",
        escape(client_id),
        escape(&back.redirect_uri()),
        escape(back.state()),
        escape(&pkce.challenge()),
        Pkce::method(),
    );
    if !scopes.is_empty() {
        url.push_str(&format!("&scope={}", escape(&scopes.join(" "))));
    }
    Some(url)
}

/// Espera a que el navegador regrese y contesta el código.
///
/// Bloquea. Quien llama lo corre en su propio hilo, porque el usuario puede
/// tardar lo que quiera en decidir.
pub fn wait_for_code(back: &Loopback) -> Result<String, AuthError> {
    back.listener.set_nonblocking(false)?;
    let deadline = std::time::Instant::now() + PATIENCE;

    while std::time::Instant::now() < deadline {
        let (mut stream, _) = back.listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        let head = request_line(&mut stream);
        let Some(target) = head.split_whitespace().nth(1) else {
            let _ = answer(&mut stream, "No entendí esa petición.");
            continue;
        };
        // El navegador pide el favicon además del callback; eso no es la
        // respuesta que se espera.
        let Some(query) = target.split_once('?').map(|(_, q)| q.to_string()) else {
            let _ = answer(&mut stream, "Falta la respuesta del servicio.");
            continue;
        };

        let mut code = None;
        let mut state = None;
        let mut failed = None;
        for (k, v) in query.split('&').filter_map(|p| p.split_once('=')) {
            match k {
                "code" => code = Some(unescape(v)),
                "state" => state = Some(unescape(v)),
                "error" => failed = Some(unescape(v)),
                _ => {}
            }
        }

        if let Some(e) = failed {
            let _ = answer(&mut stream, "No se conectó la cuenta. Puedes cerrar esta pestaña.");
            return Err(AuthError::NoCode(e));
        }
        // Se compara antes de usar el código: un `state` que no es el
        // nuestro quiere decir que esta respuesta la provocó alguien más.
        if state.as_deref() != Some(back.state()) {
            let _ = answer(&mut stream, "Esa respuesta no corresponde a esta ventana.");
            return Err(AuthError::WrongState);
        }
        let Some(code) = code else {
            let _ = answer(&mut stream, "El servicio no mandó un código.");
            continue;
        };
        let _ = answer(&mut stream, "Listo. Ya puedes cerrar esta pestaña y volver a Chilpa.");
        return Ok(code);
    }
    Err(AuthError::NoCode("se acabó la espera".into()))
}

/// Cómo se manda una petición al servicio.
///
/// Se inyecta en lugar de traer un cliente HTTP adentro. Así este crate no
/// depende de la red, y `chilpa-plugin` puede usar su tipo [`Auth`] para
/// leer un manifest sin que el sandbox acabe enlazando un cliente HTTP que
/// no tiene por qué existir ahí.
///
/// Recibe la URL y el cuerpo ya formado; contesta el código de estado y el
/// cuerpo de la respuesta.
pub type Post<'a> = &'a dyn Fn(&str, &str) -> Result<(u16, String), String>;

/// Cambia el código por los tokens.
pub fn exchange(
    auth: &Auth,
    code: &str,
    pkce: &Pkce,
    back: &Loopback,
    now_unix: i64,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    let Auth::OAuth2 { token_url, client_id, .. } = auth else {
        return Err(AuthError::Unreadable("ese source no usa OAuth".into()));
    };
    let form = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &back.redirect_uri()),
        ("client_id", client_id),
        ("code_verifier", pkce.verifier()),
    ]);
    post(token_url, &form, now_unix, "", post_with)
}

/// Pide un access nuevo con el refresh, sin molestar al usuario.
pub fn refresh(
    auth: &Auth,
    tokens: &Tokens,
    now_unix: i64,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    let Auth::OAuth2 { token_url, client_id, .. } = auth else {
        return Err(AuthError::Unreadable("ese source no usa OAuth".into()));
    };
    let form = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
        ("client_id", client_id),
    ]);
    // Muchos servicios no devuelven un refresh nuevo al renovar; el que se
    // tenía sigue valiendo, y perderlo obligaría a reconectar la cuenta.
    post(token_url, &form, now_unix, &tokens.refresh_token, post_with)
}

fn post(
    url: &str,
    body: &str,
    now_unix: i64,
    keep_refresh: &str,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    if !secure(url) {
        return Err(AuthError::Unreadable("el token_url no es https".into()));
    }
    let (status, text) = post_with(url, body).map_err(AuthError::Network)?;
    let text = text.as_str();
    if !(200..300).contains(&status) {
        // Se corta: el cuerpo de un error puede traer el token de otra
        // cuenta, o media página de HTML.
        let short: String = text.chars().take(200).collect();
        return Err(AuthError::Refused(status, short));
    }

    let body: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AuthError::Unreadable(e.to_string()))?;
    let access = body.get("access_token").and_then(|t| t.as_str()).unwrap_or_default();
    if access.is_empty() {
        return Err(AuthError::NoToken);
    }
    let refresh = body
        .get("refresh_token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .unwrap_or(keep_refresh);
    let expires_at = body.get("expires_in").and_then(|e| e.as_i64()).map(|s| now_unix + s);

    Ok(Tokens {
        access_token: access.to_string(),
        refresh_token: refresh.to_string(),
        expires_at,
    })
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", escape(v))).collect::<Vec<_>>().join("&")
}

/// Escapa para una URL. Solo se dejan pasar los caracteres que el RFC 3986
/// llama "unreserved"; todo lo demás va en por ciento.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// La primera línea de la petición, leyendo hasta encontrarla.
///
/// Una lectura no trae una petición: TCP entrega lo que quiera y cuando
/// quiera. Medido en loopback, la primera trajo `GET /` y nada más, y
/// quedarse con eso hacía que la respuesta del navegador se tirara y la
/// espera nunca terminara.
fn request_line(stream: &mut std::net::TcpStream) -> String {
    // Una petición cuya primera línea no cabe aquí no es un navegador
    // regresando de dar permiso.
    const MAX: usize = 8 * 1024;
    let mut raw = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while raw.len() < MAX {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                if byte[0] != b'\r' {
                    raw.push(byte[0]);
                }
            }
        }
    }
    String::from_utf8_lossy(&raw).to_string()
}

fn answer(stream: &mut std::net::TcpStream, message: &str) -> std::io::Result<()> {
    let page = format!(
        "<!doctype html><html lang=\"es\"><meta charset=\"utf-8\">\
         <title>Chilpa</title><body style=\"font-family:sans-serif;padding:3rem\">\
         <p>{message}</p></body></html>"
    );
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth::OAuth2 {
            authorize_url: "https://cuentas.ejemplo.com/authorize".into(),
            token_url: "https://cuentas.ejemplo.com/token".into(),
            client_id: "chilpa app".into(),
            scopes: vec!["leer".into(), "escribir".into()],
        }
    }

    #[test]
    fn the_url_the_user_is_sent_to_carries_the_challenge_and_never_the_verifier() {
        let back = Loopback::bind().unwrap();
        let pkce = Pkce::new();
        let url = authorize_url(&auth(), &pkce, &back).unwrap();

        assert!(url.contains("response_type=code"));
        assert!(url.contains(&format!("code_challenge={}", escape(&pkce.challenge()))));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(!url.contains(pkce.verifier()), "el verificador no sale de este proceso");
        assert!(url.contains("scope=leer%20escribir"), "{url}");
        assert!(url.contains("client_id=chilpa%20app"), "lo que lleva espacios va escapado");
        assert!(url.contains("127.0.0.1"), "regresa a loopback: {url}");
    }

    #[test]
    fn the_redirect_is_a_loopback_port_that_the_system_gave_us() {
        let back = Loopback::bind().unwrap();
        let uri = back.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:"), "{uri}");
        assert!(!uri.ends_with(":0"), "un puerto de verdad: {uri}");
    }

    #[test]
    fn two_connections_do_not_share_a_state() {
        assert_ne!(Loopback::bind().unwrap().state(), Loopback::bind().unwrap().state());
    }

    #[test]
    fn what_goes_in_a_url_comes_back_the_same() {
        for s in ["hola", "con espacio", "a+b/c=d", "ñandú", "%41"] {
            assert_eq!(unescape(&escape(s)), s, "{s}");
        }
    }

    #[test]
    fn a_static_token_source_has_no_url_to_send_anyone_to() {
        let back = Loopback::bind().unwrap();
        assert!(authorize_url(&Auth::Token, &Pkce::new(), &back).is_none());
    }
}
