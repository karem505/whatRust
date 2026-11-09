//! Linux GPU check for WebKitGTK's DMA-BUF renderer (issue #51).
//!
//! The DMA-BUF renderer keeps scrolling and typing smooth, but it has crashed
//! or rendered blank when NVIDIA's proprietary driver drives the display, so
//! whatRust turns it off there. A hybrid laptop (Intel/AMD drives the screen,
//! the NVIDIA GPU sits idle for offload) also has `/proc/driver/nvidia/version`,
//! and turning the renderer off there made scrolling sluggish for no reason.
//! It is now turned off only when every display GPU is NVIDIA.

use std::path::Path;

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
        if let Ok(v) = std::fs::read_to_string(entry.path().join("device/vendor")) {
            out.push(v.trim().to_string());
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

#[cfg(test)]
mod tests {
    use super::{disable_dmabuf, drm_vendors};

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
}
