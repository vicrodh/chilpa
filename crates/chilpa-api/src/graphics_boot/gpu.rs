//! Preferred GPU, ported from QBZ (`renderer_qt.rs`, PARITY-DEBT #83).
//!
//! Qt has no public way to pick an adapter. On Linux its supported escape
//! hatch is `QT_VK_PHYSICAL_DEVICE_INDEX`, read when the Vulkan QRhi is
//! created, so a chosen GPU always means Vulkan: measured in QBZ on an Intel
//! Arc + RTX 4070 hybrid, the PRIME variables gave llvmpipe or an OpenGL ES
//! context whose shaders do not load, and Vulkan with the index gave the
//! chosen device with no shader error. macOS cannot select (QRhi hardcodes
//! the system default device).
//!
//! The index is the one Qt's own Vulkan instance gives this process, never
//! one from an instance of our own: on that hybrid an independent instance
//! listed Intel=0/NVIDIA=1 while Qt's listed NVIDIA=0/Intel=1, and feeding
//! the wrong one picked the other GPU and killed the Wayland connection.
//! The window's crate lists Qt's devices; the preference keeps a stable
//! identity beside the readable name, never the index.

use super::preflight;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Settings > Appearance > Preferred GPU: the device's name, or "auto".
pub const GPU: &str = "appearance.gpu";
/// Its stable identity, which wins over the name.
pub const GPU_IDENTITY: &str = "appearance.gpuIdentity";

/// One physical GPU in this launch's Qt Vulkan instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    /// The originating Qt instance's index. Only a local inventory may
    /// supply this process's `QT_VK_PHYSICAL_DEVICE_INDEX`.
    pub index: u32,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub identity: String,
    pub discrete: bool,
}

impl GpuInfo {
    /// The row's label: "<name> (discrete|integrated)".
    pub fn label(&self) -> String {
        let class = if self.discrete { chilpa_i18n::t("discrete") } else { chilpa_i18n::t("integrated") };
        format!("{} ({})", self.name, class)
    }
}

#[derive(serde::Deserialize)]
struct QtGpuJson {
    index: u32,
    name: String,
    vendor: u32,
    device: u32,
    #[serde(rename = "type")]
    device_type: u32,
    #[serde(default)]
    uuid: String,
}

/// `vk:<uuid>` when the driver gives one, else the PCI ids and the name.
fn stable_identity(raw: &QtGpuJson) -> String {
    let uuid = raw.uuid.trim().to_ascii_lowercase();
    if uuid.len() == 32 && uuid.bytes().all(|b| b.is_ascii_hexdigit()) && uuid.bytes().any(|b| b != b'0') {
        format!("vk:{uuid}")
    } else {
        format!("pci:{:04x}:{:04x}:{}", raw.vendor, raw.device, raw.name.trim().to_ascii_lowercase())
    }
}

fn inventory_from_raw(raw: Vec<QtGpuJson>) -> Vec<GpuInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for gpu in raw {
        let name = gpu.name.trim().to_string();
        // A CPU device is lavapipe or llvmpipe, not a GPU to offer.
        if name.is_empty() || gpu.device_type == 4 || !seen.insert(gpu.index) {
            continue;
        }
        out.push(GpuInfo {
            index: gpu.index,
            name,
            vendor_id: gpu.vendor,
            device_id: gpu.device,
            identity: stable_identity(&gpu),
            discrete: gpu.device_type == 2,
        });
    }
    out.sort_by_key(|gpu| gpu.index);
    out
}

struct Inventory {
    devices: Vec<GpuInfo>,
    local: bool,
}

impl Inventory {
    fn selection(&self) -> Option<&[GpuInfo]> {
        self.local.then_some(self.devices.as_slice())
    }
}

static GPUS: OnceLock<Inventory> = OnceLock::new();

pub(super) fn parse_inventory_json(json: &str) -> Result<Vec<GpuInfo>, serde_json::Error> {
    serde_json::from_str::<Vec<QtGpuJson>>(json).map(inventory_from_raw)
}

/// The devices Qt listed, as the window's crate hands them over: a JSON
/// array of `{index, name, vendor, device, type, uuid}`. Once per process;
/// an unreadable list is no devices, never a reason not to start.
pub fn set_inventory_json(json: &str) {
    set_inventory(json, true);
}

/// A child's inventory supplies Settings rows, never a device index for
/// this process's Qt instance.
pub(super) fn set_child_inventory_json(json: &str) {
    set_inventory(json, false);
}

fn set_inventory(json: &str, local: bool) {
    let list = match parse_inventory_json(json) {
        Ok(list) => list,
        Err(error) => {
            eprintln!("gpu: Qt's Vulkan inventory was not readable: {error}");
            Vec::new()
        }
    };
    if list.is_empty() {
        eprintln!("gpu: Qt exposed no selectable Vulkan hardware; Auto stays in control");
    }
    for gpu in &list {
        eprintln!(
            "gpu: Qt device {} '{}' ({}, vendor={:04x} device={:04x})",
            gpu.index, gpu.name, if gpu.discrete { "discrete" } else { "integrated" }, gpu.vendor_id, gpu.device_id
        );
    }
    let _ = GPUS.set(Inventory { devices: list, local });
}

/// The devices available to Settings. Indices from an inventory child
/// belong only to that child and cannot select this process's adapter.
pub fn gpus() -> &'static [GpuInfo] {
    GPUS.get().map(|inventory| inventory.devices.as_slice()).unwrap_or(&[])
}

/// The one device the predicate names, or none when it names two.
fn unique<'a>(list: &'a [GpuInfo], mut predicate: impl FnMut(&GpuInfo) -> bool) -> Option<&'a GpuInfo> {
    let mut matches = list.iter().filter(|gpu| predicate(gpu));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// A kept choice as a device. The identity wins; without one the name an
/// older build kept, or the class keys "discrete" and "integrated". `None`
/// is Auto, or a choice that names no present device.
fn resolve_in<'a>(list: &'a [GpuInfo], pref: &str, identity: &str) -> Option<&'a GpuInfo> {
    let pref = pref.trim();
    if pref.is_empty() || pref == "auto" {
        return None;
    }
    let identity = identity.trim();
    if !identity.is_empty() {
        if let Some(hit) = unique(list, |gpu| gpu.identity == identity) {
            // A build that knew only the name may have changed it without
            // the identity: when the name clearly names another device, or
            // the class disagrees, the identity is stale.
            let names_another = list.iter().any(|gpu| gpu.name == pref) && hit.name != pref;
            let class_disagrees = matches!(pref, "discrete" | "integrated") && hit.discrete != (pref == "discrete");
            if !names_another && !class_disagrees {
                return Some(hit);
            }
        } else {
            // Never from a missing identity to a device of the same model:
            // that could be the other of two identical cards.
            return None;
        }
    }
    if let Some(hit) = unique(list, |gpu| gpu.name == pref) {
        return Some(hit);
    }
    match pref {
        "discrete" => unique(list, |gpu| gpu.discrete),
        "integrated" => unique(list, |gpu| !gpu.discrete),
        _ => None,
    }
}

fn saved() -> (String, String) {
    (super::pref(GPU, "auto"), super::pref(GPU_IDENTITY, ""))
}

/// The kept choice, as one of the present devices.
pub fn resolve_saved() -> Option<&'static GpuInfo> {
    let (name, identity) = saved();
    resolve_in(gpus(), &name, &identity)
}

/// Whether a choice can be honoured here: Linux, and two devices or more to
/// choose between. A row with one real answer is not shown.
pub fn selectable() -> bool {
    cfg!(target_os = "linux") && gpus().len() > 1
}

/// The choice, kept as one pair: the readable name for older builds and the
/// identity that wins. The index is never kept.
pub fn save_preference(gpu: Option<&GpuInfo>) {
    let (name, identity) = gpu.map(|g| (g.name.as_str(), g.identity.as_str())).unwrap_or(("auto", ""));
    super::save_prefs(&[(GPU, name), (GPU_IDENTITY, identity)]);
    eprintln!("gpu: -> {name} (applies at the next start)");
}

// ---------------------------------------------------------------------------
// The GPU's own sentinel and the proof from a child
// ---------------------------------------------------------------------------

/// A device that fails before the window exists (a bad adapter, a
/// presentation or cross-device import failure) gets its own sentinel, so a
/// device failure never rewrites the renderer row and the other way round.
pub(super) fn sentinel() -> std::path::PathBuf {
    chilpa_dirs::data().join("gpu_attempt")
}

static APPROVED: Mutex<Option<String>> = Mutex::new(None);
static FALLBACK_NOTICE: AtomicBool = AtomicBool::new(false);

fn set_approved(identity: Option<String>) {
    *APPROVED.lock().unwrap_or_else(|e| e.into_inner()) = identity;
}

pub(super) fn approved(identity: &str) -> bool {
    APPROVED.lock().unwrap_or_else(|e| e.into_inner()).as_deref() == Some(identity)
}

pub(super) fn any_approved() -> bool {
    APPROVED.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub(super) fn take_fallback_notice() -> bool {
    FALLBACK_NOTICE.swap(false, Ordering::SeqCst)
}

fn revert_if_previous_launch_died() -> bool {
    let path = sentinel();
    let Some(choice) = super::armed(&path) else { return false };
    super::disarm(&path);
    eprintln!("gpu: the previous launch forced '{choice}' and never presented a frame; Preferred GPU back to Auto");
    save_preference(None);
    FALLBACK_NOTICE.store(true, Ordering::SeqCst);
    true
}

fn reject_saved(reason: &str) {
    set_approved(None);
    super::disarm(&sentinel());
    save_preference(None);
    FALLBACK_NOTICE.store(true, Ordering::SeqCst);
    eprintln!("gpu preflight: {reason}; Preferred GPU back to Auto before the window starts");
}

/// A variable that already picks a device or a driver: whoever set it meant
/// it, and it can change the inventory itself.
fn override_env() -> Option<&'static str> {
    ["QT_VK_PHYSICAL_DEVICE_INDEX", "DRI_PRIME", "__NV_PRIME_RENDER_OFFLOAD", "__VK_LAYER_NV_optimus", "MESA_VK_DEVICE_SELECT", "VK_DRIVER_FILES", "VK_ICD_FILENAMES"]
        .into_iter()
        .find(|name| super::env_overrides(name))
}

/// An explicit software or OpenGL choice wins over a GPU, which needs
/// Vulkan: probing one then would be wasted and misleading.
fn renderer_choice_blocks(choice: &str) -> bool {
    matches!(choice.trim().to_ascii_lowercase().as_str(), "software" | "cpu" | "soft" | "gl" | "gles" | "femtovg")
}

fn renderer_blocks_preflight() -> bool {
    if super::env_overrides(super::RHI_ENV) || super::env_overrides(super::QUICK_ENV) {
        return true;
    }
    let from_env = std::env::var(super::RENDERER_ENV)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty() && v != "auto");
    renderer_choice_blocks(&from_env.unwrap_or_else(|| super::pref(super::RENDERER, "auto")))
}

fn needs_parent_inventory(linux: bool, approved: bool, device_override: bool, renderer_override: bool) -> bool {
    linux && approved && !device_override && !renderer_override
}

/// Only a saved GPU that passed presentation and can still be applied
/// needs this process's Qt device order. Auto and explicit backend choices
/// can obtain Settings rows without loading Vulkan drivers here.
pub fn needs_local_inventory() -> bool {
    needs_parent_inventory(
        cfg!(target_os = "linux"),
        any_approved(),
        override_env().is_some(),
        super::env_overrides(super::RHI_ENV) || super::env_overrides(super::QUICK_ENV),
    )
}

/// A kept GPU is proved in a disposable child before this process builds
/// anything: a compositor that rejects the device's buffers kills only the
/// child, and this process keeps Auto and starts as usual.
pub fn preflight_saved_gpu_at_boot() {
    set_approved(None);
    if !cfg!(target_os = "linux") {
        return;
    }
    // A renderer reverted to Auto may make a GPU probe relevant again.
    let _ = super::revert_if_previous_launch_died();
    if revert_if_previous_launch_died() {
        return;
    }
    if let Some(name) = override_env() {
        eprintln!("gpu preflight: {name} is set; the external choice is left alone");
        return;
    }
    if renderer_blocks_preflight() {
        eprintln!("gpu preflight: an explicit software or OpenGL renderer wins; no GPU probe");
        return;
    }
    let (name, identity) = saved();
    if name.trim().is_empty() || name == "auto" {
        return;
    }
    eprintln!("gpu preflight: proving the kept device '{name}' ({identity}) in a child");
    let (output, timed_out) = match preflight::spawn_gpu_child(&name, &identity) {
        Ok(result) => result,
        Err(error) => return reject_saved(&error),
    };
    if timed_out {
        return reject_saved("the child went past the 8-second watchdog");
    }
    if output.status.success() {
        match preflight::approved_identity(&output.stdout) {
            Some(approved) => {
                eprintln!("gpu preflight: presentation proved for {approved}");
                set_approved(Some(approved));
            }
            None => reject_saved("the child ended well without proving an identity"),
        }
        return;
    }
    let detail = preflight::output_tail(&output);
    match output.status.code() {
        Some(preflight::GPU_NOT_SELECTABLE) => reject_saved("this session exposes fewer than two selectable GPUs"),
        Some(preflight::GPU_NOT_FOUND) => reject_saved("the kept device is no longer present"),
        _ if detail.is_empty() => reject_saved(&format!("the child failed with {:?}", output.status)),
        _ => reject_saved(&format!("the child failed with {:?}; last output: {detail}", output.status)),
    }
}

/// The child's half: the device it was asked to prove, from this process's
/// own inventory, with Vulkan forced on it. `present` opens the probe window
/// and answers 0 when frames went out.
pub fn run_child(present: impl FnOnce() -> i32) -> i32 {
    let (name, identity) = preflight::take_gpu_child_target();
    if !cfg!(target_os = "linux") {
        return preflight::GPU_NOT_SELECTABLE;
    }
    let Some(list) = GPUS.get().and_then(Inventory::selection) else {
        return preflight::GPU_NOT_SELECTABLE;
    };
    if list.len() < 2 {
        eprintln!("gpu preflight: fewer than two selectable devices -> Auto");
        return preflight::GPU_NOT_SELECTABLE;
    }
    let Some(gpu) = resolve_in(list, &name, &identity) else {
        eprintln!("gpu preflight: the device '{name}' ({identity}) is not present");
        return preflight::GPU_NOT_FOUND;
    };
    std::env::set_var(super::RHI_ENV, "vulkan");
    std::env::set_var("QT_VK_PHYSICAL_DEVICE_INDEX", gpu.index.to_string());
    eprintln!("gpu preflight: probing '{}' ({}) at Qt index {}", gpu.name, gpu.identity, gpu.index);
    let result = present();
    if result == 0 {
        preflight::print_gpu_proof(&gpu.identity);
    } else {
        eprintln!("gpu preflight: '{}' did not prove presentation (code {result})", gpu.name);
    }
    result
}

/// The kept GPU, applied once the application object exists (Qt's Vulkan
/// instance can be listed) and before the first window (QRhi reads the
/// variables when it is created). Only a device the child proved in this
/// same launch reaches the window.
pub fn apply_preference() {
    let reverted = revert_if_previous_launch_died();
    if let Some(taken) = override_env() {
        eprintln!("gpu: {taken} already set; the choice is left to it");
        return;
    }
    if let Some(forced) = [super::RHI_ENV, super::QUICK_ENV].into_iter().find(|name| super::env_overrides(name)) {
        eprintln!("gpu: the renderer row already forced {forced}; a GPU needs Vulkan, so the renderer choice wins");
        return;
    }
    let (name, identity) = if reverted { ("auto".to_string(), String::new()) } else { saved() };
    if name.trim().is_empty() || name == "auto" {
        super::disarm(&sentinel());
        return;
    }
    if !selectable() {
        super::disarm(&sentinel());
        eprintln!("gpu: '{name}' asked for, but this session exposes fewer than two selectable GPUs -> Auto");
        return;
    }
    let Some(list) = GPUS.get().and_then(Inventory::selection) else {
        return reject_saved("this process has no local Qt device inventory");
    };
    let Some(gpu) = resolve_in(list, &name, &identity) else {
        super::disarm(&sentinel());
        eprintln!("gpu: the kept device '{name}' ({identity}) is not in Qt's inventory -> Auto");
        return;
    };
    if !approved(&gpu.identity) {
        return reject_saved(&format!("no proof of presentation for '{}' ({})", gpu.name, gpu.identity));
    }
    // A name or class kept by an older build becomes the identity, once.
    if name != gpu.name || identity != gpu.identity {
        save_preference(Some(gpu));
    }
    let marker = format!("{}|{}|qt-index={}", gpu.identity, gpu.name, gpu.index);
    if !super::arm(&sentinel(), &marker) {
        eprintln!("gpu: the startup sentinel could not be armed");
    }
    std::env::set_var(super::RHI_ENV, "vulkan");
    std::env::set_var("QT_VK_PHYSICAL_DEVICE_INDEX", gpu.index.to_string());
    eprintln!("gpu: '{}' ({}) -> {}=vulkan QT_VK_PHYSICAL_DEVICE_INDEX={}", gpu.name, gpu.identity, super::RHI_ENV, gpu.index);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(index: u32, name: &str, device_type: u32, uuid: &str) -> QtGpuJson {
        QtGpuJson { index, name: name.to_string(), vendor: 0x10de, device: 0x2860, device_type, uuid: uuid.to_string() }
    }

    fn info(index: u32, name: &str, identity: &str, discrete: bool) -> GpuInfo {
        GpuInfo { index, name: name.into(), vendor_id: 1, device_id: 2, identity: identity.into(), discrete }
    }

    #[test]
    fn qt_inventory_keeps_qt_indices_and_filters_cpu_adapters() {
        let list = inventory_from_raw(vec![
            raw(2, "lavapipe", 4, ""),
            raw(1, "Intel Arc", 1, "8680557d080000000002000000000000"),
            raw(0, "NVIDIA RTX", 2, "4e8f268581e1cbdcac12c7c15fa6624e"),
        ]);
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].index, list[0].name.as_str()), (0, "NVIDIA RTX"));
        assert_eq!((list[1].index, list[1].name.as_str()), (1, "Intel Arc"));
        assert!(list[0].discrete && !list[1].discrete);
    }

    #[test]
    fn stable_identity_wins_over_duplicate_model_names_and_index_changes() {
        let list = vec![info(1, "Same GPU", "vk:first", true), info(0, "Same GPU", "vk:wanted", true)];
        assert_eq!(resolve_in(&list, "Same GPU", "vk:wanted").map(|g| g.index), Some(0));
        assert!(resolve_in(&list, "Same GPU", "vk:missing").is_none());
        assert!(resolve_in(&list, "Same GPU", "").is_none(), "a name may not guess between identical devices");
        assert!(resolve_in(&list, "discrete", "").is_none(), "a class may not guess between two discrete GPUs");
        assert!(resolve_in(&list, "auto", "vk:wanted").is_none(), "auto is auto");
    }

    #[test]
    fn a_newer_readable_choice_replaces_an_identity_left_by_an_older_build() {
        let list = vec![info(0, "Integrated", "vk:integrated", false), info(1, "Discrete", "vk:discrete", true)];
        assert_eq!(resolve_in(&list, "Discrete", "vk:integrated").map(|g| g.index), Some(1));
        assert_eq!(resolve_in(&list, "integrated", "").map(|g| g.index), Some(0), "a class names the one it can");
    }

    #[test]
    fn the_identity_falls_back_to_pci_ids_when_the_driver_gives_no_uuid() {
        assert_eq!(stable_identity(&raw(0, " Example GPU ", 2, "00000000000000000000000000000000")), "pci:10de:2860:example gpu");
        assert_eq!(stable_identity(&raw(0, "X", 2, "4E8F268581E1CBDCAC12C7C15FA6624E")), "vk:4e8f268581e1cbdcac12c7c15fa6624e");
    }

    #[test]
    fn only_renderer_choices_that_preclude_vulkan_block_the_probe() {
        for choice in ["software", "cpu", "soft", "gl", "gles", "femtovg"] {
            assert!(renderer_choice_blocks(choice), "{choice}");
        }
        for choice in ["auto", "wgpu", "gpu", "hardware", "hw", "unknown"] {
            assert!(!renderer_choice_blocks(choice), "{choice}");
        }
    }

    #[test]
    fn auto_and_explicit_backend_choices_do_not_need_parent_vulkan_inventory() {
        assert!(!needs_parent_inventory(true, false, false, false), "Auto has no approved GPU");
        assert!(needs_parent_inventory(true, true, false, false), "a saved GPU needs the parent's Qt order");
        assert!(!needs_parent_inventory(true, true, true, false), "an explicit device overrides the saved GPU");
        assert!(!needs_parent_inventory(true, true, false, true), "an explicit Qt backend overrides the saved GPU");
        assert!(!needs_parent_inventory(false, true, false, false), "GPU selection remains Linux-only");
    }

    #[test]
    fn child_indices_cannot_select_a_parent_device() {
        let child = Inventory { devices: vec![info(0, "Discrete", "vk:discrete", true)], local: false };
        let parent = Inventory {
            devices: vec![info(0, "Integrated", "vk:integrated", false), info(1, "Discrete", "vk:discrete", true)],
            local: true,
        };
        assert_eq!(resolve_in(&child.devices, "Discrete", "vk:discrete").unwrap().index, 0);
        assert!(child.selection().is_none(), "a Settings inventory is never a selection inventory");
        let selected = resolve_in(parent.selection().unwrap(), "Discrete", "vk:discrete").unwrap();
        assert_eq!(selected.index, 1, "a saved identity resolves against the parent's own instance order");
    }

    #[test]
    fn the_label_says_the_class() {
        assert!(info(0, "RTX", "vk:a", true).label().starts_with("RTX ("));
    }
}
