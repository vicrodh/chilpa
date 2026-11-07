use super::*;
use crate::fake_bus::{FakeService, PrivateBus};
use std::sync::mpsc;
use zbus::zvariant::Structure;

const WAIT: Duration = Duration::from_secs(5);

/// A Settings portal that says dark with this accent, or, when `old`,
/// speaks only version 1, which wraps each value in one more variant.
fn settings(bus: &PrivateBus, old: bool) -> FakeService {
    FakeService::serve(bus, DESKTOP, move |conn, call| {
        let header = call.header();
        let Ok((_, key)) = call.body().deserialize::<(String, String)>() else { return };
        let value = match key.as_str() {
            "color-scheme" => Value::U32(1),
            "accent-color" => Value::from(Structure::from((0.2f64, 0.4f64, 0.6f64))),
            _ => {
                let _ = conn.reply_dbus_error(&header, zbus::fdo::Error::Failed("not found".into()));
                return;
            }
        };
        let _ = match header.member().map(|member| member.as_str()) {
            Some("ReadOne") if old => conn.reply_dbus_error(&header, zbus::fdo::Error::UnknownMethod("ReadOne".into())),
            Some("ReadOne") => conn.reply(&header, &value),
            Some("Read") => conn.reply(&header, &Value::Value(Box::new(value))),
            _ => Ok(()),
        };
    })
}

fn told() -> (impl Fn(Appearance) + Send + 'static, mpsc::Receiver<Appearance>) {
    let (tx, rx) = mpsc::channel();
    (move |appearance| { let _ = tx.send(appearance); }, rx)
}

const DARK_WITH_ACCENT: Appearance = Appearance { scheme: Scheme::Dark, accent: Some([51, 102, 153]) };

#[test]
fn the_appearance_is_read_once_then_followed_until_the_portal_is_dropped() {
    let bus = PrivateBus::start();
    let fake = settings(&bus, false);
    let (tell, rx) = told();
    let portal = Portal::on(bus.bus(), tell).unwrap();
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), DARK_WITH_ACCENT);
    assert_eq!(fake.members(), ["ReadOne", "ReadOne"]);
    let change = |namespace: &str, key: &str, value: Value<'_>| {
        fake.connection().emit_signal(None::<&str>, DESKTOP_PATH, SETTINGS, "SettingChanged", &(namespace, key, value)).unwrap();
    };
    change("org.kde.kdeglobals.General", "ColorScheme", Value::from("BreezeLight"));
    change(APPEARANCE, "color-scheme", Value::U32(1));
    change(APPEARANCE, "color-scheme", Value::U32(2));
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), Appearance { scheme: Scheme::Light, ..DARK_WITH_ACCENT },
        "another namespace and an unchanged value say nothing");
    change(APPEARANCE, "accent-color", Value::from(Structure::from((-1.0f64, -1.0f64, -1.0f64))));
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), Appearance { scheme: Scheme::Light, accent: None }, "out of range is no accent");
    drop(portal);
    assert_eq!(rx.recv_timeout(WAIT), Err(mpsc::RecvTimeoutError::Disconnected), "its thread ends with it");
}

#[test]
fn an_old_portal_is_read_through_its_wrapped_values() {
    let bus = PrivateBus::start();
    let fake = settings(&bus, true);
    let (tell, rx) = told();
    let _portal = Portal::on(bus.bus(), tell).unwrap();
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), DARK_WITH_ACCENT);
    assert_eq!(fake.members(), ["ReadOne", "Read", "ReadOne", "Read"]);
}

#[test]
fn a_desktop_without_the_portal_is_told_once_as_no_preference() {
    let bus = PrivateBus::start();
    let (tell, rx) = told();
    let portal = Portal::on(bus.bus(), tell).unwrap();
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), Appearance::default());
    assert!(portal.documents().host_paths_under(&[PathBuf::from("/doc/a1/Music")], &[PathBuf::from("/doc")]).is_empty());
}

#[test]
fn document_paths_are_split_into_their_id_and_what_follows_their_name() {
    let mounts = [PathBuf::from("/run/user/1000/doc"), PathBuf::from("/run/flatpak/doc")];
    let split = |path: &str| document_path(Path::new(path), &mounts);
    assert_eq!(split("/run/user/1000/doc/a1b2/Music/Rock/01.flac"), Some(DocumentPath { id: "a1b2".into(), rest: "Rock/01.flac".into() }));
    assert_eq!(split("/run/flatpak/doc/c3/Music"), Some(DocumentPath { id: "c3".into(), rest: PathBuf::new() }));
    assert_eq!(split("/run/user/1000/doc/a1b2"), None, "the id's own folder is no document");
    assert_eq!(split("/home/someone/Music"), None);
}

#[test]
fn host_paths_come_from_the_documents_portal_with_what_followed_the_name() {
    let bus = PrivateBus::start();
    let _settings = settings(&bus, false);
    let documents = FakeService::serve(&bus, DOCUMENTS, |conn, call| {
        let header = call.header();
        let Ok((ids,)) = call.body().deserialize::<(Vec<String>,)>() else { return };
        let known: HashMap<String, Vec<u8>> = ids.into_iter().filter(|id| id == "a1").map(|id| (id, b"/home/someone/Music\0".to_vec())).collect();
        let _ = conn.reply(&header, &known);
    });
    let (tell, rx) = told();
    let portal = Portal::on(bus.bus(), tell).unwrap();
    rx.recv_timeout(WAIT).unwrap();
    let mounts = [PathBuf::from("/doc")];
    let asked = [PathBuf::from("/doc/a1/Music"), PathBuf::from("/doc/a1/Music/Rock"), PathBuf::from("/doc/b2/Other"), PathBuf::from("/home/x")];
    let found = portal.documents().host_paths_under(&asked, &mounts);
    assert_eq!(found, HashMap::from([
        (asked[0].clone(), PathBuf::from("/home/someone/Music")),
        (asked[1].clone(), PathBuf::from("/home/someone/Music/Rock")),
    ]));
    let (ids,): (Vec<String>,) = documents.calls()[0].body().deserialize().unwrap();
    assert_eq!(ids, ["a1", "b2"], "each id once, and only those of documents");
    drop(portal);
    assert!(documents_after_close(&asked, &mounts, &bus).is_empty());
}

/// Asking through a portal that was dropped answers nothing.
fn documents_after_close(asked: &[PathBuf], mounts: &[PathBuf], bus: &PrivateBus) -> HashMap<PathBuf, PathBuf> {
    let (tell, rx) = told();
    let portal = Portal::on(bus.bus(), tell).unwrap();
    rx.recv_timeout(WAIT).unwrap();
    let documents = portal.documents();
    drop(portal);
    documents.host_paths_under(asked, mounts)
}
