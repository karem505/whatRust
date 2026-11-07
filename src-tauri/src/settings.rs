use crate::i18n::trf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub autostart: bool,
    pub hotkey_enabled: bool,
    pub hotkey: String,
    /// Global shortcut that toggles the privacy blur in every account window.
    /// Off by default: a new global shortcut must not grab a key combination
    /// (Ctrl+Shift+B is the browsers' bookmarks bar) from users who never asked.
    pub blur_hotkey_enabled: bool,
    pub blur_hotkey: String,
    pub notifications: bool,
    pub notify_when_focused: bool,
    pub notify_active_chat: bool,
    pub respect_muted: bool,
    /// macOS only: the app lives in the menu bar, no Dock icon and no
    /// standard window. Every account stays connected in the background and
    /// opens as a panel from the tray. Off by default. Ignored on other platforms.
    pub menubar_only: bool,
    /// Linux/macOS: host every account in one window with a switcher instead
    /// of one window per account. Off by default; applies after a restart.
    /// Windows always uses a single window. Menu-bar-only mode wins if both
    /// are on.
    pub single_window: bool,
    /// macOS only: hide the Dock icon but keep the normal main window. The
    /// menu bar icon is forced on, since it becomes the way back to the app.
    pub hide_dock_icon: bool,
    /// Display zoom for the WhatsApp webview, 1.0 = the site's own sizing.
    pub zoom: f64,
    /// macOS only (issue #44): show the icon in the menu bar. With it off, the
    /// Dock icon brings a hidden window back. Windows and Linux always show the
    /// tray icon, since there it is the only way back to a hidden window.
    pub menu_bar_icon: bool,
    /// Interface language: `en` or `pt-BR`. Defaults to the OS language
    /// (Portuguese for a Portuguese system, English otherwise).
    pub language: String,
    /// Folder downloads are saved to (issue #40). `None` keeps the OS Downloads
    /// folder. Checked when saved and again before each download; a folder that
    /// has gone away or is no longer writable falls back to Downloads.
    pub download_dir: Option<String>,
    /// Use the PNG copied into the config dir as the app icon (issue #54).
    pub custom_icon: bool,
    /// Network proxy for the WhatsApp webviews: `off`, `socks5` or `http`
    /// (issue #52). Applied when a webview is created, so changes need a restart.
    pub proxy_mode: String,
    pub proxy_host: String,
    pub proxy_port: u32,
    /// Look for a newer release on GitHub shortly after start and every six
    /// hours (api.github.com). On by default.
    pub auto_update_check: bool,
    /// Download and install a newer release without asking, where that needs no
    /// password (AppImage, macOS app: replaced in place; Windows: downloaded and
    /// verified, installed on a click). On by default; never for .deb or Flatpak.
    pub auto_update_install: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            start_minimized: false,
            autostart: false,
            hotkey_enabled: true,
            hotkey: "CmdOrCtrl+Shift+W".to_string(),
            blur_hotkey_enabled: false,
            blur_hotkey: "CmdOrCtrl+Shift+B".to_string(),
            notifications: true,
            notify_when_focused: true,
            notify_active_chat: true,
            respect_muted: true,
            menubar_only: false,
            single_window: false,
            hide_dock_icon: false,
            zoom: 1.0,
            menu_bar_icon: true,
            language: crate::i18n::system_default().to_string(),
            download_dir: None,
            custom_icon: false,
            proxy_mode: crate::proxy::MODE_OFF.to_string(),
            proxy_host: String::new(),
            proxy_port: 0,
            auto_update_check: true,
            auto_update_install: true,
        }
    }
}

/// Do two accelerator strings name the same key combination? Compared after
/// parsing, so "CmdOrCtrl+Shift+B" and "Shift+CmdOrCtrl+B" match. Unparseable
/// strings never match (registration reports those separately).
#[cfg(desktop)]
pub fn same_shortcut(a: &str, b: &str) -> bool {
    use tauri_plugin_global_shortcut::Shortcut;
    match (a.trim().parse::<Shortcut>(), b.trim().parse::<Shortcut>()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Is `pressed` the privacy-blur shortcut (enabled, valid, and distinct from
/// the show/hide shortcut, which always wins a tie)?
#[cfg(desktop)]
pub fn is_blur_shortcut(s: &Settings, pressed: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    s.blur_hotkey_enabled
        && !same_shortcut(&s.blur_hotkey, &s.hotkey)
        && s.blur_hotkey
            .trim()
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .is_ok_and(|b| &b == pressed)
}

pub fn start_hidden(preference: bool, args: &[String]) -> bool {
    if args.iter().any(|a| a == "--show") {
        return false;
    }
    if cfg!(windows) {
        // A manual launch from Start or a desktop shortcut has no startup flag
        // and should always show the window. Apply the preference only to OS
        // startup. Accept --minimized for older Run entries, but still respect
        // the saved preference; new entries use the explicit flag below.
        preference
            && args
                .iter()
                .any(|a| a == "--autostart" || a == "--minimized")
    } else {
        preference || args.iter().any(|a| a == "--minimized")
    }
}

/// Zoom bounds. The settings UI only offers four presets inside this range; the
/// clamp exists so a hand-edited settings.json cannot leave the app at a zoom
/// level from which the settings window is unreadable.
pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;

/// Levels the zoom shortcuts step through (Ctrl +/-, Ctrl+wheel), all inside
/// `ZOOM_MIN..=ZOOM_MAX`. They include the four presets of the Display size
/// control, so shortcut and dropdown agree where they overlap.
pub const ZOOM_STEPS: [f64; 10] = [0.5, 0.67, 0.75, 0.85, 1.0, 1.15, 1.25, 1.5, 1.75, 2.0];

/// What a zoom shortcut asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomAction {
    In,
    Out,
    Reset,
}

impl ZoomAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "in" => Some(Self::In),
            "out" => Some(Self::Out),
            "reset" => Some(Self::Reset),
            _ => None,
        }
    }
}

/// The zoom after one shortcut step. Steps to the next level strictly beyond
/// the current value, so an off-grid value (hand-edited, or from an older
/// build) lands on a level instead of skipping one; at the ends it stays put.
pub fn step_zoom(current: f64, action: ZoomAction) -> f64 {
    let cur = sanitize_zoom(current);
    const EPS: f64 = 1e-6;
    match action {
        ZoomAction::Reset => 1.0,
        ZoomAction::In => ZOOM_STEPS
            .iter()
            .copied()
            .find(|z| *z > cur + EPS)
            .unwrap_or(ZOOM_MAX),
        ZoomAction::Out => ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < cur - EPS)
            .unwrap_or(ZOOM_MIN),
    }
}

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
        self.language = crate::i18n::sanitize_code(&self.language);
        self.download_dir = self.download_dir.filter(|d| !d.trim().is_empty());
        // Menu-bar-only mode needs WKWebView's inactive-scheduling policy
        // (macOS 14+). Dropping the flag here means a settings file or an
        // older build can never leave the app in a mode it cannot sustain.
        #[cfg(target_os = "macos")]
        if !crate::menubar::supported() {
            self.menubar_only = false;
        }
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
    use crate::jsonfile::Loaded;
    let Ok(path) = settings_path(app) else {
        return Settings::default();
    };
    match crate::jsonfile::read::<Settings>(&path) {
        Loaded::Ok(s) | Loaded::Recovered(s) => s.sanitized(),
        Loaded::Missing => Settings::default(),
        Loaded::Corrupt => {
            crate::jsonfile::preserve_corrupt(&path);
            Settings::default()
        }
    }
}

pub fn save(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let path = settings_path(app)?;
    crate::jsonfile::save(&path, s)?;
    Ok(())
}

/// Apply side effects of settings (autostart + global shortcut + the macOS menu bar).
/// Returns any non-fatal side-effect failures as one warning string;
/// persisted settings and zoom changes are still applied.
pub fn apply(app: &AppHandle, s: &Settings) -> Option<String> {
    // Zoom is a webview property, so it applies on every platform and takes
    // effect on the open account windows without a reload.
    let lang = crate::i18n::Lang::from_code(&s.language);
    if lang != crate::i18n::current() {
        crate::i18n::set(lang);
        crate::i18n::refresh(app);
    }
    crate::window::apply_zoom_all(app, s.zoom);
    crate::appicon::apply(app, s.custom_icon, false);

    // Menu-bar mode is only available where a hidden WKWebView keeps running
    // (macOS 14+); elsewhere the setting is ignored rather than dropping every
    // account's session the moment it is enabled.
    #[cfg(target_os = "macos")]
    // Single-window mode keeps its window set for the whole run, so a live
    // menu-bar toggle only takes effect after a restart (menu-bar wins then).
    let menubar_only = s.menubar_only && crate::menubar::supported() && !crate::sessions::enabled();
    #[cfg(target_os = "macos")]
    let hide_dock = menubar_only || s.hide_dock_icon;

    // In menu-bar-only mode the tray icon is the app's only entry point, so
    // it can never be hidden.
    #[cfg(target_os = "macos")]
    crate::tray::apply_visibility(app, s.menu_bar_icon || hide_dock);
    #[cfg(not(target_os = "macos"))]
    crate::tray::apply_visibility(app, s.menu_bar_icon);

    // macOS menu-bar mode: reconcile the live window set with the settings.
    #[cfg(target_os = "macos")]
    {
        if menubar_only {
            crate::menubar::enter_only(app);
        } else if crate::menubar::leave_only(app) {
            // Leaving menu-bar-only mode: reopen missing windows and show the
            // active one, so the app is never left with no visible window.
            for a in crate::accounts::load(app).accounts {
                let label = crate::accounts::window_label(&a.id);
                if app.get_webview_window(&label).is_none() {
                    let _ = crate::window::open_account_window(app, &a, true);
                }
            }
            crate::window::show_active(app);
        }

        // Menu-bar-only apps have no Dock presence.
        let policy = if hide_dock {
            tauri::ActivationPolicy::Accessory
        } else {
            tauri::ActivationPolicy::Regular
        };
        let h = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = h.set_activation_policy(policy);
        });

        // Hold the App Nap shield only while menu-bar-only mode is on.
        crate::menubar::set_keep_alive(menubar_only);
    }

    #[cfg(desktop)]
    {
        let mut warnings = Vec::new();

        #[cfg(target_os = "linux")]
        let flatpak_autostart_handled = flatpak_id()
            .map(|id| {
                if let Err(e) = apply_flatpak_autostart(&id, s.autostart) {
                    warnings.push(trf(
                        "Não foi possível atualizar a inicialização automática: {0}",
                        &[&e],
                    ));
                }
                true
            })
            .unwrap_or(false);
        #[cfg(not(target_os = "linux"))]
        let flatpak_autostart_handled = false;

        if !flatpak_autostart_handled {
            use tauri_plugin_autostart::ManagerExt;
            let autostart = app.autolaunch();
            let result = if s.autostart {
                autostart.enable()
            } else {
                // Windows reports NotFound when removing a Run entry that isn't
                // there, so turning off what is already off would warn.
                autostart
                    .is_enabled()
                    .and_then(|on| if on { autostart.disable() } else { Ok(()) })
            };
            if let Err(e) = result {
                warnings.push(trf(
                    "Não foi possível atualizar a inicialização automática: {0}",
                    &[&e],
                ));
            }
        }

        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if s.hotkey_enabled && !s.hotkey.trim().is_empty() {
            if let Err(e) = gs.register(s.hotkey.as_str()) {
                warnings.push(trf(
                    "Não foi possível registrar o atalho (ele pode estar em uso): {0}",
                    &[&e],
                ));
            }
        }
        if s.blur_hotkey_enabled && !s.blur_hotkey.trim().is_empty() {
            if same_shortcut(&s.blur_hotkey, &s.hotkey) {
                warnings.push(trf(
                    "O atalho para desfocar não pode ser igual ao atalho de mostrar/ocultar: {0}",
                    &[&s.blur_hotkey],
                ));
            } else if let Err(e) = gs.register(s.blur_hotkey.as_str()) {
                warnings.push(trf(
                    "Não foi possível registrar o atalho (ele pode estar em uso): {0}",
                    &[&e],
                ));
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
    use super::{sanitize_zoom, step_zoom, Settings, ZoomAction, ZOOM_MAX, ZOOM_MIN, ZOOM_STEPS};

    #[cfg(target_os = "linux")]
    use super::{flatpak_autostart_entry, is_valid_flatpak_id};

    #[test]
    fn old_settings_keep_default_download_folder_and_icon() {
        let s: Settings = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
        assert_eq!(s.download_dir, None);
        assert!(!s.custom_icon);
        let blank = Settings {
            download_dir: Some("  ".into()),
            ..Settings::default()
        };
        assert_eq!(blank.sanitized().download_dir, None);
    }

    #[test]
    fn old_settings_enable_new_notification_preferences() {
        let s: Settings = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
        assert!(!s.notifications);
        assert!(s.notify_when_focused && s.notify_active_chat && s.respect_muted);
    }

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
        // The menu-bar mode is opt-in.
        assert!(!s.menubar_only);
        assert_eq!(s.hotkey, "CmdOrCtrl+Shift+W");
        assert!(!s.autostart);
        assert_eq!(s.zoom, 1.0);
        assert!(s.menu_bar_icon);
    }

    #[test]
    fn zoom_steps_are_in_range_and_ascending() {
        assert!(ZOOM_STEPS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(ZOOM_STEPS[0], ZOOM_MIN);
        assert_eq!(ZOOM_STEPS[ZOOM_STEPS.len() - 1], ZOOM_MAX);
        assert!(ZOOM_STEPS.contains(&1.0));
    }

    #[test]
    fn zoom_in_and_out_walk_the_levels() {
        assert_eq!(step_zoom(1.0, ZoomAction::In), 1.15);
        assert_eq!(step_zoom(1.0, ZoomAction::Out), 0.85);
        assert_eq!(step_zoom(1.15, ZoomAction::Out), 1.0);
        assert_eq!(step_zoom(0.75, ZoomAction::In), 0.85);
        assert_eq!(step_zoom(1.5, ZoomAction::Reset), 1.0);
    }

    #[test]
    fn zoom_stops_at_the_ends() {
        assert_eq!(step_zoom(ZOOM_MAX, ZoomAction::In), ZOOM_MAX);
        assert_eq!(step_zoom(ZOOM_MIN, ZoomAction::Out), ZOOM_MIN);
    }

    #[test]
    fn off_grid_zoom_lands_on_the_next_level() {
        assert_eq!(step_zoom(1.05, ZoomAction::In), 1.15);
        assert_eq!(step_zoom(1.05, ZoomAction::Out), 1.0);
        assert_eq!(step_zoom(f64::NAN, ZoomAction::In), 1.15);
        assert_eq!(step_zoom(9.0, ZoomAction::Out), 1.75);
    }

    #[test]
    fn zoom_action_parses_only_known_names() {
        assert_eq!(ZoomAction::parse("in"), Some(ZoomAction::In));
        assert_eq!(ZoomAction::parse("sideways"), None);
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
        // Absent in a settings.json written before issue #44: keep the icon.
        assert!(s.menu_bar_icon);
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
    #[test]
    fn explicit_foreground_launch_overrides_background_preference() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(super::start_hidden(true, &args(&[])), !cfg!(windows));
        assert_eq!(
            super::start_hidden(false, &args(&["--minimized"])),
            !cfg!(windows)
        );
        assert!(!super::start_hidden(true, &args(&["--show"])));
        assert!(!super::start_hidden(
            true,
            &args(&["--show", "--minimized"])
        ));
        assert!(!super::start_hidden(false, &args(&[])));
    }

    #[cfg(windows)]
    #[test]
    fn start_minimized_only_affects_system_autostart() {
        assert!(!super::start_hidden(true, &[]));
        assert!(super::start_hidden(true, &["--autostart".into()]));
        assert!(!super::start_hidden(false, &["--autostart".into()]));
        assert!(!super::start_hidden(false, &["--minimized".into()]));
        assert!(!super::start_hidden(
            true,
            &["--autostart".into(), "--show".into()]
        ));
    }
}

#[cfg(all(test, desktop))]
mod blur_shortcut_tests {
    use super::{is_blur_shortcut, same_shortcut, Settings};
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn blur_shortcut_is_off_by_default() {
        let s = Settings::default();
        assert!(!s.blur_hotkey_enabled);
        let b: Shortcut = s.blur_hotkey.parse().unwrap();
        assert!(!is_blur_shortcut(&s, &b));
    }

    #[test]
    fn enabled_blur_shortcut_matches_only_itself() {
        let s = Settings {
            blur_hotkey_enabled: true,
            ..Settings::default()
        };
        let blur: Shortcut = "CmdOrCtrl+Shift+B".parse().unwrap();
        let toggle: Shortcut = s.hotkey.parse().unwrap();
        assert!(is_blur_shortcut(&s, &blur));
        assert!(!is_blur_shortcut(&s, &toggle));
    }

    #[test]
    fn show_hide_shortcut_wins_a_tie() {
        let s = Settings {
            blur_hotkey_enabled: true,
            blur_hotkey: "Shift+CmdOrCtrl+W".into(),
            ..Settings::default()
        };
        assert!(same_shortcut(&s.blur_hotkey, &s.hotkey));
        let pressed: Shortcut = s.hotkey.parse().unwrap();
        assert!(!is_blur_shortcut(&s, &pressed));
    }

    #[test]
    fn unparseable_shortcuts_never_match() {
        assert!(!same_shortcut("not a key", "not a key"));
    }
}
