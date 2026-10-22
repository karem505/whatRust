use crate::i18n::{tr, trf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, MutexGuard,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Runtime lock state. `unlocked == false` means the app is locked.
pub struct LockState {
    pub unlocked: Mutex<bool>,
    /// Labels of windows hidden by `lock_now`, to re-show on `unlock`.
    pub hidden: Mutex<Vec<String>>,
    prompt_requested: AtomicBool,
}

impl LockState {
    pub fn new(unlocked: bool) -> Self {
        Self {
            unlocked: Mutex::new(unlocked),
            hidden: Mutex::new(Vec::new()),
            prompt_requested: AtomicBool::new(false),
        }
    }
}

/// Lock a mutex, recovering the data if another thread panicked while holding
/// it. Release builds use `panic = "abort"`, so a poisoned lock must never turn
/// into a second crash.
pub fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Delay after `failures` consecutive wrong passwords: free for the first
/// three, then 2s doubling up to a minute.
pub fn throttle_delay(failures: u32) -> Duration {
    if failures < 3 {
        return Duration::ZERO;
    }
    Duration::from_secs((1u64 << (failures - 2).min(6)).min(60))
}

/// Brute-force limiter for the lock screen. Lives in memory only: restarting the
/// app resets it, which still leaves each attempt costing an Argon2 hash.
#[derive(Default)]
pub struct UnlockThrottle {
    failures: u32,
    blocked_until: Option<Instant>,
}

impl UnlockThrottle {
    /// `Err(seconds_left)` while attempts are blocked.
    pub fn check(&self, now: Instant) -> Result<(), u64> {
        match self.blocked_until {
            Some(t) if t > now => Err((t - now).as_secs() + 1),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        let d = throttle_delay(self.failures);
        self.blocked_until = (!d.is_zero()).then(|| now + d);
    }

    pub fn record_success(&mut self) {
        *self = Self::default();
    }
}

pub static THROTTLE: Mutex<Option<UnlockThrottle>> = Mutex::new(None);

pub const MIN_PASSWORD_LEN: usize = 6;

/// Shared validation for choosing a new app-lock password.
pub fn validate_new_password(new: &str, confirm: &str) -> Result<(), String> {
    if new != confirm {
        return Err(tr("As senhas não coincidem.").into());
    }
    if new.chars().count() < MIN_PASSWORD_LEN {
        return Err(trf(
            "A senha deve ter pelo menos {0} caracteres.",
            &[&MIN_PASSWORD_LEN],
        ));
    }
    Ok(())
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
        .map(|s| *guard(&s.unlocked))
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
    lock_with_prompt(app, true);
}
pub fn lock_on_hide(app: &AppHandle) {
    lock_with_prompt(app, false);
}
fn lock_with_prompt(app: &AppHandle, prompt: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || lock_on_main(&handle, prompt));
}
fn lock_on_main(app: &AppHandle, prompt: bool) {
    let state = match app.try_state::<LockState>() {
        Some(s) => s,
        None => return, // not yet set up — bail
    };
    if !prompt {
        state.prompt_requested.store(false, Ordering::SeqCst);
        if let Some(w) = app.get_window("lock") {
            let _ = w.hide();
        }
    }
    // Idempotent: if already locked, just ensure the lock screen is up — do NOT
    // re-hide windows or clobber the recorded `hidden` list (which unlock restores).
    {
        // Test-and-set under one guard so two racing callers cannot both hide.
        let mut unlocked = guard(&state.unlocked);
        if !*unlocked {
            drop(unlocked);
            if prompt {
                show_lock_window(app);
            }
            return;
        }
        *unlocked = false;
    }
    crate::notify::on_lock(app);
    crate::integration::broadcast(app);
    let mut hidden = Vec::new();
    for (label, w) in app.windows() {
        if should_hide(&label) && w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            #[cfg(windows)]
            crate::sessions::layout(app, &label);
            hidden.push(label);
        }
    }
    *guard(&state.hidden) = hidden;
    if prompt {
        show_lock_window(app);
    }
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
    state.prompt_requested.store(false, Ordering::SeqCst);
    crate::notify::on_unlock(app);
    *guard(&state.unlocked) = true;
    if let Some(w) = app.get_webview_window("lock") {
        let _ = w.destroy();
    }
    let hidden = std::mem::take(&mut *guard(&state.hidden));
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
    crate::integration::broadcast(app);
}

/// Create-or-show the dedicated lock window. `content_protected` blocks screen
/// capture of the lock screen; `prevent_close` while locked stops the X from
/// relaunching into an unlocked state.
pub fn show_lock_window(app: &AppHandle) {
    if let Some(state) = app.try_state::<LockState>() {
        state.prompt_requested.store(true, Ordering::SeqCst);
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        static CREATE: Mutex<()> = Mutex::new(());
        let _guard = CREATE.lock().unwrap_or_else(|e| e.into_inner());
        if !is_unlocked(&app)
            && app
                .state::<LockState>()
                .prompt_requested
                .load(Ordering::SeqCst)
        {
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
        .initialization_script(crate::i18n::init_script())
        .title(tr("whatRust — Bloqueado"))
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
        if is_unlocked(app)
            || !app
                .state::<LockState>()
                .prompt_requested
                .load(Ordering::SeqCst)
        {
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
    fn throttle_delay_grows_then_caps() {
        assert_eq!(throttle_delay(0), Duration::ZERO);
        assert_eq!(throttle_delay(2), Duration::ZERO);
        assert_eq!(throttle_delay(3), Duration::from_secs(2));
        assert_eq!(throttle_delay(4), Duration::from_secs(4));
        assert_eq!(throttle_delay(50), Duration::from_secs(60));
    }

    #[test]
    fn throttle_blocks_after_repeated_failures_and_resets() {
        let t0 = Instant::now();
        let mut t = UnlockThrottle::default();
        for _ in 0..2 {
            t.record_failure(t0);
            assert!(t.check(t0).is_ok());
        }
        t.record_failure(t0);
        assert!(t.check(t0).is_err());
        assert!(t.check(t0 + Duration::from_secs(5)).is_ok());
        t.record_success();
        assert!(t.check(t0).is_ok());
    }

    #[test]
    fn password_validation() {
        assert!(validate_new_password("abcdef", "abcdef").is_ok());
        assert!(validate_new_password("abcdef", "abcdeg").is_err());
        assert!(validate_new_password("abc", "abc").is_err());
    }

    #[test]
    fn is_lock_label_matches_only_lock() {
        assert!(is_lock_label("lock"));
        assert!(!is_lock_label("settings"));
        assert!(!is_lock_label("wa-default"));
    }
}
