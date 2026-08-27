use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

pub fn show(app: &tauri::AppHandle, title: &str, body: &str) {
    // NOTE: tauri-plugin-notification's `show()` dispatches the real toast on a
    // detached async task and discards its result, so the value returned here is
    // effectively always Ok — it does NOT reflect whether the OS actually
    // rendered the toast. We still log that this point was reached (issue #3
    // diagnostics): if "notify::show dispatched" appears in the log but no toast
    // shows, the failure is downstream in the Windows toast layer, not in our
    // command/IPC path. No message content is logged (PII).
    let r = app.notification().builder().title(title).body(body).show();
    crate::dlog::log(&format!("notify::show dispatched (plugin returned {r:?})"));
}

/// Grace window after a page-driven toast during which the Rust-side unread
/// fallback stays silent (avoids double toasts for the same message). Long
/// enough to swallow the 2s title-poll double fire, short enough that a
/// genuinely NEW message a few seconds later still notifies.
const SHIM_GRACE: Duration = Duration::from_secs(10);

/// How long "already toasted this exact count" stays suppressed. The title
/// polls every 2s and WhatsApp re-renders the same count on React re-renders;
/// a window of 90s collapses those repeats without hiding a later rise.
const SAME_COUNT_WINDOW: Duration = Duration::from_secs(90);

// ---------------------------------------------------------------------------
// Rust-side unread toast fallback (issue #3).
//
// The v0.3.3 diagnostic build proved that on some Windows 11 installs WhatsApp
// Web never calls window.Notification NOR ServiceWorkerRegistration
// .showNotification (log shows session start + AUMID ok, but no
// `commands::notify` when messages arrive), so the JS-driven path silently
// produces no toast there. A page-injected script cannot reach the service
// worker's own context, so the fallback must live in Rust. The one signal
// WhatsApp always updates is the unread count in the document <title>, forwarded
// by bridge.js via `set_unread` on every change (plus a 2s poll) — that is the
// engine-independent "new messages arrived" event this fallback is driven by.
// ---------------------------------------------------------------------------

/// Timestamp of the last page-driven (`notify` command) toast per window label,
/// used to avoid double-toasting when both paths fire for the same message.
static SHIM_FIRED: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();

fn shim_map() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    SHIM_FIRED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record that the page-driven notification path fired for a window, so the
/// Rust-side unread fallback stays quiet for [`SHIM_GRACE`].
pub fn record_shim_notify(window_label: &str) {
    let mut map = shim_map().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(window_label.to_string(), std::time::Instant::now());
}

/// Last unread-fallback toast raised per window label, so repeated <title>
/// polls carrying the same count do not stack toasts.
static LAST_TOAST: OnceLock<Mutex<HashMap<String, (std::time::Instant, u32)>>> = OnceLock::new();

fn last_toast_map() -> &'static Mutex<HashMap<String, (std::time::Instant, u32)>> {
    LAST_TOAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Raise the Rust-side "unread messages" toast for an account window when the
/// unread count rises. All gates are logged (counts only — no PII):
/// - app locked: suppress (no previews on the lock screen);
/// - notifications disabled in settings: suppress;
/// - the account window is focused: suppress (user is already looking at it);
/// - the page shim toasted this window within [`SHIM_GRACE`]: suppress (it has
///   the real title/body; ours would be a duplicate);
/// - we already toasted this exact count for the window within
///   [`SAME_COUNT_WINDOW`]: suppress (title re-renders must not stack toasts).
pub fn maybe_unread_toast(app: &tauri::AppHandle, window_label: &str, count: u32) {
    crate::dlog::log(&format!(
        "notify::unread fallback considered (count={count})"
    ));

    if !crate::lock::is_unlocked(app) {
        crate::dlog::log("notify::unread fallback suppressed: app is locked");
        return;
    }
    if !crate::settings::load(app).notifications {
        crate::dlog::log("notify::unread fallback suppressed: notifications disabled in settings");
        return;
    }

    // Focused-window check happens on the main thread; do the shared-state
    // decisions here under one lock pass.
    let now = std::time::Instant::now();
    {
        let mut shim = shim_map().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = shim.get(window_label) {
            if now.duration_since(*t) <= SHIM_GRACE {
                crate::dlog::log("notify::unread fallback suppressed: shim notified recently");
                return;
            }
        }
        // Prune stale shim entries (bounded map: one entry per window label,
        // but prune anything older than the grace window anyway).
        shim.retain(|_, t| now.duration_since(*t) <= SHIM_GRACE);
        let mut last = last_toast_map().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, c)) = last.get(window_label) {
            if *c == count && now.duration_since(*at) <= SAME_COUNT_WINDOW {
                return;
            }
        }
        last.insert(window_label.to_string(), (now, count));
    }

    // Suppress when the user is actively reading this account window. Reading
    // the window's visibility (not focus events) avoids a race with the window
    // being hidden to tray.
    if let Some(w) = app.get_webview_window(window_label) {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            crate::dlog::log("notify::unread fallback suppressed: window focused");
            return;
        }
    }

    // Attribute the toast when several accounts exist (same rule as the page
    // command), and keep the body generic — this layer never knows message
    // content, only a count (no PII).
    let f = crate::accounts::load(app);
    let title = if f.accounts.len() > 1 {
        match crate::accounts::id_from_label(window_label) {
            Some(id) => match f.accounts.iter().find(|a| a.id == id) {
                Some(acct) => format!("{}: new messages", acct.name),
                None => "WhatsApp: new messages".to_string(),
            },
            None => "WhatsApp: new messages".to_string(),
        }
    } else {
        "WhatsApp".to_string()
    };
    let body = if count == 1 {
        "You have 1 unread message.".to_string()
    } else {
        format!("You have {count} unread messages.")
    };
    show(app, &title, &body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grace_constant_is_reasonable() {
        // Long enough to swallow the 2s title-poll double fire, short enough
        // that a genuinely NEW message a few seconds later still notifies.
        assert!(SHIM_GRACE >= Duration::from_secs(5));
        assert!(SHIM_GRACE <= Duration::from_secs(15));
    }

    #[test]
    fn same_count_window_is_reasonable() {
        // Must outlive the 2s title poll so repeats are collapsed, but stay
        // far below a minute so a fresh message soon after still notifies.
        assert!(SAME_COUNT_WINDOW >= Duration::from_secs(30));
        assert!(SAME_COUNT_WINDOW <= Duration::from_secs(120));
    }
}
