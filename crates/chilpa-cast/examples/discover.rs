//! Read-only discovery: no receiver connection or transport command.
fn main() {
    let cast = std::thread::spawn(|| chilpa_cast::discovery::chromecast(std::time::Duration::from_secs(5)));
    let dlna = chilpa_cast::discovery::dlna(std::time::Duration::from_secs(3));
    for devices in [dlna, cast.join().unwrap()] {
        match devices {
            Ok(devices) => for device in devices { println!("{:?}: {}", device.kind, device.model); },
            Err(error) => eprintln!("{error}"),
        }
    }
}
