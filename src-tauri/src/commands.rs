use crate::accounts::{self, ActiveAccount, UnreadMap};
use crate::applock::{self, AppLockConfig};
use crate::lock;
use crate::settings::Settings;
use serde::Serialize;
use tauri::Manager;

/// Account-management commands must NOT be reachable from a remote WhatsApp page.
/// Account windows carry the `wa-<id>` label; the trusted local `settings` window
/// does not. Tauri injects the calling `window`; the remote page cannot forge its
/// label. WhatsApp pages keep `notify`/`set_unread` (they need them) but are denied
/// every account-management command.
fn is_remote(window: &tauri::Window) -> bool {
    is_remote_label(window.label())
}

/// Pure predicate behind `is_remote`, broken out so it can be unit-tested without a
/// live `tauri::Window`.
fn is_remote_label(label: &str) -> bool {
    label.starts_with("wa-")
}

#[tauri::command]
pub fn notify(window: tauri::Window, app: tauri::AppHandle, title: String, body: String) {
    // issue #3 diagnostics: confirm the command is actually reached from the
    // injected bridge. If this line never appears in the log when a message
    // arrives, the page never called our Notification shim (e.g. it used the
    // service-worker showNotification path), not the OS toast layer. No message
    // content is logged (PII) — only that an event occurred.
    crate::dlog::log("commands::notify invoked");
    // Tell the Rust-side unread fallback (set_unread path) that the page's own
    // notification fired, so it stays quiet for SHIM_GRACE and we never double
    // toast the same message through both paths.
    crate::notify::record_shim_notify(window.label());
    // While locked, suppress notifications entirely so message previews don't leak
    // to the OS notification center / lock screen. The tray unread badge still updates
    // via set_unread (a count only, no content).
    if !crate::lock::is_unlocked(&app) {
        crate::dlog::log("commands::notify suppressed: app is locked");
        return;
    }
    if !crate::settings::load(&app).notifications {
        crate::dlog::log("commands::notify suppressed: notifications disabled in settings");
        return;
    }
    // Prefix the account name when more than one account exists, so notifications
    // are attributable (e.g. "Work: New message").
    let f = accounts::load(&app);
    let title = if f.accounts.len() > 1 {
        if let Some(id) = accounts::id_from_label(window.label()) {
            if let Some(acct) = f.accounts.iter().find(|a| a.id == id) {
                format!("{}: {}", acct.name, title)
            } else {
                title
            }
        } else {
            title
        }
    } else {
        title
    };
    crate::notify::show(&app, &title, &body);
}

/// Diagnostic breadcrumb from the injected page script (bridge.js) into the same
/// file log as the Rust side, so a drag-drop failure can be traced end-to-end on a
/// build with no console. Allowed from the WhatsApp page; it only appends a short,
/// length-capped string we author in bridge.js — no page-controlled PII.
#[tauri::command]
pub fn dlog(msg: String) {
    let msg: String = msg.chars().take(300).collect();
    crate::dlog::log(&format!("js: {msg}"));
}

#[tauri::command]
pub fn set_unread(window: tauri::Window, app: tauri::AppHandle, title: String) {
    let count = crate::unread::parse_unread(&title);
    let Some(id) = accounts::id_from_label(window.label()) else {
        return;
    };

    // Update the per-account count and compute the aggregate, then drop all
    // UnreadMap guards BEFORE calling tray::rebuild_menu (which re-locks the map)
    // to avoid a deadlock.
    let (total, previous) = {
        let state = app.state::<UnreadMap>();
        let mut map = state.lock().unwrap();
        let previous = map.get(id).copied().unwrap_or(0);
        map.insert(id.to_string(), count);
        (accounts::aggregate_unread(&map), previous)
    };

    // Rust-side notification fallback (issue #3). Issue diagnostics proved the
    // page never calls our Notification/showNotification shims on some installs
    // (Windows 11: log shows session start + AUMID ok, but no `commands::notify`),
    // so the JS-driven path silently produces no toast there. The unread count in
    // the <title> is a reliable, engine-independent signal that new messages
    // arrived, so drive a toast from here when the count RISES. Content is never
    // known at this layer (only a count), so the body stays generic — no PII.
    // Skipped when: count didn't increase, the page already notified through the
    // shim (`notify` command ran within the same window), the app is locked
    // (previews must not leak), notifications are disabled, or the window is
    // focused (user is looking at the chat already).
    if count > previous {
        crate::notify::maybe_unread_toast(&app, window.label(), count);
    }

    crate::tray::update_badge(&app, total);
    crate::tray::rebuild_menu(&app);
}

#[tauri::command]
pub fn get_settings(window: tauri::Window, app: tauri::AppHandle) -> Result<Settings, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    Ok(crate::settings::load(&app))
}

#[tauri::command]
pub fn set_settings(
    window: tauri::Window,
    app: tauri::AppHandle,
    settings: Settings,
) -> Result<Option<String>, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let settings = settings.sanitized();
    crate::settings::save(&app, &settings).map_err(|e| e.to_string())?;
    Ok(crate::settings::apply(&app, &settings))
}

#[tauri::command]
pub fn open_settings(window: tauri::Window, app: tauri::AppHandle) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    crate::window::open_settings_window(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Account-management commands (local-only).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub unread: u32,
    pub open: bool,
}

fn account_views(app: &tauri::AppHandle) -> Vec<AccountView> {
    let f = accounts::load(app);
    let map = app.state::<UnreadMap>();
    let map = map.lock().unwrap();
    let mut views: Vec<AccountView> = f
        .accounts
        .iter()
        .map(|a| AccountView {
            id: a.id.clone(),
            name: a.name.clone(),
            order: a.order,
            unread: map.get(&a.id).copied().unwrap_or(0),
            open: app
                .get_webview_window(&accounts::window_label(&a.id))
                .is_some(),
        })
        .collect();
    views.sort_by_key(|v| v.order);
    views
}

#[tauri::command]
pub fn list_accounts(
    window: tauri::Window,
    app: tauri::AppHandle,
) -> Result<Vec<AccountView>, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    Ok(account_views(&app))
}

#[tauri::command]
pub fn add_account(
    window: tauri::Window,
    app: tauri::AppHandle,
    name: String,
) -> Result<AccountView, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    // macOS < 14 cannot isolate additional accounts (no data_store_identifier).
    crate::window::ensure_isolation_supported()?;

    let name = name.trim();
    if name.is_empty() {
        return Err("account name cannot be empty".into());
    }

    let mut f = accounts::load(&app);
    let acct = accounts::add(&mut f, name);
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    crate::window::open_account_window(&app, &acct, false).map_err(|e| e.to_string())?;
    crate::tray::rebuild_menu(&app);

    Ok(AccountView {
        id: acct.id,
        name: acct.name,
        order: acct.order,
        unread: 0,
        open: true,
    })
}

#[tauri::command]
pub fn remove_account(
    window: tauri::Window,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;

    let mut f = accounts::load(&app);
    let removed = accounts::remove(&mut f, &id)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    // Close the window if open.
    if let Some(w) = app.get_webview_window(&accounts::window_label(&removed.id)) {
        let _ = w.destroy();
    }
    // Drop the per-account unread count.
    {
        let state = app.state::<UnreadMap>();
        let mut map = state.lock().unwrap();
        map.remove(&removed.id);
    }
    accounts::delete_profile(&app, &removed.id);

    // Recompute the aggregate badge and the menu.
    let total = {
        let state = app.state::<UnreadMap>();
        let map = state.lock().unwrap();
        accounts::aggregate_unread(&map)
    };
    crate::tray::update_badge(&app, total);
    crate::tray::rebuild_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn rename_account(
    window: tauri::Window,
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("account name cannot be empty".into());
    }

    let mut f = accounts::load(&app);
    accounts::rename(&mut f, &id, name)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    if let Some(w) = app.get_webview_window(&accounts::window_label(&id)) {
        let _ = w.set_title(&format!("whatRust — {name}"));
    }
    crate::tray::rebuild_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn open_account(
    window: tauri::Window,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let f = accounts::load(&app);
    let Some(acct) = f.accounts.iter().find(|a| a.id == id) else {
        return Err(format!("unknown account: {id}"));
    };
    // Open the window if it was closed, then show + focus it.
    if app
        .get_webview_window(&accounts::window_label(&acct.id))
        .is_none()
    {
        crate::window::open_account_window(&app, acct, false).map_err(|e| e.to_string())?;
    }
    crate::window::show_account(&app, &accounts::window_label(&acct.id));
    // Track as the active account.
    if let Some(active) = app.try_state::<ActiveAccount>() {
        *active.lock().unwrap() = accounts::window_label(&acct.id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// App-lock commands.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LockStatus {
    pub enabled: bool,
    pub biometric_available: bool,
    pub biometric_enabled: bool,
    pub biometric_label: String,
    pub lock_on_launch: bool,
    pub lock_on_hide: bool,
    pub idle_secs: u32,
}

/// Read-only status for BOTH the settings window and the lock screen. Never returns
/// the password hash. Allowed from any non-remote window, even while locked.
#[tauri::command]
pub fn get_lock_status(window: tauri::Window, app: tauri::AppHandle) -> Result<LockStatus, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    let c = applock::load(&app);
    let available = matches!(
        crate::biometric::availability(),
        crate::biometric::Availability::Available
    );
    Ok(LockStatus {
        enabled: c.is_active(),
        biometric_available: available,
        biometric_enabled: c.biometric_enabled && available,
        biometric_label: crate::biometric::label().to_string(),
        lock_on_launch: c.lock_on_launch,
        lock_on_hide: c.lock_on_hide,
        idle_secs: c.idle_secs,
    })
}

/// Enable the lock by setting the first password. Errors if already enabled (use
/// `change_app_lock_password`). Runs only from an unlocked, local window.
#[tauri::command]
pub fn set_app_lock_password(
    window: tauri::Window,
    app: tauri::AppHandle,
    new: String,
    confirm: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    if c.is_active() {
        return Err("app lock is already enabled".into());
    }
    if new != confirm {
        return Err("passwords do not match".into());
    }
    if new.chars().count() < 4 {
        return Err("password must be at least 4 characters".into());
    }
    c.password_phc = Some(applock::hash_password(&new)?);
    c.enabled = true;
    applock::save(&app, &c).map_err(|e| e.to_string())?;
    crate::tray::rebuild_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn change_app_lock_password(
    window: tauri::Window,
    app: tauri::AppHandle,
    current: String,
    new: String,
    confirm: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    let phc = c.password_phc.clone().ok_or("app lock is not enabled")?;
    if !applock::verify_password(&current, &phc) {
        return Err("current password is incorrect".into());
    }
    if new != confirm {
        return Err("passwords do not match".into());
    }
    if new.chars().count() < 4 {
        return Err("password must be at least 4 characters".into());
    }
    c.password_phc = Some(applock::hash_password(&new)?);
    applock::save(&app, &c).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn disable_app_lock(
    window: tauri::Window,
    app: tauri::AppHandle,
    current: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    let phc = c.password_phc.clone().ok_or("app lock is not enabled")?;
    if !applock::verify_password(&current, &phc) {
        return Err("current password is incorrect".into());
    }
    c = AppLockConfig::default(); // fully reset: disabled, no hash, no biometric, default triggers
    applock::save(&app, &c).map_err(|e| e.to_string())?;
    crate::tray::rebuild_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn set_app_lock_options(
    window: tauri::Window,
    app: tauri::AppHandle,
    lock_on_launch: bool,
    lock_on_hide: bool,
    idle_secs: u32,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    c.lock_on_launch = lock_on_launch;
    c.lock_on_hide = lock_on_hide;
    c.idle_secs = idle_secs;
    applock::save(&app, &c).map_err(|e| e.to_string())
}

/// Enable/disable biometric. Enabling requires the lock to be set, the platform to
/// report Available, and one successful test authentication.
#[tauri::command]
pub fn set_biometric_enabled(
    window: tauri::Window,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    if enabled {
        if !c.is_active() {
            return Err("set an app-lock password first".into());
        }
        if !matches!(
            crate::biometric::availability(),
            crate::biometric::Availability::Available
        ) {
            return Err("biometric authentication is not available on this device".into());
        }
        if !crate::biometric::authenticate(&app, "Enable biometric unlock for whatRust")? {
            return Err("biometric test did not succeed".into());
        }
    }
    c.biometric_enabled = enabled;
    applock::save(&app, &c).map_err(|e| e.to_string())
}

/// Manual "Lock now" from the settings window.
#[tauri::command]
pub fn lock_app(window: tauri::Window, app: tauri::AppHandle) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    if !applock::load(&app).is_active() {
        return Err("app lock is not enabled".into());
    }
    lock::lock_now(&app);
    Ok(())
}

/// Unlock with the password. Lock-window only; works while locked (no require_unlocked).
#[tauri::command]
pub fn unlock_password(
    window: tauri::Window,
    app: tauri::AppHandle,
    password: String,
) -> Result<bool, String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    let c = applock::load(&app);
    let Some(phc) = c.password_phc else {
        // No lock configured — treat as already unlocked.
        lock::unlock(&app);
        return Ok(true);
    };
    if applock::verify_password(&password, &phc) {
        lock::unlock(&app);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Unlock with biometric. Lock-window only; works while locked.
#[tauri::command]
pub fn unlock_biometric(window: tauri::Window, app: tauri::AppHandle) -> Result<bool, String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    if !applock::load(&app).biometric_enabled {
        return Err("biometric unlock is not enabled".into());
    }
    if crate::biometric::authenticate(&app, "Unlock whatRust")? {
        lock::unlock(&app);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Forgot-password reset: wipe ALL account sessions + the app-lock config, then
/// relaunch fresh (logged out, lock disabled). Lock-window only.
#[tauri::command]
pub fn reset_app_lock(window: tauri::Window, app: tauri::AppHandle) -> Result<(), String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    crate::applock::reset_all(&app);
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::is_remote_label;

    #[test]
    fn is_remote_wa_prefix_is_true() {
        assert!(is_remote_label("wa-default"));
    }

    #[test]
    fn is_remote_wa_acct_is_true() {
        assert!(is_remote_label("wa-acct-2"));
    }

    #[test]
    fn is_remote_settings_is_false() {
        assert!(!is_remote_label("settings"));
    }
}
