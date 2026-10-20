//! Account-scoped delivery policy and bounded, content-free session diagnostics.
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Page,
    WebView2,
    UnreadFallback,
    Visual,
    Test,
}

#[derive(Clone, Serialize)]
pub struct Notice {
    pub id: String,
    pub account_id: String,
    pub source: Source,
    pub time: u64,
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Default)]
pub struct Recent {
    inner: Mutex<Inner>,
    generation: AtomicU64,
    #[cfg(windows)]
    delivery_gate: Mutex<()>,
    #[cfg(windows)]
    cleared_generation: AtomicU64,
}
#[derive(Default)]
struct Inner {
    delivered: HashMap<String, (String, Instant)>,
    coverage: HashMap<String, VecDeque<(String, Instant)>>,
    seen: HashMap<(String, String), Instant>,
    native: HashSet<String>,
    records: VecDeque<Notice>,
    #[cfg(windows)]
    toasts: HashMap<String, windows::UI::Notifications::ToastNotification>,
}
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn event_id() -> String {
    format!("event-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
const DEDUP: Duration = Duration::from_secs(60);
pub const FALLBACK_WAIT: Duration = Duration::from_secs(3);
impl Recent {
    pub fn epoch(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    #[cfg(any(windows, test))]
    fn cancel_pending(&self, label: &str, id: &str, identity: &str) {
        self.finish(label, id, identity, false);
        let mut inner = self.inner.lock().unwrap();
        if let Some(n) = inner.records.iter_mut().find(|n| n.id == id) {
            n.status = "suppressed".into();
            n.reason = Some("locked".into());
        }
    }
    fn reserve(&self, label: &str, identity: &str, now: Instant) -> bool {
        let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        i.seen.retain(|_, t| now.duration_since(*t) <= DEDUP);
        // Bound both memory and burst bookkeeping. IDs are not message content.
        if i.seen.len() >= 2048 {
            i.seen.clear();
        }
        if i.seen.contains_key(&(label.into(), identity.into())) {
            return false;
        }
        i.seen.insert((label.into(), identity.into()), now);
        true
    }
    pub fn notified_since(&self, label: &str, since: Instant) -> bool {
        self.inner
            .lock()
            .unwrap()
            .delivered
            .get(label)
            .is_some_and(|(_, t)| *t >= since)
    }
    pub fn consume_coverage(&self, label: &str, since: Instant) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let receipts = inner.coverage.entry(label.into()).or_default();
        while receipts.front().is_some_and(|(_, time)| *time < since) {
            receipts.pop_front();
        }
        receipts.pop_front().is_some()
    }
    pub fn native_ready(&self, label: &str) -> bool {
        self.inner.lock().unwrap().native.contains(label)
    }
    pub fn set_native(&self, label: &str, ready: bool) {
        let mut i = self.inner.lock().unwrap();
        if ready {
            i.native.insert(label.into());
        } else {
            i.native.remove(label);
        }
    }
    pub fn records(&self) -> Vec<Notice> {
        self.inner
            .lock()
            .unwrap()
            .records
            .iter()
            .rev()
            .cloned()
            .collect()
    }
    fn record(&self, label: &str, id: &str, source: Source, status: &str, reason: Option<&str>) {
        let mut i = self.inner.lock().unwrap();
        i.records.push_back(Notice {
            id: id.into(),
            account_id: crate::accounts::id_from_label(label).unwrap_or("").into(),
            source,
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            status: status.into(),
            reason: reason.map(str::to_string),
        });
        while i.records.len() > 100 {
            i.records.pop_front();
        }
    }
    fn finish(&self, label: &str, id: &str, identity: &str, success: bool) {
        let mut i = self.inner.lock().unwrap();
        let success = success && !i.records.iter().any(|r| r.id == id && r.status == "failed");
        let real_source = i
            .records
            .iter()
            .find(|r| r.id == id)
            .is_none_or(|r| matches!(r.source, Source::Page | Source::WebView2));
        if success && real_source {
            let receipts = i.coverage.entry(label.into()).or_default();
            receipts.push_back((id.into(), Instant::now()));
            while receipts.len() > 256 {
                receipts.pop_front();
            }
            i.delivered
                .insert(label.into(), (id.into(), Instant::now()));
        } else if !success {
            if let Some(receipts) = i.coverage.get_mut(label) {
                receipts.retain(|(key, _)| key != id);
            }
            i.seen.remove(&(label.into(), identity.into()));
            if i.delivered.get(label).is_some_and(|(last, _)| last == id) {
                i.delivered.remove(label);
            }
        }
        if let Some(r) = i.records.iter_mut().find(|r| r.id == id) {
            r.status = if success { "submitted" } else { "failed" }.into();
            r.reason = if success {
                None
            } else {
                Some("windows_rejected".into())
            };
        }
    }
    fn lifecycle(&self, id: &str, status: &str) {
        if let Some(r) = self
            .inner
            .lock()
            .unwrap()
            .records
            .iter_mut()
            .find(|r| r.id == id)
        {
            r.status = status.into();
        }
    }
}

pub fn fallback_due(previous: Option<u32>, now: u32, focused: bool, settled: bool) -> bool {
    settled && matches!(previous,Some(p)if now>p) && !focused
}
pub fn fallback_text(n: u32) -> (String, String) {
    (
        "New message".into(),
        if n == 1 {
            "You have a new message.".into()
        } else {
            format!("You have new messages in {n} chats.")
        },
    )
}

pub fn suppression(
    s: &crate::settings::Settings,
    unlocked: bool,
    focused: bool,
    active: bool,
    source: Source,
    muted: Option<bool>,
) -> Option<&'static str> {
    if !unlocked {
        return Some("locked");
    }
    if !s.notifications {
        return Some("notifications_disabled");
    }
    if source != Source::Test && focused && !s.notify_when_focused {
        return Some("foreground_disabled");
    }
    if source != Source::Test && active && focused && !s.notify_active_chat {
        return Some("active_chat_disabled");
    }
    if source == Source::Visual && s.respect_muted {
        match muted {
            Some(false) => {}
            Some(true) => return Some("muted"),
            None => return Some("mute_state_unknown"),
        }
    }
    None
}

pub fn from_page(
    app: &AppHandle,
    label: &str,
    title: &str,
    body: &str,
    icon: Option<&str>,
    epoch: Option<u64>,
) {
    let recent = app.state::<Recent>();
    if epoch.is_some_and(|e| e != recent.epoch()) {
        recent.record(
            label,
            &event_id(),
            Source::Page,
            "suppressed",
            Some("locked"),
        );
        crate::dlog::log("notification: stale page event suppressed after lock transition");
        return;
    }
    from_account(app, label, title, body, icon, Source::Page);
}
/// Advance again on unlock so work detected during initial locked startup
/// cannot become deliverable merely because authentication just completed.
pub fn on_unlock(app: &AppHandle) {
    app.state::<Recent>()
        .generation
        .fetch_add(1, Ordering::SeqCst);
}

pub fn from_account(
    app: &AppHandle,
    label: &str,
    title: &str,
    body: &str,
    icon: Option<&str>,
    source: Source,
) {
    from_event(
        app,
        label,
        title,
        body,
        icon,
        source,
        &event_id(),
        false,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn from_event(
    app: &AppHandle,
    label: &str,
    title: &str,
    body: &str,
    icon: Option<&str>,
    source: Source,
    identity: &str,
    active: bool,
    muted: Option<bool>,
) {
    #[cfg(windows)]
    let focused = crate::sessions::account_focused(app, label);
    #[cfg(not(windows))]
    let focused = app.get_webview(label).is_some_and(|v| {
        v.window().is_focused().unwrap_or(false) && v.window().is_visible().unwrap_or(false)
    });
    let s = crate::settings::load(app);
    let id = event_id();
    let r = app.state::<Recent>();
    if let Some(reason) = suppression(
        &s,
        crate::lock::is_unlocked(app),
        focused,
        active,
        source,
        muted,
    ) {
        r.record(label, &id, source, "suppressed", Some(reason));
        crate::dlog::log(&format!("notification: {source:?} suppressed ({reason})"));
        #[cfg(windows)]
        if source == Source::WebView2 {
            crate::window::native_report(app, identity, "discard");
        }
        return;
    }
    if !r.reserve(label, identity, Instant::now()) {
        return;
    }
    r.record(label, &id, source, "pending", None);
    let title: String = title.chars().take(256).collect();
    let body: String = body.chars().take(4096).collect();
    let title = attributed_title(app, label, &title);
    let icon = icon.and_then(|b64| crate::notif_icon::store(app, b64));
    #[cfg(windows)]
    windows_toast(
        app,
        &title,
        &body,
        icon,
        Some(label.into()),
        Some(Delivery {
            label: label.into(),
            id,
            identity: identity.into(),
            native: source == Source::WebView2,
        }),
    );
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        let mut b = app.notification().builder().title(title).body(body);
        if let Some(p) = icon {
            b = b.icon(p.to_string_lossy());
        }
        r.finish(label, &id, identity, b.show().is_ok());
    }
}
fn attributed_title(app: &AppHandle, label: &str, title: &str) -> String {
    let f = crate::accounts::load(app);
    if f.accounts.len() > 1 {
        if let Some(a) = f
            .accounts
            .iter()
            .find(|a| Some(a.id.as_str()) == crate::accounts::id_from_label(label))
        {
            return format!("{}: {title}", a.name);
        }
    }
    title.into()
}

pub fn unread_increased(app: &AppHandle, label: &str, n: u32) {
    let app = app.clone();
    let label = label.to_string();
    let started = Instant::now();
    let epoch = app.state::<Recent>().epoch();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK_WAIT);
        if app.state::<Recent>().epoch() != epoch || !crate::lock::is_unlocked(&app) {
            return;
        }
        if app
            .state::<Recent>()
            .notified_since(&label, started - Duration::from_secs(5))
        {
            return;
        }
        let unread = crate::accounts::id_from_label(&label).is_some_and(|id| {
            app.state::<crate::accounts::UnreadMap>()
                .lock()
                .unwrap()
                .get(id)
                .is_some_and(|c| *c >= n)
        });
        if unread {
            let (t, b) = fallback_text(n);
            from_account(&app, &label, &t, &b, None, Source::UnreadFallback);
        }
    });
}

pub fn visual_signal(
    app: &AppHandle,
    label: &str,
    identity: &str,
    active: bool,
    muted: Option<bool>,
) {
    let app = app.clone();
    let label = label.to_string();
    let identity: String = identity.chars().take(180).collect();
    // The queue is bounded by the adapter's batch/rate limit; native delivery
    // gets first refusal. No content is taken from the DOM or kept in history.
    let epoch = app.state::<Recent>().epoch();
    tauri::async_runtime::spawn(async move {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_millis(1800)).await;
        if app.state::<Recent>().epoch() != epoch || !crate::lock::is_unlocked(&app) {
            return;
        }
        if app
            .state::<Recent>()
            .consume_coverage(&label, start - Duration::from_millis(300))
        {
            return;
        }
        from_event(
            &app,
            &label,
            "Nova mensagem",
            "Você recebeu uma mensagem no WhatsApp.",
            None,
            Source::Visual,
            &format!("visual:{identity}"),
            active,
            muted,
        );
    });
}

pub fn show(app: &AppHandle, title: &str, body: &str) {
    show_with_icon(app, title, body, None);
}
pub fn show_for_account(app: &AppHandle, label: &str, title: &str, body: &str) {
    if !crate::lock::is_unlocked(app) {
        return;
    }
    #[cfg(windows)]
    windows_toast(app, title, body, None, Some(label.into()), None);
    #[cfg(not(windows))]
    show(app, title, body);
}
pub fn show_with_icon(app: &AppHandle, title: &str, body: &str, icon: Option<&Path>) {
    if !crate::lock::is_unlocked(app) {
        return;
    }
    #[cfg(windows)]
    windows_toast(app, title, body, icon.map(Path::to_path_buf), None, None);
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        let mut b = app.notification().builder().title(title).body(body);
        if let Some(p) = icon {
            b = b.icon(p.to_string_lossy());
        }
        let _ = b.show();
    }
}
#[derive(Clone)]
#[cfg(windows)]
struct Delivery {
    label: String,
    id: String,
    identity: String,
    native: bool,
}

fn escape_xml(s: &str) -> String {
    s.chars().filter(|c| matches!(*c, '\t'|'\n'|'\r'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')).collect::<String>().replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(windows)]
fn clear_history_before_delivery(app: &AppHandle, generation: u64) {
    use windows::{core::HSTRING, UI::Notifications::ToastNotificationManager};
    let recent = app.state::<Recent>();
    if recent.cleared_generation.load(Ordering::SeqCst) < generation {
        if let Ok(history) = ToastNotificationManager::History() {
            let _ = history.ClearWithId(&HSTRING::from(&app.config().identifier));
        }
        recent
            .cleared_generation
            .store(generation, Ordering::SeqCst);
    }
}

/// Called immediately on lock. Old asynchronous work must remain cancelled
/// even if the user unlocks before its timer or icon conversion finishes.
pub fn on_lock(app: &AppHandle) {
    let recent = app.state::<Recent>();
    recent.generation.fetch_add(1, Ordering::SeqCst);
    #[cfg(windows)]
    {
        let generation = recent.epoch();
        let toasts = std::mem::take(&mut recent.inner.lock().unwrap().toasts);
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let recent = handle.state::<Recent>();
            let _gate = recent.delivery_gate.lock().unwrap();
            use windows::{core::HSTRING, UI::Notifications::ToastNotificationManager};
            if let Ok(notifier) = ToastNotificationManager::CreateToastNotifierWithId(
                &HSTRING::from(&handle.config().identifier),
            ) {
                for toast in toasts.into_values() {
                    let _ = notifier.Hide(&toast);
                }
            }
            clear_history_before_delivery(&handle, generation);
        });
    }
}

#[cfg(windows)]
pub fn cancel_native(app: &AppHandle, identity: &str) {
    // CloseRequested runs on the WebView2 UI thread. WinRT history removal
    // can wait for Shell callbacks; never do that in this browser callback.
    let handle = app.clone();
    let identity = identity.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        use windows::{core::HSTRING, UI::Notifications::ToastNotificationManager};
        if let Ok(history) = ToastNotificationManager::History() {
            let _ = history.RemoveGroupedTagWithId(
                &HSTRING::from(&identity),
                &HSTRING::from("messages"),
                &HSTRING::from(&handle.config().identifier),
            );
        }
        native_report_if_needed(&handle, &identity, "closed");
    });
}

#[cfg(windows)]
fn native_report_if_needed(app: &AppHandle, id: &str, status: &str) {
    crate::window::native_report(app, id, status);
}

#[cfg(windows)]
fn windows_toast(
    app: &AppHandle,
    title: &str,
    body: &str,
    icon: Option<std::path::PathBuf>,
    account: Option<String>,
    delivery: Option<Delivery>,
) {
    use windows::{
        core::{IInspectable, HSTRING},
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{
            ToastDismissedEventArgs, ToastFailedEventArgs, ToastNotification,
            ToastNotificationManager,
        },
    };
    let app = app.clone();
    let title = title.to_string();
    let body = body.to_string();
    let epoch = app.state::<Recent>().epoch();
    std::thread::spawn(move || {
        let recent = app.state::<Recent>();
        let _gate = recent.delivery_gate.lock().unwrap();
        clear_history_before_delivery(&app, epoch);
        if !crate::lock::is_unlocked(&app) || app.state::<Recent>().epoch() != epoch {
            if let Some(d) = &delivery {
                app.state::<Recent>()
                    .cancel_pending(&d.label, &d.id, &d.identity);
                if d.native {
                    crate::window::native_report(&app, &d.identity, "discard");
                }
            }
            return;
        }
        let send = (|| -> windows::core::Result<ToastNotification> {
            let document = XmlDocument::new()?;
            let image = icon
                .as_ref()
                .and_then(|p| tauri::Url::from_file_path(p).ok())
                .map(|u| {
                    format!(
                        "<image placement=\"appLogoOverride\" hint-crop=\"circle\" src=\"{}\"/>",
                        escape_xml(u.as_str())
                    )
                })
                .unwrap_or_default();
            document.LoadXml(&HSTRING::from(format!("<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text>{image}</binding></visual></toast>",escape_xml(&title),escape_xml(&body))))?;
            let toast = ToastNotification::CreateToastNotification(&document)?;
            if let Some(d) = &delivery {
                toast.SetTag(&HSTRING::from(
                    d.identity.chars().take(64).collect::<String>(),
                ))?;
                toast.SetGroup(&HSTRING::from("messages"))?;
            }
            let click = app.clone();
            let clicked = delivery.clone();
            toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_, _| {
                    if let Some(d) = &clicked {
                        click.state::<Recent>().lifecycle(&d.id, "activated");
                        if d.native {
                            crate::window::native_report(&click, &d.identity, "clicked");
                        }
                    }
                    if let Some(label) = &account {
                        crate::window::show_account(&click, label);
                    } else {
                        crate::window::show_main(&click);
                    }
                    Ok(())
                },
            ))?;
            let dismiss = app.clone();
            let dismissed = delivery.clone();
            toast.Dismissed(&TypedEventHandler::<
                ToastNotification,
                ToastDismissedEventArgs,
            >::new(move |_, _| {
                if let Some(d) = &dismissed {
                    let r = dismiss.state::<Recent>();
                    r.lifecycle(&d.id, "dismissed");
                    r.inner.lock().unwrap().toasts.remove(&d.id);
                    if d.native {
                        crate::window::native_report(&dismiss, &d.identity, "closed");
                    }
                }
                Ok(())
            }))?;
            let fail = app.clone();
            let failed = delivery.clone();
            toast.Failed(
                &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, _| {
                    if let Some(d) = &failed {
                        let r = fail.state::<Recent>();
                        r.finish(&d.label, &d.id, &d.identity, false);
                        r.inner.lock().unwrap().toasts.remove(&d.id);
                        if d.native {
                            crate::window::native_report(&fail, &d.identity, "discard");
                        }
                    }
                    crate::dlog::log("notification: Windows reported asynchronous failure");
                    Ok(())
                }),
            )?;
            if !crate::lock::is_unlocked(&app) || app.state::<Recent>().epoch() != epoch {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800704C7u32 as i32,
                )));
            }
            let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
                &app.config().identifier,
            ))?;
            notifier.Show(&toast)?;
            if !crate::lock::is_unlocked(&app) || app.state::<Recent>().epoch() != epoch {
                let _ = notifier.Hide(&toast);
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800704C7u32 as i32,
                )));
            }
            Ok(toast)
        })();
        if let Some(d) = &delivery {
            let recent = app.state::<Recent>();
            if !crate::lock::is_unlocked(&app) || recent.epoch() != epoch {
                recent.cancel_pending(&d.label, &d.id, &d.identity);
            } else {
                recent.finish(&d.label, &d.id, &d.identity, send.is_ok());
            }
            if let Ok(toast) = &send {
                let mut i = recent.inner.lock().unwrap();

                if i.toasts.len() >= 100 {
                    if let Some(key) = i.toasts.keys().next().cloned() {
                        i.toasts.remove(&key);
                    }
                }
                if crate::lock::is_unlocked(&app) && recent.epoch() == epoch {
                    i.toasts.insert(d.id.clone(), toast.clone());
                }
            }
            if d.native {
                crate::window::native_report(
                    &app,
                    &d.identity,
                    if send.is_ok() && crate::lock::is_unlocked(&app) && recent.epoch() == epoch {
                        "shown"
                    } else {
                        "discard"
                    },
                );
            }
        }
        if let Err(error) = &send {
            crate::dlog::log(&format!("notification: HRESULT={:#x}", error.code().0));
        }
        crate::dlog::log(if send.is_ok() {
            "notification: submitted to Windows (banner visibility unconfirmed)"
        } else {
            "notification: Windows rejected delivery"
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_xml_cannot_be_broken_by_message_content() {
        assert_eq!(escape_xml("<&\"'\0"), "&lt;&amp;&quot;&apos;");
    }
    #[test]
    fn each_real_receipt_covers_only_one_visual_signal() {
        let r = Recent::default();
        let t = Instant::now();
        r.record("wa-a", "one", Source::WebView2, "pending", None);
        r.finish("wa-a", "one", "native-1", true);
        assert!(r.consume_coverage("wa-a", t));
        assert!(!r.consume_coverage("wa-a", t));
        r.record("wa-a", "visual", Source::Visual, "pending", None);
        r.finish("wa-a", "visual", "visual-2", true);
        assert!(!r.consume_coverage("wa-a", t));
    }
    #[test]
    fn asynchronous_failure_removes_the_delivery_receipt() {
        let r = Recent::default();
        let t = Instant::now();
        r.record("wa-a", "one", Source::WebView2, "pending", None);
        r.finish("wa-a", "one", "native-1", true);
        r.finish("wa-a", "one", "native-1", false);
        assert!(!r.consume_coverage("wa-a", t));
        assert!(!r.notified_since("wa-a", t));
        r.finish("wa-a", "one", "native-1", true);
        assert!(!r.notified_since("wa-a", t));
    }
    #[test]
    fn failed_delivery_does_not_cover_fallback_and_can_retry() {
        let r = Recent::default();
        let t = Instant::now();
        assert!(r.reserve("wa-a", "id", t));
        r.record("wa-a", "receipt", Source::Page, "pending", None);
        r.finish("wa-a", "receipt", "id", false);
        assert!(!r.notified_since("wa-a", t));
        assert!(r.reserve("wa-a", "id", t));
    }
    #[test]
    fn identity_is_account_scoped_and_repeated_content_does_not_matter() {
        let r = Recent::default();
        let t = Instant::now();
        assert!(r.reserve("wa-a", "msg1", t));
        assert!(!r.reserve("wa-a", "msg1", t));
        assert!(r.reserve("wa-a", "msg2", t));
        assert!(r.reserve("wa-b", "msg1", t));
    }
    #[test]
    fn success_covers_only_its_account() {
        let r = Recent::default();
        let t = Instant::now();
        r.finish("wa-a", "r", "i", true);
        assert!(r.notified_since("wa-a", t));
        assert!(!r.notified_since("wa-b", t));
    }
    #[test]
    fn foreground_and_active_default_to_enabled() {
        let s = crate::settings::Settings::default();
        assert_eq!(
            suppression(&s, true, true, true, Source::Visual, Some(false)),
            None
        );
    }
    #[test]
    fn silence_lock_and_unknown_are_conservative() {
        let s = crate::settings::Settings::default();
        assert_eq!(
            suppression(&s, false, true, true, Source::Visual, Some(false)),
            Some("locked")
        );
        assert_eq!(
            suppression(&s, true, true, true, Source::Visual, None),
            Some("mute_state_unknown")
        );
        assert_eq!(
            suppression(&s, true, true, true, Source::Visual, Some(true)),
            Some("muted")
        );
    }
    #[test]
    fn settings_can_suppress_foreground_or_current_chat() {
        let mut s = crate::settings::Settings {
            notify_active_chat: false,
            ..Default::default()
        };
        assert_eq!(
            suppression(&s, true, true, true, Source::Visual, Some(false)),
            Some("active_chat_disabled")
        );
        s.notify_when_focused = false;
        assert_eq!(
            suppression(&s, true, true, false, Source::Page, None),
            Some("foreground_disabled")
        );
    }
    #[test]
    fn history_is_bounded_and_has_no_content() {
        let r = Recent::default();
        for _ in 0..150 {
            r.record("wa-a", "id", Source::Page, "pending", None);
        }
        assert_eq!(r.records().len(), 100);
        let value = serde_json::to_value(r.records()).unwrap();
        assert!(value[0].get("body").is_none());
    }
    #[test]
    fn old_unread_fallback_requires_settled_background_increase() {
        assert!(fallback_due(Some(1), 2, false, true));
        assert!(!fallback_due(None, 2, false, true));
        assert!(!fallback_due(Some(1), 2, true, true));
        assert!(!fallback_due(Some(1), 2, false, false));
    }
    #[test]
    fn lock_generation_invalidates_queued_work_across_unlocks() {
        let recent = Recent::default();
        let queued = recent.epoch();
        recent.generation.fetch_add(1, Ordering::SeqCst);
        assert_ne!(queued, recent.epoch());
        recent.record("wa-a", "queued", Source::Page, "pending", None);
        recent.cancel_pending("wa-a", "queued", "event");
        let record = &recent.records()[0];
        assert_eq!(record.status, "suppressed");
        assert_eq!(record.reason.as_deref(), Some("locked"));
    }
}
