//! Single-window account ownership. One live WebView per profile; on Windows
//! detach moves it into its own window.
//!
//! Always on for Windows. On Linux and macOS it is the opt-in "Show all accounts
//! in one window" mode (see [`enabled`]); detach/attach and the native window
//! menu stay Windows-only.
#![cfg_attr(not(windows), allow(dead_code))]
use crate::accounts::{self, Account, ActiveAccount};
use crate::i18n::tr;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
#[cfg(windows)]
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, WebviewBuilder, WebviewUrl, Window, WindowBuilder};

/// Whether this process hosts every account in one window. Fixed at startup:
/// changing the setting needs a restart, so no live window set is ever
/// converted between the two models.
static ENABLED: AtomicBool = AtomicBool::new(cfg!(windows));

/// Windows always uses the single window. Elsewhere it is the value chosen by
/// [`init_mode`] at startup, `false` until then.
pub fn enabled() -> bool {
    cfg!(windows) || ENABLED.load(Ordering::Relaxed)
}

/// Decide the window model for this run from the saved settings. Menu-bar-only
/// mode (macOS) wins over single-window; macOS older than 14 cannot isolate
/// accounts, so it keeps the one-account-per-window path.
pub fn init_mode(s: &crate::settings::Settings) {
    if cfg!(windows) {
        return;
    }
    let menubar = cfg!(target_os = "macos") && s.menubar_only && crate::menubar::supported();
    let isolated = crate::window::ensure_isolation_supported().is_ok();
    ENABLED.store(s.single_window && !menubar && isolated, Ordering::Relaxed);
}

#[derive(Default)]
pub struct Sessions {
    main_active: Mutex<Option<String>>,
    fallback: Mutex<HashSet<String>>,
    pending: Mutex<Option<String>>,
    panel_parent: Mutex<Option<String>>,
    menus_hidden: Mutex<HashMap<String, bool>>,
    view_visibility: Mutex<HashMap<String, bool>>,
    applied_bounds: Mutex<HashMap<String, (i32, i32, u32, u32)>>,
    pending_layout: Mutex<HashSet<String>>,
}

fn state(app: &AppHandle) -> tauri::State<'_, Sessions> {
    app.state::<Sessions>()
}

fn set_view_visible(app: &AppHandle, view: &tauri::Webview, visible: bool) {
    let changed = {
        let s = state(app);
        let mut map = crate::lock::guard(&s.view_visibility);
        map.insert(view.label().into(), visible) != Some(visible)
    };
    // Release the cache lock before COM: showing can generate native events.
    if changed {
        if visible {
            state(app)
                .applied_bounds
                .lock()
                .unwrap()
                .remove(view.label());
        }
        #[cfg(target_os = "linux")]
        let result = set_gtk_visible(view, visible);
        #[cfg(not(target_os = "linux"))]
        let result = if visible { view.show() } else { view.hide() };
        if result.is_err() {
            state(app)
                .view_visibility
                .lock()
                .unwrap()
                .remove(view.label());
        }
    }
}

/// Native focus/resize callbacks execute while Tauri owns listener locks.
/// Only queue here; COM geometry/visibility must run after that callback exits.
pub fn schedule_layout(app: &AppHandle, parent: &str) {
    if !state(app)
        .pending_layout
        .lock()
        .unwrap()
        .insert(parent.into())
    {
        return;
    }
    let handle = app.clone();
    let parent = parent.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let ui = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            crate::lock::guard(&state(&ui).pending_layout).remove(&parent);
            layout(&ui, &parent);
        });
    });
}
fn forget_view_geometry(app: &AppHandle, label: &str) {
    crate::lock::guard(&state(app).applied_bounds).remove(label);
    crate::lock::guard(&state(app).view_visibility).remove(label);
}

fn reparent_on_ui(app: &AppHandle, view: &tauri::Webview, parent: &Window) -> Result<(), String> {
    let handle = app.clone();
    let view = view.clone();
    let parent = parent.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _health = crate::ui_health::enter("reparent webview");
        // Wry holds its window-id mutex until reparent returns. Doing that on
        // a worker deadlocks if the UI is concurrently processing layout for
        // that same dispatcher. Hide first so focus callbacks have completed.
        set_view_visible(&handle, &view, false);
        forget_view_geometry(&handle, view.label());
        let result = view.reparent(&parent).map_err(|e| e.to_string());
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
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
        .icon(crate::appicon::current(app))?
        .visible(!hidden)
        .build()?;
    #[cfg(windows)]
    install_menu(app, &w)?;
    let shell = WebviewBuilder::new("shell-main", WebviewUrl::App("shell.html".into()))
        .initialization_script(crate::i18n::init_script())
        .data_directory(app.path().app_data_dir()?.join("management"))
        .on_navigation(crate::integration::local_url);
    let shell_view = w.add_child(
        shell,
        tauri::LogicalPosition::new(0., 0.),
        tauri::LogicalSize::new(1100., 800.),
    )?;
    prepare_view(&shell_view);
    #[cfg(target_os = "linux")]
    pin_shell_height(&shell_view);
    #[cfg(not(target_os = "linux"))]
    let _ = shell_view;
    let a = app.clone();
    w.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            schedule_layout(&a, "main")
        }
        tauri::WindowEvent::Focused(focused) => {
            if *focused {
                let label = crate::lock::guard(&state(&a).main_active).clone();
                if let Some(label) = label {
                    *a.state::<ActiveAccount>().lock().unwrap() = label;
                }
            }
            schedule_layout(&a, "main");
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let handle = a.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let ui = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if crate::settings::load(&ui).close_to_tray {
                        if crate::applock::load(&ui).should_lock_on_hide() {
                            crate::lock::lock_on_hide(&ui);
                        }
                        if let Some(w) = ui.get_window("main") {
                            let _ = w.hide();
                            layout(&ui, "main");
                        }
                    } else {
                        ui.exit(0);
                    }
                });
            });
        }
        _ => {}
    });
    Ok(w)
}

/// wry's `set_visible(true)` calls `show_all`, a no-op on the `no_show_all`
/// views from [`prepare_view`], so flip the GTK widget directly.
#[cfg(target_os = "linux")]
fn set_gtk_visible(view: &tauri::Webview, visible: bool) -> tauri::Result<()> {
    view.with_webview(move |platform| {
        use gtk::prelude::{StaticType, WidgetExt};
        let wv = platform.inner();
        // The recovery bar lives in a pinned scrolled-window wrapper.
        let wrap = wv.ancestor(gtk::ScrolledWindow::static_type());
        if visible {
            if let Some(w) = &wrap {
                w.show();
            }
            wv.show();
        } else {
            wv.hide();
            if let Some(w) = &wrap {
                w.hide();
            }
        }
    })
}

/// GTK: `Window::show` runs `show_all`, which would re-show every child view
/// we deliberately hid (all accounts stacked in the box). Exempt the view so
/// only our explicit `show()`/`hide()` calls change its visibility.
pub(crate) fn prepare_view(view: &tauri::Webview) {
    #[cfg(target_os = "linux")]
    {
        let _ = view.with_webview(|platform| {
            use gtk::prelude::WidgetExt;
            platform.inner().set_no_show_all(true);
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = view;
}

/// GTK packs child webviews into a vertical box, so bounds do not apply, and a
/// WebKit view reports a large natural height that a plain size request cannot
/// shrink. Wrap the recovery bar in a non-propagating scrolled window pinned to
/// its slim height, packed first (top) and not expanding.
#[cfg(target_os = "linux")]
fn pin_shell_height(view: &tauri::Webview) {
    let _ = view.with_webview(|platform| {
        use gtk::prelude::{BoxExt, Cast, ContainerExt, ScrolledWindowExt, WidgetExt};
        let wv = platform.inner();
        let Some(parent) = wv.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) else {
            return;
        };
        let wrap = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        wrap.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        wrap.set_min_content_height(48);
        wrap.set_propagate_natural_height(false);
        wrap.set_vexpand(false);
        wrap.set_no_show_all(true);
        parent.remove(&wv);
        wrap.add(&wv);
        parent.pack_start(&wrap, false, false, 0);
        parent.reorder_child(&wrap, 0);
        wv.set_vexpand(false);
    });
}

/// Rebuild the native window menu after a language change.
#[cfg(windows)]
pub fn refresh_menus(app: &AppHandle) {
    for (label, w) in app.windows() {
        if label == "main" || label.starts_with("detached-") {
            let _ = install_menu(app, &w);
        }
    }
}

#[cfg(windows)]
fn install_menu(app: &AppHandle, w: &Window) -> tauri::Result<()> {
    let accounts = MenuItemBuilder::with_id("app.accounts", tr("Contas"))
        .accelerator("Ctrl+Shift+A")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("app.settings", tr("Configurações do whatRust"))
        .accelerator("Ctrl+,")
        .build(app)?;
    let back = MenuItemBuilder::with_id("app.back", tr("Voltar ao WhatsApp")).build(app)?;
    let reload = MenuItemBuilder::with_id("app.reload", tr("Recarregar conta")).build(app)?;
    let lock = MenuItemBuilder::with_id("app.lock", tr("Bloquear")).build(app)?;
    let quit = MenuItemBuilder::with_id("app.quit", tr("Sair do whatRust")).build(app)?;
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
        return crate::lock::guard(&state(app).main_active).clone();
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
        set_view_visible(app, &view, false);
    }
    if crate::lock::guard(&state(app).main_active).is_none() {
        *crate::lock::guard(&state(app).main_active) = Some(label.clone());
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
    let _health = crate::ui_health::enter("account activation");
    if !crate::lock::is_unlocked(app) {
        *crate::lock::guard(&state(app).pending) = Some(label.to_string());
        crate::lock::show_lock_window(app);
        return;
    }
    let Some(view) = app.get_webview(label) else {
        return;
    };
    let parent = view.window();
    if parent.label() == "main" {
        *crate::lock::guard(&state(app).main_active) = Some(label.to_string());
    }
    if let Some(panel) = app.get_webview("settings") {
        if panel.window().label() == parent.label() {
            set_view_visible(app, &panel, false);
            *crate::lock::guard(&state(app).panel_parent) = None;
        }
    }
    for (other, v) in app.webviews() {
        if other.starts_with("wa-") && v.window().label() == parent.label() && other != label {
            set_view_visible(app, &v, false);
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
    set_view_visible(app, &view, true);
    // WebView2 must be visible before its final bounds/viewport are applied.
    layout(app, parent.label());
    let _ = parent.set_focus();
    let _ = view.set_focus();
    crate::integration::broadcast(app);
}

pub fn layout(app: &AppHandle, parent: &str) {
    let handle = app.clone();
    let parent = parent.to_string();
    let _ = app.run_on_main_thread(move || layout_on_main(&handle, &parent));
}
fn layout_on_main(app: &AppHandle, parent: &str) {
    let _health = crate::ui_health::enter("account layout");
    let Some(w) = app.get_window(parent) else {
        return;
    };
    // Child WebViews do not automatically inherit the native parent's hidden
    // or minimized state. Without this, document.hidden stays false and the
    // open chat may suppress its own desktop notification.
    let minimized = w.is_minimized().unwrap_or(false);
    let visible = w.is_visible().unwrap_or(false) && !minimized && crate::lock::is_unlocked(app);
    let active = selected(app, parent);
    let panel_open = crate::lock::guard(&state(app).panel_parent).as_deref() == Some(parent);
    for (label, view) in app.webviews() {
        if view.window().label() != parent {
            continue;
        }
        if !visible {
            set_view_visible(app, &view, false);
        } else if label.starts_with("wa-") {
            if !panel_open && active.as_deref() == Some(label.as_str()) {
                set_view_visible(app, &view, true);
            } else {
                set_view_visible(app, &view, false);
            }
        } else if label == "settings" {
            if panel_open {
                set_view_visible(app, &view, true);
            } else {
                set_view_visible(app, &view, false);
            }
        }
    }
    // Keep the last usable viewport while minimized; don't turn it into 1x1.
    if minimized {
        return;
    }
    let Ok(size) = w.inner_size() else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let top = if parent == "main"
        && active
            .as_ref()
            .is_some_and(|l| crate::lock::guard(&state(app).fallback).contains(l))
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
        let geometry = (0, y as i32, size.width.max(1), height);
        let changed = state(app)
            .applied_bounds
            .lock()
            .unwrap()
            .insert(label.clone(), geometry)
            != Some(geometry);
        if changed {
            let _stage = crate::ui_health::enter("apply webview bounds");
            if v.set_bounds(tauri::Rect {
                position: tauri::PhysicalPosition::new(geometry.0, geometry.1).into(),
                size: tauri::PhysicalSize::new(geometry.2, geometry.3).into(),
            })
            .is_err()
            {
                crate::lock::guard(&state(app).applied_bounds).remove(&label);
            }
        }
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
            if visible && !panel_open && (active.is_none() || offset > 0) {
                set_view_visible(app, &v, true);
            } else {
                set_view_visible(app, &v, false);
            }
        }
    }
}

// Tauri holds its menu mutex while SetMenu synchronously dispatches WM_SIZE.
// Changing menus inside layout re-enters that mutex and freezes the UI thread.
// Record transitions before invoking native APIs; resize only changes bounds.
fn sync_menu(app: &AppHandle, parent: &str) {
    // The native window menu exists on Windows only.
    if !cfg!(windows) {
        return;
    }
    let _health = crate::ui_health::enter("native menu");
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
        let mut menus = crate::lock::guard(&s.menus_hidden);
        menus.insert(parent.into(), hidden) != Some(hidden)
    };
    if changed {
        if let Some(w) = app.get_window(parent) {
            let result = if hidden { w.hide_menu() } else { w.show_menu() };
            if result.is_err() {
                crate::lock::guard(&state(app).menus_hidden).remove(parent);
            }
        }
    }
}

pub fn integration_changed(app: &AppHandle, label: &str, mounted: bool) {
    let s = state(app);
    let mut f = crate::lock::guard(&s.fallback);
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
    let _health = crate::ui_health::enter("detach account");
    crate::lock::require_unlocked(app)?;
    let label = accounts::window_label(id);
    let v = app
        .get_webview(&label)
        .ok_or_else(|| tr("Conta não está aberta").into_owned())?;
    if v.window().label() != "main" {
        activate(app, &label);
        return Ok(());
    }
    let parent_label = format!("detached-{id}");
    let w = WindowBuilder::new(app, &parent_label)
        .title(tr("whatRust — Conta destacada"))
        .background_color(tauri::utils::config::Color(22, 23, 23, 255))
        .inner_size(1000., 760.)
        .min_inner_size(560., 480.)
        .icon(crate::appicon::current(app))
        .map_err(|e| e.to_string())?
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    install_menu(app, &w).map_err(|e| e.to_string())?;
    reparent_on_ui(app, &v, &w)?;
    let a = app.clone();
    let account = id.to_string();
    let p = parent_label.clone();
    let l = label.clone();
    w.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            schedule_layout(&a, &p)
        }
        tauri::WindowEvent::Focused(true) => {
            *a.state::<ActiveAccount>().lock().unwrap() = l.clone();
            schedule_layout(&a, &p);
        }
        tauri::WindowEvent::Focused(false) => schedule_layout(&a, &p),
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let a = a.clone();
            let id = account.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = attach_after_window_close(&a, &id);
            });
        }
        _ => {}
    });
    choose_remaining(app, Some(&label));
    activate(app, &label);
    Ok(())
}

pub fn attach(app: &AppHandle, id: &str) -> Result<(), String> {
    attach_inner(app, id, true)
}

/// Closing a detached window returns its live WebView to the main window while
/// preserving the account that was already selected there. Explicitly choosing
/// “Reintegrar à janela” still activates the returned account via [`attach`].
fn attach_after_window_close(app: &AppHandle, id: &str) -> Result<(), String> {
    attach_inner(app, id, false)
}

fn attach_inner(app: &AppHandle, id: &str, activate_returned_account: bool) -> Result<(), String> {
    let _health = crate::ui_health::enter("attach account");
    crate::lock::require_unlocked(app)?;
    let label = accounts::window_label(id);
    let v = app
        .get_webview(&label)
        .ok_or_else(|| tr("Conta não está aberta").into_owned())?;
    let old = v.window();
    if old.label() == "main" {
        if activate_returned_account {
            activate(app, &label);
        }
        return Ok(());
    }
    let selected_before_close = crate::lock::guard(&state(app).main_active).clone();
    if let Some(panel) = app.get_webview("settings") {
        if panel.window().label() == old.label() {
            set_view_visible(app, &panel, false);
            reparent_on_ui(
                app,
                &panel,
                &initialize(app, true).map_err(|e| e.to_string())?,
            )?;
        }
    }
    reparent_on_ui(app, &v, &initialize(app, true).map_err(|e| e.to_string())?)?;
    crate::lock::guard(&state(app).menus_hidden).remove(old.label());
    let _ = old.destroy();
    if activate_returned_account {
        activate(app, &label);
    } else {
        restore_main_after_detached_close(app, &label, selected_before_close);
    }
    Ok(())
}

fn restore_main_after_detached_close(
    app: &AppHandle,
    returned_label: &str,
    previously_selected: Option<String>,
) {
    let on_main = |label: &str| {
        app.get_webview(label)
            .is_some_and(|view| view.window().label() == "main")
    };
    let selected = previously_selected
        .filter(|label| label != returned_label && on_main(label))
        .or_else(|| {
            accounts::load(app)
                .accounts
                .into_iter()
                .find_map(|account| {
                    let label = accounts::window_label(&account.id);
                    (label != returned_label && on_main(&label)).then_some(label)
                })
        })
        .unwrap_or_else(|| returned_label.to_string());

    *crate::lock::guard(&state(app).main_active) = Some(selected.clone());
    *app.state::<ActiveAccount>().lock().unwrap() = selected.clone();
    sync_menu(app, "main");
    if let (Some(window), Some(view)) = (app.get_window("main"), app.get_webview(&selected)) {
        let name = accounts::id_from_label(&selected)
            .and_then(|id| {
                accounts::load(app)
                    .accounts
                    .into_iter()
                    .find(|a| a.id == id)
            })
            .map(|account| account.name)
            .unwrap_or_else(|| "WhatsApp".into());
        let _ = window.set_title(&format!("whatRust — {name}"));
        let _ = window.show();
        let _ = window.unminimize();
        layout(app, "main");
        let _ = window.set_focus();
        if crate::lock::guard(&state(app).panel_parent).as_deref() == Some("main") {
            if let Some(panel) = app.get_webview("settings") {
                let _ = panel.set_focus();
            }
        } else {
            let _ = view.set_focus();
        }
    }
    crate::integration::broadcast(app);
}

fn choose_remaining(app: &AppHandle, exclude: Option<&str>) {
    let current = crate::lock::guard(&state(app).main_active).clone();
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
    *crate::lock::guard(&state(app).main_active) = next.clone();
    if let Some(label) = next {
        activate(app, &label);
    } else {
        sync_menu(app, "main");
        layout(app, "main");
        if let Some(v) = app.get_webview("shell-main") {
            set_view_visible(app, &v, true);
        }
    }
}

pub fn remove_view(app: &AppHandle, id: &str) -> tauri::Result<()> {
    if let Some(v) = app.get_webview(&accounts::window_label(id)) {
        let owner = v.window();
        v.close()?;
        crate::lock::guard(&state(app).view_visibility).remove(v.label());
        if owner.label().starts_with("detached-") {
            let _ = owner.destroy();
        }
    }
    choose_remaining(app, None);
    Ok(())
}

pub fn open_panel(app: &AppHandle, parent: Option<&str>) -> tauri::Result<()> {
    let _health = crate::ui_health::enter("account panel");
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
            reparent_on_ui(app, &v, &owner).map_err(std::io::Error::other)?;
        }
        v
    } else {
        owner.add_child(
            WebviewBuilder::new("settings", WebviewUrl::App("index.html".into()))
                .initialization_script(crate::i18n::init_script())
                .initialization_script(if cfg!(windows) {
                    ""
                } else {
                    "window.__WHATRUST_SINGLE__=true;"
                })
                .initialization_script(format!(r#"document.addEventListener("DOMContentLoaded",function(){{{theme_script}}},{{once:true}})"#))
                .data_directory(app.path().app_data_dir()?.join("management"))
                .on_navigation(crate::integration::local_url),
            tauri::LogicalPosition::new(0., 0.),
            tauri::LogicalSize::new(800., 600.),
        )?
    };
    prepare_view(&panel);
    let _ = panel.eval(theme_script);
    for (l, v) in app.webviews() {
        if l.starts_with("wa-") && v.window().label() == owner.label() {
            set_view_visible(app, &v, false);
        }
    }
    *crate::lock::guard(&state(app).panel_parent) = Some(owner.label().into());
    layout(app, owner.label());
    // A worker can finish constructing a panel after a lock request.
    if !crate::lock::is_unlocked(app) {
        let _ = owner.hide();
        crate::lock::show_lock_window(app);
        return Ok(());
    }
    set_view_visible(app, &panel, true);
    let _ = owner.show();
    let _ = owner.unminimize();
    let _ = owner.set_focus();
    let _ = panel.set_focus();
    Ok(())
}

pub fn close_panel(app: &AppHandle) {
    *crate::lock::guard(&state(app).panel_parent) = None;
    if let Some(v) = app.get_webview("settings") {
        let parent = v.window();
        set_view_visible(app, &v, false);
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
                crate::lock::lock_on_hide(app);
            } else {
                let _ = w.hide();
                layout(app, w.label());
            }
        } else {
            activate(app, &label);
        }
    } else {
        crate::window::show_active(app);
    }
}
pub fn restore_pending(app: &AppHandle) {
    let label = crate::lock::guard(&state(app).pending).take();
    if let Some(label) = label {
        activate(app, &label);
    }
}

/// Whether an OS file drop on `label`'s view may be delivered: only the account
/// the user is looking at (selected in the visible main window, no panel over
/// it). GTK would not deliver to a hidden child anyway; this is the safety net.
pub fn drop_target(app: &AppHandle, label: &str) -> bool {
    app.get_webview(label).is_some_and(|v| {
        let parent = v.window();
        parent.is_visible().unwrap_or(false)
            && !parent.is_minimized().unwrap_or(false)
            && selected(app, parent.label()).as_deref() == Some(label)
            && crate::lock::guard(&state(app).panel_parent).as_deref() != Some(parent.label())
            && crate::lock::is_unlocked(app)
    })
}

pub fn account_focused(app: &AppHandle, label: &str) -> bool {
    app.get_webview(label).is_some_and(|v| {
        let parent = v.window();
        parent.is_visible().unwrap_or(false)
            && !parent.is_minimized().unwrap_or(false)
            && parent.is_focused().unwrap_or(false)
            && selected(app, parent.label()).as_deref() == Some(label)
            && crate::lock::guard(&state(app).panel_parent).as_deref() != Some(parent.label())
    })
}
