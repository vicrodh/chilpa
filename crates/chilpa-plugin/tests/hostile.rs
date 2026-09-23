//! La suite de plugins hostiles que pide el ADR-003.
//!
//! Cada uno de estos plugins intenta una cosa distinta: tronar, colgarse,
//! comerse la memoria, mentir sobre su versión, salir a un dominio que no
//! declaró, leer un almacén que no pidió, o contestar una dirección que no
//! es suya. En todos los casos se afirman dos cosas:
//!
//! 1. **Chilpa sigue.** La llamada se detiene, no el proceso.
//! 2. **No llegó a donde no debía.** El host de prueba se acuerda de todo lo
//!    que le pidieron, así que se puede afirmar que no le pidieron nada.
//!
//! Los plugins van escritos en texto de wasm para que la suite corra sin
//! ninguna cadena de compilación aparte.

use chilpa_plugin::{
    Halt, State, Host, TestHost, Limits, PluginError, Registry, Reply, Services,
};
use std::sync::{Arc, Mutex};

/// El manifest de siempre, con las capacidades que se le pidan.
fn manifest(capabilities: &str) -> String {
    format!(
        r#"{{
            "id": "com.prueba.hostil",
            "name": "Hostil",
            "version": "0.1.0",
            "api_version": 1,
            "kind": "source",
            "capabilities": [{capabilities}]
        }}"#
    )
}

/// Lo que todo plugin tiene que traer para que lo carguen.
const PRELUDE: &str = r#"
  (memory (export "memory") 1)
  (global $siguiente (mut i32) (i32.const 1024))
  (func (export "chilpa_alloc") (param i32) (result i32)
    (local i32)
    (local.set 1 (global.get $siguiente))
    (global.set $siguiente (i32.add (global.get $siguiente) (local.get 0)))
    (local.get 1))
  (data (i32.const 0) "{\"ok\":true}")
"#;

/// La respuesta fija que deja el preludio: `{"ok":true}`, en (0, 11).
const FIXED_REPLY: &str = "(i64.const 11)";

fn compile(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("el plugin de prueba compila")
}

/// Un host de prueba y la copia con la que la prueba lo mira.
fn services() -> (Services, TestHost) {
    let notes = TestHost::new();
    let s: Services = Arc::new(Mutex::new(notes.clone()));
    (s, notes)
}

/// Instala un plugin y lo llama una vez.
fn run(
    capabilities: &str,
    wat: &str,
    operation: &str,
) -> (Registry, TestHost, Result<Reply, PluginError>) {
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    let (svc, notes) = services();
    assert!(reg.install(&host, &manifest(capabilities), &compile(wat), |_| true), "debía cargar");
    let r = reg.call(&host, &svc, "com.prueba.hostil", operation, b"{}", Limits::short());
    (reg, notes, r)
}

#[test]
fn a_well_behaved_one_loads_and_answers() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let (reg, _notes, r) = run("", &wat, "buscar");
    assert_eq!(r.unwrap(), Reply::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.state("com.prueba.hostil"), State::Healthy);
    assert_eq!(reg.usable(), vec!["com.prueba.hostil"]);
}

#[test]
fn one_that_crashes_is_stopped_and_chilpa_carries_on() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let (mut reg, _notes, r) = run("", &wat, "buscar");
    assert!(matches!(r, Err(PluginError::Crashed(_))), "{r:?}");
    // Y se puede seguir pidiendo: una falla no es cuarentena.
    assert!(reg.state("com.prueba.hostil").usable());
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let _ = reg.call(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::short());
    let _ = reg.call(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::short());
    assert!(
        matches!(reg.state("com.prueba.hostil"), State::Quarantined { .. }),
        "a la tercera va a cuarentena"
    );
    // Y en cuarentena ya no se le llama.
    let r = reg.call(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::short());
    assert!(matches!(r, Err(PluginError::NotAllowed(_))), "{r:?}");
}

#[test]
fn one_that_hangs_is_cut_off_by_fuel() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (loop $otra (br $otra))
              (i64.const 0)))"#
    );
    let start = std::time::Instant::now();
    let (_reg, _notes, r) = run("", &wat, "buscar");
    assert_eq!(r, Err(PluginError::Stopped(Halt::OutOfFuel)), "{r:?}");
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "se cortó pronto");
}

#[test]
fn one_asking_for_more_memory_than_allowed_is_refused() {
    // Ocho megabytes son 128 páginas; pide 200 de un jalón. Contesta la
    // respuesta buena solo si el motor se lo negó.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (if (i32.eq (memory.grow (i32.const 200)) (i32.const -1))
                (then (return {FIXED_REPLY})))
              (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "buscar");
    assert_eq!(
        r.unwrap(),
        Reply::Ok(br#"{"ok":true}"#.to_vec()),
        "el motor le negó la memoria; si hubiera crecido, contestaría nada"
    );
}

#[test]
fn one_eating_memory_in_a_loop_is_stopped() {
    // Crece de a una página para siempre. Con el límite puesto nunca crece,
    // y lo que lo detiene, medido, es el reloj y no el combustible: pedir
    // memoria es caro en tiempo y barato en instrucciones. Por eso hacen
    // falta los dos límites, y por eso aquí se afirma que se detuvo y no
    // con cuál de los dos.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (loop $otra
                (drop (memory.grow (i32.const 1)))
                (br $otra))
              (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "buscar");
    match r {
        Err(PluginError::Stopped(d)) => {
            assert!(!d.to_string().is_empty(), "y se dice por qué se detuvo");
        }
        other => panic!("debía detenerse por alguno de sus límites: {other:?}"),
    }
}

#[test]
fn one_lying_about_its_version_does_not_load() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 7))
            (func (export "buscar") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.install(&host, &manifest(""), &compile(&wat), |_| true), "no debía cargar");
    let inst = reg.installed().next().unwrap();
    assert!(!inst.loaded());
    assert!(inst.problem.as_ref().unwrap().contains("no se carga"), "{:?}", inst.problem);
    assert!(reg.usable().is_empty());
}

#[test]
fn one_missing_exports_does_not_load() {
    // Sin memoria ni alloc: no hay forma de hablarle.
    let wat = r#"(module (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#;
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.install(&host, &manifest(""), &compile(wat), |_| true));
    let problem = reg.installed().next().unwrap().problem.clone().unwrap();
    assert!(problem.contains("memory"), "{problem}");
}

/// Un plugin que le pide al host esa URL. El largo se calcula, no se
/// escribe a mano: uno mal puesto trunca el JSON y la prueba pasaría porque
/// la petición no se entiende, no porque el dominio esté prohibido.
fn plugin_that_requests(url: &str) -> String {
    let request = format!(r#"{{"url":"{url}"}}"#);
    let escaped = request.replace('"', "\\\"");
    let largo = request.len();
    format!(
        r#"(module
            (import "chilpa" "http" (func $http (param i32 i32 i32 i32) (result i32)))
            {PRELUDE}
            (data (i32.const 256) "{escaped}")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $http (i32.const 256) (i32.const {largo}) (i32.const 2048) (i32.const 512)))
              {FIXED_REPLY}))"#
    )
}

#[test]
fn one_reaching_a_domain_it_did_not_declare_never_reaches_the_host() {
    // Declara ejemplo.com y pide otra-cosa.net.
    let wat = plugin_that_requests("https://otra-cosa.net/robar");
    let (_reg, notes, r) =
        run(r#"{"type": "network", "domains": ["ejemplo.com"]}"#, &wat, "buscar");
    assert!(r.is_ok(), "la llamada termina; lo que se niega es la salida: {r:?}");
    let requests = notes.examine(|a| a.requests.len());
    assert_eq!(requests, 0, "el host nunca vio esa petición");
}

#[test]
fn one_reaching_the_domain_it_did_declare_gets_through() {
    let wat = plugin_that_requests("https://api.ejemplo.com/buscar");
    let (_reg, notes, r) =
        run(r#"{"type": "network", "domains": ["ejemplo.com"]}"#, &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let (how_many, who) = notes.examine(|a| {
        (a.requests.len(), a.requests.first().map(|p| p.0.clone()).unwrap_or_default())
    });
    assert_eq!(how_many, 1, "el subdominio del dominio declarado sí pasa");
    assert_eq!(who, "com.prueba.hostil", "la petición va a nombre del plugin, no de otro");
}

#[test]
fn one_without_network_permission_never_reaches_the_host_however_hard_it_tries() {
    let wat = plugin_that_requests("https://api.ejemplo.com/x");
    let (_reg, notes, r) = run("", &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(notes.examine(|a| a.requests.len()), 0, "sin capacidad de red no hay red");
}

#[test]
fn one_without_storage_neither_reads_nor_writes() {
    let wat = format!(
        r#"(module
            (import "chilpa" "store_get" (func $get (param i32 i32 i32 i32) (result i32)))
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            {PRELUDE}
            (data (i32.const 256) "token")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $get (i32.const 256) (i32.const 5) (i32.const 2048) (i32.const 512)))
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {FIXED_REPLY}))"#
    );
    let (_reg, notes, r) = run("", &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let (reads, writes) = notes.examine(|a| (a.reads.len(), a.writes.len()));
    assert_eq!(reads, 0, "sin capacidad de almacén no se lee nada");
    assert_eq!(writes, 0, "ni se escribe");
}

#[test]
fn a_plugin_store_always_goes_under_its_own_name() {
    let wat = format!(
        r#"(module
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            {PRELUDE}
            (data (i32.const 256) "llave")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {FIXED_REPLY}))"#
    );
    let (_reg, notes, r) = run(r#"{"type": "storage"}"#, &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let who = notes.examine(|a| a.writes.first().map(|e| e.0.clone()).unwrap_or_default());
    assert_eq!(
        who, "com.prueba.hostil",
        "el id lo pone el host desde el manifest, no el guest"
    );
}

#[test]
fn one_answering_with_a_foreign_address_does_not_make_chilpa_read_beyond() {
    // Contesta una dirección enorme, muy por fuera de su memoria.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 4000000000) (i64.const 32)) (i64.const 1000))))"#
    );
    let (_reg, _notes, r) = run("", &wat, "buscar");
    assert_eq!(r, Err(PluginError::OutOfMemory), "{r:?}");
}

#[test]
fn one_that_answers_nothing_is_not_confused_with_one_that_answered_empty() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "buscar");
    assert!(matches!(&r, Ok(Reply::Error(e)) if e.contains("no contestó")), "{r:?}");
}

#[test]
fn an_operation_the_plugin_lacks_is_reported_and_that_is_all() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let (_reg, _notes, r) = run("", &wat, "una_que_no_existe");
    assert!(matches!(&r, Err(PluginError::MissingExport(n)) if n == "una_que_no_existe"), "{r:?}");
}

#[test]
fn a_broken_plugin_does_not_take_the_others_with_it() {
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let mut reg = Registry::new();

    let good = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let broken = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let m = |id: &str| manifest("").replace("com.prueba.hostil", id);

    assert!(reg.install(&host, &m("com.bueno.plugin"), &compile(&good), |_| true));
    assert!(reg.install(&host, &m("com.roto.plugin"), &compile(&broken), |_| true));

    for _ in 0..3 {
        let _ = reg.call(&host, &svc, "com.roto.plugin", "buscar", b"{}", Limits::short());
    }
    assert!(matches!(reg.state("com.roto.plugin"), State::Quarantined { .. }));

    // Y el bueno sigue trabajando.
    let r = reg.call(&host, &svc, "com.bueno.plugin", "buscar", b"{}", Limits::short());
    assert_eq!(r.unwrap(), Reply::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.usable(), vec!["com.bueno.plugin"]);
    assert_eq!(reg.quarantined().len(), 1);
}

#[test]
fn only_the_user_gets_it_out_of_quarantine() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let mut reg = Registry::new();
    reg.install(&host, &manifest(""), &compile(&wat), |_| true);
    for _ in 0..3 {
        let _ = reg.call(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::short());
    }
    assert!(reg.usable().is_empty());
    reg.reactivate("com.prueba.hostil");
    assert_eq!(reg.usable(), vec!["com.prueba.hostil"], "el usuario lo revivió");
}
