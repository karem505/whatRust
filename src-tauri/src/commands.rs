#[cfg(not(windows))]
use crate::accounts::ActiveAccount;
use crate::accounts::{self, UnreadMap};
use crate::applock::{self, AppLockConfig};
use crate::i18n::{tr, trf};
use crate::lock;
use crate::settings::Settings;
use serde::Serialize;
use tauri::Manager;

/// Account-management commands must NOT be reachable from a remote WhatsApp page.
/// Account windows carry the `wa-<id>` label; the trusted local `settings` window
/// does not. Tauri injects the calling `window`; the remote page cannot forge its
/// label. WhatsApp pages keep `notify`/`set_unread` (they need them) but are denied
/// every account-management command.
fn is_remote(window: &tauri::Webview) -> bool {
    is_remote_label(window.label()) || !crate::integration::trusted_local(window)
}

/// Pure predicate behind `is_remote`, broken out so it can be unit-tested without a
/// live `tauri::Webview`.
fn is_remote_label(label: &str) -> bool {
    label.starts_with("wa-")
}

/// `icon` is the sender's picture as a small base64 PNG (bridge.js), when
/// WhatsApp gave one and it could be loaded.
#[tauri::command(async)]
pub fn notify(
    window: tauri::Webview,
    app: tauri::AppHandle,
    title: String,
    body: String,
    icon: Option<String>,
    epoch: Option<u64>,
) {
    if crate::integration::remote(&window).is_err() {
        return;
    }
    // issue #3 diagnostics: confirm the command is actually reached from the
    // injected bridge. No message content is logged (PII).
    crate::dlog::log("commands::notify invoked");
    crate::notify::from_page(&app, window.label(), &title, &body, icon.as_deref(), epoch);
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

/// "Open" on the download toast (issue #21). The page names a download only by
/// the id we gave it; a program or installer is shown in its folder instead of
/// run (see downloads.rs).
#[tauri::command]
pub fn open_download(app: tauri::AppHandle, id: u64) -> Result<(), String> {
    lock::require_unlocked(&app)?;
    let path = app
        .state::<crate::downloads::Downloads>()
        .path(id)
        .ok_or_else(|| tr("Download não encontrado").into_owned())?;
    if crate::downloads::is_risky(&path) {
        crate::dlog::log("download: open refused for a program, revealing instead");
        crate::opener::reveal_file(&app, path);
    } else {
        crate::opener::open_file(&app, path);
    }
    Ok(())
}

/// "Show in folder" on the download toast.
#[tauri::command]
pub fn reveal_download(app: tauri::AppHandle, id: u64) -> Result<(), String> {
    lock::require_unlocked(&app)?;
    let path = app
        .state::<crate::downloads::Downloads>()
        .path(id)
        .ok_or_else(|| tr("Download não encontrado").into_owned())?;
    crate::opener::reveal_file(&app, path);
    Ok(())
}

#[tauri::command(async)]
pub fn set_unread(
    window: tauri::Webview,
    app: tauri::AppHandle,
    title: String,
    settled: Option<bool>,
) {
    if crate::integration::remote(&window).is_err() {
        return;
    }
    let count = crate::unread::parse_unread(&title);
    let Some(id) = accounts::id_from_label(window.label()) else {
        return;
    };

    // Update the per-account count and compute the aggregate, then drop all
    // UnreadMap guards BEFORE calling tray::rebuild_menu (which re-locks the map)
    // to avoid a deadlock.
    let (previous, total) = {
        let state = app.state::<UnreadMap>();
        let mut map = crate::lock::guard(&state);
        let previous = map.insert(id.to_string(), count);
        (previous, accounts::aggregate_unread(&map))
    };

    crate::tray::update_badge(&app, total);
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);

    // A new unread chat while the window is in the background: make sure the
    // user hears about it even when WhatsApp raises its own notification where
    // whatRust can't see it (issue #3). Covered by a real notification, this
    // stays silent; see notify::unread_increased.
    let focused = window.window().is_focused().unwrap_or(false)
        && window.window().is_visible().unwrap_or(false);
    if !cfg!(windows)
        && crate::notify::fallback_due(previous, count, focused, settled.unwrap_or(false))
    {
        crate::notify::unread_increased(&app, window.label(), count);
    }
}

#[tauri::command]
pub fn get_settings(window: tauri::Webview, app: tauri::AppHandle) -> Result<Settings, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    Ok(crate::settings::load(&app))
}

#[tauri::command]
pub fn set_settings(
    window: tauri::Webview,
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

// Commands that build a window are `async`. Tauri runs a synchronous command
// inside WebView2's IPC callback, and creating another WebView2 there deadlocks
// on Windows (wry#583); an async command runs on the async runtime instead.
// Keep every window-creating command async.
#[tauri::command]
pub async fn open_settings(window: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
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
    pub is_default: bool,
    pub detached: bool,
}

fn account_views(app: &tauri::AppHandle) -> Vec<AccountView> {
    let f = accounts::load(app);
    let map = app.state::<UnreadMap>();
    let map = crate::lock::guard(&map);
    let mut views: Vec<AccountView> = f
        .accounts
        .iter()
        .map(|a| AccountView {
            id: a.id.clone(),
            name: a.name.clone(),
            order: a.order,
            unread: map.get(&a.id).copied().unwrap_or(0),
            open: app.get_webview(&accounts::window_label(&a.id)).is_some(),
            detached: cfg!(windows)
                && app
                    .get_webview(&accounts::window_label(&a.id))
                    .is_some_and(|v| v.window().label().starts_with("detached-")),
            is_default: accounts::startup_account(&f).is_some_and(|startup| startup.id == a.id),
        })
        .collect();
    views.sort_by_key(|v| v.order);
    views
}

/// Small account list for the WhatsApp-window switcher. Remote windows may only
/// read display data and request focusing one configured account.
#[derive(Debug, Clone, Serialize)]
pub struct SwitcherAccount {
    pub id: String,
    pub name: String,
    pub unread: u32,
}

#[tauri::command]
pub fn switcher_accounts(
    window: tauri::Webview,
    app: tauri::AppHandle,
) -> Result<Vec<SwitcherAccount>, String> {
    crate::integration::remote(&window)?;
    lock::require_unlocked(&app)?;
    Ok(account_views(&app)
        .into_iter()
        .map(|account| SwitcherAccount {
            id: account.id,
            name: account.name,
            unread: account.unread,
        })
        .collect())
}

#[tauri::command]
pub async fn switch_account(
    window: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    crate::integration::remote(&window)?;
    lock::require_unlocked(&app)?;
    let _guard = accounts::lock_mutations();
    let f = accounts::load(&app);
    let Some(acct) = f.accounts.iter().find(|a| a.id == id) else {
        return Err(tr("Conta não encontrada").into());
    };
    if app.get_webview(&accounts::window_label(&acct.id)).is_none() {
        crate::window::open_account_window(&app, acct, false).map_err(|e| e.to_string())?;
    }
    crate::window::show_account(&app, &accounts::window_label(&acct.id));
    #[cfg(not(windows))]
    if let Some(active) = app.try_state::<ActiveAccount>() {
        *crate::lock::guard(&active) = accounts::window_label(&acct.id);
    }
    Ok(())
}

#[tauri::command]
pub fn list_accounts(
    window: tauri::Webview,
    app: tauri::AppHandle,
) -> Result<Vec<AccountView>, String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    Ok(account_views(&app))
}

#[tauri::command]
pub fn set_default_account(
    window: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let _guard = accounts::lock_mutations();
    let mut f = accounts::load(&app);
    accounts::set_startup_account(&mut f, &id)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_account(
    window: tauri::Webview,
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
        return Err("Informe um nome para a conta.".into());
    }

    let _guard = accounts::lock_mutations();
    let mut f = accounts::load(&app);
    let acct = accounts::add(&mut f, name);
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    if let Err(e) = crate::window::open_account_window(&app, &acct, false) {
        // Don't leave an account behind that never got a window. The sequence
        // number stays consumed, so a retry gets a fresh profile id.
        f.accounts.retain(|a| a.id != acct.id);
        accounts::save(&app, &f).map_err(|rollback| {
            trf(
                "{0}; e não foi possível restaurar a conta: {1}",
                &[&e, &rollback],
            )
        })?;
        return Err(e.to_string());
    }
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);

    let is_default = accounts::startup_account(&f).is_some_and(|startup| startup.id == acct.id);
    Ok(AccountView {
        id: acct.id,
        name: acct.name,
        order: acct.order,
        unread: 0,
        open: true,
        is_default,
        detached: false,
    })
}

#[tauri::command]
pub async fn remove_account(
    window: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;

    let _guard = accounts::lock_mutations();
    let mut f = accounts::load(&app);
    let removed = accounts::remove(&mut f, &id)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    // Close the window if open.
    if let Some(_w) = app.get_webview(&accounts::window_label(&removed.id)) {
        let _ = _w.clear_all_browsing_data();
        #[cfg(windows)]
        crate::sessions::remove_view(&app, &removed.id).map_err(|e| e.to_string())?;
        #[cfg(not(windows))]
        let _ = _w.close();
    }
    // Drop the per-account unread count.
    {
        let state = app.state::<UnreadMap>();
        let mut map = crate::lock::guard(&state);
        map.remove(&removed.id);
    }
    accounts::delete_profile(&app, &removed.id);

    // If the removed account was the active one, point at a surviving account
    // so toggle/show don't target a window that no longer exists. (On Windows
    // the active view is tracked by sessions.rs instead.)
    #[cfg(not(windows))]
    if let Some(active) = app.try_state::<ActiveAccount>() {
        let mut label = active.lock().unwrap_or_else(|e| e.into_inner());
        if *label == accounts::window_label(&removed.id) {
            if let Some(next) = accounts::startup_account(&f) {
                *label = accounts::window_label(&next.id);
            }
        }
    }

    // Recompute the aggregate badge and the menu.
    let total = {
        let state = app.state::<UnreadMap>();
        let map = crate::lock::guard(&state);
        accounts::aggregate_unread(&map)
    };
    crate::tray::update_badge(&app, total);
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);
    Ok(())
}

#[tauri::command]
pub async fn rename_account(
    window: tauri::Webview,
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
        return Err("Informe um nome para a conta.".into());
    }

    let _guard = accounts::lock_mutations();
    let mut f = accounts::load(&app);
    accounts::rename(&mut f, &id, name)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())?;

    if let Some(w) = app.get_webview(&accounts::window_label(&id)) {
        #[cfg(windows)]
        let visible_account =
            crate::sessions::selected(&app, w.window().label()).as_deref() == Some(w.label());
        #[cfg(not(windows))]
        let visible_account = true;
        if visible_account {
            let _ = w.window().set_title(&format!("whatRust — {name}"));
        }
    }
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);
    Ok(())
}

#[tauri::command]
pub async fn open_account(
    window: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let _guard = accounts::lock_mutations();
    let f = accounts::load(&app);
    let Some(acct) = f.accounts.iter().find(|a| a.id == id) else {
        return Err(trf("Conta não encontrada: {0}", &[&id]));
    };
    // Open the window if it was closed, then show + focus it.
    if app.get_webview(&accounts::window_label(&acct.id)).is_none() {
        crate::window::open_account_window(&app, acct, false).map_err(|e| e.to_string())?;
    }
    crate::window::show_account(&app, &accounts::window_label(&acct.id));
    // Track as the active account.
    #[cfg(not(windows))]
    if let Some(active) = app.try_state::<ActiveAccount>() {
        *crate::lock::guard(&active) = accounts::window_label(&acct.id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// App-lock commands.
// ---------------------------------------------------------------------------

/// Argon2 is deliberately slow; keep it off the thread that pumps UI events.
async fn hash_off_thread(password: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || applock::hash_password(&password))
        .await
        .map_err(|e| e.to_string())?
}

async fn verify_off_thread(password: String, phc: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || applock::verify_password(&password, &phc))
        .await
        .unwrap_or(false)
}

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
#[tauri::command(async)]
pub fn get_lock_status(
    window: tauri::Webview,
    app: tauri::AppHandle,
) -> Result<LockStatus, String> {
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
pub async fn set_app_lock_password(
    window: tauri::Webview,
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
        return Err(tr("O bloqueio já está ativado.").into());
    }
    lock::validate_new_password(&new, &confirm)?;
    c.password_phc = Some(hash_off_thread(new).await?);
    c.enabled = true;
    applock::save(&app, &c).map_err(|e| e.to_string())?;
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);
    Ok(())
}

#[tauri::command]
pub async fn change_app_lock_password(
    window: tauri::Webview,
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
    let phc = c
        .password_phc
        .clone()
        .ok_or_else(|| tr("O bloqueio não está ativado.").into_owned())?;
    if !verify_off_thread(current, phc).await {
        return Err(tr("A senha atual está incorreta.").into());
    }
    lock::validate_new_password(&new, &confirm)?;
    c.password_phc = Some(hash_off_thread(new).await?);
    applock::save(&app, &c).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disable_app_lock(
    window: tauri::Webview,
    app: tauri::AppHandle,
    current: String,
) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    lock::require_unlocked(&app)?;
    let mut c = applock::load(&app);
    let phc = c
        .password_phc
        .clone()
        .ok_or_else(|| tr("O bloqueio não está ativado.").into_owned())?;
    if !verify_off_thread(current, phc).await {
        return Err(tr("A senha atual está incorreta.").into());
    }
    c = AppLockConfig::default(); // fully reset: disabled, no hash, no biometric, default triggers
    applock::save(&app, &c).map_err(|e| e.to_string())?;
    crate::tray::rebuild_menu(&app);
    crate::integration::broadcast(&app);
    Ok(())
}

#[tauri::command]
pub fn set_app_lock_options(
    window: tauri::Webview,
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
#[tauri::command(async)]
pub fn set_biometric_enabled(
    window: tauri::Webview,
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
            return Err(tr("Defina uma senha de bloqueio primeiro.").into());
        }
        if !matches!(
            crate::biometric::availability(),
            crate::biometric::Availability::Available
        ) {
            return Err(
                tr("A autenticação biométrica não está disponível neste dispositivo.").into(),
            );
        }
        if !crate::biometric::authenticate(
            &app,
            tr("Ativar desbloqueio por biometria no whatRust").as_ref(),
        )? {
            return Err(tr("Não foi possível confirmar a biometria.").into());
        }
    }
    c.biometric_enabled = enabled;
    applock::save(&app, &c).map_err(|e| e.to_string())
}

/// Manual "Lock now" from the settings window. Async: it builds the lock window.
#[tauri::command]
pub async fn lock_app(window: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    if is_remote(&window) {
        return Err("forbidden".into());
    }
    if !applock::load(&app).is_active() {
        return Err(tr("O bloqueio não está ativado.").into());
    }
    lock::lock_now(&app);
    Ok(())
}

/// Unlock with the password. Lock-window only; works while locked (no require_unlocked).
/// Wrong guesses are rate-limited, and the Argon2 check runs off the UI thread.
#[tauri::command]
pub async fn unlock_password(
    window: tauri::Webview,
    app: tauri::AppHandle,
    password: String,
) -> Result<bool, String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    let c = applock::load(&app);
    let Some(phc) = c.password_phc else {
        // No lock configured: treat as already unlocked.
        lock::unlock(&app);
        return Ok(true);
    };
    {
        let t = lock::guard(&lock::THROTTLE);
        if let Some(Err(secs)) = t.as_ref().map(|t| t.check(std::time::Instant::now())) {
            return Err(format!("throttled:{secs}"));
        }
    }
    let ok = verify_off_thread(password, phc).await;
    {
        let mut t = lock::guard(&lock::THROTTLE);
        let t = t.get_or_insert_with(Default::default);
        if ok {
            t.record_success();
        } else {
            t.record_failure(std::time::Instant::now());
        }
    }
    if ok {
        lock::unlock(&app);
    }
    Ok(ok)
}

/// Unlock with biometric. Lock-window only; works while locked.
#[tauri::command(async)]
pub fn unlock_biometric(window: tauri::Webview, app: tauri::AppHandle) -> Result<bool, String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    if !applock::load(&app).biometric_enabled {
        return Err(tr("O desbloqueio por biometria não está ativado.").into());
    }
    if crate::biometric::authenticate(&app, tr("Desbloquear o whatRust").as_ref())? {
        lock::unlock(&app);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Forgot-password reset: wipe ALL account sessions + the app-lock config, then
/// relaunch fresh (logged out, lock disabled). Lock-window only.
#[tauri::command]
pub fn reset_app_lock(
    window: tauri::Webview,
    app: tauri::AppHandle,
    confirmed: bool,
) -> Result<(), String> {
    if !lock::is_lock_window(&window) {
        return Err("forbidden".into());
    }
    if !confirmed {
        return Err(tr("A redefinição precisa ser confirmada.").into());
    }
    crate::applock::request_reset(&app)?;
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
