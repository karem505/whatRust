//! Windows account ownership. One live WebView per profile; detach moves it.
use crate::accounts::{self, Account, ActiveAccount};
use std::collections::HashSet;
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
                if crate::applock::load(&a).lock_on_hide {
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
        // Reuse every existing download/media/profile hook. Move the view BEFORE
        // destroying its temporary native window; never recreate the session.
        let _staged = crate::window::build_account_window(app, account, true)?;
        let original = app.get_window(&label).ok_or(tauri::Error::WindowNotFound)?;
        let view = app
            .get_webview(&label)
            .ok_or(tauri::Error::WindowNotFound)?;
        view.reparent(&parent)?;
        original.destroy()?;
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
    layout(app, parent.label());
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
    let _ = view.show();
    let _ = parent.show();
    let _ = parent.unminimize();
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
    let size = size.to_logical::<f64>(scale);
    let active = selected(app, parent);
    let integrated = active.as_ref().is_some_and(|l| {
        app.state::<crate::integration::Integration>()
            .0
            .lock()
            .unwrap()
            .get(l)
            .is_some_and(|a| a.mounted)
    });
    if integrated {
        let _ = w.hide_menu();
    } else {
        let _ = w.show_menu();
    }
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
        let y = if label.starts_with("wa-") { top } else { 0. };
        let _ = v.set_bounds(tauri::Rect {
            position: tauri::LogicalPosition::new(0., y).into(),
            size: tauri::LogicalSize::new(size.width, (size.height - y).max(1.)).into(),
        });
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
        layout(app, v.window().label());
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
    } else if let Some(v) = app.get_webview("shell-main") {
        let _ = v.show();
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
    let panel = if let Some(v) = app.get_webview("settings") {
        if v.window().label() != owner.label() {
            v.reparent(&owner)?;
        }
        v
    } else {
        owner.add_child(
            WebviewBuilder::new("settings", WebviewUrl::App("index.html".into()))
                .data_directory(app.path().app_data_dir()?.join("management"))
                .on_navigation(crate::integration::local_url),
            tauri::LogicalPosition::new(0., 0.),
            tauri::LogicalSize::new(800., 600.),
        )?
    };
    for (l, v) in app.webviews() {
        if l.starts_with("wa-") && v.window().label() == owner.label() {
            let _ = v.hide();
        }
    }
    *state(app).panel_parent.lock().unwrap() = Some(owner.label().into());
    layout(app, owner.label());
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
            if crate::applock::load(app).lock_on_hide {
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
