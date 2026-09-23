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
pub fn pack(ptr: u32, len: u32) -> i64 {
    ((u64::from(ptr) << 32) | u64::from(len)) as i64
}

/// Lo contrario. Un cero quiere decir "no contesté nada".
pub fn unpack(v: i64) -> Option<(u32, u32)> {
    if v == 0 {
        return None;
    }
    let v = v as u64;
    Some(((v >> 32) as u32, (v & 0xffff_ffff) as u32))
}

/// Lo que un plugin contestó, ya leído.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Reply {
    /// Contestó, y esto es su JSON.
    Ok(Vec<u8>),
    /// Contestó un error suyo, con su texto.
    Error(String),
}

/// Lee la respuesta de un plugin, que siempre es JSON.
///
/// Un JSON con `error` es un error del plugin, no una falla del plugin: son
/// cosas distintas y se cuentan distinto.
pub fn read_reply(bytes: Vec<u8>) -> Reply {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Reply::Error("contestó algo que no es JSON".into());
    };
    if let Some(e) = v.get("error") {
        let text = e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
        return Reply::Error(text);
    }
    Reply::Ok(bytes)
}

/// Lo que una función del host contesta cuando devuelve datos.
///
/// El guest da un búfer y su tamaño; si no cabe, no se escribe nada y se
/// contesta cuánto hacía falta. Así nunca hay una escritura a medias.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    /// Cupo, y se escribieron tantos bytes.
    Fits(u32),
    /// No cupo; hacían falta tantos.
    TooBig(u32),
    /// No había nada que dar.
    Nothing,
}

impl Room {
    /// Lo que se le contesta al guest: positivo cabe, negativo no cabe,
    /// menos uno no hay nada.
    pub fn code(self) -> i32 {
        match self {
            Room::Fits(n) => n as i32,
            Room::TooBig(n) => -(n as i32),
            Room::Nothing => -1,
        }
    }

    /// Decide si unos datos caben en el búfer que dio el guest.
    pub fn para(data: Option<&[u8]>, room: u32) -> Self {
        match data {
            None => Room::Nothing,
            Some(d) if d.len() as u32 <= room => Room::Fits(d.len() as u32),
            Some(d) => Room::TooBig(d.len() as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_and_unpacking_are_inverses() {
        for (ptr, len) in [(0u32, 5u32), (1024, 0), (0xdead_beef, 0x1234)] {
            let v = pack(ptr, len);
            assert_eq!(unpack(v), Some((ptr, len)), "con ({ptr}, {len})");
        }
    }

    #[test]
    fn a_zero_means_it_did_not_answer() {
        assert_eq!(unpack(0), None);
        // Y una dirección cero con largo cero es lo mismo que nada.
        assert_eq!(pack(0, 0), 0);
    }

    #[test]
    fn a_reply_carrying_an_error_reads_as_an_error() {
        let r = read_reply(r#"{"error":"no encontré esa pista"}"#.as_bytes().to_vec());
        assert_eq!(r, Reply::Error("no encontré esa pista".into()));
    }

    #[test]
    fn a_good_reply_is_handed_over_unchanged() {
        let json = br#"{"tracks":[]}"#.to_vec();
        assert_eq!(read_reply(json.clone()), Reply::Ok(json));
    }

    #[test]
    fn anything_that_is_not_json_is_the_plugin_fault() {
        let r = read_reply(b"basura".to_vec());
        assert!(matches!(r, Reply::Error(e) if e.contains("no es JSON")));
    }

    #[test]
    fn the_room_check_says_how_much_was_needed() {
        assert_eq!(Room::para(Some(b"hola"), 10), Room::Fits(4));
        assert_eq!(Room::para(Some(b"hola"), 4), Room::Fits(4), "justo cabe");
        assert_eq!(Room::para(Some(b"hola"), 2), Room::TooBig(4));
        assert_eq!(Room::para(None, 10), Room::Nothing);
    }

    #[test]
    fn the_room_codes_are_not_mistaken_for_one_another() {
        assert_eq!(Room::Fits(4).code(), 4);
        assert_eq!(Room::TooBig(4).code(), -4);
        assert_eq!(Room::Nothing.code(), -1);
        // Cupo cero que sí cupo es cero, y no se confunde con "nada".
        assert_eq!(Room::Fits(0).code(), 0);
        assert_ne!(Room::Fits(0).code(), Room::Nothing.code());
    }
}
