//! I expose a diagnostic client for testing the same core that the facade uses.
use chilpa_sync::{pair, prepare_review, recover, synchronize_with_verification, Cancellation, Input, Playlist, State, MAX_METADATA_BYTES};
use chilpa_devices::transport::{Adb, AdbSelector, Mounted, Transport};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { tracks: Vec<Input>, playlists: Vec<Playlist>, #[serde(default)] remove_unselected: bool }

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = BTreeMap::new();
    let mut apply = false;
    let mut approve_removals = false;
    let mut verify_copies = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--list-devices" => {
                #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
                for device in chilpa_devices::transport::mtp::devices()? {
                    println!("mtp_device={} name={}", device.location, device.name);
                    for volume in chilpa_devices::transport::mtp::volumes(device.location)? {
                        println!("mtp_storage={}/{} name={} capacity={} available={} writable={}", device.location, volume.id, volume.name, volume.capacity, volume.available, volume.writable);
                    }
                }
                return Ok(());
            }
            "--apply" => apply = true,
            "--approve-removals" => approve_removals = true,
            "--verify-copies" => verify_copies = true,
            "--root" | "--state" | "--request" | "--adb" | "--mtp" | "--pair-name" => {
                let value = args.next().ok_or("missing option value")?;
                if options.insert(arg, value).is_some() { return Err("duplicate option".into()); }
            }
            _ => return Err("usage: chilpa-sync --list-devices | --root PATH --state DB --request JSON [--mtp LOCATION/STORAGE | --adb transport:ID|serial:SERIAL] [--pair-name NAME] [--apply] [--approve-removals] [--verify-copies]".into()),
        }
    }
    let root = options.get("--root").ok_or("missing --root")?;
    let state_path = options.get("--state").ok_or("missing --state")?;
    let request_path = options.get("--request").ok_or("missing --request")?;
    let mut bytes = Vec::new();
    std::fs::File::open(request_path)?.take(MAX_METADATA_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_METADATA_BYTES { return Err("request exceeds 16 MiB".into()); }
    let request: Request = serde_json::from_slice(&bytes)?;
    if options.contains_key("--mtp") && options.contains_key("--adb") { return Err("select one device transport".into()); }
    let mut transport: Box<dyn Transport> = if let Some(selector) = options.get("--mtp") {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            let (location, storage) = selector.split_once('/').ok_or("invalid --mtp selector")?;
            Box::new(chilpa_devices::transport::mtp::Mtp::open(location.parse()?, storage.parse()?, chilpa_sync::RelativePath::new(root.clone())?)?)
        }
        #[cfg(target_os = "android")]
        { let _ = selector; return Err("this diagnostic client offers MTP host transport on desktop platforms".into()); }
    } else if let Some(selector) = options.get("--adb") {
        let selector = if let Some(id) = selector.strip_prefix("transport:") { AdbSelector::Transport(id.parse()?) }
            else if let Some(serial) = selector.strip_prefix("serial:").filter(|s| !s.is_empty()) { AdbSelector::Serial(serial.into()) }
            else { return Err("invalid --adb selector".into()) };
        Box::new(Adb::open(selector, root)?)
    } else { Box::new(Mounted::open(Path::new(root))?) };
    let mut state = State::open(Path::new(state_path))?;
    let cancel = Cancellation::default();
    if let Some(name) = options.get("--pair-name") { pair(&mut state, transport.as_mut(), name, &cancel)?; }
    if state.interrupted()? {
        if !apply { return Err("interrupted sync requires --apply to reconcile its journal".into()); }
        let recovered = recover(&mut state, transport.as_mut(), &cancel)?;
        println!("recovered_operations={recovered}");
    }
    let plan = prepare_review(&state, transport.as_mut(), &request.tracks, &request.playlists, request.remove_unselected, &cancel)?;
    println!("target={} actions={} unchanged={} removals={} required_bytes={}", plan.target().id, plan.actions().len(),
        plan.unchanged_files(), plan.deletion_count(), plan.required_bytes());
    if apply {
        synchronize_with_verification(&mut state, transport.as_mut(), &plan, plan.id(), approve_removals, verify_copies, &cancel, &mut |_| ())?;
        println!("sync_complete managed_files={}", state.managed_count()?);
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() { eprintln!("chilpa-sync: {error}"); std::process::exit(1); }
}
