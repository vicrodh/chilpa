//! Una petición HTTP hecha por cuenta de alguien más.
//!
//! Es lo que Chilpa hace cuando un plugin pide salir a la red (ADR-007). El
//! plugin dice a dónde y con qué; **la credencial la pone Chilpa**, de este
//! lado, y en la memoria del plugin nunca estuvo.
//!
//! Entra y sale JSON porque es lo que cualquier lenguaje sabe leer y lo que
//! se puede mirar con los ojos cuando algo falla.
//!
//! Lo que este módulo no hace, a propósito: no decide si el dominio está
//! permitido. Eso ya se revisó antes de llegar aquí, con el manifest en la
//! mano, y repetirlo en dos lados es tener dos reglas que se pueden separar.

use std::collections::BTreeMap;
use std::time::Duration;

/// Cuánto se espera a que el otro lado conteste.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Cuánto se acepta de respuesta. Más que esto es alguien mandando un disco
/// por una API, y no tiene por qué caber en memoria.
const MAX: usize = 8 * 1024 * 1024;

/// Lo que se pidió, ya leído del JSON.
#[derive(Debug, Default)]
struct Request {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    body: String,
}

/// Hace la petición y contesta JSON.
///
/// Nunca devuelve `Err`: un problema de red es una respuesta con `error`,
/// porque del otro lado hay un plugin que solo entiende JSON.
pub fn send(request_json: &[u8], credential: Option<&str>) -> Vec<u8> {
    let p = match read(request_json) {
        Ok(p) => p,
        Err(e) => return error(&e),
    };
    if !p.url.starts_with("https://") && !p.url.starts_with("http://") {
        return error("la URL tiene que ser http o https");
    }

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .new_agent();

    // Los encabezados que de verdad se mandan. El plugin no puede poner su
    // propia autorización: esa la pone Chilpa, de este lado.
    let mut headers: Vec<(String, String)> = p
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("authorization"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(c) = credential {
        headers.push(("Authorization".into(), c.to_string()));
    }
    headers.push(("User-Agent".into(), "Chilpa".into()));

    // ureq distingue por tipo los métodos que llevan cuerpo de los que no,
    // así que las dos ramas se escriben aparte.
    let reply = if matches!(p.method.as_str(), "POST" | "PUT" | "PATCH") {
        let mut req = match p.method.as_str() {
            "PUT" => agent.put(&p.url),
            "PATCH" => agent.patch(&p.url),
            _ => agent.post(&p.url),
        };
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        req.send(&p.body)
    } else {
        let mut req = match p.method.as_str() {
            "DELETE" => agent.delete(&p.url),
            "HEAD" => agent.head(&p.url),
            _ => agent.get(&p.url),
        };
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        req.call()
    };

    match reply {
        Ok(r) => read_reply(r),
        Err(e) => error(&format!("no se pudo: {e}")),
    }
}

fn read_reply(r: ureq::http::Response<ureq::Body>) -> Vec<u8> {
    let status = r.status().as_u16();
    let headers: BTreeMap<String, String> = r
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let body = match r.into_body().read_to_string() {
        Ok(s) if s.len() <= MAX => s,
        Ok(_) => return error("la respuesta no cabe"),
        Err(e) => return error(&format!("no se pudo leer la respuesta: {e}")),
    };
    serde_json::to_vec(&serde_json::json!({
        "status": status,
        "headers": headers,
        "body": body,
    }))
    .unwrap_or_else(|_| error("no se pudo armar la respuesta"))
}

fn read(json: &[u8]) -> Result<Request, String> {
    let v: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("la petición no es JSON: {e}"))?;
    let url = v
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or("la petición no trae url")?
        .to_string();
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let headers = v
        .get("headers")
        .and_then(|h| h.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
    Ok(Request { url, method, headers, body })
}

fn error(reason: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "error": reason })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("siempre se contesta JSON")
    }

    #[test]
    fn a_request_that_is_not_json_answers_an_error_in_json() {
        let r = as_json(&send(b"basura", None));
        assert!(r["error"].as_str().unwrap().contains("no es JSON"));
    }

    #[test]
    fn no_url_means_no_request() {
        let r = as_json(&send(b"{}", None));
        assert!(r["error"].as_str().unwrap().contains("no trae url"));
    }

    #[test]
    fn a_scheme_that_is_not_http_is_refused_without_going_out() {
        for url in ["file:///etc/passwd", "ftp://algo/x", "chilpa://interno"] {
            let request = format!(r#"{{"url":"{url}"}}"#);
            let r = as_json(&send(request.as_bytes(), None));
            assert!(
                r["error"].as_str().unwrap().contains("http"),
                "\"{url}\" no debería intentarse: {r}"
            );
        }
    }

    #[test]
    fn what_the_plugin_asked_for_is_read() {
        let p = read(br#"{"url":"https://a.b/c","method":"post","headers":{"X":"1"},"body":"hola"}"#)
            .unwrap();
        assert_eq!(p.url, "https://a.b/c");
        assert_eq!(p.method, "POST", "el método se normaliza");
        assert_eq!(p.headers.get("X").map(String::as_str), Some("1"));
        assert_eq!(p.body, "hola");
    }

    #[test]
    fn no_method_means_a_get() {
        let p = read(br#"{"url":"https://a.b"}"#).unwrap();
        assert_eq!(p.method, "GET");
        assert!(p.headers.is_empty() && p.body.is_empty());
    }
}
