//! Cómo se pasan los mensajes entre Chilpa y un plugin.
//!
//! Wasm solo sabe de enteros, así que todo lo que no sea un entero viaja por
//! la memoria del guest. El trato es corto a propósito (ADR-007):
//!
//! 1. Chilpa le pide al plugin un pedazo de memoria con `chilpa_alloc(len)`.
//! 2. Escribe ahí el mensaje, en JSON.
//! 3. Llama a la función de la operación con `(ptr, len)`.
//! 4. El plugin contesta un `i64` con la dirección y el largo de su
//!    respuesta empacados, y Chilpa la lee.
//!
//! **Los errores son datos, no un canal aparte.** La respuesta siempre es
//! JSON; si algo salió mal, trae `error`. Así hay una sola forma de contestar
//! y no dos que se puedan contradecir.

/// El nombre de la memoria que un plugin tiene que exportar.
pub const MEMORIA: &str = "memory";

/// Las funciones que todo plugin exporta, pase lo que pase.
pub const F_VERSION: &str = "chilpa_api_version";
pub const F_ALLOC: &str = "chilpa_alloc";

/// El módulo bajo el que se importan las funciones del host.
pub const MODULO_HOST: &str = "chilpa";

/// Empaca una dirección y un largo en el `i64` que contesta el guest.
pub fn empacar(ptr: u32, len: u32) -> i64 {
    ((u64::from(ptr) << 32) | u64::from(len)) as i64
}

/// Lo contrario. Un cero quiere decir "no contesté nada".
pub fn desempacar(v: i64) -> Option<(u32, u32)> {
    if v == 0 {
        return None;
    }
    let v = v as u64;
    Some(((v >> 32) as u32, (v & 0xffff_ffff) as u32))
}

/// Lo que un plugin contestó, ya leído.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Respuesta {
    /// Contestó, y esto es su JSON.
    Ok(Vec<u8>),
    /// Contestó un error suyo, con su texto.
    Error(String),
}

/// Lee la respuesta de un plugin, que siempre es JSON.
///
/// Un JSON con `error` es un error del plugin, no una falla del plugin: son
/// cosas distintas y se cuentan distinto.
pub fn leer_respuesta(bytes: Vec<u8>) -> Respuesta {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Respuesta::Error("contestó algo que no es JSON".into());
    };
    if let Some(e) = v.get("error") {
        let texto = e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
        return Respuesta::Error(texto);
    }
    Respuesta::Ok(bytes)
}

/// Lo que una función del host contesta cuando devuelve datos.
///
/// El guest da un búfer y su tamaño; si no cabe, no se escribe nada y se
/// contesta cuánto hacía falta. Así nunca hay una escritura a medias.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cupo {
    /// Cupo, y se escribieron tantos bytes.
    Cabe(u32),
    /// No cupo; hacían falta tantos.
    NoCabe(u32),
    /// No había nada que dar.
    Nada,
}

impl Cupo {
    /// Lo que se le contesta al guest: positivo cabe, negativo no cabe,
    /// menos uno no hay nada.
    pub fn codigo(self) -> i32 {
        match self {
            Cupo::Cabe(n) => n as i32,
            Cupo::NoCabe(n) => -(n as i32),
            Cupo::Nada => -1,
        }
    }

    /// Decide si unos datos caben en el búfer que dio el guest.
    pub fn para(datos: Option<&[u8]>, cupo: u32) -> Self {
        match datos {
            None => Cupo::Nada,
            Some(d) if d.len() as u32 <= cupo => Cupo::Cabe(d.len() as u32),
            Some(d) => Cupo::NoCabe(d.len() as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empacar_y_desempacar_son_lo_contrario() {
        for (ptr, len) in [(0u32, 5u32), (1024, 0), (0xdead_beef, 0x1234)] {
            let v = empacar(ptr, len);
            assert_eq!(desempacar(v), Some((ptr, len)), "con ({ptr}, {len})");
        }
    }

    #[test]
    fn un_cero_quiere_decir_que_no_contesto() {
        assert_eq!(desempacar(0), None);
        // Y una dirección cero con largo cero es lo mismo que nada.
        assert_eq!(empacar(0, 0), 0);
    }

    #[test]
    fn una_respuesta_con_error_se_lee_como_error() {
        let r = leer_respuesta(r#"{"error":"no encontré esa pista"}"#.as_bytes().to_vec());
        assert_eq!(r, Respuesta::Error("no encontré esa pista".into()));
    }

    #[test]
    fn una_respuesta_buena_se_entrega_tal_cual() {
        let json = br#"{"tracks":[]}"#.to_vec();
        assert_eq!(leer_respuesta(json.clone()), Respuesta::Ok(json));
    }

    #[test]
    fn lo_que_no_es_json_es_un_error_del_plugin() {
        let r = leer_respuesta(b"basura".to_vec());
        assert!(matches!(r, Respuesta::Error(e) if e.contains("no es JSON")));
    }

    #[test]
    fn el_cupo_dice_cuanto_hacia_falta() {
        assert_eq!(Cupo::para(Some(b"hola"), 10), Cupo::Cabe(4));
        assert_eq!(Cupo::para(Some(b"hola"), 4), Cupo::Cabe(4), "justo cabe");
        assert_eq!(Cupo::para(Some(b"hola"), 2), Cupo::NoCabe(4));
        assert_eq!(Cupo::para(None, 10), Cupo::Nada);
    }

    #[test]
    fn los_codigos_del_cupo_no_se_confunden_entre_si() {
        assert_eq!(Cupo::Cabe(4).codigo(), 4);
        assert_eq!(Cupo::NoCabe(4).codigo(), -4);
        assert_eq!(Cupo::Nada.codigo(), -1);
        // Cupo cero que sí cupo es cero, y no se confunde con "nada".
        assert_eq!(Cupo::Cabe(0).codigo(), 0);
        assert_ne!(Cupo::Cabe(0).codigo(), Cupo::Nada.codigo());
    }
}
