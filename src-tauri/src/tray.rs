#[derive(Debug, PartialEq, Eq)]
pub enum BadgeState {
    Clear,
    Count(u32),
}

/// Decide what the tray should show for a given unread count.
pub fn badge_state(count: u32) -> BadgeState {
    if count == 0 {
        BadgeState::Clear
    } else {
        BadgeState::Count(count)
    }
}

use crate::accounts::{self, UnreadMap};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

// macOS menu-bar items are expected to be *template* images: a monochrome
// silhouette that AppKit tints for the current appearance (black on a light menu
// bar, white on a dark one). Our green app mark is the one thing up there that
// doesn't follow the system, so macOS gets its own pair of assets — generated
// from the same app-icon.svg by `scripts/make-tray-template.mjs`. Windows and
// Linux tray areas have no such convention and keep the colour icon.
#[cfg(target_os = "macos")]
const ICON_NORMAL: &[u8] = include_bytes!("../icons/tray-macos-template.png");
#[cfg(target_os = "macos")]
const ICON_UNREAD: &[u8] = include_bytes!("../icons/tray-macos-template-unread.png");
#[cfg(not(target_os = "macos"))]
const ICON_NORMAL: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(not(target_os = "macos"))]
const ICON_UNREAD: &[u8] = include_bytes!("../icons/tray-unread.png");

/// `NSImage.isTemplate` is macOS-only; the flag is ignored elsewhere.
const ICON_IS_TEMPLATE: bool = cfg!(target_os = "macos");

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // Placeholder menu; rebuild_menu fills in the per-account items at startup.
    let menu = MenuBuilder::new(app).build()?;

    TrayIconBuilder::with_id("main-tray")
        .icon(Image::from_bytes(ICON_NORMAL)?)
        .icon_as_template(ICON_IS_TEMPLATE)
        .tooltip("WhatsApp")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            // While locked, every menu action except Quit just (re)shows the lock screen.
            if !crate::lock::is_unlocked(app) && id != "quit" {
                crate::lock::show_lock_window(app);
                return;
            }
            if let Some(acct_id) = id.strip_prefix("acct:") {
                // Menu-bar-only mode prevents the main window from showing up.
                #[cfg(target_os = "macos")]
                if crate::settings::load(app).menubar_only {
                    crate::window::menubar_reveal(app, None);
                    return;
                }
                crate::window::show_account(app, &accounts::window_label(acct_id));
                return;
            }
            match id {
                "accounts" | "settings" => crate::window::open_settings_window(app),
                "reload" => {
                    #[cfg(target_os = "macos")]
                    if crate::settings::load(app).menubar_only {
                        if let Some(w) = app.get_webview_window(crate::window::MENUBAR_LABEL) {
                            let _ = w.eval("window.location.reload()");
                        }
                        return;
                    }
                    if let Some(active) = app.try_state::<crate::accounts::ActiveAccount>() {
                        let label = active.lock().unwrap().clone();
                        if let Some(w) = app.get_webview_window(&label) {
                            let _ = w.eval("window.location.reload()");
                        }
                    }
                }
                "lock" => crate::lock::lock_now(app),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Note: Linux does not deliver left-click tray events; use the menu there.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = &event
            {
                // macOS, setting enabled: the tray icon toggles the menu bar popup
                #[cfg(target_os = "macos")]
                if crate::settings::load(tray.app_handle()).menubar_popup {
                    crate::window::menubar_toggle_tray(tray.app_handle(), event.clone());
                    return;
                }
                crate::window::show_active(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuild the tray menu from the current accounts list: one `acct:<id>` item per
/// account showing `name (n)`, then the static `Accounts… / Settings / Reload / [Lock now] / Quit`.
pub fn rebuild_menu(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };

    let lock_active = crate::applock::load(app).is_active();

    let f = accounts::load(app);

    // Snapshot per-account unread, then DROP the UnreadMap guard before building the
    // static items (update_badge / other paths may re-lock the map).
    let counts: std::collections::HashMap<String, u32> = {
        let state = app.state::<UnreadMap>();
        let map = state.lock().unwrap();
        map.clone()
    };

    let mut accts = f.accounts.clone();
    accts.sort_by_key(|a| a.order);

    let mut builder = MenuBuilder::new(app);
    for a in &accts {
        let unread = counts.get(&a.id).copied().unwrap_or(0);
        let label = if unread > 0 {
            format!("{} ({})", a.name, unread)
        } else {
            a.name.clone()
        };
        let item = match MenuItemBuilder::with_id(format!("acct:{}", a.id), label).build(app) {
            Ok(i) => i,
            Err(_) => return,
        };
        builder = builder.item(&item);
    }

    let Ok(accounts_item) = MenuItemBuilder::with_id("accounts", "Accounts\u{2026}").build(app)
    else {
        return;
    };
    let Ok(settings) = MenuItemBuilder::with_id("settings", "Settings").build(app) else {
        return;
    };
    let Ok(reload) = MenuItemBuilder::with_id("reload", "Reload").build(app) else {
        return;
    };
    let Ok(quit) = MenuItemBuilder::with_id("quit", "Quit").build(app) else {
        return;
    };

    let mut tail = builder
        .separator()
        .item(&accounts_item)
        .item(&settings)
        .item(&reload);
    if lock_active {
        let Ok(lock_item) = MenuItemBuilder::with_id("lock", "Lock now").build(app) else {
            return;
        };
        tail = tail.item(&lock_item);
    }
    let menu = match tail.item(&quit).build() {
        Ok(m) => m,
        Err(_) => return,
    };

    let _ = tray.set_menu(Some(menu));
}

/// Update the tray to reflect the current (aggregate) unread count.
pub fn update_badge(app: &AppHandle, count: u32) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    match badge_state(count) {
        BadgeState::Clear => {
            let _ = tray.set_title(None::<String>);
            let _ = tray.set_tooltip(Some("WhatsApp"));
            if let Ok(icon) = Image::from_bytes(ICON_NORMAL) {
                // Set image + template flag atomically: on macOS a set_icon followed
                // by set_icon_as_template renders the icon twice and visibly flickers.
                let _ = tray.set_icon_with_as_template(Some(icon), ICON_IS_TEMPLATE);
            }
        }
        BadgeState::Count(c) => {
            let _ = tray.set_title(Some(c.to_string()));
            let _ = tray.set_tooltip(Some(format!("{c} unread — WhatsApp")));
            if let Ok(icon) = Image::from_bytes(ICON_UNREAD) {
                let _ = tray.set_icon_with_as_template(Some(icon), ICON_IS_TEMPLATE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{badge_state, BadgeState};

    #[test]
    fn zero_is_clear() {
        assert_eq!(badge_state(0), BadgeState::Clear);
    }

    #[test]
    fn positive_is_count() {
        assert_eq!(badge_state(1), BadgeState::Count(1));
        assert_eq!(badge_state(42), BadgeState::Count(42));
    }
}
