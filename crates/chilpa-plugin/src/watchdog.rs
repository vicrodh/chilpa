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
pub const FAILURES_TO_QUARANTINE: usize = 3;

/// En cuánto tiempo tienen que caber esas fallas para contar como seguidas.
pub const WINDOW: Duration = Duration::from_secs(60);

/// Cómo está un plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum State {
    /// Funcionando.
    Healthy,
    /// Ha fallado, pero todavía no lo suficiente.
    Stumbling { failures: usize },
    /// Apagado hasta que el usuario lo reactive.
    Quarantined { reason: String },
}

impl State {
    pub fn usable(&self) -> bool {
        !matches!(self, State::Quarantined { .. })
    }
}

#[derive(Default)]
struct Tally {
    failures: Vec<Instant>,
    quarantine: Option<String>,
}

/// Lleva la cuenta de cómo se porta cada plugin.
#[derive(Default)]
pub struct Watchdog {
    tallies: HashMap<String, Tally>,
}

impl Watchdog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cómo está un plugin. Uno que nunca ha hecho nada está bien.
    pub fn state(&self, id: &str) -> State {
        let Some(c) = self.tallies.get(id) else { return State::Healthy };
        if let Some(reason) = &c.quarantine {
            return State::Quarantined { reason: reason.clone() };
        }
        let recent = c.failures.iter().filter(|t| t.elapsed() < WINDOW).count();
        if recent == 0 {
            State::Healthy
        } else {
            State::Stumbling { failures: recent }
        }
    }

    /// Cierto si se le puede pedir algo.
    pub fn usable(&self, id: &str) -> bool {
        self.state(id).usable()
    }

    /// Anota que una llamada salió bien. Perdona lo anterior.
    pub fn success(&mut self, id: &str) {
        if let Some(c) = self.tallies.get_mut(id) {
            if c.quarantine.is_none() {
                c.failures.clear();
            }
        }
    }

    /// Anota una falla. Devuelve el estado en que quedó.
    pub fn failure(&mut self, id: &str, reason: &str) -> State {
        self.failure_at(id, reason, Instant::now())
    }

    /// Lo mismo, con el momento a la mano. Es lo que hace que esto se pueda
    /// probar sin dormir un minuto.
    pub fn failure_at(&mut self, id: &str, reason: &str, cuando: Instant) -> State {
        let c = self.tallies.entry(id.to_string()).or_default();
        if c.quarantine.is_some() {
            return self.state(id);
        }
        // Lo viejo se olvida: lo que se castiga es fallar seguido.
        c.failures.retain(|t| cuando.duration_since(*t) < WINDOW);
        c.failures.push(cuando);
        if c.failures.len() >= FAILURES_TO_QUARANTINE {
            c.quarantine = Some(format!(
                "falló {FAILURES_TO_QUARANTINE} veces seguidas; la última: {reason}"
            ));
        }
        self.state(id)
    }

    /// Manda a cuarentena de inmediato, sin contar. Es para lo que no tiene
    /// vuelta: un manifest que no se entiende, un módulo que no carga.
    pub fn quarantine(&mut self, id: &str, reason: &str) {
        let c = self.tallies.entry(id.to_string()).or_default();
        c.quarantine = Some(reason.to_string());
    }

    /// Lo saca de cuarentena. Solo lo llama el usuario.
    pub fn reactivate(&mut self, id: &str) {
        if let Some(c) = self.tallies.get_mut(id) {
            c.quarantine = None;
            c.failures.clear();
        }
    }

    /// Los que están en cuarentena, con su razón.
    pub fn quarantined(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .tallies
            .iter()
            .filter_map(|(id, c)| c.quarantine.clone().map(|r| (id.clone(), r)))
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_that_has_never_done_anything_is_healthy() {
        let w = Watchdog::new();
        assert_eq!(w.state("com.a.b"), State::Healthy);
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn two_failures_are_not_quarantine_but_they_show() {
        let mut w = Watchdog::new();
        assert_eq!(w.failure("com.a.b", "truena"), State::Stumbling { failures: 1 });
        assert_eq!(w.failure("com.a.b", "truena"), State::Stumbling { failures: 2 });
        assert!(w.usable("com.a.b"), "todavía se le puede pedir algo");
    }

    #[test]
    fn on_the_third_it_goes_to_quarantine_and_says_why() {
        let mut w = Watchdog::new();
        w.failure("com.a.b", "una");
        w.failure("com.a.b", "otra");
        let state = w.failure("com.a.b", "se quedó sin combustible");
        match &state {
            State::Quarantined { reason } => {
                assert!(reason.contains("3 veces"), "{reason}");
                assert!(reason.contains("sin combustible"), "la última razón: {reason}");
            }
            other => panic!("debía quedar en cuarentena: {other:?}"),
        }
        assert!(!w.usable("com.a.b"));
    }

    #[test]
    fn one_good_call_forgives_what_came_before() {
        let mut w = Watchdog::new();
        w.failure("com.a.b", "una");
        w.failure("com.a.b", "otra");
        w.success("com.a.b");
        assert_eq!(w.state("com.a.b"), State::Healthy);
        // Y ahora vuelve a tener tres oportunidades.
        w.failure("com.a.b", "una");
        w.failure("com.a.b", "otra");
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn old_failures_do_not_count() {
        let mut w = Watchdog::new();
        let now = Instant::now();
        let a_while_ago = now - WINDOW - Duration::from_secs(1);
        w.failure_at("com.a.b", "vieja", a_while_ago);
        w.failure_at("com.a.b", "vieja", a_while_ago);
        // Esta es la primera de la ventana actual, no la tercera.
        assert_eq!(w.failure_at("com.a.b", "nueva", now), State::Stumbling { failures: 1 });
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn only_the_user_takes_it_out_of_quarantine() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.failure("com.a.b", "truena");
        }
        assert!(!w.usable("com.a.b"));
        // Fallar más no cambia nada, y un éxito tampoco lo saca.
        w.failure("com.a.b", "otra vez");
        w.success("com.a.b");
        assert!(!w.usable("com.a.b"), "un éxito no lo saca de cuarentena");

        w.reactivate("com.a.b");
        assert_eq!(w.state("com.a.b"), State::Healthy);
    }

    #[test]
    fn some_things_go_to_quarantine_at_once() {
        let mut w = Watchdog::new();
        w.quarantine("com.a.b", "el manifest pide una versión de API que no existe");
        assert!(!w.usable("com.a.b"));
        assert_eq!(
            w.quarantined(),
            vec![(
                "com.a.b".to_string(),
                "el manifest pide una versión de API que no existe".to_string()
            )]
        );
    }

    #[test]
    fn a_broken_plugin_does_not_drag_the_others_down() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.failure("com.roto.plugin", "truena");
        }
        assert!(!w.usable("com.roto.plugin"));
        assert!(w.usable("com.sano.plugin"), "el otro sigue entero");
        assert_eq!(w.quarantined().len(), 1);
    }
}
