//! Window title bar on Linux tiling window managers (Hyprland, Sway, i3, ...).
//!
//! Tiling compositors don't draw title bars, and on Wayland Hyprland does not
//! offer server-side decorations at all, so GTK falls back to drawing its own
//! header bar with minimise/maximise/close buttons on every whatRust window.
//! Those buttons do nothing useful there (the compositor owns window size and
//! placement) and look out of place next to every other app, which shows none.
//!
//! Setting `title_bar`: `auto` (default) hides the bar when a tiling window
//! manager is detected, `show` and `hide` force it. Linux only; on macOS and
//! Windows the native title bar is always kept. The lock screen is always
//! undecorated and is left alone.

use tauri::{AppHandle, Manager};

pub const AUTO: &str = "auto";
pub const SHOW: &str = "show";
pub const HIDE: &str = "hide";

/// Desktop names (as in `XDG_CURRENT_DESKTOP`) of tiling window managers.
const TILING: &[&str] = &[
    "hyprland",
    "sway",
    "swayfx",
    "river",
    "niri",
    "i3",
    "bspwm",
    "qtile",
    "dwm",
    "dwl",
    "awesome",
    "herbstluftwm",
    "xmonad",
    "leftwm",
    "spectrwm",
    "miracle-wm",
];

/// Sockets/variables only a given tiling compositor sets, with its name.
const SOCKETS: &[(&str, &str)] = &[
    ("HYPRLAND_INSTANCE_SIGNATURE", "Hyprland"),
    ("SWAYSOCK", "Sway"),
    ("NIRI_SOCKET", "niri"),
    ("I3SOCK", "i3"),
];

/// The tiling window manager this session runs, if any (for the settings
/// page and the decision below). `get` reads an environment variable.
fn detect_with(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    for (var, name) in SOCKETS {
        if get(var).is_some_and(|v| !v.trim().is_empty()) {
            return Some((*name).to_string());
        }
    }
    for var in [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ] {
        let Some(value) = get(var) else { continue };
        for part in value.split([':', ';']) {
            let part = part.trim();
            if TILING.iter().any(|t| part.eq_ignore_ascii_case(t)) {
                return Some(part.to_string());
            }
        }
    }
    None
}

/// The tiling window manager of this session (Linux), `None` elsewhere.
pub fn tiling_wm() -> Option<String> {
    if cfg!(target_os = "linux") {
        detect_with(|k| std::env::var(k).ok())
    } else {
        None
    }
}

/// Should windows have a title bar, for this setting and detection result?
fn show_for(setting: &str, tiling: bool, linux: bool) -> bool {
    if !linux {
        return true;
    }
    match setting {
        SHOW => true,
        HIDE => false,
        _ => !tiling,
    }
}

/// Normalise a saved value: anything unknown means `auto`.
pub fn sanitize(value: &str) -> String {
    match value {
        SHOW | HIDE => value.to_string(),
        _ => AUTO.to_string(),
    }
}

/// Title bar on for new windows, from the saved setting.
pub fn show(app: &AppHandle) -> bool {
    show_for(
        &crate::settings::load(app).title_bar,
        tiling_wm().is_some(),
        cfg!(target_os = "linux"),
    )
}

/// Apply a (changed) setting to every open window except the lock screen.
pub fn apply_all(app: &AppHandle, setting: &str) {
    let on = show_for(setting, tiling_wm().is_some(), cfg!(target_os = "linux"));
    for (label, w) in app.windows() {
        if label != "lock" && w.is_decorated().ok() != Some(on) {
            let _ = w.set_decorations(on);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_with, sanitize, show_for};
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn hyprland_is_detected_by_its_socket_or_desktop_name() {
        assert_eq!(
            detect_with(env(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc_123")])).as_deref(),
            Some("Hyprland")
        );
        assert_eq!(
            detect_with(env(&[("XDG_CURRENT_DESKTOP", "Hyprland")])).as_deref(),
            Some("Hyprland")
        );
    }

    #[test]
    fn other_tiling_managers_are_detected() {
        assert!(detect_with(env(&[("SWAYSOCK", "/run/user/1000/sway.sock")])).is_some());
        assert!(detect_with(env(&[("XDG_CURRENT_DESKTOP", "niri")])).is_some());
        assert!(detect_with(env(&[("DESKTOP_SESSION", "i3")])).is_some());
        assert!(detect_with(env(&[("XDG_CURRENT_DESKTOP", "river")])).is_some());
    }

    #[test]
    fn stacking_desktops_keep_their_title_bar() {
        for d in [
            "GNOME",
            "ubuntu:GNOME",
            "KDE",
            "XFCE",
            "X-Cinnamon",
            "COSMIC",
            "LXQt",
        ] {
            assert!(
                detect_with(env(&[("XDG_CURRENT_DESKTOP", d)])).is_none(),
                "{d}"
            );
        }
        assert!(detect_with(env(&[])).is_none());
        assert!(detect_with(env(&[("HYPRLAND_INSTANCE_SIGNATURE", "  ")])).is_none());
    }

    #[test]
    fn auto_hides_only_on_tiling_and_overrides_win() {
        assert!(!show_for("auto", true, true));
        assert!(show_for("auto", false, true));
        assert!(show_for("show", true, true));
        assert!(!show_for("hide", false, true));
    }

    #[test]
    fn macos_and_windows_always_keep_the_native_title_bar() {
        assert!(show_for("hide", true, false));
        assert!(show_for("auto", true, false));
    }

    #[test]
    fn unknown_values_mean_auto() {
        assert_eq!(sanitize("show"), "show");
        assert_eq!(sanitize("hide"), "hide");
        assert_eq!(sanitize(""), "auto");
        assert_eq!(sanitize("bogus"), "auto");
    }
}
