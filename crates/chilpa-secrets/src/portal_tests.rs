use super::*;
use crate::fake_portal::{Answer, FakePortal, PrivateBus};
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

fn waits() -> Waits {
    Waits { call: Duration::from_secs(2), whole: Duration::from_secs(5) }
}

fn refusal(outcome: Result<Vec<u8>, Unanswered>) -> Unanswered {
    match outcome {
        Ok(value) => panic!("the portal was expected to refuse, and it handed over {} bytes", value.len()),
        Err(unanswered) => unanswered,
    }
}

#[test]
fn the_portal_hands_over_the_apps_value_through_the_descriptor() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, Answer::Value(vec![7; 64]));
    assert_eq!(retrieve(&bus.bus(), &waits()).unwrap(), vec![7; 64]);
    let requests = fake.requests();
    assert_eq!(requests.len(), 1, "one request");
    assert!(requests[0].starts_with("chilpa_"), "the request object is named by the caller: {}", requests[0]);
    assert!(requests[0].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'), "a valid object path element");
}

#[test]
fn a_store_that_keeps_its_end_open_does_not_hold_the_value_up() {
    let bus = PrivateBus::start();
    let _fake = FakePortal::serve(&bus, Answer::ValueKeptOpen(vec![5; 64]));
    let started = Instant::now();
    assert_eq!(retrieve(&bus.bus(), &waits()).unwrap(), vec![5; 64]);
    assert!(started.elapsed() < FIRST_BYTES, "what was there was read without waiting for the end: {:?}", started.elapsed());
}

#[test]
fn without_a_portal_or_its_interface_there_is_no_value() {
    let bus = PrivateBus::start();
    let started = Instant::now();
    let nobody = refusal(retrieve(&bus.bus(), &waits()));
    assert_eq!(nobody.what, "the desktop offers no per-app key", "nobody owns the portal's name");
    assert!(started.elapsed() < Duration::from_secs(2), "the bus says so at once");
    let _fake = FakePortal::serve(&bus, Answer::NoInterface);
    let missing = refusal(retrieve(&bus.bus(), &waits()));
    assert_eq!(missing.what, "the desktop offers no per-app key");
}

#[test]
fn a_request_the_desktop_ends_hands_over_nothing() {
    let bus = PrivateBus::start();
    let _fake = FakePortal::serve(&bus, Answer::Ended(1));
    let ended = refusal(retrieve(&bus.bus(), &waits()));
    assert!(ended.what.contains("response 1"), "{}", ended.what);
}

#[test]
fn a_portal_that_never_answers_is_given_up_on_within_the_bound() {
    let bus = PrivateBus::start();
    let _fake = FakePortal::serve(&bus, Answer::Never);
    let bounded = Waits { call: Duration::from_secs(2), whole: Duration::from_millis(400) };
    let started = Instant::now();
    let silent = refusal(retrieve(&bus.bus(), &bounded));
    assert!(silent.what.starts_with("no answer within"), "{}", silent.what);
    assert!(started.elapsed() < Duration::from_secs(2), "the wait is the bound: {:?}", started.elapsed());
}

#[test]
fn a_value_too_short_or_too_long_is_refused() {
    for value in [vec![1; MIN_LEN - 1], vec![1; MAX_LEN + 1]] {
        let bus = PrivateBus::start();
        let _fake = FakePortal::serve(&bus, Answer::Value(value.clone()));
        let refused = refusal(retrieve(&bus.bus(), &waits()));
        assert!(refused.what.starts_with("the value is"), "{} bytes: {}", value.len(), refused.what);
    }
}

#[test]
fn boxes_opened_at_once_ask_the_portal_once_and_share_its_outcome() {
    let shared = Shared::new();
    let asked = AtomicUsize::new(0);
    let values: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let boxes: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    shared.resolve(|| {
                        asked.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(100));
                        Ok(vec![3; 64])
                    })
                })
            })
            .collect();
        boxes.into_iter().map(|b| b.join().unwrap().unwrap()).collect()
    });
    assert_eq!(asked.load(Ordering::SeqCst), 1, "one question for the whole process");
    assert!(values.iter().all(|value| *value == vec![3; 64]));

    let refused = Shared::new();
    for _ in 0..2 {
        let outcome = refused.resolve(|| {
            asked.fetch_add(1, Ordering::SeqCst);
            Err(Unanswered::new("the desktop offers no per-app key", ""))
        });
        assert!(matches!(outcome, Err(SecretError::Portal(ref why)) if why == "the desktop offers no per-app key"));
    }
    assert_eq!(asked.load(Ordering::SeqCst), 2, "a refusal is not asked again either");
}

#[test]
fn the_key_follows_the_value_and_the_service_and_is_never_the_value() {
    let value = [9u8; 64];
    let key = master_key(&value, "chilpa").unwrap();
    assert_eq!(key, master_key(&value, "chilpa").unwrap(), "the same on every start");
    assert_ne!(key, master_key(&value, "another").unwrap(), "another box, another key");
    assert_ne!(key, master_key(&[8u8; 64], "chilpa").unwrap(), "another install, another key");
    assert_ne!(&key[..], &value[..KEY_LEN], "the value is not used as the key");
}

#[test]
fn the_first_line_of_a_refusal_survives_the_log_redaction() {
    // The session log blanks a line that has any of these words; the bus's
    // own messages name the portal's interface, which has one of them.
    let blanked = ["secret", "password", "credential", "token"];
    let answers = [Answer::NoInterface, Answer::Ended(2), Answer::Value(vec![1; 8]), Answer::Never];
    for answer in answers {
        let bus = PrivateBus::start();
        let _fake = FakePortal::serve(&bus, answer);
        let bounded = Waits { call: Duration::from_secs(2), whole: Duration::from_millis(400) };
        let refused = refusal(retrieve(&bus.bus(), &bounded));
        let what = refused.what.to_lowercase();
        assert!(blanked.iter().all(|word| !what.contains(word)), "{what}");
    }
    let bus = PrivateBus::start();
    let _fake = FakePortal::serve(&bus, Answer::NoInterface);
    let missing = refusal(retrieve(&bus.bus(), &waits()));
    assert!(!missing.detail.is_empty(), "the bus's own message is kept for a terminal");
}
