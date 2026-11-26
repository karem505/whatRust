//! Settings → Appearance → Theme: `system` (default), `light` or `dark`.
//!
//! `system` keeps today's behaviour: whatRust's own pages (settings, shell,
//! dialogs, lock screen) follow the theme WhatsApp Web reports for the active
//! account (see `integration.js`), falling back to the OS colour scheme.
//!
//! `light` / `dark` set the app-wide native theme, which the webviews expose
//! as `prefers-color-scheme`. WhatsApp Web follows it when its own theme is
//! "System default"; whatRust's pages always follow it.

use std::sync::Mutex;
use tauri::{AppHandle, Theme};

pub const SYSTEM: &str = "system";
pub const LIGHT: &str = "light";
pub const DARK: &str = "dark";

/// Normalise a saved value: anything unknown means `system`.
pub fn sanitize(value: &str) -> String {
    match value {
        LIGHT | DARK => value.to_string(),
        _ => SYSTEM.to_string(),
    }
}

/// The native theme for a setting; `None` follows the OS.
fn native(setting: &str) -> Option<Theme> {
    match setting {
        LIGHT => Some(Theme::Light),
        DARK => Some(Theme::Dark),
        _ => None,
    }
}

/// Theme for whatRust's own pages (`data-theme`): a forced setting wins over
/// the theme detected in WhatsApp's page; with neither, `system` lets the
/// pages' CSS follow `prefers-color-scheme`.
pub fn resolve(setting: &str, detected: Option<String>) -> String {
    match setting {
        LIGHT | DARK => setting.to_string(),
        _ => detected.unwrap_or_else(|| SYSTEM.into()),
    }
}

/// The setting last handed to the runtime. Starts at `system`, so users who
/// never change it get no native call at all (today's behaviour).
static APPLIED: Mutex<Option<String>> = Mutex::new(None);

/// The setting in effect (`system` until `apply` runs at startup).
pub fn current() -> String {
    crate::lock::guard(&APPLIED)
        .clone()
        .unwrap_or_else(|| SYSTEM.into())
}

/// Apply a (changed) setting to every window, now and for windows opened later.
pub fn apply(app: &AppHandle, setting: &str) {
    let setting = sanitize(setting);
    {
        let mut applied = crate::lock::guard(&APPLIED);
        if applied.as_deref().unwrap_or(SYSTEM) == setting {
            return;
        }
        *applied = Some(setting.clone());
    }
    app.set_theme(native(&setting));
    crate::sessions::refresh_theme(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_values_mean_system() {
        assert_eq!(sanitize("dark"), DARK);
        assert_eq!(sanitize("light"), LIGHT);
        assert_eq!(sanitize("system"), SYSTEM);
        assert_eq!(sanitize(""), SYSTEM);
        assert_eq!(sanitize("Dark"), SYSTEM);
    }

    #[test]
    fn native_theme_only_when_forced() {
        assert_eq!(native(SYSTEM), None);
        assert_eq!(native(LIGHT), Some(Theme::Light));
        assert_eq!(native(DARK), Some(Theme::Dark));
    }

    #[test]
    fn forced_setting_wins_over_detected_theme() {
        assert_eq!(resolve(DARK, Some("light".into())), DARK);
        assert_eq!(resolve(LIGHT, Some("dark".into())), LIGHT);
        assert_eq!(resolve(SYSTEM, Some("dark".into())), "dark");
        assert_eq!(resolve(SYSTEM, None), SYSTEM);
        assert_eq!(resolve(DARK, None), DARK);
    }
}
