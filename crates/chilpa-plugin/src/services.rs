//! Lo que el host le presta a un plugin.
//!
//! Un guest de wasm no tiene red, ni disco, ni reloj: solo existe lo que
//! Chilpa le pasa. Estas son las cuatro puertas de la v1, y no hay una
//! quinta (ADR-007).
//!
//! La regla que manda sobre todas: **el plugin nunca ve una credencial.** No
//! porque se le pida que no la guarde, sino porque la petición la hace el
//! host y el token se agrega del lado de acá. En la memoria del guest nunca
//! estuvo.
//!
//! Este crate no sabe hablar HTTP ni abrir una base de datos, a propósito:
//! lo implementa quien lo usa. Así el host de plugins no arrastra una pila
//! de red, y las pruebas pueden mirar exactamente qué pidió un plugin.

/// Qué tan importante es una línea de log.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn from_code(n: i32) -> Self {
        match n {
            0 => Level::Debug,
            2 => Level::Warn,
            3 => Level::Error,
            _ => Level::Info,
        }
    }
}

/// Lo que el host hace por un plugin.
///
/// Todo lleva el id del plugin porque lo que se guarda, se pide y se anota
/// es suyo y de nadie más.
pub trait HostServices: Send {
    /// Una línea al log, con el plugin por delante.
    fn log(&mut self, plugin: &str, level: Level, message: &str);

    /// La hora, en segundos unix.
    fn now_unix(&mut self) -> i64;

    /// Lee del almacén del plugin. `None` si no hay nada con esa llave.
    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>>;

    /// Escribe en el almacén del plugin. Falso si no se pudo.
    fn store_set(&mut self, plugin: &str, key: &str, valor: &[u8]) -> bool;

    /// Hace una petición por el plugin.
    ///
    /// Recibe y devuelve JSON. **Quien implementa esto pone la credencial**;
    /// el plugin solo dijo a dónde y con qué cuerpo. Y no puede quedarse
    /// esperando para siempre: el guest se puede cortar por combustible o
    /// por reloj, pero una llamada del host que se cuelga cuelga el hilo.
    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8>;
}

/// Un host que no deja hacer nada. Sirve para plugins que no pidieron
/// ninguna capacidad, y para las pruebas.
#[derive(Default)]
pub struct NoServices;

impl HostServices for NoServices {
    fn log(&mut self, _plugin: &str, _level: Level, _message: &str) {}
    fn now_unix(&mut self) -> i64 {
        0
    }
    fn store_get(&mut self, _plugin: &str, _key: &str) -> Option<Vec<u8>> {
        None
    }
    fn store_set(&mut self, _plugin: &str, _key: &str, _valor: &[u8]) -> bool {
        false
    }
    fn http(&mut self, _plugin: &str, _request: &[u8]) -> Vec<u8> {
        br#"{"error":"este plugin no tiene permiso de red"}"#.to_vec()
    }
}

/// Un host de mentiras que anota todo lo que le pidieron.
///
/// Es lo que hace que la suite de plugins hostiles pueda afirmar no solo que
/// Chilpa sobrevivió, sino que el plugin no llegó a donde no debía.
///
/// Se puede clonar: una copia se le da al host y otra se queda la prueba
/// para mirar lo que pasó. Por eso lo anotado vive detrás de su propio
/// candado y no del que envuelve al host.
#[derive(Default, Clone)]
pub struct TestHost(std::sync::Arc<std::sync::Mutex<Recorded>>);

/// Todo lo que un plugin le pidió al host de prueba.
#[derive(Default)]
pub struct Recorded {
    pub logs: Vec<(String, Level, String)>,
    pub reads: Vec<(String, String)>,
    pub writes: Vec<(String, String, Vec<u8>)>,
    pub requests: Vec<(String, String)>,
    /// Lo que `store_get` contesta, por llave.
    pub store: std::collections::HashMap<String, Vec<u8>>,
    /// Lo que `http` contesta. Vacío da una respuesta de relleno.
    pub http_reply: Vec<u8>,
    pub clock: i64,
}

impl TestHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mira lo anotado sin poder cambiarlo.
    pub fn examine<T>(&self, f: impl FnOnce(&Recorded) -> T) -> T {
        let save = self.0.lock().expect("el candado del host de prueba");
        f(&save)
    }

    /// Prepara algo antes de correr.
    pub fn prepare(&self, f: impl FnOnce(&mut Recorded)) {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        f(&mut save);
    }
}

impl HostServices for TestHost {
    fn log(&mut self, plugin: &str, level: Level, message: &str) {
        self.prepare(|a| a.logs.push((plugin.to_string(), level, message.to_string())));
    }
    fn now_unix(&mut self) -> i64 {
        self.examine(|a| a.clock)
    }
    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>> {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save.reads.push((plugin.to_string(), key.to_string()));
        save.store.get(key).cloned()
    }
    fn store_set(&mut self, plugin: &str, key: &str, valor: &[u8]) -> bool {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save.writes.push((plugin.to_string(), key.to_string(), valor.to_vec()));
        save.store.insert(key.to_string(), valor.to_vec());
        true
    }
    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8> {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save
            .requests
            .push((plugin.to_string(), String::from_utf8_lossy(request).to_string()));
        if save.http_reply.is_empty() {
            br#"{"status":200,"body":""}"#.to_vec()
        } else {
            save.http_reply.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_servicios_la_red_contesta_que_no() {
        let mut s = NoServices;
        let r = s.http("com.a.b", b"{}");
        assert!(String::from_utf8_lossy(&r).contains("no tiene permiso"));
        assert_eq!(s.store_get("com.a.b", "x"), None);
        assert!(!s.store_set("com.a.b", "x", b"y"), "no deja escribir");
    }

    #[test]
    fn el_host_de_prueba_se_acuerda_de_todo() {
        let mut h = TestHost::new();
        h.prepare(|a| a.clock = 1234);
        h.log("com.a.b", Level::Warn, "algo");
        h.store_set("com.a.b", "token", b"secreto");
        assert_eq!(h.store_get("com.a.b", "token").as_deref(), Some(&b"secreto"[..]));
        h.http("com.a.b", br#"{"url":"https://api.ejemplo.com"}"#);

        h.examine(|a| {
            assert_eq!(a.logs.len(), 1);
            assert_eq!(a.logs[0].1, Level::Warn);
            assert_eq!(a.writes.len(), 1);
            assert_eq!(a.reads.len(), 1);
            assert_eq!(a.requests.len(), 1);
            assert!(a.requests[0].1.contains("api.ejemplo.com"));
        });
        assert_eq!(h.now_unix(), 1234);
    }

    #[test]
    fn una_copia_del_host_de_prueba_ve_lo_mismo() {
        let mut h = TestHost::new();
        let copy = h.clone();
        h.log("com.a.b", Level::Info, "algo");
        assert_eq!(copy.examine(|a| a.logs.len()), 1, "la copia mira el mismo cuaderno");
    }

    #[test]
    fn los_niveles_desconocidos_caen_en_info() {
        assert_eq!(Level::from_code(0), Level::Debug);
        assert_eq!(Level::from_code(1), Level::Info);
        assert_eq!(Level::from_code(2), Level::Warn);
        assert_eq!(Level::from_code(3), Level::Error);
        assert_eq!(Level::from_code(99), Level::Info, "lo raro no se pierde, se degrada");
    }
}
