//! Issue #101: lower WebView2's memory while the window is hidden to the tray.
//!
//! WebView2 has an official hint for this, `ICoreWebView2_19::MemoryUsageTargetLevel`.
//! `LOW` makes the browser process trim its working set, but - unlike suspending
//! the page (`TrySuspend`) or unloading the account ("Sleep unused accounts") -
//! scripts and timers keep running, so WhatsApp Web still receives messages and
//! notifications. The level goes back to `NORMAL` when the window is shown.
//!
//! Windows only (one main window holds every account's webview); a no-op
//! elsewhere. There is no event for "the window was hidden" that covers every
//! way the app hides it (close button, tray click, lock, shortcut), so a small
//! poller watches the main window's visibility and reacts to changes only.

/// What to do after observing the main window's visibility.
#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Level {
    Normal,
    Low,
}

/// Tracks the level last applied; yields a level only when it must change.
#[cfg(any(windows, test))]
#[derive(Default)]
pub struct Tracker {
    applied: Option<Level>,
}

#[cfg(any(windows, test))]
impl Tracker {
    pub fn observe(&mut self, visible: bool) -> Option<Level> {
        let wanted = if visible { Level::Normal } else { Level::Low };
        match (self.applied, wanted) {
            // Webviews start at NORMAL: nothing to do until the window is hidden.
            (None, Level::Normal) => {
                self.applied = Some(wanted);
                None
            }
            (Some(prev), w) if prev == w => None,
            _ => {
                self.applied = Some(wanted);
                Some(wanted)
            }
        }
    }
}

#[cfg(windows)]
pub fn start(app: &tauri::AppHandle) {
    use tauri::Manager;
    let app = app.clone();
    std::thread::spawn(move || {
        let mut tracker = Tracker::default();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let Some(main) = app.get_window("main") else {
                continue;
            };
            let visible =
                main.is_visible().unwrap_or(true) && !main.is_minimized().unwrap_or(false);
            if let Some(level) = tracker.observe(visible) {
                apply(&app, level);
            }
        }
    });
}

#[cfg(not(windows))]
pub fn start(_app: &tauri::AppHandle) {}

/// Set the level on every account webview.
#[cfg(windows)]
fn apply(app: &tauri::AppHandle, level: Level) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;
    let target = match level {
        Level::Low => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        Level::Normal => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    let mut done = 0;
    for (label, view) in app.webviews() {
        if !label.starts_with("wa-") {
            continue;
        }
        let _ = view.with_webview(move |platform| {
            // SAFETY: with_webview runs on the UI thread that owns the controller.
            unsafe {
                if let Ok(core) = platform.controller().CoreWebView2() {
                    if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                        let _ = core19.SetMemoryUsageTargetLevel(target);
                    }
                }
            }
        });
        done += 1;
    }
    crate::dlog::log(&format!("memory: target {level:?} on {done} webview(s)"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changes_are_applied() {
        let mut t = Tracker::default();
        assert_eq!(t.observe(true), None);
        assert_eq!(t.observe(true), None);
        assert_eq!(t.observe(false), Some(Level::Low));
        assert_eq!(t.observe(false), None);
        assert_eq!(t.observe(true), Some(Level::Normal));
        assert_eq!(t.observe(false), Some(Level::Low));
    }

    #[test]
    fn hidden_from_the_start_goes_low() {
        let mut t = Tracker::default();
        assert_eq!(t.observe(false), Some(Level::Low));
    }
}
