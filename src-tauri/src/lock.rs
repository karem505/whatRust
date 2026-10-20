use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Runtime lock state. `unlocked == false` means the app is locked.
pub struct LockState {
    pub unlocked: Mutex<bool>,
    /// Labels of windows hidden by `lock_now`, to re-show on `unlock`.
    pub hidden: Mutex<Vec<String>>,
}

impl LockState {
    pub fn new(unlocked: bool) -> Self {
        Self {
            unlocked: Mutex::new(unlocked),
            hidden: Mutex::new(Vec::new()),
        }
    }
}

/// Whether a window label should be hidden behind the lock screen. Pure, unit-tested.
/// The lock window itself is never hidden; everything user-facing (`wa-*` + settings) is.
pub fn should_hide(label: &str) -> bool {
    label != "lock"
        && (label.starts_with("wa-")
            || label == "settings"
            || label == "main"
            || label.starts_with("detached-"))
}

/// Pure predicate behind the lock-window IPC guard.
pub fn is_lock_label(label: &str) -> bool {
    label == "lock"
}

pub fn is_lock_window(window: &tauri::Webview) -> bool {
    is_lock_label(window.label()) && crate::integration::trusted_local(window)
}

/// Read the current unlocked flag. Defaults to `true` (unlocked) when the state is
/// not yet managed, so nothing is accidentally gated before setup runs.
pub fn is_unlocked(app: &AppHandle) -> bool {
    app.try_state::<LockState>()
        .map(|s| *s.unlocked.lock().unwrap())
        .unwrap_or(true)
}

/// IPC guard for sensitive commands.
pub fn require_unlocked(app: &AppHandle) -> Result<(), String> {
    if is_unlocked(app) {
        Ok(())
    } else {
        Err("locked".into())
    }
}

/// Lock the app: mark locked, hide every user-facing window (recording which were
/// visible), and show the lock screen.
pub fn lock_now(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || lock_on_main(&handle));
}

fn lock_on_main(app: &AppHandle) {
    let state = match app.try_state::<LockState>() {
        Some(s) => s,
        None => return, // not yet set up — bail
    };
    // Idempotent: if already locked, just ensure the lock screen is up — do NOT
    // re-hide windows or clobber the recorded `hidden` list (which unlock restores).
    if !*state.unlocked.lock().unwrap() {
        show_lock_window(app);
        return;
    }
    *state.unlocked.lock().unwrap() = false;
    let mut hidden = Vec::new();
    for (label, w) in app.windows() {
        if should_hide(&label) && w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            #[cfg(windows)]
            crate::sessions::layout(app, &label);
            hidden.push(label);
        }
    }
    *state.hidden.lock().unwrap() = hidden;
    show_lock_window(app);
}

/// Unlock the app: mark unlocked, destroy the lock window, and restore the windows
/// that were visible when we locked (or fall back to the active account).
pub fn unlock(app: &AppHandle) {
    // Password IPC may execute inside this very WebView's callback. Always
    // return from that callback before destroying its WebView2 controller.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let ui = handle.clone();
        let _ = handle.run_on_main_thread(move || unlock_on_main(&ui));
    });
}

fn unlock_on_main(app: &AppHandle) {
    let state = match app.try_state::<LockState>() {
        Some(s) => s,
        None => return, // not yet set up — bail
    };
    *state.unlocked.lock().unwrap() = true;
    if let Some(w) = app.get_webview_window("lock") {
        let _ = w.destroy();
    }
    let hidden = std::mem::take(&mut *state.hidden.lock().unwrap());
    if hidden.is_empty() {
        crate::window::show_active(app);
    } else {
        for label in hidden {
            if let Some(w) = app.get_window(&label) {
                let _ = w.show();
                #[cfg(windows)]
                crate::sessions::layout(app, &label);
            }
        }
    }
    #[cfg(windows)]
    crate::sessions::restore_pending(app);
}

/// Create-or-show the dedicated lock window. `content_protected` blocks screen
/// capture of the lock screen; `prevent_close` while locked stops the X from
/// relaunching into an unlocked state.
pub fn show_lock_window(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        static CREATE: Mutex<()> = Mutex::new(());
        let _guard = CREATE.lock().unwrap_or_else(|e| e.into_inner());
        if !is_unlocked(&app) {
            show_lock_window_inner(&app);
        }
    });
}

fn show_lock_window_inner(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("lock") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "lock", WebviewUrl::App("lock.html".into()))
        .title("whatRust — Bloqueado")
        .inner_size(420.0, 540.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .content_protected(true)
        .center()
        .focused(true)
        .build();

    if let Ok(win) = built {
        // Unlock can race a queued window creation; never leave a stale lock
        // screen on top after authentication has already succeeded.
        if is_unlocked(app) {
            let _ = win.destroy();
            return;
        }
        let app_handle = app.clone();
        win.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !is_unlocked(&app_handle) {
                    api.prevent_close();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_hide_covers_accounts_and_settings() {
        assert!(should_hide("wa-default"));
        assert!(should_hide("wa-acct-3"));
        assert!(should_hide("settings"));
    }

    #[test]
    fn should_hide_never_hides_the_lock_window() {
        assert!(!should_hide("lock"));
        assert!(!should_hide("some-future-internal-panel"));
    }

    #[test]
    fn is_lock_label_matches_only_lock() {
        assert!(is_lock_label("lock"));
        assert!(!is_lock_label("settings"));
        assert!(!is_lock_label("wa-default"));
    }
}
