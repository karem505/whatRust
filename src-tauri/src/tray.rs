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
use crate::i18n::{tr, trf};
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
                crate::window::show_account(app, &accounts::window_label(acct_id));
                return;
            }
            match id {
                "accounts" | "settings" => crate::window::open_settings_window(app),
                "reload" => {
                    if let Some(active) = app.try_state::<crate::accounts::ActiveAccount>() {
                        let label = crate::lock::guard(&active).clone();
                        if let Some(w) = app.get_webview(&label) {
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
            } = event
            {
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
        let map = crate::lock::guard(&state);
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

    let Ok(accounts_item) = MenuItemBuilder::with_id("accounts", tr("Contas…")).build(app) else {
        return;
    };
    let Ok(settings) = MenuItemBuilder::with_id("settings", tr("Configurações")).build(app)
    else {
        return;
    };
    let Ok(reload) = MenuItemBuilder::with_id("reload", tr("Recarregar")).build(app) else {
        return;
    };
    let Ok(quit) = MenuItemBuilder::with_id("quit", tr("Sair")).build(app) else {
        return;
    };

    let mut tail = builder
        .separator()
        .item(&accounts_item)
        .item(&settings)
        .item(&reload);
    if lock_active {
        let Ok(lock_item) = MenuItemBuilder::with_id("lock", tr("Bloquear agora")).build(app)
        else {
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

/// Show or hide the menu-bar icon (macOS, issue #44). Elsewhere the tray icon
/// stays: on Windows and Linux it is the only way back to a hidden window,
/// while macOS always has the Dock icon (see `RunEvent::Reopen` in lib.rs).
#[allow(unused_variables)]
pub fn apply_visibility(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(visible);
    }
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
            let _ = tray.set_tooltip(Some(trf("{0} conversas não lidas — WhatsApp", &[&c])));
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
