//! Narrow page adapter API. Never grant account/password management to WhatsApp.
use crate::{accounts, notify};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Webview};
#[derive(Default)]
pub struct Integration(pub Mutex<HashMap<String, Adapter>>);
#[derive(Clone, Default, Serialize)]
pub struct Adapter {
    pub mounted: bool,
    pub detector: bool,
    pub theme: Option<String>,
    #[serde(skip)]
    pub last_signal: Option<Instant>,
    #[serde(skip)]
    pub signals: u32,
}
pub fn local_url(u: &tauri::Url) -> bool {
    matches!(
        (u.scheme(), u.host_str()),
        ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
    )
}
pub fn trusted_local(v: &Webview) -> bool {
    (matches!(v.label(), "settings" | "lock") || v.label().starts_with("shell-"))
        && v.url().is_ok_and(|u| local_url(&u))
}
fn local(v: &Webview, app: &AppHandle) -> Result<(), String> {
    if !trusted_local(v) || !v.url().is_ok_and(|u| local_url(&u)) {
        return Err("forbidden".into());
    }
    crate::lock::require_unlocked(app)
}
pub fn remote(v: &Webview) -> Result<(), String> {
    if !v.label().starts_with("wa-")
        || !v
            .url()
            .is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some("web.whatsapp.com"))
    {
        return Err("forbidden".into());
    }
    Ok(())
}
#[derive(Clone, Serialize)]
pub struct DisplayAccount {
    pub id: String,
    pub name: String,
    pub unread: u32,
    pub detached: bool,
    pub is_default: bool,
}
pub fn display_accounts(app: &AppHandle) -> Vec<DisplayAccount> {
    let f = accounts::load(app);
    let counts = app.state::<accounts::UnreadMap>().lock().unwrap().clone();
    let startup = accounts::startup_account(&f).map(|a| a.id.clone());
    let mut list = f.accounts.clone();
    list.sort_by_key(|a| a.order);
    list.into_iter()
        .map(|a| DisplayAccount {
            id: a.id.clone(),
            name: a.name,
            unread: counts.get(&a.id).copied().unwrap_or(0),
            detached: cfg!(windows)
                && app
                    .get_webview(&accounts::window_label(&a.id))
                    .is_some_and(|v| v.window().label().starts_with("detached-")),
            is_default: startup.as_deref() == Some(&a.id),
        })
        .collect()
}
#[derive(Serialize)]
pub struct BridgeState {
    pub accounts: Vec<DisplayAccount>,
    pub current_id: String,
    pub native_notifications: bool,
    pub notification_epoch: u64,
    pub notifications_locked: bool,
    pub windows: bool,
}
fn bridge_state(app: &AppHandle, label: &str) -> BridgeState {
    BridgeState {
        accounts: if crate::lock::is_unlocked(app) {
            display_accounts(app)
        } else {
            vec![]
        },
        current_id: accounts::id_from_label(label).unwrap_or("").into(),
        native_notifications: app.state::<notify::Recent>().native_ready(label),
        notification_epoch: app.state::<notify::Recent>().epoch(),
        notifications_locked: !crate::lock::is_unlocked(app),
        windows: cfg!(windows),
    }
}
#[tauri::command]
pub fn integration_state(window: Webview, app: AppHandle) -> Result<BridgeState, String> {
    remote(&window)?;
    Ok(bridge_state(&app, window.label()))
}
pub fn broadcast(app: &AppHandle) {
    for (label, v) in app.webviews() {
        if label.starts_with("wa-") {
            if let Ok(json) = serde_json::to_string(&bridge_state(app, &label)) {
                let _ = v.eval(format!(
                    "window.__whatrustState && window.__whatrustState({json})"
                ));
            }
        } else if label == "settings" {
            let _ = v.eval("typeof loadAccounts === 'function' && !accountsBusy && loadAccounts()");
        } else if label.starts_with("shell-") {
            #[cfg(windows)]
            let current = crate::sessions::selected(app, v.window().label())
                .and_then(|label| accounts::id_from_label(&label).map(str::to_owned));
            #[cfg(not(windows))]
            let current: Option<String> = None;
            if let Ok(json) = serde_json::to_string(&current) {
                let _ = v.eval(format!("window.refresh && window.refresh({json})"));
            }
        }
    }
}
#[tauri::command(async)]
pub fn report_integration(
    window: Webview,
    app: AppHandle,
    mounted: bool,
    detector: bool,
    theme: Option<String>,
) -> Result<(), String> {
    remote(&window)?;
    app.state::<Integration>()
        .0
        .lock()
        .unwrap()
        .entry(window.label().into())
        .or_default()
        .clone_from(&Adapter {
            mounted,
            detector,
            theme: theme.filter(|t| matches!(t.as_str(), "dark" | "light")),
            last_signal: None,
            signals: 0,
        });
    #[cfg(windows)]
    crate::sessions::integration_changed(&app, window.label(), mounted);
    crate::dlog::log(&format!(
        "integration: layout completed controls_mounted={mounted} detector={detector}"
    ));
    Ok(())
}
#[derive(Deserialize)]
pub struct MessageSignal {
    pub id: String,
    pub active: bool,
    pub muted: Option<bool>,
}
#[tauri::command]
pub fn report_message_signal(
    window: Webview,
    app: AppHandle,
    signal: MessageSignal,
) -> Result<(), String> {
    remote(&window)?;
    if !cfg!(windows) {
        return Ok(());
    }
    if signal.id.is_empty() || signal.id.len() > 180 {
        return Err("invalid signal".into());
    }
    let state = app.state::<Integration>();
    let mut m = state.0.lock().unwrap();
    let a = m.entry(window.label().into()).or_default();
    // Bound abusive/faulty adapters independently of the JavaScript limiter.
    if a.last_signal
        .is_none_or(|t| t.elapsed() > Duration::from_secs(60))
    {
        a.last_signal = Some(Instant::now());
        a.signals = 0;
    }
    if a.signals >= 600 {
        return Ok(());
    }
    a.signals += 1;
    drop(m);
    notify::visual_signal(
        &app,
        window.label(),
        &signal.id,
        signal.active,
        signal.muted,
    );
    Ok(())
}
#[tauri::command]
pub async fn open_app_panel(window: Webview, app: AppHandle) -> Result<(), String> {
    if window.label().starts_with("wa-") {
        remote(&window)?;
    } else {
        local(&window, &app)?;
    }
    #[cfg(windows)]
    {
        crate::sessions::open_panel(&app, Some(window.window().label()))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    crate::window::open_settings_window(&app);
    Ok(())
}
#[tauri::command]
pub fn close_app_panel(window: Webview, app: AppHandle) -> Result<(), String> {
    local(&window, &app)?;
    #[cfg(windows)]
    crate::sessions::close_panel(&app);
    Ok(())
}
#[tauri::command]
pub async fn detach_account(window: Webview, app: AppHandle, id: String) -> Result<(), String> {
    local(&window, &app)?;
    let _g = accounts::lock_mutations();
    #[cfg(windows)]
    crate::sessions::detach(&app, &id)?;
    #[cfg(not(windows))]
    return Err("Disponível no Windows".into());
    broadcast(&app);
    Ok(())
}
#[tauri::command]
pub async fn attach_account(window: Webview, app: AppHandle, id: String) -> Result<(), String> {
    local(&window, &app)?;
    let _g = accounts::lock_mutations();
    #[cfg(windows)]
    crate::sessions::attach(&app, &id)?;
    #[cfg(not(windows))]
    return Err("Disponível no Windows".into());
    broadcast(&app);
    Ok(())
}
#[tauri::command]
pub fn reorder_accounts(window: Webview, app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    local(&window, &app)?;
    let _g = accounts::lock_mutations();
    let mut f = accounts::load(&app);
    accounts::reorder(&mut f, &ids)?;
    accounts::save(&app, &f).map_err(|e| e.to_string())?;
    broadcast(&app);
    crate::tray::rebuild_menu(&app);
    Ok(())
}
#[derive(Serialize)]
pub struct Diagnostics {
    pub version: String,
    pub accounts: Vec<DisplayAccount>,
    pub adapters: HashMap<String, Adapter>,
    pub native: HashMap<String, bool>,
    pub recent: Vec<notify::Notice>,
}
#[tauri::command]
pub fn notification_diagnostics(window: Webview, app: AppHandle) -> Result<Diagnostics, String> {
    local(&window, &app)?;
    let accounts = display_accounts(&app);
    let native = accounts
        .iter()
        .map(|a| {
            (
                a.id.clone(),
                app.state::<notify::Recent>()
                    .native_ready(&accounts::window_label(&a.id)),
            )
        })
        .collect();
    Ok(Diagnostics {
        version: app.package_info().version.to_string(),
        accounts,
        adapters: app.state::<Integration>().0.lock().unwrap().clone(),
        native,
        recent: app.state::<notify::Recent>().records(),
    })
}
#[tauri::command]
pub fn test_notification(window: Webview, app: AppHandle) -> Result<String, String> {
    local(&window, &app)?;
    if !crate::settings::load(&app).notifications {
        return Err("Ative as notificações antes do teste.".into());
    }
    let label = app
        .state::<accounts::ActiveAccount>()
        .lock()
        .unwrap()
        .clone();
    notify::from_account(
        &app,
        &label,
        "Teste do whatRust",
        "Se você está vendo este aviso, o Windows exibiu o teste.",
        None,
        notify::Source::Test,
    );
    Ok("Teste solicitado. Confira o banner e a central do Windows; aceitação não confirma exibição.".into())
}
#[tauri::command]
pub fn open_notification_settings(window: Webview, app: AppHandle) -> Result<(), String> {
    local(&window, &app)?;
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                w!("ms-settings:notifications"),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err("Não foi possível abrir as configurações do Windows".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_origin_allowlist_is_exact() {
        for u in [
            "http://tauri.localhost/index.html",
            "tauri://localhost/index.html",
        ] {
            assert!(local_url(&u.parse().unwrap()));
        }
        for u in [
            "https://web.whatsapp.com",
            "http://localhost:8080",
            "https://tauri.localhost.evil.test",
        ] {
            assert!(!local_url(&u.parse().unwrap()));
        }
    }
}
