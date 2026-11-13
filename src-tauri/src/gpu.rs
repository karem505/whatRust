//! Linux GPU check for WebKitGTK's DMA-BUF renderer (issue #51).
//!
//! The DMA-BUF renderer keeps scrolling and typing smooth, but it has crashed
//! or rendered blank when NVIDIA's proprietary driver drives the display, so
//! whatRust turns it off there. A hybrid laptop (Intel/AMD drives the screen,
//! the NVIDIA GPU sits idle for offload) also has `/proc/driver/nvidia/version`,
//! and turning the renderer off there made scrolling sluggish for no reason.
//! It is now turned off only when every display GPU is NVIDIA.
//!
//! Old GPUs (issue #87): on Intel graphics older than Sandy Bridge Mesa offers
//! only OpenGL 2.1, too little for WebKitGTK's GPU compositor. WhatsApp's QR
//! page still draws, but once the chat list loads the window stays blank. Those
//! GPUs, and anyone who picks Settings → Rendering → Compatible, get software
//! rendering instead.

use crate::settings::{sanitize_rendering, RENDERING_SOFTWARE};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const NVIDIA: &str = "0x10de";

/// Should the DMA-BUF renderer be disabled? `nvidia_loaded`: the proprietary
/// kernel module is present. `vendors`: PCI vendor IDs of the DRM cards
/// (`/sys/class/drm/cardN/device/vendor`); empty when they can't be read.
pub fn disable_dmabuf(nvidia_loaded: bool, vendors: &[String]) -> bool {
    if !nvidia_loaded {
        return false;
    }
    // Unknown layout: keep the old, safe behaviour.
    if vendors.is_empty() {
        return true;
    }
    vendors
        .iter()
        .all(|v| v.trim().eq_ignore_ascii_case(NVIDIA))
}

/// PCI vendor IDs of the DRM cards (`card0`, `card1`, ... but not connectors
/// like `card1-eDP-1`).
fn drm_vendors(root: &Path) -> Vec<String> {
    drm_cards(root).into_iter().map(|(v, _)| v).collect()
}

/// `(vendor, device)` PCI IDs of the DRM cards, as written in sysfs ("0x8086").
fn drm_cards(root: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(n) = name.strip_prefix("card") else {
            continue;
        };
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let dev = entry.path().join("device");
        if let Ok(v) = std::fs::read_to_string(dev.join("vendor")) {
            let d = std::fs::read_to_string(dev.join("device")).unwrap_or_default();
            out.push((v.trim().to_string(), d.trim().to_string()));
        }
    }
    out
}

/// Set `WEBKIT_DISABLE_DMABUF_RENDERER=1` when NVIDIA drives the display. A
/// value already set (e.g. `flatpak override --env=...=0`) is respected.
pub fn apply_webkit_renderer_env() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return;
    }
    let nvidia = Path::new("/proc/driver/nvidia/version").exists();
    if disable_dmabuf(nvidia, &drm_vendors(Path::new("/sys/class/drm"))) {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

const INTEL: &str = "0x8086";

/// PCI device IDs of Intel GPUs before Sandy Bridge (gen 2 to gen 5: i830 to
/// Ironlake), from the kernel's `i915_pciids.h`. Mesa gives these OpenGL 2.1
/// at most.
const OLD_INTEL: &[u16] = &[
    // gen 2
    0x3577, 0x2562, 0x3582, 0x358e, 0x2572, //
    // gen 3 (915, 945, G33, Pineview)
    0x2582, 0x258a, 0x2592, 0x2772, 0x27a2, 0x27ae, 0x29b2, 0x29c2, 0x29d2, 0xa001, 0xa011,
    // gen 4 (965, G45, GM45)
    0x29a2, 0x2992, 0x2982, 0x2972, 0x2a02, 0x2a12, 0x2a42, 0x2e02, 0x2e12, 0x2e22, 0x2e32, 0x2e42,
    0x2e92, //
    // gen 5 (Ironlake)
    0x0042, 0x0046,
];

/// Is every display GPU an Intel GPU too old for WebKit's GPU compositor?
/// False when the cards can't be read.
pub fn legacy_gpu(cards: &[(String, String)]) -> bool {
    !cards.is_empty()
        && cards.iter().all(|(vendor, device)| {
            vendor.eq_ignore_ascii_case(INTEL)
                && u16::from_str_radix(device.trim_start_matches("0x"), 16)
                    .is_ok_and(|id| OLD_INTEL.contains(&id))
        })
}

/// Render in software? `setting` is Settings → Rendering; `auto` decides by GPU.
pub fn use_software_rendering(setting: &str, cards: &[(String, String)]) -> bool {
    setting == RENDERING_SOFTWARE || legacy_gpu(cards)
}

static SOFTWARE: AtomicBool = AtomicBool::new(false);
static LEGACY: AtomicBool = AtomicBool::new(false);

/// Was software rendering turned on for this run?
pub fn software_rendering_active() -> bool {
    SOFTWARE.load(Ordering::Relaxed)
}

/// Did the GPU check find an old GPU (so `auto` already renders in software)?
pub fn legacy_gpu_detected() -> bool {
    LEGACY.load(Ordering::Relaxed)
}

/// Settings → Rendering, read straight from settings.json: it must be known
/// before the first webview exists, earlier than the app's settings loader.
pub fn rendering_setting(identifier: &str) -> String {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")));
    let value = base
        .and_then(|b| std::fs::read_to_string(b.join(identifier).join("settings.json")).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("rendering")?.as_str().map(str::to_string))
        .unwrap_or_default();
    sanitize_rendering(&value)
}

fn set_default_env(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

/// Software rendering for old GPUs or on request: Mesa's software rasteriser
/// (llvmpipe, full OpenGL) and WebKit's simpler paths. Values the user set are
/// respected. Call before the first webview exists.
pub fn apply_software_rendering(identifier: &str) {
    let cards = drm_cards(Path::new("/sys/class/drm"));
    let legacy = legacy_gpu(&cards);
    LEGACY.store(legacy, Ordering::Relaxed);
    if !use_software_rendering(&rendering_setting(identifier), &cards) {
        return;
    }
    SOFTWARE.store(true, Ordering::Relaxed);
    set_default_env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    set_default_env("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    set_default_env("LIBGL_ALWAYS_SOFTWARE", "1");
}

/// GStreamer's hardware (VA-API) video decoders, old and new plugin names.
const VA_DECODERS: &[&str] = &[
    "vah264dec",
    "vah265dec",
    "vavp8dec",
    "vavp9dec",
    "vaav1dec",
    "vampeg2dec",
    "vaapih264dec",
    "vaapih265dec",
    "vaapivp8dec",
    "vaapivp9dec",
    "vaapiav1dec",
    "vaapimpeg2dec",
    "vaapidecodebin",
];

/// The `GST_PLUGIN_FEATURE_RANK` value that turns the VA-API decoders off.
fn va_decoders_off() -> String {
    VA_DECODERS
        .iter()
        .map(|d| format!("{d}:NONE"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Decode video in software inside the webview. Frames from the VA-API
/// decoders come out black or transparent when WebKitGTK draws them on a
/// canvas, which is how WhatsApp makes the thumbnail of a video being sent
/// (black thumbnail, or 100 retries and a failed attachment). Software decoding
/// gives correct frames; WhatsApp videos are small, so the cost is low. A value
/// the user set is respected, so `GST_PLUGIN_FEATURE_RANK=` restores the
/// hardware decoders.
pub fn apply_gstreamer_env() {
    if std::env::var_os("GST_PLUGIN_FEATURE_RANK").is_none() {
        std::env::set_var("GST_PLUGIN_FEATURE_RANK", va_decoders_off());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        disable_dmabuf, drm_cards, drm_vendors, legacy_gpu, sanitize_rendering,
        use_software_rendering, va_decoders_off,
    };

    fn card(vendor: &str, device: &str) -> (String, String) {
        (vendor.to_string(), device.to_string())
    }

    #[test]
    fn ironlake_is_legacy() {
        // ThinkPad X201i from issue #87: Intel HD Graphics (ILK), OpenGL 2.1.
        assert!(legacy_gpu(&[card("0x8086", "0x0046")]));
        assert!(legacy_gpu(&[card("0x8086", "0x2a42")])); // GM45
    }

    #[test]
    fn current_gpus_are_not_legacy() {
        assert!(!legacy_gpu(&[card("0x8086", "0x9bc4")])); // UHD (Comet Lake)
        assert!(!legacy_gpu(&[card("0x8086", "0x0126")])); // Sandy Bridge: GL 3.3
        assert!(!legacy_gpu(&[card("0x1002", "0x0046")])); // AMD, same device number
        assert!(!legacy_gpu(&[
            card("0x8086", "0x0046"),
            card("0x10de", "0x1cbc")
        ]));
        assert!(!legacy_gpu(&[card("0x8086", "junk")]));
        assert!(!legacy_gpu(&[]));
    }

    #[test]
    fn software_rendering_by_setting_or_gpu() {
        let new = [card("0x8086", "0x9bc4")];
        let old = [card("0x8086", "0x0042")];
        assert!(!use_software_rendering("auto", &new));
        assert!(use_software_rendering("software", &new));
        assert!(use_software_rendering("auto", &old));
        assert_eq!(sanitize_rendering("software"), "software");
        assert_eq!(sanitize_rendering("gpu"), "auto");
        assert_eq!(sanitize_rendering(""), "auto");
    }

    #[test]
    fn drm_cards_reads_vendor_and_device() {
        let root = std::env::temp_dir().join(format!("wr-drm-cards-{}", std::process::id()));
        let dev = root.join("card0/device");
        std::fs::create_dir_all(&dev).unwrap();
        std::fs::write(dev.join("vendor"), "0x8086\n").unwrap();
        std::fs::write(dev.join("device"), "0x0046\n").unwrap();
        assert_eq!(drm_cards(&root), vec![card("0x8086", "0x0046")]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn v(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mesa_only_keeps_dmabuf() {
        assert!(!disable_dmabuf(false, &v(&["0x8086"])));
        assert!(!disable_dmabuf(false, &v(&["0x1002"])));
    }

    #[test]
    fn hybrid_laptop_keeps_dmabuf() {
        // Intel drives the screen; NVIDIA module loaded, with or without its
        // own DRM card (nvidia-drm modeset).
        assert!(!disable_dmabuf(true, &v(&["0x8086"])));
        assert!(!disable_dmabuf(true, &v(&["0x8086", "0x10de"])));
        assert!(!disable_dmabuf(true, &v(&["0x10de", "0x1002"])));
    }

    #[test]
    fn nvidia_only_disables_dmabuf() {
        assert!(disable_dmabuf(true, &v(&["0x10de"])));
        assert!(disable_dmabuf(true, &v(&["0x10DE\n"])));
    }

    #[test]
    fn unreadable_drm_with_nvidia_stays_safe() {
        assert!(disable_dmabuf(true, &[]));
    }

    #[test]
    fn drm_vendors_reads_cards_and_skips_connectors() {
        let dir = std::env::temp_dir().join(format!("wr-drm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (card, vendor) in [("card0", "0x10de\n"), ("card1", "0x8086\n")] {
            std::fs::create_dir_all(dir.join(card).join("device")).unwrap();
            std::fs::write(dir.join(card).join("device/vendor"), vendor).unwrap();
        }
        std::fs::create_dir_all(dir.join("card1-eDP-1/device")).unwrap();
        std::fs::write(dir.join("card1-eDP-1/device/vendor"), "0x9999").unwrap();
        std::fs::create_dir_all(dir.join("renderD128")).unwrap();
        let mut got = drm_vendors(&dir);
        got.sort();
        assert_eq!(got, vec!["0x10de".to_string(), "0x8086".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn va_decoders_are_all_ranked_none() {
        let v = va_decoders_off();
        for d in [
            "vah264dec",
            "vah265dec",
            "vavp9dec",
            "vaav1dec",
            "vaapih264dec",
        ] {
            assert!(v.split(',').any(|e| e == format!("{d}:NONE")), "{d}");
        }
        assert!(v
            .split(',')
            .all(|e| e.ends_with(":NONE") && !e.contains(' ')));
    }
}
