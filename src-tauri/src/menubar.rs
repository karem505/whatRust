//! macOS menu-bar mode (`menubar_only`).
//!
//! The panel is the account's window (`wa-<id>`), restyled: one webview
//! per account, one session per store, one notification per message. A
//! dismissed panel is a hidden window whose page keeps running: account
//! windows set the WKWebView inactive-scheduling policy to `throttle`
//! (macOS 14+, Tauri's `background_throttling`), so WhatsApp's socket stays
//! up while nothing is on screen. The geometry captured before the first
//! restyle is replayed when the window goes back to normal.

/// Pure helpers, unit-tested on every platform (the panel is macOS-only).
pub(crate) mod geom {
    use std::time::{Duration, Instant};

    /// Panel size (logical pts); landscape so chat list + conversation fit.
    pub const PANEL: (f64, f64) = (760.0, 560.0);
    /// A tray click this soon after a focus-loss dismissal closes the panel
    /// rather than reopening it (the click itself caused the dismissal).
    pub const REOPEN_GRACE: Duration = Duration::from_millis(500);

    /// Top-left origin (monitor-local, physical px) of a `panel_w` panel
    /// centred under tray icon `(ix, iy, iw, ih)`, clamped inside `mon_w`.
    pub fn panel_origin(
        ix: f64,
        iy: f64,
        iw: f64,
        ih: f64,
        panel_w: f64,
        mon_w: f64,
        margin: f64,
    ) -> (f64, f64) {
        let min = margin.min(mon_w / 2.0);
        let x = (ix + (iw - panel_w) / 2.0).clamp(min, (mon_w - panel_w - margin).max(min));
        (x.max(0.0), (iy + ih).max(0.0))
    }

    pub fn within_grace(last_hide: Option<Instant>, now: Instant) -> bool {
        last_hide.is_some_and(|t| now.duration_since(t) < REOPEN_GRACE)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn panel_sits_below_the_icon_and_stays_on_the_monitor() {
            let (x, y) = panel_origin(1500.0, 0.0, 44.0, 24.0, 1520.0, 3456.0, 16.0);
            assert_eq!(y, 24.0); // just under the menu bar, centred on the icon
            assert!((x - (1500.0 + (44.0 - 1520.0) / 2.0)).abs() < 0.001);
            for icon in [10.0, 3400.0] {
                // icons near either edge cannot push the panel off-screen
                let (x, _) = panel_origin(icon, 0.0, 44.0, 24.0, 1520.0, 3456.0, 16.0);
                assert!((16.0..=3456.0 - 1520.0 - 16.0).contains(&x));
            }
            // ...not even on a monitor narrower than the panel.
            let (x, y) = panel_origin(0.0, 0.0, 44.0, 24.0, 3000.0, 1000.0, 8.0);
            assert!(x.is_finite() && x >= 0.0 && y == 24.0);
        }

        #[test]
        fn reopen_grace_expires() {
            let now = Instant::now();
            assert!(within_grace(Some(now), now));
            assert!(!within_grace(Some(now - REOPEN_GRACE), now));
            assert!(!within_grace(None, now));
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::geom;
    use crate::accounts::ActiveAccount;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;
    use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

    /// Geometry (outer pos x, y + inner size w, h, physical px) to restore a
    /// window to when it stops being a panel.
    type SavedGeometry = (i32, i32, u32, u32);
    /// Restyled windows. `None` until first use (`HashMap::new` is not a const
    /// constructor).
    static RESTYLED: Mutex<Option<HashMap<String, SavedGeometry>>> = Mutex::new(None);
    /// The one window currently presented as a panel.
    static PRESENTED: Mutex<Option<String>> = Mutex::new(None);
    /// Last focus-loss dismissal (drives the reopen grace).
    static LAST_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
    /// Whether menu-bar-only mode was on when we last looked (toggle detection).
    static ONLY_MODE: AtomicBool = AtomicBool::new(false);
    /// App Nap shield, held only while menu-bar-only mode is on.
    static KEEP_ALIVE: Mutex<Option<ActivityPtr>> = Mutex::new(None);

    /// NSProcessInfoActivity as a raw pointer: the objc2 object is not `Send`
    /// and this must live in a static. SAFETY: only touched under KEEP_ALIVE.
    struct ActivityPtr(*mut objc2::runtime::AnyObject);
    unsafe impl Send for ActivityPtr {}

    /// The main queue the macOS window setters are queued on. They apply
    /// asynchronously while `show()` applies immediately, so anything that
    /// must land after a setter runs through here.
    fn on_main_queue(f: impl FnOnce() + Send + 'static) {
        use dispatch2::DispatchQueue;
        DispatchQueue::main().exec_async(f);
    }

    /// Hand the app back to whoever had it, so a dismissed panel does not
    /// stay key and swallow keystrokes.
    fn deactivate_app() {
        on_main_queue(move || unsafe {
            let cls = objc2::runtime::AnyClass::get(c"NSApplication").expect("NSApplication class");
            let app: *mut objc2::runtime::AnyObject = objc2::msg_send![cls, sharedApplication];
            let _: () = objc2::msg_send![app, deactivate];
        });
    }

    /// Apply a frame (outer position + inner size, physical px, top-left
    /// origin) SYNCHRONOUSLY. The window setters apply asynchronously, which
    /// is fine while the app runs, but the exit saves window state before
    /// queued work can land, so a panel restore at quit has to be immediate.
    /// Must run on the main thread.
    fn apply_frame_sync(win: &WebviewWindow, app: &AppHandle, x: i32, y: i32, w: u32, h: u32) {
        use objc2_core_foundation::{CGPoint, CGRect, CGSize};
        let Ok(p) = win.ns_window() else {
            return;
        };
        let scale = win.scale_factor().unwrap_or(1.0);
        // NSWindow frames are in points with a bottom-left origin.
        let (x, y) = (x as f64 / scale, y as f64 / scale);
        let (w, h) = (w as f64 / scale, h as f64 / scale);
        let main_h = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| m.size().height as f64 / m.scale_factor())
            .unwrap_or(1050.0);
        let frame = CGRect::new(CGPoint::new(x, main_h - y - h), CGSize::new(w, h));
        unsafe {
            let ns = p as *mut objc2::runtime::AnyObject;
            let no: bool = false;
            let _: () = objc2::msg_send![ns, setFrame: frame, display: no];
        }
    }

    fn with_restyled<R>(f: impl FnOnce(&mut HashMap<String, SavedGeometry>) -> R) -> Option<R> {
        let mut guard = RESTYLED.lock().ok()?;
        Some(f(guard.get_or_insert_with(HashMap::new)))
    }

    pub fn is_restyled(label: &str) -> bool {
        with_restyled(|m| m.contains_key(label)).unwrap_or(false)
    }

    /// A dismissed panel: restyled but not presented, i.e. its window is
    /// hidden (`is_visible()` is false and it cannot take clicks).
    pub fn is_dismissed(label: &str) -> bool {
        is_restyled(label) && presented().as_deref() != Some(label)
    }

    fn presented() -> Option<String> {
        PRESENTED.lock().ok().and_then(|p| p.clone())
    }

    fn set_presented(label: Option<String>) {
        if let Ok(mut p) = PRESENTED.lock() {
            *p = label;
        }
    }

    /// The presented panel. A marker whose window is gone or hidden is
    /// dropped, so tray toggles never target a window that is not there.
    fn panel_on_screen(app: &AppHandle) -> Option<String> {
        let label = presented()?;
        let live = app
            .get_webview_window(&label)
            .is_some_and(|w| w.is_visible().unwrap_or(false));
        if live {
            Some(label)
        } else {
            set_presented(None);
            None
        }
    }

    /// The account a panel targets: the active one, else the first by order.
    fn target_label(app: &AppHandle) -> Option<String> {
        let label = app
            .try_state::<ActiveAccount>()
            .and_then(|a| a.lock().ok().map(|g| g.clone()));
        if let Some(l) = label.filter(|l| crate::accounts::id_from_label(l).is_some()) {
            return Some(l);
        }
        let mut accounts = crate::accounts::load(app).accounts;
        accounts.sort_by_key(|a| a.order);
        accounts
            .first()
            .map(|a| crate::accounts::window_label(&a.id))
    }

    /// The account's window, created hidden if it does not exist. Built by
    /// `open_account_window`, so the panel gets the same user agent, link
    /// routing, downloads and session isolation as the window.
    fn ensure_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
        if let Some(w) = app.get_webview_window(label) {
            return Some(w);
        }
        let id = crate::accounts::id_from_label(label)?;
        let account = crate::accounts::load(app)
            .accounts
            .into_iter()
            .find(|a| a.id == id)?;
        crate::window::open_account_window(app, &account, true).ok()
    }

    /// Present `label` (default: the target account) as the panel under the
    /// tray icon `rect`, parking top-right of the monitor without one.
    pub fn reveal(app: &AppHandle, label: Option<&str>, rect: Option<(f64, f64, f64, f64)>) {
        if !crate::lock::is_unlocked(app) {
            crate::lock::show_lock_window(app);
            return;
        }
        let Some(label) = label
            .map(str::to_string)
            .or_else(|| target_label(app))
            .filter(|l| crate::accounts::id_from_label(l).is_some())
        else {
            return;
        };
        let Some(win) = ensure_window(app, &label) else {
            return;
        };
        if let Some(other) = presented() {
            if other != label {
                dismiss(app, &other); // one panel at a time
            }
        }
        // The panel goes on the monitor holding the tray icon, which can be
        // a different screen than the window is parked on. The rect is
        // physical px, so hit-test against the physical monitor bounds.
        let monitor = rect
            .and_then(|(x, y, ..)| {
                app.available_monitors()
                    .ok()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|m| {
                        let p = m.position();
                        let s = m.size();
                        let (l, t) = (p.x as f64, p.y as f64);
                        let (r, b) = (l + s.width as f64, t + s.height as f64);
                        x >= l && y >= t && x < r && y < b
                    })
            })
            .or_else(|| win.current_monitor().ok().flatten())
            .or_else(|| win.primary_monitor().ok().flatten());
        let scale = monitor
            .as_ref()
            .map(|m| m.scale_factor())
            .unwrap_or_else(|| win.scale_factor().unwrap_or(1.0));
        let (w, h) = (geom::PANEL.0 * scale, geom::PANEL.1 * scale);

        // Remember the window's real geometry, before the first restyle.
        let pos = win.outer_position().unwrap_or_default();
        let size = win.inner_size().unwrap_or_default();
        with_restyled(|m| {
            m.entry(label.clone())
                .or_insert((pos.x, pos.y, size.width, size.height));
        });

        // Set SIZE before POSITION: the position anchors the top-left using
        // the window's current height, so the panel size must apply first.
        let (mon_pos, mon_w) = monitor
            .as_ref()
            .map(|m| (*m.position(), m.size().width as f64))
            .unwrap_or((PhysicalPosition::new(0, 0), w + 16.0 * scale));
        let (x, y) = match rect {
            Some((ix, iy, iw, ih)) => geom::panel_origin(
                ix - mon_pos.x as f64,
                iy - mon_pos.y as f64,
                iw,
                ih,
                w,
                mon_w,
                8.0 * scale,
            ),
            // Park top-right, just under the menu bar (scale-aware).
            None => (mon_w - w - 16.0 * scale, 36.0 * scale),
        };
        let _ = win.set_decorations(false);
        let _ = win.set_resizable(false);
        let _ = win.set_always_on_top(true);
        let _ = win.set_size(PhysicalSize::new(w as u32, h as u32));
        let _ = win.set_position(PhysicalPosition::new(
            mon_pos.x + x.max(0.0) as i32,
            mon_pos.y + y.max(0.0) as i32,
        ));
        // show() would run ahead of the setters above and flash the old
        // full-size frame; queued here it runs after them. The window is
        // hidden, so it appears at its final size and position. PRESENTED is
        // published in the same block, once the window is really on screen.
        let w = win.clone();
        on_main_queue(move || {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            set_presented(Some(label));
        });
    }

    /// Dismiss the panel but KEEP IT ALIVE: hide the window. The webview
    /// keeps running while hidden (see `background_throttling`).
    pub fn dismiss(app: &AppHandle, label: &str) {
        let Some(win) = app.get_webview_window(label) else {
            if presented().as_deref() == Some(label) {
                set_presented(None); // window gone
            }
            with_restyled(|m| m.remove(label));
            return;
        };
        let was_presented = presented().as_deref() == Some(label);
        if was_presented {
            // Clear PRESENTED before hiding: the hide triggers Focused(false),
            // which must not re-enter this dismissal.
            set_presented(None);
            if let Ok(mut t) = LAST_HIDE.lock() {
                *t = Some(Instant::now());
            }
        }
        let _ = win.hide();
        if was_presented {
            deactivate_app();
        }
    }

    /// Focus-loss dismissal of a presented panel.
    pub fn on_focus_lost(app: &AppHandle, label: &str) {
        if crate::lock::is_unlocked(app) && presented().as_deref() == Some(label) {
            dismiss(app, label);
        }
    }

    /// Take a restyled window back to a normal window, replaying the captured
    /// geometry. Returns false if it was not restyled; the caller shows and
    /// focuses it either way.
    pub fn restore_window(app: &AppHandle, label: &str) -> bool {
        let Some((x, y, w, h)) = with_restyled(|m| m.remove(label)).flatten() else {
            return false;
        };
        if presented().as_deref() == Some(label) {
            set_presented(None);
        }
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_always_on_top(false);
            let _ = win.set_resizable(true);
            let _ = win.set_decorations(true);
            apply_frame_sync(&win, app, x, y, w, h);
        }
        true
    }

    /// Restore every restyled window. Called when menu-bar mode is left,
    /// and on quit before window state is saved. Windows that were dismissed
    /// stay off screen except the active one, which the caller shows.
    pub fn restore_all(app: &AppHandle) {
        let labels: Vec<String> =
            with_restyled(|m| m.keys().cloned().collect()).unwrap_or_default();
        let active = app
            .try_state::<ActiveAccount>()
            .and_then(|a| a.lock().ok().map(|g| g.clone()));
        for label in labels {
            let dismissed = is_dismissed(&label);
            restore_window(app, &label);
            if dismissed && Some(label.clone()) != active {
                if let Some(win) = app.get_webview_window(&label) {
                    let _ = win.hide();
                }
            }
        }
    }

    /// After an unlock, put a window the lock hid back how it was: the
    /// presented panel returns as a panel, a dismissed one stays hidden.
    /// Returns false when the window was never restyled.
    pub fn restore_after_lock(app: &AppHandle, label: &str) -> bool {
        if !is_restyled(label) {
            return false;
        }
        if presented().as_deref() == Some(label) {
            reveal(app, Some(label), None);
        } else if let Some(win) = app.get_webview_window(label) {
            let _ = win.hide();
        }
        true
    }

    /// Tray left-click in menu-bar mode.
    pub fn tray_click(app: &AppHandle, rect: Option<(f64, f64, f64, f64)>) {
        if !crate::lock::is_unlocked(app) {
            crate::lock::show_lock_window(app);
            return;
        }
        if let Some(label) = panel_on_screen(app) {
            dismiss(app, &label);
            return;
        }
        if rect.is_some()
            && LAST_HIDE
                .lock()
                .ok()
                .and_then(|t| *t)
                .is_some_and(|t| geom::within_grace(Some(t), Instant::now()))
        {
            return; // this very click dismissed the panel
        }
        reveal(app, None, rect);
    }

    /// The shortcut / `whatrust --toggle`: presented -> dismissed,
    /// otherwise -> presented.
    pub fn toggle(app: &AppHandle) {
        tray_click(app, None);
    }

    /// Menu-bar-only mode: no standard windows. Every account KEEPS its
    /// webview, hidden and connected (the inactive-scheduling policy keeps
    /// the page running), so notifications keep arriving. A presented panel
    /// stays up and hides on its next focus loss.
    pub fn enter_only(app: &AppHandle) {
        if !supported() {
            return; // macOS < 14: a hidden webview would be suspended
        }
        for a in crate::accounts::load(app).accounts {
            let label = crate::accounts::window_label(&a.id);
            if app.get_webview_window(&label).is_none() {
                let _ = crate::window::open_account_window(app, &a, true);
            }
        }
        let presented = presented();
        for (label, win) in app.webview_windows() {
            if label.starts_with("wa-") && Some(label.as_str()) != presented.as_deref() {
                let pos = win.outer_position().unwrap_or_default();
                let size = win.inner_size().unwrap_or_default();
                with_restyled(|m| {
                    m.entry(label.clone())
                        .or_insert((pos.x, pos.y, size.width, size.height));
                });
                let _ = win.hide();
            }
        }
        ONLY_MODE.store(true, Ordering::Release);
    }

    /// Leave menu-bar-only mode if it was on. True on the transition.
    pub fn leave_only(app: &AppHandle) -> bool {
        if !ONLY_MODE.swap(false, Ordering::AcqRel) {
            return false;
        }
        restore_all(app);
        true
    }

    /// Seed the mode tracker at launch so the first `settings::apply` does
    /// not read as a live toggle.
    pub fn note_mode(menubar_only: bool) {
        ONLY_MODE.store(menubar_only, Ordering::Release);
    }

    /// Whether menu-bar-only mode can work on this machine. A hidden webview
    /// keeps its page (and WhatsApp's socket) alive only on macOS 14+, where
    /// WKWebView honours the inactive-scheduling policy (Tauri's
    /// `background_throttling`). On macOS 12 and 13 a hidden page is
    /// suspended, so the mode is not offered.
    pub fn supported() -> bool {
        use objc2_foundation::NSProcessInfo;
        let major = NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion;
        super::allowed_from_version(major as i64)
    }

    /// Hold a `UserInitiatedAllowingIdleSystemSleep` activity so App Nap
    /// does not slow the app's own timers (the unread fallback, tray menu
    /// rebuilds) while menu-bar-only mode runs. The webviews stay alive via
    /// `background_throttling`; this only covers the Rust side. Held ONLY
    /// while menu-bar-only mode is on: a window app keeps the default power
    /// policy.
    pub fn set_keep_alive(on: bool) {
        let Ok(mut slot) = KEEP_ALIVE.lock() else {
            return;
        };
        if on {
            if slot.is_some() {
                return;
            }
            use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
            let reason = NSString::from_str("whatRust keeps its menu-bar sessions connected");
            let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
                &reason,
            );
            *slot = Some(ActivityPtr(
                objc2::rc::Retained::into_raw(activity) as *mut objc2::runtime::AnyObject
            ));
        } else if let Some(ptr) = slot.take() {
            use objc2_foundation::NSProcessInfo;
            // SAFETY: the pointer is exactly what beginActivityWithOptions:reason:
            // returned (retained).
            if let Some(obj) = unsafe { objc2::rc::Retained::from_raw(ptr.0) } {
                // SAFETY: the activity is the one this process info handed out.
                unsafe {
                    let info = NSProcessInfo::processInfo();
                    let _: () = objc2::msg_send![&*info, endActivity: &*obj];
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;

/// Menu-bar-only mode is macOS-only; other platforms have no panel mode.
#[cfg(not(target_os = "macos"))]
pub fn supported() -> bool {
    false
}

/// Whether menu-bar-only mode is on: the panel is the app. The `supported`
/// gate keeps a settings file (or a build that ran on macOS 14+) from putting
/// an older Mac into a mode whose webviews would be suspended when hidden.
pub fn enabled(app: &tauri::AppHandle) -> bool {
    supported() && crate::settings::load(app).menubar_only
}

/// Whether `major` is a macOS version whose WKWebView honours the
/// inactive-scheduling policy (macOS 14, where `inactiveSchedulingPolicy`
/// shipped). Pure, so the gate is testable without an OS look-up.
pub(crate) fn allowed_from_version(major: i64) -> bool {
    major >= 14
}

#[cfg(test)]
mod tests {
    use super::allowed_from_version;

    #[test]
    fn menu_bar_mode_needs_the_macos14_scheduling_policy() {
        assert!(!allowed_from_version(12));
        assert!(!allowed_from_version(13));
        assert!(allowed_from_version(14));
        assert!(allowed_from_version(15));
    }
}
