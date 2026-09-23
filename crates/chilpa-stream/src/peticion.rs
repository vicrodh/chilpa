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
const ESPERA: Duration = Duration::from_secs(20);

/// Cuánto se acepta de respuesta. Más que esto es alguien mandando un disco
/// por una API, y no tiene por qué caber en memoria.
const MAXIMO: usize = 8 * 1024 * 1024;

/// Lo que se pidió, ya leído del JSON.
#[derive(Debug, Default)]
struct Peticion {
    url: String,
    metodo: String,
    encabezados: BTreeMap<String, String>,
    cuerpo: String,
}

/// Hace la petición y contesta JSON.
///
/// Nunca devuelve `Err`: un problema de red es una respuesta con `error`,
/// porque del otro lado hay un plugin que solo entiende JSON.
pub fn hacer(peticion_json: &[u8], credencial: Option<&str>) -> Vec<u8> {
    let p = match leer(peticion_json) {
        Ok(p) => p,
        Err(e) => return error(&e),
    };
    if !p.url.starts_with("https://") && !p.url.starts_with("http://") {
        return error("la URL tiene que ser http o https");
    }

    let agente = ureq::Agent::config_builder()
        .timeout_global(Some(ESPERA))
        .build()
        .new_agent();

    // Los encabezados que de verdad se mandan. El plugin no puede poner su
    // propia autorización: esa la pone Chilpa, de este lado.
    let mut encabezados: Vec<(String, String)> = p
        .encabezados
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("authorization"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(c) = credencial {
        encabezados.push(("Authorization".into(), c.to_string()));
    }
    encabezados.push(("User-Agent".into(), "Chilpa".into()));

    // ureq distingue por tipo los métodos que llevan cuerpo de los que no,
    // así que las dos ramas se escriben aparte.
    let respuesta = if matches!(p.metodo.as_str(), "POST" | "PUT" | "PATCH") {
        let mut req = match p.metodo.as_str() {
            "PUT" => agente.put(&p.url),
            "PATCH" => agente.patch(&p.url),
            _ => agente.post(&p.url),
        };
        for (k, v) in &encabezados {
            req = req.header(k, v);
        }
        req.send(&p.cuerpo)
    } else {
        let mut req = match p.metodo.as_str() {
            "DELETE" => agente.delete(&p.url),
            "HEAD" => agente.head(&p.url),
            _ => agente.get(&p.url),
        };
        for (k, v) in &encabezados {
            req = req.header(k, v);
        }
        req.call()
    };

    match respuesta {
        Ok(r) => leer_respuesta(r),
        Err(e) => error(&format!("no se pudo: {e}")),
    }
}

fn leer_respuesta(r: ureq::http::Response<ureq::Body>) -> Vec<u8> {
    let status = r.status().as_u16();
    let encabezados: BTreeMap<String, String> = r
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let cuerpo = match r.into_body().read_to_string() {
        Ok(s) if s.len() <= MAXIMO => s,
        Ok(_) => return error("la respuesta no cabe"),
        Err(e) => return error(&format!("no se pudo leer la respuesta: {e}")),
    };
    serde_json::to_vec(&serde_json::json!({
        "status": status,
        "headers": encabezados,
        "body": cuerpo,
    }))
    .unwrap_or_else(|_| error("no se pudo armar la respuesta"))
}

fn leer(json: &[u8]) -> Result<Peticion, String> {
    let v: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("la petición no es JSON: {e}"))?;
    let url = v
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or("la petición no trae url")?
        .to_string();
    let metodo = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let encabezados = v
        .get("headers")
        .and_then(|h| h.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let cuerpo = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
    Ok(Peticion { url, metodo, encabezados, cuerpo })
}

fn error(motivo: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "error": motivo })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn como_json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("siempre se contesta JSON")
    }

    #[test]
    fn una_peticion_que_no_es_json_contesta_un_error_en_json() {
        let r = como_json(&hacer(b"basura", None));
        assert!(r["error"].as_str().unwrap().contains("no es JSON"));
    }

    #[test]
    fn sin_url_no_hay_peticion() {
        let r = como_json(&hacer(b"{}", None));
        assert!(r["error"].as_str().unwrap().contains("no trae url"));
    }

    #[test]
    fn un_esquema_que_no_es_http_se_rechaza_sin_salir() {
        for url in ["file:///etc/passwd", "ftp://algo/x", "chilpa://interno"] {
            let peticion = format!(r#"{{"url":"{url}"}}"#);
            let r = como_json(&hacer(peticion.as_bytes(), None));
            assert!(
                r["error"].as_str().unwrap().contains("http"),
                "\"{url}\" no debería intentarse: {r}"
            );
        }
    }

    #[test]
    fn se_lee_lo_que_el_plugin_pidio() {
        let p = leer(br#"{"url":"https://a.b/c","method":"post","headers":{"X":"1"},"body":"hola"}"#)
            .unwrap();
        assert_eq!(p.url, "https://a.b/c");
        assert_eq!(p.metodo, "POST", "el método se normaliza");
        assert_eq!(p.encabezados.get("X").map(String::as_str), Some("1"));
        assert_eq!(p.cuerpo, "hola");
    }

    #[test]
    fn sin_metodo_es_un_get() {
        let p = leer(br#"{"url":"https://a.b"}"#).unwrap();
        assert_eq!(p.metodo, "GET");
        assert!(p.encabezados.is_empty() && p.cuerpo.is_empty());
    }
}
