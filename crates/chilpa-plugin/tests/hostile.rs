//! The hostile plugin suite ADR-003 calls for.
//!
//! Each of these plugins tries something different: blowing up, hanging,
//! eating memory, lying about its version, reaching a domain it did not
//! declare, reading a store it did not ask for, or answering with an address
//! that is not its own. In every case two things are asserted:
//!
//! 1. **Chilpa carries on.** The call is stopped, not the process.
//! 2. **It did not reach where it should not.** The test host remembers
//!    everything it was asked for, so it can be asserted that it was asked
//!    for nothing.
//!
//! The plugins are written in wasm text so the suite runs without any
//! separate toolchain.

use chilpa_plugin::{
    Halt, State, Host, TestHost, Limits, PluginError, Registry, Reply, Services,
};
use std::sync::{Arc, Mutex};

/// The usual manifest, with whatever capabilities it is given.
fn manifest(capabilities: &str) -> String {
    format!(
        r#"{{
            "id": "com.test.hostile",
            "name": "Hostile",
            "version": "0.1.0",
            "api_version": 1,
            "kind": "source",
            "capabilities": [{capabilities}]
        }}"#
    )
}

/// What every plugin has to carry to be loaded.
const PRELUDE: &str = r#"
  (memory (export "memory") 1)
  (global $next (mut i32) (i32.const 1024))
  (func (export "chilpa_alloc") (param i32) (result i32)
    (local i32)
    (local.set 1 (global.get $next))
    (global.set $next (i32.add (global.get $next) (local.get 0)))
    (local.get 1))
  (data (i32.const 0) "{\"ok\":true}")
"#;

/// The fixed reply the prelude leaves behind: `{"ok":true}`, at (0, 11).
const FIXED_REPLY: &str = "(i64.const 11)";

fn compile(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("the test plugin compiles")
}

/// A test host and the copy the test watches it through.
fn services() -> (Services, TestHost) {
    let notes = TestHost::new();
    let s: Services = Arc::new(Mutex::new(notes.clone()));
    (s, notes)
}

/// Installs a plugin and calls it once.
fn run(
    capabilities: &str,
    wat: &str,
    operation: &str,
) -> (Registry, TestHost, Result<Reply, PluginError>) {
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    let (svc, notes) = services();
    assert!(reg.install(&host, &manifest(capabilities), &compile(wat), |_| true), "it should have loaded");
    let r = reg.call(&host, &svc, "com.test.hostile", operation, b"{}", Limits::short());
    (reg, notes, r)
}

#[test]
fn a_well_behaved_one_loads_and_answers() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let (reg, _notes, r) = run("", &wat, "search");
    assert_eq!(r.unwrap(), Reply::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.state("com.test.hostile"), State::Healthy);
    assert_eq!(reg.usable(), vec!["com.test.hostile"]);
}

#[test]
fn one_that_crashes_is_stopped_and_chilpa_carries_on() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) (unreachable)))"#
    );
    let (mut reg, _notes, r) = run("", &wat, "search");
    assert!(matches!(r, Err(PluginError::Crashed(_))), "{r:?}");
    // And it can still be asked: one failure is not quarantine.
    assert!(reg.state("com.test.hostile").usable());
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let _ = reg.call(&host, &svc, "com.test.hostile", "search", b"{}", Limits::short());
    let _ = reg.call(&host, &svc, "com.test.hostile", "search", b"{}", Limits::short());
    assert!(
        matches!(reg.state("com.test.hostile"), State::Quarantined { .. }),
        "the third one sends it to quarantine"
    );
    // And in quarantine it is no longer called.
    let r = reg.call(&host, &svc, "com.test.hostile", "search", b"{}", Limits::short());
    assert!(matches!(r, Err(PluginError::NotAllowed(_))), "{r:?}");
}

#[test]
fn one_that_hangs_is_cut_off_by_fuel() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (loop $again (br $again))
              (i64.const 0)))"#
    );
    let start = std::time::Instant::now();
    let (_reg, _notes, r) = run("", &wat, "search");
    assert_eq!(r, Err(PluginError::Stopped(Halt::OutOfFuel)), "{r:?}");
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "it was cut off soon");
}

#[test]
fn one_asking_for_more_memory_than_allowed_is_refused() {
    // Eight megabytes is 128 pages; it asks for 200 in one go. It gives the
    // good reply only if the engine refused it.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (if (i32.eq (memory.grow (i32.const 200)) (i32.const -1))
                (then (return {FIXED_REPLY})))
              (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "search");
    assert_eq!(
        r.unwrap(),
        Reply::Ok(br#"{"ok":true}"#.to_vec()),
        "the engine refused it the memory; had it grown, it would answer nothing"
    );
}

#[test]
fn one_eating_memory_in_a_loop_is_stopped() {
    // It grows one page at a time forever. With the limit in place it never
    // grows, and what stops it, measured, is the clock and not the fuel:
    // asking for memory is expensive in time and cheap in instructions.
    // That is why both limits are needed, and why what is asserted here is
    // that it was stopped and not by which of the two.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (loop $again
                (drop (memory.grow (i32.const 1)))
                (br $again))
              (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "search");
    match r {
        Err(PluginError::Stopped(d)) => {
            assert!(!d.to_string().is_empty(), "and why it was stopped is said");
        }
        other => panic!("it should have been stopped by one of its limits: {other:?}"),
    }
}

#[test]
fn one_lying_about_its_version_does_not_load() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 7))
            (func (export "search") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.install(&host, &manifest(""), &compile(&wat), |_| true), "it should not have loaded");
    let inst = reg.installed().next().unwrap();
    assert!(!inst.loaded());
    assert!(inst.problem.as_ref().unwrap().contains("does not load"), "{:?}", inst.problem);
    assert!(reg.usable().is_empty());
}

#[test]
fn one_missing_exports_does_not_load() {
    // No memory and no alloc: there is no way to talk to it.
    let wat = r#"(module (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#;
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.install(&host, &manifest(""), &compile(wat), |_| true));
    let problem = reg.installed().next().unwrap().problem.clone().unwrap();
    assert!(problem.contains("memory"), "{problem}");
}

/// A plugin that asks the host for that URL. The length is computed, not
/// typed by hand: a wrong one truncates the JSON and the test would pass
/// because the request is unintelligible, not because the domain is barred.
fn plugin_that_requests(url: &str) -> String {
    let request = format!(r#"{{"url":"{url}"}}"#);
    let escaped = request.replace('"', "\\\"");
    let length = request.len();
    format!(
        r#"(module
            (import "chilpa" "http" (func $http (param i32 i32 i32 i32) (result i32)))
            {PRELUDE}
            (data (i32.const 256) "{escaped}")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (drop (call $http (i32.const 256) (i32.const {length}) (i32.const 2048) (i32.const 512)))
              {FIXED_REPLY}))"#
    )
}

#[test]
fn one_reaching_a_domain_it_did_not_declare_never_reaches_the_host() {
    // It declares example.com and asks for something-else.net.
    let wat = plugin_that_requests("https://something-else.net/steal");
    let (_reg, notes, r) =
        run(r#"{"type": "network", "domains": ["example.com"]}"#, &wat, "search");
    assert!(r.is_ok(), "the call finishes; what is refused is the way out: {r:?}");
    let requests = notes.examine(|a| a.requests.len());
    assert_eq!(requests, 0, "the host never saw that request");
}

#[test]
fn one_reaching_the_domain_it_did_declare_gets_through() {
    let wat = plugin_that_requests("https://api.example.com/search");
    let (_reg, notes, r) =
        run(r#"{"type": "network", "domains": ["example.com"]}"#, &wat, "search");
    assert!(r.is_ok(), "{r:?}");
    let (how_many, who) = notes.examine(|a| {
        (a.requests.len(), a.requests.first().map(|p| p.0.clone()).unwrap_or_default())
    });
    assert_eq!(how_many, 1, "a subdomain of the declared domain does pass");
    assert_eq!(who, "com.test.hostile", "the request goes in the plugin's name, not another's");
}

#[test]
fn one_without_network_permission_never_reaches_the_host_however_hard_it_tries() {
    let wat = plugin_that_requests("https://api.example.com/x");
    let (_reg, notes, r) = run("", &wat, "search");
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(notes.examine(|a| a.requests.len()), 0, "with no network capability there is no network");
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
            (func (export "search") (param i32 i32) (result i64)
              (drop (call $get (i32.const 256) (i32.const 5) (i32.const 2048) (i32.const 512)))
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {FIXED_REPLY}))"#
    );
    let (_reg, notes, r) = run("", &wat, "search");
    assert!(r.is_ok(), "{r:?}");
    let (reads, writes) = notes.examine(|a| (a.reads.len(), a.writes.len()));
    assert_eq!(reads, 0, "with no store capability nothing is read");
    assert_eq!(writes, 0, "and nothing is written");
}

#[test]
fn a_plugin_store_always_goes_under_its_own_name() {
    let wat = format!(
        r#"(module
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            {PRELUDE}
            (data (i32.const 256) "key")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {FIXED_REPLY}))"#
    );
    let (_reg, notes, r) = run(r#"{"type": "storage"}"#, &wat, "search");
    assert!(r.is_ok(), "{r:?}");
    let who = notes.examine(|a| a.writes.first().map(|e| e.0.clone()).unwrap_or_default());
    assert_eq!(
        who, "com.test.hostile",
        "the host supplies the id from the manifest, not the guest"
    );
}

#[test]
fn one_answering_with_a_foreign_address_does_not_make_chilpa_read_beyond() {
    // It answers with a huge address, far outside its own memory.
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 4000000000) (i64.const 32)) (i64.const 1000))))"#
    );
    let (_reg, _notes, r) = run("", &wat, "search");
    assert_eq!(r, Err(PluginError::OutOfMemory), "{r:?}");
}

#[test]
fn one_that_answers_nothing_is_not_confused_with_one_that_answered_empty() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) (i64.const 0)))"#
    );
    let (_reg, _notes, r) = run("", &wat, "search");
    assert!(matches!(&r, Ok(Reply::Error(e)) if e.contains("answered nothing")), "{r:?}");
}

#[test]
fn an_operation_the_plugin_lacks_is_reported_and_that_is_all() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let (_reg, _notes, r) = run("", &wat, "one_that_does_not_exist");
    assert!(matches!(&r, Err(PluginError::MissingExport(n)) if n == "one_that_does_not_exist"), "{r:?}");
}

#[test]
fn a_broken_plugin_does_not_take_the_others_with_it() {
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let mut reg = Registry::new();

    let good = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) {FIXED_REPLY}))"#
    );
    let broken = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) (unreachable)))"#
    );
    let m = |id: &str| manifest("").replace("com.test.hostile", id);

    assert!(reg.install(&host, &m("com.good.plugin"), &compile(&good), |_| true));
    assert!(reg.install(&host, &m("com.broken.plugin"), &compile(&broken), |_| true));

    for _ in 0..3 {
        let _ = reg.call(&host, &svc, "com.broken.plugin", "search", b"{}", Limits::short());
    }
    assert!(matches!(reg.state("com.broken.plugin"), State::Quarantined { .. }));

    // And the good one carries on working.
    let r = reg.call(&host, &svc, "com.good.plugin", "search", b"{}", Limits::short());
    assert_eq!(r.unwrap(), Reply::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.usable(), vec!["com.good.plugin"]);
    assert_eq!(reg.quarantined().len(), 1);
}

#[test]
fn only_the_user_gets_it_out_of_quarantine() {
    let wat = format!(
        r#"(module {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "search") (param i32 i32) (result i64) (unreachable)))"#
    );
    let host = Host::new().unwrap();
    let (svc, _) = services();
    let mut reg = Registry::new();
    reg.install(&host, &manifest(""), &compile(&wat), |_| true);
    for _ in 0..3 {
        let _ = reg.call(&host, &svc, "com.test.hostile", "search", b"{}", Limits::short());
    }
    assert!(reg.usable().is_empty());
    reg.reactivate("com.test.hostile");
    assert_eq!(reg.usable(), vec!["com.test.hostile"], "the user brought it back");
}
