//! El flujo completo contra un servicio de mentiras, sin navegador.
//!
//! El navegador lo hace la prueba: pide la URL de regreso igual que lo haría
//! él. Lo que importa es que el código llegue amarrado a su `state`, que el
//! verificador viaje solo al cambiarlo por tokens, y que renovar no pierda
//! el refresh.

use chilpa_auth::{authorize_url, exchange, refresh, Auth, Loopback, Pkce, Tokens};
use std::io::{Read, Write};
use std::net::TcpListener;

/// Un servicio que entrega tokens y guarda qué le pidieron.
struct Service {
    port: u16,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

fn service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mine = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            // Se lee hasta tener el cuerpo entero: el cliente manda los
            // encabezados y el cuerpo por separado, y quedarse con la
            // primera lectura haría pasar la prueba sin mirar nada.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let body = loop {
                let read = stream.read(&mut buf).unwrap_or(0);
                if read == 0 {
                    break String::new();
                }
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                let want: usize = head
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse().ok()))
                    .unwrap_or(0);
                if body.len() >= want {
                    break body.to_string();
                }
            };
            mine.lock().unwrap().push(body.clone());

            // Al renovar no manda refresh nuevo, como muchos servicios.
            let json = if body.contains("grant_type=refresh_token") {
                r#"{"access_token":"segundo","token_type":"Bearer","expires_in":3600}"#
            } else {
                r#"{"access_token":"primero","refresh_token":"para-renovar",
                    "token_type":"Bearer","expires_in":3600}"#
            };
            // `Connection: close` porque este servidor atiende una petición
            // por conexión y el cliente las reutiliza: sin esto la segunda
            // llamada se queda esperando a nadie.
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

fn auth(port: u16) -> Auth {
    Auth::OAuth2 {
        authorize_url: format!("http://127.0.0.1:{port}/authorize"),
        token_url: format!("http://127.0.0.1:{port}/token"),
        client_id: "chilpa".into(),
        scopes: vec!["leer".into()],
    }
}

/// Hace lo que haría el navegador: pedir la URL de regreso.
fn browser(redirect: &str, query: &str) {
    let rest = redirect.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    let _ = write!(stream, "GET /{path}?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    let _ = stream.flush();
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink);
}

fn now() -> i64 {
    1_000_000
}

#[test]
fn a_whole_connection_from_the_browser_coming_back_to_a_renewed_token() {
    let svc = service();
    let auth = auth(svc.port);
    assert_eq!(auth.problem(), None, "loopback por http se acepta");

    let back = Loopback::bind().unwrap();
    let pkce = Pkce::new();
    let url = authorize_url(&auth, &pkce, &back).unwrap();
    assert!(url.contains("code_challenge="), "{url}");
    assert!(!url.contains(pkce.verifier()), "el verificador no va en la URL");

    // El navegador regresa con el código y el mismo state.
    let redirect = back.redirect_uri();
    let state = back.state().to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, &format!("code=el-codigo&state={state}"));
    });

    let code = chilpa_auth::wait_for_code(&back).expect("llegó el código");
    assert_eq!(code, "el-codigo");

    // Se cambia por tokens. Aquí, y solo aquí, viaja el verificador.
    let tokens = exchange(&auth, &code, &pkce, &back, now()).expect("cambió");
    assert_eq!(tokens.access_token, "primero");
    assert_eq!(tokens.refresh_token, "para-renovar");
    assert_eq!(tokens.expires_at, Some(now() + 3600));
    assert!(!tokens.stale(now()), "recién sacado");

    let asked = svc.seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("grant_type=authorization_code"));
    assert!(asked[0].contains(&format!("code_verifier={}", pkce.verifier())), "{}", asked[0]);
    assert!(asked[0].contains("client_id=chilpa"));

    // Y cuando caduca, se renueva sin molestar a nadie.
    let old = Tokens { expires_at: Some(now() - 1), ..tokens.clone() };
    assert!(old.stale(now()));
    assert!(old.renewable());
    let fresh = refresh(&auth, &old, now()).expect("renovó");
    assert_eq!(fresh.access_token, "segundo");
    assert_eq!(
        fresh.refresh_token, "para-renovar",
        "el servicio no mandó uno nuevo, así que se conserva el que había"
    );
    assert_eq!(fresh.expires_at, Some(now() + 3600));
}

#[test]
fn a_reply_with_someone_else_state_is_refused_without_using_the_code() {
    let back = Loopback::bind().unwrap();
    let redirect = back.redirect_uri();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, "code=robado&state=de-alguien-mas");
    });
    let e = chilpa_auth::wait_for_code(&back).expect_err("no se acepta");
    assert!(matches!(e, chilpa_auth::AuthError::WrongState), "{e}");
}

#[test]
fn a_user_who_says_no_is_reported_and_not_retried() {
    let back = Loopback::bind().unwrap();
    let redirect = back.redirect_uri();
    let state = back.state().to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, &format!("error=access_denied&state={state}"));
    });
    let e = chilpa_auth::wait_for_code(&back).expect_err("dijo que no");
    assert!(matches!(e, chilpa_auth::AuthError::NoCode(ref s) if s == "access_denied"), "{e}");
}
