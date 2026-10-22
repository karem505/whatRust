//! Per-platform desktop biometric / local-auth. The default ("password-only") account
//! never touches this; it's an optional unlock shortcut. Every backend returns a plain
//! yes/no — no key material — so it can gate the UI but cannot decrypt anything.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Usable now (Hello configured / Touch ID enrolled / polkit reachable).
    Available,
    /// The mechanism exists but isn't set up (no enrolled print, no agent, etc.).
    NotConfigured,
    /// Not supported on this OS / version. Currently constructed by Windows;
    /// the macOS and Linux backends return Available/NotConfigured.
    #[cfg_attr(not(windows), allow(dead_code))]
    Unsupported,
}

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
compile_error!("biometric auth is not implemented for this platform");

pub fn availability() -> Availability {
    platform::availability()
}

/// Blocking. `reason` must be non-empty (macOS aborts on an empty reason).
pub fn authenticate(app: &tauri::AppHandle, reason: &str) -> Result<bool, String> {
    platform::authenticate(app, reason)
}

pub fn label() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Windows Hello"
    }
    #[cfg(target_os = "macos")]
    {
        "Touch ID"
    }
    #[cfg(target_os = "linux")]
    {
        "autenticação do sistema"
    }
}
