//! Windows account ownership. One live WebView per profile; detach moves it.
use crate::accounts::{self, Account, ActiveAccount};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Manager, WebviewBuilder, WebviewUrl, Window, WindowBuilder,
};

#[derive(Default)]
pub struct Sessions {
    main_active: Mutex<Option<String>>,
    fallback: Mutex<HashSet<String>>,
    pending: Mutex<Option<String>>,
    panel_parent: Mutex<Option<String>>,
    menus_hidden: Mutex<HashMap<String, bool>>,
}

fn state(app: &AppHandle) -> tauri::State<'_, Sessions> {
    app.state::<Sessions>()
}

pub fn initialize(app: &AppHandle, hidden: bool) -> tauri::Result<Window> {
    if let Some(w) = app.get_window("main") {
        return Ok(w);
    }
    let w = WindowBuilder::new(app, "main")
        .title("whatRust")
        .background_color(tauri::utils::config::Color(22, 23, 23, 255))
        .inner_size(1100., 800.)
        .min_inner_size(560., 480.)
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/128x128.png"
        ))?)?
        .visible(!hidden)
        .build()?;
    install_menu(app, &w)?;
    let shell = WebviewBuilder::new("shell-main", WebviewUrl::App("shell.html".into()))
        .data_directory(app.path().app_data_dir()?.join("management"))
        .on_navigation(crate::integration::local_url);
    w.add_child(
        shell,
        tauri::LogicalPosition::new(0., 0.),
        tauri::LogicalSize::new(1100., 800.),
    )?;
    let a = app.clone();
    w.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            layout(&a, "main")
        }
        tauri::WindowEvent::Focused(true) => {
            if let Some(label) = state(&a).main_active.lock().unwrap().clone() {
                *a.state::<ActiveAccount>().lock().unwrap() = label;
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if crate::settings::load(&a).close_to_tray {
                api.prevent_close();
                let c = crate::applock::load(&a);
                if c.should_lock_on_hide() {
                    crate::lock::lock_now(&a);
                }
                if let Some(w) = a.get_window("main") {
                    let _ = w.hide();
                }
            } else {
                a.exit(0);
            }
        }
        _ => {}
    });
    Ok(w)
}

fn install_menu(app: &AppHandle, w: &Window) -> tauri::Result<()> {
    let accounts = MenuItemBuilder::with_id("app.accounts", "Contas")
        .accelerator("Ctrl+Shift+A")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("app.settings", "Configurações do whatRust")
        .accelerator("Ctrl+,")
        .build(app)?;
    let back = MenuItemBuilder::with_id("app.back", "Voltar ao WhatsApp").build(app)?;
    let reload = MenuItemBuilder::with_id("app.reload", "Recarregar conta").build(app)?;
    let lock = MenuItemBuilder::with_id("app.lock", "Bloquear").build(app)?;
    let quit = MenuItemBuilder::with_id("app.quit", "Sair do whatRust").build(app)?;
    let submenu = SubmenuBuilder::new(app, "whatRust")
        .items(&[&accounts, &settings, &back, &reload, &lock, &quit])
        .build()?;
    let menu = MenuBuilder::new(app).item(&submenu).build()?;
    w.set_menu(menu)?;
    let a = app.clone();
    let parent = w.label().to_string();
    w.on_menu_event(move |_, event| {
        let a=a.clone(); let parent=parent.clone(); let id=event.id().as_ref().to_string();
        tauri::async_runtime::spawn_blocking(move || {
            match id.as_str() {
                "app.accounts" | "app.settings" => {
                    let _=open_panel(&a,Some(&parent));
                    if id=="app.accounts" { if let Some(v)=a.get_webview("settings") { let _=v.eval("window.dispatchEvent(new HashChangeEvent('hashchange'));document.querySelector('a[href=\"#sec-contas\"]').click()"); } }
                },
                "app.back"=>close_panel(&a),
                "app.reload"=> { if crate::lock::is_unlocked(&a) { if let Some(label)=selected(&a,&parent) { if let Some(v)=a.get_webview(&label) { let _=v.reload(); } } } },
                "app.lock"=> { if crate::applock::load(&a).is_active() { crate::lock::lock_now(&a); } },
                "app.quit"=>a.exit(0),
                _=>{}
            }
        });
    });
    Ok(())
}

pub fn selected(app: &AppHandle, parent: &str) -> Option<String> {
    if parent == "main" {
        return state(app).main_active.lock().unwrap().clone();
    }
    app.webviews()
        .into_iter()
        .find(|(l, v)| l.starts_with("wa-") && v.window().label() == parent)
        .map(|(l, _)| l)
}

pub fn open_account(app: &AppHandle, account: &Account, hidden: bool) -> tauri::Result<()> {
    let parent = initialize(app, hidden)?;
    let label = accounts::window_label(&account.id);
    if app.get_webview(&label).is_none() {
        let view = crate::window::build_account_view(app, &parent, account)?;
        let _ = view.hide();
    }
    if state(app).main_active.lock().unwrap().is_none() {
        *state(app).main_active.lock().unwrap() = Some(label.clone());
    }
    if !hidden {
        activate(app, &label);
    } else {
        layout(app, "main");
    }
    Ok(())
}

pub fn activate(app: &AppHandle, label: &str) {
    // Commit identity, visibility, title and published state in one UI task.
    // Calls from notifications, tray and IPC must never interleave these steps.
    let handle = app.clone();
    let label = label.to_string();
    if app
        .run_on_main_thread(move || activate_on_main(&handle, &label))
        .is_err()
    {
        crate::dlog::log("account activation: UI dispatch failed");
    }
}

fn activate_on_main(app: &AppHandle, label: &str) {
    if !crate::lock::is_unlocked(app) {
        *state(app).pending.lock().unwrap() = Some(label.to_string());
        crate::lock::show_lock_window(app);
        return;
    }
    let Some(view) = app.get_webview(label) else {
        return;
    };
    let parent = view.window();
    if parent.label() == "main" {
        *state(app).main_active.lock().unwrap() = Some(label.to_string());
    }
    if let Some(panel) = app.get_webview("settings") {
        if panel.window().label() == parent.label() {
            let _ = panel.hide();
            *state(app).panel_parent.lock().unwrap() = None;
        }
    }
    for (other, v) in app.webviews() {
        if other.starts_with("wa-") && v.window().label() == parent.label() && other != label {
            let _ = v.hide();
        }
    }
    *app.state::<ActiveAccount>().lock().unwrap() = label.to_string();
    sync_menu(app, parent.label());
    let name = accounts::id_from_label(label)
        .and_then(|id| {
            accounts::load(app)
                .accounts
                .into_iter()
                .find(|a| a.id == id)
        })
        .map(|a| a.name)
        .unwrap_or_else(|| "WhatsApp".into());
    let _ = parent.set_title(&format!("whatRust — {name}"));
    let _ = parent.show();
    let _ = parent.unminimize();
    let _ = view.show();
    // WebView2 must be visible before its final bounds/viewport are applied.
    layout(app, parent.label());
    let _ = parent.set_focus();
    let _ = view.set_focus();
    crate::integration::broadcast(app);
}

pub fn layout(app: &AppHandle, parent: &str) {
    let Some(w) = app.get_window(parent) else {
        return;
    };
    let Ok(size) = w.inner_size() else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let active = selected(app, parent);
    let top = if parent == "main"
        && active
            .as_ref()
            .is_some_and(|l| state(app).fallback.lock().unwrap().contains(l))
    {
        48.
    } else {
        0.
    };
    for (label, v) in app.webviews() {
        if v.window().label() != parent {
            continue;
        }
        // Use the current physical client size: no DPI rounding gap, and never
        // mutate the native menu from a resize callback (SetMenu emits WM_SIZE).
        let offset = (top * scale).round() as u32;
        let is_account = label.starts_with("wa-");
        let y = if is_account { offset } else { 0 };
        let shell = label.starts_with("shell-");
        let height = if shell && active.is_some() {
            offset.max(1)
        } else {
            size.height.saturating_sub(y).max(1)
        };
        let _ = v.set_bounds(tauri::Rect {
            position: tauri::PhysicalPosition::new(0, y as i32).into(),
            size: tauri::PhysicalSize::new(size.width.max(1), height).into(),
        });
        if shell {
            let theme = active
                .as_ref()
                .and_then(|label| {
                    app.state::<crate::integration::Integration>()
                        .0
                        .lock()
                        .unwrap()
                        .get(label)
                        .and_then(|a| a.theme.clone())
                })
                .unwrap_or_else(|| "system".into());
            if let Ok(json) = serde_json::to_string(&theme) {
                let _=v.eval(format!("if(document.documentElement&&document.documentElement.dataset.theme!=={json})document.documentElement.dataset.theme={json}"));
            }
            let panel_open = state(app).panel_parent.lock().unwrap().as_deref() == Some(parent);
            if !panel_open && (active.is_none() || offset > 0) {
                let _ = v.show();
            } else {
                let _ = v.hide();
            }
        }
    }
}

// Tauri holds its menu mutex while SetMenu synchronously dispatches WM_SIZE.
// Changing menus inside layout re-enters that mutex and freezes the UI thread.
// Record transitions before invoking native APIs; resize only changes bounds.
fn sync_menu(app: &AppHandle, parent: &str) {
    let hidden = selected(app, parent).is_some_and(|label| {
        app.state::<crate::integration::Integration>()
            .0
            .lock()
            .unwrap()
            .get(&label)
            .is_some_and(|adapter| adapter.mounted)
    });
    let changed = {
        let s = state(app);
        let mut menus = s.menus_hidden.lock().unwrap();
        menus.insert(parent.into(), hidden) != Some(hidden)
    };
    if changed {
        if let Some(w) = app.get_window(parent) {
            let result = if hidden { w.hide_menu() } else { w.show_menu() };
            if result.is_err() {
                state(app).menus_hidden.lock().unwrap().remove(parent);
            }
        }
    }
}

pub fn integration_changed(app: &AppHandle, label: &str, mounted: bool) {
    let s = state(app);
    let mut f = s.fallback.lock().unwrap();
    if mounted {
        f.remove(label);
    } else {
        f.insert(label.into());
    }
    drop(f);
    if let Some(v) = app.get_webview(label) {
        let parent = v.window();
        if selected(app, parent.label()).as_deref() == Some(label) {
            sync_menu(app, parent.label());
            layout(app, parent.label());
        }
    }
}

pub fn detach(app: &AppHandle, id: &str) -> Result<(), String> {
    crate::lock::require_unlocked(app)?;
    let label = accounts::window_label(id);
    let v = app.get_webview(&label).ok_or("Conta não está aberta")?;
    if v.window().label() != "main" {
        activate(app, &label);
        return Ok(());
    }
    let parent_label = format!("detached-{id}");
    let w = WindowBuilder::new(app, &parent_label)
        .title("whatRust — Conta destacada")
        .background_color(tauri::utils::config::Color(22, 23, 23, 255))
        .inner_size(1000., 760.)
        .min_inner_size(560., 480.)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    install_menu(app, &w).map_err(|e| e.to_string())?;
    v.reparent(&w).map_err(|e| e.to_string())?;
    let a = app.clone();
    let account = id.to_string();
    let p = parent_label.clone();
    let l = label.clone();
    w.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            layout(&a, &p)
        }
        tauri::WindowEvent::Focused(true) => {
            *a.state::<ActiveAccount>().lock().unwrap() = l.clone();
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let a = a.clone();
            let id = account.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = attach(&a, &id);
            });
        }
        _ => {}
    });
    choose_remaining(app, Some(&label));
    activate(app, &label);
    Ok(())
}

pub fn attach(app: &AppHandle, id: &str) -> Result<(), String> {
    crate::lock::require_unlocked(app)?;
    let label = accounts::window_label(id);
    let v = app.get_webview(&label).ok_or("Conta não está aberta")?;
    let old = v.window();
    if old.label() == "main" {
        activate(app, &label);
        return Ok(());
    }
    if let Some(panel) = app.get_webview("settings") {
        if panel.window().label() == old.label() {
            let _ = panel.hide();
            panel
                .reparent(&initialize(app, true).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
    }
    v.reparent(&initialize(app, true).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    state(app).menus_hidden.lock().unwrap().remove(old.label());
    let _ = old.destroy();
    activate(app, &label);
    Ok(())
}

fn choose_remaining(app: &AppHandle, exclude: Option<&str>) {
    let current = state(app).main_active.lock().unwrap().clone();
    if let Some(label) = current {
        if exclude != Some(label.as_str())
            && app
                .get_webview(&label)
                .is_some_and(|v| v.window().label() == "main")
        {
            activate(app, &label);
            return;
        }
    }

    let next = accounts::load(app).accounts.into_iter().find_map(|a| {
        let label = accounts::window_label(&a.id);
        (exclude != Some(label.as_str())
            && app
                .get_webview(&label)
                .is_some_and(|v| v.window().label() == "main"))
        .then_some(label)
    });
    *state(app).main_active.lock().unwrap() = next.clone();
    if let Some(label) = next {
        activate(app, &label);
    } else {
        sync_menu(app, "main");
        layout(app, "main");
        if let Some(v) = app.get_webview("shell-main") {
            let _ = v.show();
        }
    }
}

pub fn remove_view(app: &AppHandle, id: &str) -> tauri::Result<()> {
    if let Some(v) = app.get_webview(&accounts::window_label(id)) {
        let owner = v.window();
        v.close()?;
        if owner.label().starts_with("detached-") {
            let _ = owner.destroy();
        }
    }
    choose_remaining(app, None);
    Ok(())
}

pub fn open_panel(app: &AppHandle, parent: Option<&str>) -> tauri::Result<()> {
    if !crate::lock::is_unlocked(app) {
        crate::lock::show_lock_window(app);
        return Ok(());
    }
    let owner = parent
        .and_then(|p| app.get_window(p))
        .or_else(|| {
            let label = app.state::<ActiveAccount>().lock().unwrap().clone();
            app.get_webview(&label).map(|v| v.window())
        })
        .map(Ok)
        .unwrap_or_else(|| initialize(app, false))?;
    let theme = selected(app, owner.label())
        .and_then(|label| {
            app.state::<crate::integration::Integration>()
                .0
                .lock()
                .unwrap()
                .get(&label)
                .and_then(|a| a.theme.clone())
        })
        .unwrap_or_else(|| "system".into());
    let encoded_theme = serde_json::to_string(&theme).expect("theme string");
    let theme_script = format!("document.documentElement.dataset.theme={encoded_theme}");
    let panel = if let Some(v) = app.get_webview("settings") {
        if v.window().label() != owner.label() {
            v.reparent(&owner)?;
        }
        v
    } else {
        owner.add_child(
            WebviewBuilder::new("settings", WebviewUrl::App("index.html".into()))
                .initialization_script(format!(r#"document.addEventListener("DOMContentLoaded",function(){{{theme_script}}},{{once:true}})"#))
                .data_directory(app.path().app_data_dir()?.join("management"))
                .on_navigation(crate::integration::local_url),
            tauri::LogicalPosition::new(0., 0.),
            tauri::LogicalSize::new(800., 600.),
        )?
    };
    let _ = panel.eval(theme_script);
    for (l, v) in app.webviews() {
        if l.starts_with("wa-") && v.window().label() == owner.label() {
            let _ = v.hide();
        }
    }
    *state(app).panel_parent.lock().unwrap() = Some(owner.label().into());
    layout(app, owner.label());
    // A worker can finish constructing a panel after a lock request.
    if !crate::lock::is_unlocked(app) {
        let _ = owner.hide();
        crate::lock::show_lock_window(app);
        return Ok(());
    }
    let _ = panel.show();
    let _ = owner.show();
    let _ = owner.unminimize();
    let _ = owner.set_focus();
    let _ = panel.set_focus();
    Ok(())
}

pub fn close_panel(app: &AppHandle) {
    *state(app).panel_parent.lock().unwrap() = None;
    if let Some(v) = app.get_webview("settings") {
        let parent = v.window();
        let _ = v.hide();
        if let Some(label) = selected(app, parent.label()) {
            activate(app, &label);
        }
    }
}

pub fn toggle(app: &AppHandle) {
    if !crate::lock::is_unlocked(app) {
        crate::lock::show_lock_window(app);
        return;
    }
    let label = app.state::<ActiveAccount>().lock().unwrap().clone();
    if let Some(v) = app.get_webview(&label) {
        let w = v.window();
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let c = crate::applock::load(app);
            if c.should_lock_on_hide() {
                crate::lock::lock_now(app);
            } else {
                let _ = w.hide();
            }
        } else {
            activate(app, &label);
        }
    } else {
        crate::window::show_active(app);
    }
}
pub fn restore_pending(app: &AppHandle) {
    let label = state(app).pending.lock().unwrap().take();
    if let Some(label) = label {
        activate(app, &label);
    }
}

pub fn account_focused(app: &AppHandle, label: &str) -> bool {
    app.get_webview(label).is_some_and(|v| {
        let parent = v.window();
        parent.is_visible().unwrap_or(false)
            && parent.is_focused().unwrap_or(false)
            && selected(app, parent.label()).as_deref() == Some(label)
            && state(app).panel_parent.lock().unwrap().as_deref() != Some(parent.label())
    })
}
