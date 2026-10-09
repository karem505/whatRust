use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Where a message notification came from. Logged (never the content) so a
/// report can tell which path WhatsApp used on that machine (issue #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// bridge.js: the page's `Notification` / `showNotification` shims.
    Page,
    /// WebView2's own `NotificationReceived` (Windows): notifications raised
    /// where the injected script can't reach, such as a worker.
    #[cfg_attr(not(windows), allow(dead_code))]
    WebView2,
    /// The unread count went up and no notification came with it.
    UnreadFallback,
}

/// Recent notifications, per account window, so the same alert arriving by two
/// paths shows once and the unread fallback stays quiet when WhatsApp spoke up.
#[derive(Default)]
pub struct Recent(Mutex<RecentInner>);

#[derive(Default)]
struct RecentInner {
    /// Last notification of any kind, per window label.
    last: HashMap<String, Instant>,
    /// Identical title+body seen recently, per window label.
    seen: HashMap<(String, String), Instant>,
}

/// Window in which an identical alert counts as a duplicate.
const DEDUP: Duration = Duration::from_millis(3500);
/// How long the unread fallback waits for a real notification to arrive.
pub const FALLBACK_WAIT: Duration = Duration::from_secs(3);
/// A real notification this close before an unread increase also covers it.
const FALLBACK_LOOKBACK: Duration = Duration::from_secs(5);

impl Recent {
    /// Record an alert; false if it is a duplicate of one just shown.
    fn admit(&self, label: &str, title: &str, body: &str, now: Instant) -> bool {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.seen.retain(|_, t| now.duration_since(*t) <= DEDUP);
        let key = (label.to_string(), format!("{title}\u{1f}{body}"));
        if inner.seen.contains_key(&key) {
            return false;
        }
        inner.seen.insert(key, now);
        inner.last.insert(label.to_string(), now);
        true
    }

    /// Whether `label` had a notification since `since`.
    fn notified_since(&self, label: &str, since: Instant) -> bool {
        let inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.last.get(label).is_some_and(|t| *t >= since)
    }
}

/// Should an unread change trigger the fallback? Only a real increase over a
/// count we already knew, only once the page has settled (a launch or reload
/// takes the title from "WhatsApp" to "(3) WhatsApp" while it syncs, which is
/// not news), and only while the window isn't in front of the user.
pub fn fallback_due(previous: Option<u32>, now: u32, window_focused: bool, settled: bool) -> bool {
    settled && matches!(previous, Some(p) if now > p) && !window_focused
}

/// Title and body of the fallback toast. Counts only; WhatsApp's title gives
/// the number of unread chats, and nothing about their content.
pub fn fallback_text(unread_chats: u32) -> (String, String) {
    let body = if unread_chats == 1 {
        "You have a new message.".to_string()
    } else {
        format!("You have new messages in {unread_chats} chats.")
    };
    ("New message".to_string(), body)
}

/// Prefix the account name when more than one account exists, so a toast says
/// which number it is about (e.g. "Work: Alice").
fn attributed_title(app: &AppHandle, label: &str, title: &str) -> String {
    let f = crate::accounts::load(app);
    if f.accounts.len() > 1 {
        if let Some(id) = crate::accounts::id_from_label(label) {
            if let Some(acct) = f.accounts.iter().find(|a| a.id == id) {
                return format!("{}: {}", acct.name, title);
            }
        }
    }
    title.to_string()
}

/// A message notification from an account window, by any route. Applies the
/// lock and the Notifications setting, drops duplicates, then shows the toast,
/// with the sender's picture when `icon` (base64 PNG) is given and valid.
/// No message content is logged (PII) — only the route and the outcome.
pub fn from_account(
    app: &AppHandle,
    label: &str,
    title: &str,
    body: &str,
    icon: Option<&str>,
    source: Source,
) {
    crate::dlog::log(&format!("notify: {source:?} notification"));
    // While locked, suppress notifications entirely so message previews don't
    // leak to the OS notification center / lock screen. The tray unread badge
    // still updates via set_unread (a count only, no content).
    if !crate::lock::is_unlocked(app) {
        crate::dlog::log("notify: suppressed, app is locked");
        return;
    }
    if !crate::settings::load(app).notifications {
        crate::dlog::log("notify: suppressed, notifications are off in settings");
        return;
    }
    if let Some(recent) = app.try_state::<Recent>() {
        if !recent.admit(label, title, body, Instant::now()) {
            crate::dlog::log("notify: duplicate dropped");
            return;
        }
    }
    // Only written once the toast is certain to show (unlocked, enabled, new).
    let icon = icon.and_then(|b64| crate::notif_icon::store(app, b64));
    show_with_icon(
        app,
        &attributed_title(app, label, title),
        body,
        icon.as_deref(),
    );
}

/// The unread count of `label` went up while its window wasn't focused. Wait
/// briefly for WhatsApp's own notification; if none came (it raised it where
/// whatRust can't see, e.g. its service worker), say so with a generic toast.
pub fn unread_increased(app: &AppHandle, label: &str, unread_chats: u32) {
    let (app, label) = (app.clone(), label.to_string());
    let started = Instant::now();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK_WAIT);
        let covered = app
            .try_state::<Recent>()
            .is_some_and(|r| r.notified_since(&label, started - FALLBACK_LOOKBACK));
        if covered {
            return;
        }
        // Read in the meantime: nothing to announce.
        let still_unread = crate::accounts::id_from_label(&label).is_some_and(|id| {
            app.state::<crate::accounts::UnreadMap>()
                .lock()
                .unwrap()
                .get(id)
                .is_some_and(|n| *n >= unread_chats)
        });
        if !still_unread {
            return;
        }
        let (title, body) = fallback_text(unread_chats);
        from_account(&app, &label, &title, &body, None, Source::UnreadFallback);
    });
}

/// Show a native notification.
///
/// Windows raises the WinRT toast itself (the way ZapFast does) instead of going
/// through `tauri-plugin-notification`: the plugin fires the toast on a detached
/// task and throws the result away, so a failure there could never be seen
/// (issue #3). Here the result is logged, and clicking the toast brings whatRust
/// forward. No message content is logged (PII) — only that a toast was shown.
pub fn show(app: &AppHandle, title: &str, body: &str) {
    show_with_icon(app, title, body, None);
}

/// `show` with an optional picture (a PNG file): round on a Windows toast, the
/// notification icon on Linux. macOS notifications always carry the app icon.
pub fn show_with_icon(app: &AppHandle, title: &str, body: &str, icon: Option<&Path>) {
    #[cfg(windows)]
    {
        windows_toast(app, title, body, icon.map(Path::to_path_buf));
    }
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        let mut builder = app.notification().builder().title(title).body(body);
        if let Some(icon) = icon {
            builder = builder.icon(icon.to_string_lossy());
        }
        let r = builder.show();
        crate::dlog::log(&format!(
            "notify::show dispatched{} (plugin returned {r:?})",
            if icon.is_some() { " with picture" } else { "" }
        ));
    }
}

#[cfg(windows)]
fn windows_toast(app: &AppHandle, title: &str, body: &str, icon: Option<std::path::PathBuf>) {
    use tauri_winrt_notification::{IconCrop, Toast};
    // The AUMID aumid.rs registers at startup, so the toast is attributed to
    // whatRust and allowed to render.
    let aumid = app.config().identifier.clone();
    let (app, title, body) = (app.clone(), title.to_string(), body.to_string());
    // WinRT calls can block briefly; keep them off the UI/IPC thread.
    std::thread::spawn(move || {
        let click = app.clone();
        let mut toast = Toast::new(&aumid).title(&title).text1(&body);
        if let Some(icon) = &icon {
            toast = toast.icon(icon, IconCrop::Circular, "");
        }
        let result = toast
            .on_activated(move |_| {
                let app = click.clone();
                let _ = click.run_on_main_thread(move || crate::window::show_main(&app));
                Ok(())
            })
            .show();
        match result {
            Ok(()) if icon.is_some() => crate::dlog::log("notify::show: toast shown with picture"),
            Ok(()) => crate::dlog::log("notify::show: toast shown"),
            Err(e) => crate::dlog::log(&format!("notify::show: toast FAILED: {e}")),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_alert_by_two_routes_shows_once() {
        let r = Recent::default();
        let t0 = Instant::now();
        assert!(r.admit("wa-default", "Alice", "hi", t0));
        assert!(!r.admit("wa-default", "Alice", "hi", t0 + Duration::from_secs(1)));
        // A different message, or the same text from another account, is new.
        assert!(r.admit(
            "wa-default",
            "Alice",
            "hi again",
            t0 + Duration::from_secs(1)
        ));
        assert!(r.admit("wa-acct-2", "Alice", "hi", t0 + Duration::from_secs(1)));
        // The same text later is a new message, not a duplicate.
        assert!(r.admit(
            "wa-default",
            "Alice",
            "hi",
            t0 + DEDUP + Duration::from_millis(1)
        ));
    }

    #[test]
    fn a_real_notification_covers_the_unread_fallback() {
        let r = Recent::default();
        let t0 = Instant::now();
        assert!(!r.notified_since("wa-default", t0));
        r.admit("wa-default", "Alice", "hi", t0);
        assert!(r.notified_since("wa-default", t0));
        assert!(!r.notified_since("wa-acct-2", t0), "per account");
        assert!(!r.notified_since("wa-default", t0 + Duration::from_millis(1)));
    }

    #[test]
    fn only_a_real_increase_while_away_triggers_the_fallback() {
        assert!(fallback_due(Some(1), 2, false, true));
        assert!(fallback_due(Some(0), 1, false, true));
        assert!(!fallback_due(Some(2), 2, false, true), "no change");
        assert!(!fallback_due(Some(3), 1, false, true), "messages were read");
        assert!(
            !fallback_due(None, 4, false, true),
            "first report after launch"
        );
        assert!(
            !fallback_due(Some(1), 2, true, true),
            "the user is looking at it"
        );
        assert!(
            !fallback_due(Some(0), 3, false, false),
            "the page is still syncing after a launch or reload"
        );
    }

    #[test]
    fn the_fallback_says_nothing_about_content() {
        assert_eq!(
            fallback_text(1),
            ("New message".into(), "You have a new message.".into())
        );
        assert_eq!(fallback_text(3).1, "You have new messages in 3 chats.");
    }
}
