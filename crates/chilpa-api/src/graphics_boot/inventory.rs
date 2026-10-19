//! Settings' Vulkan inventory without retaining drivers in an Auto parent.

use super::{gpu, preflight};
use std::process::{Command, Output};
use std::time::Duration;

const CHILD_ENV: &str = "CHILPA_INTERNAL_GPU_INVENTORY_CHILD";
const PROOF: &str = "CHILPA_GPU_INVENTORY=";
const MAX_JSON: usize = 32_768;
const WATCHDOG: Duration = Duration::from_secs(8);

/// This child creates only its application object and Qt Vulkan instance.
pub fn child_requested() -> bool {
    std::env::var_os(CHILD_ENV).is_some_and(|value| value.to_string_lossy().trim() == "1")
}

/// Prints a bounded inventory and never enters normal application startup.
pub fn run_child(list: impl FnOnce() -> String) -> i32 {
    use std::io::Write;
    std::env::remove_var(CHILD_ENV);
    let json = list();
    if json.len() > MAX_JSON || json.contains(['\n', '\r']) || gpu::parse_inventory_json(&json).is_err() {
        return 43;
    }
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{PROOF}{json}").and_then(|()| stdout.flush()).is_err() {
        return 43;
    }
    0
}

fn read_inventory(output: Output, timed_out: bool) -> Result<String, String> {
    if timed_out {
        return Err("the inventory child exceeded its 8-second watchdog".into());
    }
    if !output.status.success() {
        return Err(format!("the inventory child failed with {}", output.status));
    }
    let stdout = std::str::from_utf8(&output.stdout).map_err(|_| "the inventory child returned invalid UTF-8")?;
    let mut proofs = stdout.lines().filter_map(|line| line.strip_prefix(PROOF));
    let json = proofs.next().ok_or("the inventory child returned no inventory")?;
    if json.len() > MAX_JSON || proofs.next().is_some() {
        return Err("the inventory child returned an oversized or repeated inventory".into());
    }
    gpu::parse_inventory_json(json).map_err(|_| "the inventory child returned malformed inventory")?;
    Ok(json.to_string())
}

fn settle(inventory: Result<String, String>) {
    match inventory {
        Ok(json) => gpu::set_child_inventory_json(&json),
        Err(error) => {
            eprintln!("gpu: {error}; Auto stays in control");
            gpu::set_child_inventory_json("[]");
        }
    }
}

/// Settings' rows, collected off the way to the window: the child starts
/// here and a thread waits for it. QBZ lists the devices when it publishes
/// Settings, after the window; waiting here held every Auto start 230 to
/// 330 ms. Until it lands the row is not shown, as with one device.
/// Failure leaves an empty Settings inventory. Auto and backend precedence
/// stay unchanged; no preference or startup sentinel is touched here.
pub fn collect() {
    if !cfg!(target_os = "linux") {
        gpu::set_child_inventory_json("[]");
        return;
    }
    let started = std::env::current_exe()
        .map_err(|error| format!("cannot resolve the inventory executable: {error}"))
        .and_then(|executable| preflight::start_child(Command::new(executable).env(CHILD_ENV, "1")));
    let started = match started {
        Ok(started) => started,
        Err(error) => return settle(Err(error)),
    };
    let waiting = std::thread::Builder::new()
        .name("gpu-inventory".into())
        .spawn(move || settle(started.finish(WATCHDOG).and_then(|(output, timed_out)| read_inventory(output, timed_out))));
    if let Err(error) = waiting {
        settle(Err(format!("cannot wait for the inventory child: {error}")));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32, text: &str) -> Output {
        Output { status: std::process::ExitStatus::from_raw(code), stdout: text.as_bytes().to_vec(), stderr: Vec::new() }
    }

    #[test]
    fn an_empty_inventory_is_valid_and_driver_output_is_not_inventory() {
        assert_eq!(read_inventory(output(0, &format!("driver message\n{PROOF}[]\n")), false).unwrap(), "[]");
        assert!(read_inventory(output(0, "[]"), false).is_err());
    }

    #[test]
    fn a_child_inventory_keeps_stable_identity_for_settings() {
        let json = r#"[{"index":3,"name":"Example GPU","vendor":1,"device":2,"type":2,"uuid":"11111111111111111111111111111111"}]"#;
        let received = read_inventory(output(0, &format!("{PROOF}{json}\n")), false).unwrap();
        let devices = gpu::parse_inventory_json(&received).unwrap();
        assert_eq!(devices[0].identity, "vk:11111111111111111111111111111111");
        assert_eq!(devices[0].name, "Example GPU");
    }

    #[test]
    fn failed_timed_out_and_malformed_children_leave_no_inventory() {
        let good = format!("{PROOF}[]\n");
        assert!(read_inventory(output(256, &good), false).is_err());
        assert!(read_inventory(output(0, &good), true).is_err());
        for json in ["{", "{}", "[{}]", "null"] {
            assert!(read_inventory(output(0, &format!("{PROOF}{json}\n")), false).is_err());
        }
    }

    #[test]
    fn repeated_and_oversized_inventory_is_rejected() {
        assert!(read_inventory(output(0, &format!("{PROOF}[]\n{PROOF}[]\n")), false).is_err());
        let large = format!("{PROOF}[{}]", " ".repeat(MAX_JSON));
        assert!(read_inventory(output(0, &large), false).is_err());
        let mut invalid = output(0, "");
        invalid.stdout.push(0xff);
        assert!(read_inventory(invalid, false).is_err());
    }

    #[test]
    fn a_stuck_inventory_child_is_killed_and_reaped() {
        let mut command = Command::new("sh");
        command.args(["-c", "while :; do :; done"]);
        let (output, timed_out) = preflight::run_child(&mut command, Duration::from_millis(50)).unwrap();
        assert!(timed_out);
        assert!(!output.status.success());
        assert!(read_inventory(output, timed_out).is_err());
    }
}
