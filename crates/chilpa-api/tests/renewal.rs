//! Un acceso vencido se renueva solo, del lado de Chilpa.
//!
//! El plugin pide salir a su dominio con el token de ayer guardado. Chilpa
//! ve que venció, lo cambia por uno nuevo contra el servicio, guarda el
//! nuevo cifrado, y la petición del plugin sale firmada con ese. El plugin
//! no participa: no tiene cómo pedir el token ni cómo saber que hubo una
//! renovación (ADR-008).
//!
//! Es una sola prueba porque apunta `XDG_DATA_HOME` a un directorio
//! temporal, y esa variable es del proceso entero.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Un servicio que renueva tokens y guarda con qué le firmaron cada
/// petición, para poder decir qué vio.
struct Service {
    port: u16,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

fn service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mine = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let text = loop {
                let read = stream.read(&mut buf).unwrap_or(0);
                if read == 0 {
                    break String::from_utf8_lossy(&raw).to_string();
                }
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                let want: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= want {
                    break text;
                }
            };

            let target = text.split_whitespace().nth(1).unwrap_or("").to_string();
            let authorization = text
                .lines()
                .find_map(|l| l.strip_prefix("authorization: ").or(l.strip_prefix("Authorization: ")))
                .unwrap_or("")
                .trim()
                .to_string();
            mine.lock().unwrap().push((target.clone(), authorization));

            // El primero nace vencido a propósito: es una cuenta que se
            // conectó hace rato y cuyo acceso ya no sirve. El segundo, el
            // de renovar, dura una hora.
            let json = if target.starts_with("/token") {
                if text.contains("grant_type=refresh_token") {
                    r#"{"access_token":"token-nuevo","token_type":"Bearer","expires_in":3600}"#
                } else {
                    r#"{"access_token":"token-viejo","refresh_token":"para-renovar",
                        "token_type":"Bearer","expires_in":0}"#
                }
            } else {
                r#"{"ok":true}"#
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                json.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(json.as_bytes());
            let _ = stream.flush();
        }
    });
    Service { port, seen }
}

/// Un plugin que solo sale a la red, con el dominio y el puerto que se le
/// den. El largo del dato se calcula aquí.
fn install(dir: &Path, id: &str, port: u16) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "Uno", "version": "0.1.0", "api_version": 1,
                 "kind": "source",
                 "auth": {{"type": "oauth2",
                           "authorize_url": "http://127.0.0.1:{port}/authorize",
                           "token_url": "http://127.0.0.1:{port}/token",
                           "client_id": "chilpa"}},
                 "capabilities": [{{"type":"network","domains":["127.0.0.1"]}}]}}"#
        ),
    )
    .unwrap();

    let request = format!(r#"{{"url":"http://127.0.0.1:{port}/api/algo","method":"GET"}}"#);
    let esc = request.replace('"', "\\\"");
    let wat = format!(
        r#"(module
            (import "chilpa" "http" (func $http (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 8192))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 16) "{esc}")
            (data (i32.const 4096) "{{\"listo\":true}}")
            ;; Sale a la red y contesta algo fijo: lo que se mira es con qué
            ;; salió, no qué contestó.
            (func (export "salir") (param i32 i32) (result i64)
              (drop (call $http (i32.const 16) (i32.const {len})
                                (i32.const 6000) (i32.const 1024)))
              (i64.or (i64.shl (i64.const 4096) (i64.const 32)) (i64.const 14))))"#,
        len = request.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("compila"))
        .unwrap();
}

#[test]
fn an_expired_access_is_renewed_by_chilpa_and_the_plugin_never_sees_a_token() {
    let dir = std::env::temp_dir().join("chilpa-api-renewal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let svc = service();
    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(&root, "com.prueba.cuenta", svc.port);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();

    // Lo que pide incluye a dónde va a mandar al usuario a dar permiso.
    let listed = chilpa.plugins();
    assert_eq!(listed[0].state, "pendiente de aprobación", "{:?}", listed[0].problem);
    let pending = listed[0].pending.clone();
    assert!(
        pending.iter().any(|p| p.contains("dar permiso en 127.0.0.1")),
        "se aprueba también a dónde te manda: {pending:?}"
    );
    assert_eq!(chilpa.plugins()[0].account_host, "127.0.0.1");
    assert_eq!(chilpa.approve_plugin("com.prueba.cuenta"), 1);

    // Se conecta la cuenta por el camino de verdad: Chilpa da la URL, el
    // navegador regresa con el código, y Chilpa lo cambia por tokens.
    let url = chilpa.begin_connect("com.prueba.cuenta").expect("hay a dónde mandar");
    assert!(url.contains("code_challenge="), "con PKCE: {url}");
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"), "a loopback: {url}");

    let redirect = between(&url, "redirect_uri=", "&");
    let state = between(&url, "state=", "&");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&unescape(&redirect), &format!("code=el-codigo&state={state}"));
    });
    assert_eq!(chilpa.finish_connect().expect("conectó"), "com.prueba.cuenta");
    assert!(chilpa.plugins()[0].connected);
    assert!(!chilpa.plugins()[0].account_expired, "venció, pero hay con qué renovarla");

    // El plugin sale a la red. No pidió ningún token: no tiene cómo.
    chilpa.call_plugin("com.prueba.cuenta", "salir", b"{}").expect("salió");

    let seen = svc.seen.lock().unwrap().clone();
    // Dos: el intercambio del código al conectar, y la renovación de ahora.
    let at_token: Vec<_> = seen.iter().filter(|(t, _)| t.starts_with("/token")).collect();
    let calls: Vec<_> = seen.iter().filter(|(t, _)| t.starts_with("/api/")).collect();

    assert_eq!(at_token.len(), 2, "el intercambio y la renovación: {seen:?}");
    assert_eq!(calls.len(), 1, "y la petición del plugin salió: {seen:?}");
    assert_eq!(
        calls[0].1, "Bearer token-nuevo",
        "firmada con el token renovado, no con el vencido"
    );

    // Lo guardado quedó al día, así que la siguiente no vuelve a renovar.
    chilpa.call_plugin("com.prueba.cuenta", "salir", b"{}").expect("salió otra vez");
    let seen = svc.seen.lock().unwrap().clone();
    assert_eq!(
        seen.iter().filter(|(t, _)| t.starts_with("/token")).count(),
        2,
        "el token nuevo se guardó: no se renueva en cada petición"
    );

    // Y en disco no está ninguno de los dos en claro.
    let sealed =
        std::fs::read(root.join("com.prueba.cuenta").join("credential.sealed")).unwrap();
    for secret in ["token-viejo", "token-nuevo", "para-renovar"] {
        assert!(
            !sealed.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "{secret} quedó en claro"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}

/// Lo que hay entre dos marcas de una URL.
fn between(text: &str, from: &str, to: &str) -> String {
    let rest = text.split_once(from).map(|(_, r)| r).unwrap_or("");
    rest.split_once(to).map(|(v, _)| v).unwrap_or(rest).to_string()
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
            match u8::from_str_radix(hex, 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    out.push(b[i]);
                    i += 1;
                }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Hace lo que haría el navegador al regresar.
fn browser(redirect: &str, query: &str) {
    let rest = redirect.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    let _ = write!(stream, "GET /{path}?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    let _ = stream.flush();
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink);
}
