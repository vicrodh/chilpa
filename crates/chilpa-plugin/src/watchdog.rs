//! El vigilante: cuenta fallas y manda a cuarentena.
//!
//! El ADR-003 lo dice sin rodeos: *un plugin roto nunca rompe Chilpa; antes
//! de romperla, no carga*. Este módulo es la mitad que se acuerda de las
//! fallas; la otra mitad son los límites del store, que están en `limits`.
//!
//! Tres decisiones que valen la pena decir:
//!
//! 1. **Tres fallas seguidas en una ventana corta, y a cuarentena.** No al
//!    primer tropiezo: una red que se cae no es un plugin roto.
//! 2. **De la cuarentena solo sale el usuario.** Nada de reintentar solo al
//!    rato: si algo se rompió tres veces, que alguien lo mire.
//! 3. **Una llamada que sale bien limpia la cuenta.** Lo que se castiga es
//!    fallar seguido, no haber fallado alguna vez.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cuántas fallas seguidas mandan a cuarentena.
pub const FALLAS_PARA_CUARENTENA: usize = 3;

/// En cuánto tiempo tienen que caber esas fallas para contar como seguidas.
pub const VENTANA: Duration = Duration::from_secs(60);

/// Cómo está un plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Estado {
    /// Funcionando.
    Bien,
    /// Ha fallado, pero todavía no lo suficiente.
    Tropezando { fallas: usize },
    /// Apagado hasta que el usuario lo reactive.
    Cuarentena { razon: String },
}

impl Estado {
    pub fn usable(&self) -> bool {
        !matches!(self, Estado::Cuarentena { .. })
    }
}

#[derive(Default)]
struct Cuenta {
    fallas: Vec<Instant>,
    cuarentena: Option<String>,
}

/// Lleva la cuenta de cómo se porta cada plugin.
#[derive(Default)]
pub struct Watchdog {
    cuentas: HashMap<String, Cuenta>,
}

impl Watchdog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cómo está un plugin. Uno que nunca ha hecho nada está bien.
    pub fn estado(&self, id: &str) -> Estado {
        let Some(c) = self.cuentas.get(id) else { return Estado::Bien };
        if let Some(razon) = &c.cuarentena {
            return Estado::Cuarentena { razon: razon.clone() };
        }
        let recientes = c.fallas.iter().filter(|t| t.elapsed() < VENTANA).count();
        if recientes == 0 {
            Estado::Bien
        } else {
            Estado::Tropezando { fallas: recientes }
        }
    }

    /// Cierto si se le puede pedir algo.
    pub fn usable(&self, id: &str) -> bool {
        self.estado(id).usable()
    }

    /// Anota que una llamada salió bien. Perdona lo anterior.
    pub fn exito(&mut self, id: &str) {
        if let Some(c) = self.cuentas.get_mut(id) {
            if c.cuarentena.is_none() {
                c.fallas.clear();
            }
        }
    }

    /// Anota una falla. Devuelve el estado en que quedó.
    pub fn falla(&mut self, id: &str, razon: &str) -> Estado {
        self.falla_en(id, razon, Instant::now())
    }

    /// Lo mismo, con el momento a la mano. Es lo que hace que esto se pueda
    /// probar sin dormir un minuto.
    pub fn falla_en(&mut self, id: &str, razon: &str, cuando: Instant) -> Estado {
        let c = self.cuentas.entry(id.to_string()).or_default();
        if c.cuarentena.is_some() {
            return self.estado(id);
        }
        // Lo viejo se olvida: lo que se castiga es fallar seguido.
        c.fallas.retain(|t| cuando.duration_since(*t) < VENTANA);
        c.fallas.push(cuando);
        if c.fallas.len() >= FALLAS_PARA_CUARENTENA {
            c.cuarentena = Some(format!(
                "falló {FALLAS_PARA_CUARENTENA} veces seguidas; la última: {razon}"
            ));
        }
        self.estado(id)
    }

    /// Manda a cuarentena de inmediato, sin contar. Es para lo que no tiene
    /// vuelta: un manifest que no se entiende, un módulo que no carga.
    pub fn cuarentena(&mut self, id: &str, razon: &str) {
        let c = self.cuentas.entry(id.to_string()).or_default();
        c.cuarentena = Some(razon.to_string());
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivar(&mut self, id: &str) {
        if let Some(c) = self.cuentas.get_mut(id) {
            c.cuarentena = None;
            c.fallas.clear();
        }
    }

    /// Los que están en cuarentena, con su razón.
    pub fn en_cuarentena(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .cuentas
            .iter()
            .filter_map(|(id, c)| c.cuarentena.clone().map(|r| (id.clone(), r)))
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uno_que_nunca_ha_hecho_nada_esta_bien() {
        let w = Watchdog::new();
        assert_eq!(w.estado("com.a.b"), Estado::Bien);
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn dos_fallas_no_son_cuarentena_pero_se_notan() {
        let mut w = Watchdog::new();
        assert_eq!(w.falla("com.a.b", "truena"), Estado::Tropezando { fallas: 1 });
        assert_eq!(w.falla("com.a.b", "truena"), Estado::Tropezando { fallas: 2 });
        assert!(w.usable("com.a.b"), "todavía se le puede pedir algo");
    }

    #[test]
    fn a_la_tercera_va_a_cuarentena_y_se_dice_por_que() {
        let mut w = Watchdog::new();
        w.falla("com.a.b", "una");
        w.falla("com.a.b", "otra");
        let estado = w.falla("com.a.b", "se quedó sin combustible");
        match &estado {
            Estado::Cuarentena { razon } => {
                assert!(razon.contains("3 veces"), "{razon}");
                assert!(razon.contains("sin combustible"), "la última razón: {razon}");
            }
            otro => panic!("debía quedar en cuarentena: {otro:?}"),
        }
        assert!(!w.usable("com.a.b"));
    }

    #[test]
    fn una_llamada_buena_perdona_lo_anterior() {
        let mut w = Watchdog::new();
        w.falla("com.a.b", "una");
        w.falla("com.a.b", "otra");
        w.exito("com.a.b");
        assert_eq!(w.estado("com.a.b"), Estado::Bien);
        // Y ahora vuelve a tener tres oportunidades.
        w.falla("com.a.b", "una");
        w.falla("com.a.b", "otra");
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn las_fallas_viejas_no_cuentan() {
        let mut w = Watchdog::new();
        let ahora = Instant::now();
        let hace_rato = ahora - VENTANA - Duration::from_secs(1);
        w.falla_en("com.a.b", "vieja", hace_rato);
        w.falla_en("com.a.b", "vieja", hace_rato);
        // Esta es la primera de la ventana actual, no la tercera.
        assert_eq!(w.falla_en("com.a.b", "nueva", ahora), Estado::Tropezando { fallas: 1 });
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn de_la_cuarentena_solo_sale_el_usuario() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.falla("com.a.b", "truena");
        }
        assert!(!w.usable("com.a.b"));
        // Fallar más no cambia nada, y un éxito tampoco lo saca.
        w.falla("com.a.b", "otra vez");
        w.exito("com.a.b");
        assert!(!w.usable("com.a.b"), "un éxito no lo saca de cuarentena");

        w.reactivar("com.a.b");
        assert_eq!(w.estado("com.a.b"), Estado::Bien);
    }

    #[test]
    fn hay_cosas_que_van_a_cuarentena_de_inmediato() {
        let mut w = Watchdog::new();
        w.cuarentena("com.a.b", "el manifest pide una versión de API que no existe");
        assert!(!w.usable("com.a.b"));
        assert_eq!(
            w.en_cuarentena(),
            vec![(
                "com.a.b".to_string(),
                "el manifest pide una versión de API que no existe".to_string()
            )]
        );
    }

    #[test]
    fn un_plugin_roto_no_arrastra_a_los_demas() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.falla("com.roto.plugin", "truena");
        }
        assert!(!w.usable("com.roto.plugin"));
        assert!(w.usable("com.sano.plugin"), "el otro sigue entero");
        assert_eq!(w.en_cuarentena().len(), 1);
    }
}
