use super::*;

#[test]
fn a_new_control_discards_old_snapshots_but_keeps_completion_and_errors() {
    let (orders, _rx) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    let worker = Worker {
        orders, events: rx, sequence: AtomicU64::new(4), latest: Arc::new(AtomicU64::new(2)),
        connection: Arc::new(AtomicU64::new(1)), thread: None,
    };
    events.send((1, 2, 3, Event::Status(Status::default()))).unwrap();
    events.send((1, 2, 3, Event::Ended)).unwrap();
    events.send((1, 2, 3, Event::Failed("load rejected".into()))).unwrap();
    events.send((0, 2, 4, Event::Disconnected)).unwrap();
    events.send((1, 2, 4, Event::Status(Status::default()))).unwrap();
    let received = worker.poll();
    assert_eq!(received.len(), 3);
    assert!(matches!(received[0], (2, Event::Ended)));
    assert!(matches!(received[1], (2, Event::Failed(_))));
    assert!(matches!(received[2], (2, Event::Status(_))));
}
