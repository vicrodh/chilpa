//! Los límites con los que corre un plugin.
//!
//! No son avisos ni buenas intenciones: son del store de wasmtime, y el
//! motor los aplica aunque el plugin no quiera. Un plugin que se pasa no se
//! "porta mal": se detiene.
//!
//! Los tres son distintos y hacen falta los tres:
//!
//! - **Combustible** para el que se cuelga calculando.
//! - **Memoria** para el que crece sin parar.
//! - **Reloj** para el que se queda esperando algo que nunca llega, que no
//!   gasta combustible porque no está ejecutando nada.

use std::time::Duration;

/// Cuánto puede gastar un plugin por llamada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Limits {
    /// Instrucciones, más o menos. Lo que se le da por llamada.
    pub fuel: u64,
    /// Cuánta memoria lineal puede tener, en bytes.
    pub memory_bytes: usize,
    /// Cuánto puede tardar una llamada de reloj de pared.
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        // Mil millones de unidades de combustible son del orden de un
        // segundo de cómputo; un plugin que parsea una respuesta usa una
        // fracción. Treinta y dos megabytes alcanzan para tener en memoria
        // una respuesta grande y su resultado. Cinco segundos de reloj es
        // más de lo que ninguna llamada razonable necesita, contando que el
        // host es quien espera a la red, no el plugin.
        Self {
            fuel: 1_000_000_000,
            memory_bytes: 32 * 1024 * 1024,
            timeout: Duration::from_secs(5),
        }
    }
}

impl Limits {
    /// Los de una llamada que se espera corta, como resolver una letra.
    pub fn short() -> Self {
        Self {
            fuel: 50_000_000,
            memory_bytes: 8 * 1024 * 1024,
            timeout: Duration::from_secs(2),
        }
    }

    /// Los de algo que sí puede tardar, como traer un catálogo entero.
    pub fn long() -> Self {
        Self {
            fuel: 10_000_000_000,
            memory_bytes: 128 * 1024 * 1024,
            timeout: Duration::from_secs(60),
        }
    }

    /// Las páginas de wasm que corresponden a la memoria permitida.
    ///
    /// Una página son 64 KiB; es la unidad con la que crece la memoria de un
    /// guest, así que el límite se redondea hacia abajo a páginas enteras.
    pub fn max_pages(&self) -> usize {
        self.memory_bytes / (64 * 1024)
    }
}

/// Por qué se detuvo una llamada a un plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Halt {
    /// Se acabó el combustible: estaba calculando de más.
    OutOfFuel,
    /// Pidió más memoria de la que tiene permitida.
    OutOfMemory,
    /// Se pasó del reloj.
    Timeout(Duration),
}

impl std::fmt::Display for Halt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Halt::OutOfFuel => {
                write!(f, "se pasó del cómputo que tenía permitido y se detuvo")
            }
            Halt::OutOfMemory => write!(f, "pidió más memoria de la permitida"),
            Halt::Timeout(d) => write!(f, "no contestó en {d:?} y se detuvo"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_limites_por_omision_son_razonables() {
        let l = Limits::default();
        assert!(l.fuel > 0 && l.memory_bytes > 0 && !l.timeout.is_zero());
        assert_eq!(l.max_pages(), 512, "32 MiB son 512 páginas de 64 KiB");
    }

    #[test]
    fn los_cortos_son_mas_apretados_que_los_largos() {
        let (c, d, l) = (Limits::short(), Limits::default(), Limits::long());
        assert!(c.fuel < d.fuel && d.fuel < l.fuel);
        assert!(c.memory_bytes < d.memory_bytes && d.memory_bytes < l.memory_bytes);
        assert!(c.timeout < d.timeout && d.timeout < l.timeout);
    }

    #[test]
    fn la_memoria_se_redondea_a_paginas_enteras() {
        let l = Limits { memory_bytes: 100_000, ..Limits::default() };
        assert_eq!(l.max_pages(), 1, "100 KB no alcanzan para dos páginas");
        let l = Limits { memory_bytes: 1000, ..Limits::default() };
        assert_eq!(l.max_pages(), 0, "menos de una página es ninguna");
    }

    #[test]
    fn cada_detencion_se_explica_en_castellano() {
        for d in [
            Halt::OutOfFuel,
            Halt::OutOfMemory,
            Halt::Timeout(Duration::from_secs(5)),
        ] {
            let text = d.to_string();
            assert!(text.len() > 10, "{text}");
            assert!(!text.contains("Error"), "nada de jerga: {text}");
        }
    }
}
