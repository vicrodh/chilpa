//! I exercise every HTTP entry point against a local listener with no external network.
use chilpa_stream::{network::{self, Policy}, *};
use std::{io::{Read, Write}, net::TcpListener, time::Duration};

#[test]
fn offline_blocks_http_streams_workers_and_servers_and_can_be_reversed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}/audio", listener.local_addr().unwrap());
    network::configure(Policy { offline: true, ..Policy::default() });
    let request = serde_json::json!({"url": address});
    let response: serde_json::Value = serde_json::from_slice(&send_request(request.to_string().as_bytes(), None)).unwrap();
    assert!(response["error"].as_str().unwrap().contains("disabled"));
    assert!(fetch(&Fetch { url: &address, user_agent: "fixture", accept: "*/*",
        timeout: Duration::from_secs(1), max_bytes: 32, authorization: None }).is_err());
    assert!(call_server(&Call { method: "GET", url: &address, headers: &[], body: None,
        timeout: Duration::from_secs(1), max_bytes: 32 }).is_err());
    assert!(Ranged::open(RangedRequest { url: address.clone(), headers: vec![] }).is_err());
    assert!(open(&address).is_err());
    assert!(open_parts(std::slice::from_ref(&address)).is_err());
    assert_eq!(send_worker_request(&request, &["127.0.0.1".into()], Duration::from_secs(1)), Err(WorkerNetworkError::Denied));
    assert!(matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));

    network::configure(Policy::default());
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut request = [0u8; 4096];
        socket.read(&mut request).unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 4\r\nConnection: close\r\n\r\nsong").unwrap();
    });
    let mut stream = open(&address).unwrap();
    let mut first = [0];
    assert_eq!(stream.read(&mut first).unwrap(), 1);
    network::configure(Policy { offline: true, ..Policy::default() });
    assert!(stream.read(&mut first).is_err(), "an already opened stream also observes offline mode");
    server.join().unwrap();
    network::configure(Policy::default());
}
