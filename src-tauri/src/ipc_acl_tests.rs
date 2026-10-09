//! Which window, from which origin, may call which whatRust command.
//!
//! Tauri refuses a custom command from a remote origin unless a capability with
//! a matching `remote` URL grants it. whatRust had no app permission manifest,
//! so nothing could grant `notify`/`set_unread`/`dlog` to web.whatsapp.com: every
//! call from the injected bridge was rejected, and bridge.js swallows the
//! rejection — no message notifications and no unread badge (issues #3, #32).
//! These tests run the real capability files through Tauri's ACL with the mock
//! runtime, so a capability or build.rs change that breaks either direction
//! (the page losing its bridge, or gaining account/lock commands) fails here.

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewUrl, WebviewWindowBuilder};

const PAGE: &str = "https://web.whatsapp.com/";

/// The commands the WhatsApp page's bridge.js needs.
const PAGE_COMMANDS: &[&str] = &[
    "notify",
    "set_unread",
    "dlog",
    "open_download",
    "reveal_download",
];
/// Commands only the local settings window may use.
const SETTINGS_COMMANDS: &[&str] = &[
    "get_settings",
    "set_settings",
    "list_accounts",
    "add_account",
    "remove_account",
    "rename_account",
    "open_account",
    "get_lock_status",
    "set_app_lock_password",
    "change_app_lock_password",
    "disable_app_lock",
    "set_app_lock_options",
    "set_biometric_enabled",
    "lock_app",
];
/// Commands only the lock screen may use.
const LOCK_COMMANDS: &[&str] = &[
    "get_lock_status",
    "unlock_password",
    "unlock_biometric",
    "reset_app_lock",
];

fn allowed(app: &tauri::App<tauri::test::MockRuntime>, label: &str, url: &str, cmd: &str) -> bool {
    use tauri::Manager;
    let window = match app.get_webview_window(label) {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, label, WebviewUrl::External(url.parse().unwrap()))
            .build()
            .unwrap(),
    };
    let request = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: url.parse().unwrap(),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    match get_ipc_response(&window, request) {
        Ok(_) => true,
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("not allowed"),
                "{cmd} failed for another reason: {msg}"
            );
            false
        }
    }
}

fn app() -> tauri::App<tauri::test::MockRuntime> {
    mock_builder()
        // Answers every command: the ACL decides by command name before any
        // handler runs, so a catch-all proves the decision without app state.
        .invoke_handler(|invoke| {
            invoke.resolver.resolve(());
            true
        })
        .build(tauri::generate_context!())
        .expect("mock app")
}

#[test]
fn the_whatsapp_page_reaches_its_bridge_commands_in_every_account() {
    let app = app();
    for label in ["wa-default", "wa-acct-2"] {
        for cmd in PAGE_COMMANDS {
            assert!(
                allowed(&app, label, PAGE, cmd),
                "{label} must be allowed {cmd}"
            );
        }
    }
}

#[test]
fn the_whatsapp_page_cannot_reach_settings_or_lock_commands() {
    let app = app();
    for cmd in SETTINGS_COMMANDS
        .iter()
        .chain(LOCK_COMMANDS)
        .chain(&["open_settings"])
    {
        assert!(
            !allowed(&app, "wa-default", PAGE, cmd),
            "the page must not reach {cmd}"
        );
    }
    // Nor plugin commands that would bypass whatRust's lock-aware `notify`.
    assert!(!allowed(
        &app,
        "wa-default",
        PAGE,
        "plugin:notification|notify"
    ));
}

#[test]
fn the_notification_plugins_permission_query_is_answered() {
    // The plugin's init script asks this on every page load; refusing it left an
    // unhandled promise rejection in WhatsApp's page (found testing v0.6.5). The
    // mock app doesn't register the plugin, so getting past the ACL shows up as
    // "plugin notification not found" rather than "not allowed".
    let app = app();
    let window = WebviewWindowBuilder::new(
        &app,
        "wa-default",
        WebviewUrl::External(PAGE.parse().unwrap()),
    )
    .build()
    .unwrap();
    let request = InvokeRequest {
        cmd: "plugin:notification|is_permission_granted".into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: PAGE.parse().unwrap(),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    let err = get_ipc_response(&window, request).unwrap_err().to_string();
    assert!(
        !err.contains("not allowed"),
        "the ACL must let it through: {err}"
    );
}

#[test]
fn bridge_commands_are_refused_from_any_other_site() {
    let app = app();
    assert!(!allowed(
        &app,
        "wa-default",
        "https://evil.example/",
        "notify"
    ));
}

#[test]
fn local_windows_keep_exactly_their_own_commands() {
    let app = app();
    let local = "tauri://localhost/";
    for cmd in SETTINGS_COMMANDS {
        assert!(
            allowed(&app, "settings", local, cmd),
            "settings must be allowed {cmd}"
        );
    }
    for cmd in LOCK_COMMANDS {
        assert!(
            allowed(&app, "lock", local, cmd),
            "lock must be allowed {cmd}"
        );
    }
    for cmd in ["unlock_password", "reset_app_lock", "notify"] {
        assert!(
            !allowed(&app, "settings", local, cmd),
            "settings must not reach {cmd}"
        );
    }
    for cmd in ["get_settings", "add_account", "disable_app_lock", "notify"] {
        assert!(
            !allowed(&app, "lock", local, cmd),
            "lock must not reach {cmd}"
        );
    }
}
