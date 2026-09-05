use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub autostart: bool,
    pub hotkey_enabled: bool,
    pub hotkey: String,
    pub notifications: bool,
    /// Display zoom for the WhatsApp webview, 1.0 = the site's own sizing.
    pub zoom: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            start_minimized: false,
            autostart: false,
            hotkey_enabled: true,
            hotkey: "CmdOrCtrl+Shift+W".to_string(),
            notifications: true,
            zoom: 1.0,
        }
    }
}

/// Zoom bounds. The settings UI only offers four presets inside this range; the
/// clamp exists so a hand-edited settings.json cannot leave the app at a zoom
/// level from which the settings window is unreadable.
pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;

/// Bring a zoom factor into range. A non-finite value (a `null` or a string in
/// the JSON deserialises to the default, but arithmetic in an older build could
/// still have written a NaN) falls back to 1.0 rather than to a bound.
pub fn sanitize_zoom(zoom: f64) -> f64 {
    if zoom.is_finite() {
        zoom.clamp(ZOOM_MIN, ZOOM_MAX)
    } else {
        1.0
    }
}

impl Settings {
    /// Repair values an older or hand-edited settings.json can carry.
    pub fn sanitized(mut self) -> Self {
        self.zoom = sanitize_zoom(self.zoom);
        self
    }
}

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "linux")]
fn is_valid_flatpak_id(id: &str) -> bool {
    id.len() <= 255
        && id.split('.').count() >= 3
        && id.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn flatpak_id() -> Option<String> {
    std::env::var("FLATPAK_ID")
        .ok()
        .filter(|id| is_valid_flatpak_id(id))
}

#[cfg(target_os = "linux")]
fn flatpak_autostart_entry(flatpak_id: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=whatRust\n\
         Comment=Start whatRust minimized\n\
         Exec=flatpak run --command=whatrust {flatpak_id} --minimized\n\
         Icon={flatpak_id}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n\
         X-Flatpak={flatpak_id}\n"
    )
}

#[cfg(target_os = "linux")]
fn apply_flatpak_autostart(flatpak_id: &str, enabled: bool) -> std::io::Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?;
    let dir = home.join(".config/autostart");
    let path = dir.join(format!("{flatpak_id}.desktop"));
    if enabled {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(path, flatpak_autostart_entry(flatpak_id))
    } else if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
}

fn settings_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .map(Settings::sanitized)
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(s).expect("serialize settings");
    std::fs::write(path, json)?;
    Ok(())
}

/// Apply side effects of settings (autostart + global shortcut). Returns any
/// non-fatal side-effect failures as one warning string; persisted settings and
/// zoom changes are still applied.
pub fn apply(app: &AppHandle, s: &Settings) -> Option<String> {
    // Zoom is a webview property, so it applies on every platform and takes
    // effect on the open account windows without a reload.
    crate::window::apply_zoom_all(app, s.zoom);

    #[cfg(desktop)]
    {
        let mut warnings = Vec::new();

        #[cfg(target_os = "linux")]
        let flatpak_autostart_handled = flatpak_id()
            .map(|id| {
                if let Err(e) = apply_flatpak_autostart(&id, s.autostart) {
                    warnings.push(format!("autostart could not be updated: {e}"));
                }
                true
            })
            .unwrap_or(false);
        #[cfg(not(target_os = "linux"))]
        let flatpak_autostart_handled = false;

        if !flatpak_autostart_handled {
            use tauri_plugin_autostart::ManagerExt;
            let autostart = app.autolaunch();
            let result =
                if s.autostart {
                    autostart.enable()
                } else {
                    // Windows reports NotFound when deleting an absent Run value.
                    // Saving "off" when already off is a successful no-op.
                    autostart.is_enabled().and_then(|enabled| {
                        if enabled {
                            autostart.disable()
                        } else {
                            Ok(())
                        }
                    })
                };
            if let Err(e) = result {
                warnings.push(format!("autostart could not be updated: {e}"));
            }
        }

        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if s.hotkey_enabled && !s.hotkey.trim().is_empty() {
            if let Err(e) = gs.register(s.hotkey.as_str()) {
                warnings.push(format!("shortcut not registered (it may be in use): {e}"));
            }
        }
        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        }
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, s);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_zoom, Settings, ZOOM_MAX, ZOOM_MIN};

    #[cfg(target_os = "linux")]
    use super::{flatpak_autostart_entry, is_valid_flatpak_id};

    #[cfg(target_os = "linux")]
    #[test]
    fn flatpak_id_validation_blocks_path_and_desktop_entry_injection() {
        assert!(is_valid_flatpak_id("io.github.karem505.whatRust"));
        assert!(!is_valid_flatpak_id("../../autostart"));
        assert!(!is_valid_flatpak_id("io.github.karem505.whatRust\nExec=sh"));
        assert!(!is_valid_flatpak_id("io..whatRust"));
        assert!(!is_valid_flatpak_id("io.github.505whatRust"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flatpak_autostart_entry_reenters_the_sandbox() {
        let entry = flatpak_autostart_entry("io.github.karem505.whatRust");
        assert!(entry.contains(
            "Exec=flatpak run --command=whatrust io.github.karem505.whatRust --minimized"
        ));
        assert!(entry.contains("Icon=io.github.karem505.whatRust"));
        assert!(entry.contains("X-Flatpak=io.github.karem505.whatRust"));
        assert!(!entry.contains("Exec=/app/bin/whatrust"));
    }

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert!(s.close_to_tray);
        assert!(s.notifications);
        assert_eq!(s.hotkey, "CmdOrCtrl+Shift+W");
        assert!(!s.autostart);
        assert_eq!(s.zoom, 1.0);
    }

    #[test]
    fn zoom_presets_survive_sanitizing() {
        for z in [0.75, 0.85, 1.0, 1.15] {
            assert_eq!(sanitize_zoom(z), z);
        }
    }

    #[test]
    fn out_of_range_zoom_is_clamped() {
        assert_eq!(sanitize_zoom(0.01), ZOOM_MIN);
        assert_eq!(sanitize_zoom(9.0), ZOOM_MAX);
        assert_eq!(sanitize_zoom(-1.0), ZOOM_MIN);
    }

    #[test]
    fn non_finite_zoom_falls_back_to_unscaled() {
        assert_eq!(sanitize_zoom(f64::NAN), 1.0);
        assert_eq!(sanitize_zoom(f64::INFINITY), 1.0);
    }

    #[test]
    fn zoom_from_json_is_sanitized() {
        let s: Settings = serde_json::from_str(r#"{"zoom": 12}"#).unwrap();
        assert_eq!(s.sanitized().zoom, ZOOM_MAX);
        // Absent in a settings.json written by an older build.
        let s: Settings = serde_json::from_str(r#"{"autostart": true}"#).unwrap();
        assert_eq!(s.sanitized().zoom, 1.0);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"autostart": true}"#).unwrap();
        assert!(s.autostart);
        assert!(s.close_to_tray);
        assert_eq!(s.hotkey, "CmdOrCtrl+Shift+W");
    }

    #[test]
    fn empty_json_is_all_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn roundtrip() {
        let s = Settings {
            autostart: true,
            hotkey: "Ctrl+Alt+W".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
