mod accounts;
mod applock;
mod aumid;
mod biometric;
mod commands;
mod deeplink;
mod dlog;
mod lock;
mod notify;
mod settings;
mod tray;
mod unread;
mod window;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Linux: expose SharedArrayBuffer in the webview. WhatsApp Web's Chrome
    // codepath (which we present ourselves as — see window::CHROME_UA) runs its
    // wasm media/crypto workers on SharedArrayBuffer, and desktop Chrome exposes
    // SAB unconditionally. Distro WebKitGTK ships it OFF even under full
    // cross-origin isolation (verified: crossOriginIsolated=true, SAB still
    // undefined), so video upload/processing hangs on an endless spinner. JSC
    // reads this option from the environment in every web process it spawns;
    // it must be set before the first webview exists. Verified to give real
    // shared-memory semantics (wasm shared Memory + Atomics across workers).
    // Overridable: an already-set value (e.g. =0) is respected.
    #[cfg(target_os = "linux")]
    if std::env::var_os("JSC_useSharedArrayBuffer").is_none() {
        std::env::set_var("JSC_useSharedArrayBuffer", "1");
    }

    let mut builder = tauri::Builder::default();

    // single-instance MUST be registered first.
    #[cfg(desktop)]
    {
        let mut single_instance = tauri_plugin_single_instance::Builder::<tauri::Wry>::new()
            .callback(|app, args, _cwd| {
                // A second launch carrying a `whatsapp://` URL (deep link, issue
                // #23): route it to the active account window — the OS passes the
                // URL as an argv entry whether we were running or not.
                if let Some(url) = args.iter().find(|a| deeplink::is_whatsapp_url(a)) {
                    window::open_whatsapp_link(app, url);
                    return;
                }
                // `whatrust --toggle` (bind it to an OS keyboard shortcut — the reliable
                // global-hotkey path on Wayland, where in-process X11 grabs don't fire)
                // toggles the active window. Otherwise a 2nd launch raises it, except an
                // autostart relaunch carrying --minimized (stay hidden in the tray).
                // Both toggle_active and show_main defer to the lock screen when locked,
                // so neither can bypass the app lock.
                if args.iter().any(|a| a == "--toggle") {
                    window::toggle_active(app);
                } else if !args.iter().any(|a| a == "--minimized") {
                    window::show_main(app);
                }
            });

        // Flatpak only permits a sandbox to own D-Bus names under its exported
        // application ID. Native packages keep using the stable Tauri identifier.
        #[cfg(target_os = "linux")]
        if let Some(id) = settings::flatpak_id() {
            single_instance = single_instance.dbus_id(id);
        }

        builder = builder.plugin(single_instance.build());
    }

    builder = builder.plugin(tauri_plugin_notification::init());

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(
                // Restore size/position, but NOT visibility — otherwise the plugin
                // force-shows the window on launch and defeats start-minimized / --minimized.
                tauri_plugin_window_state::Builder::default()
                    .with_state_flags(
                        tauri_plugin_window_state::StateFlags::all()
                            & !tauri_plugin_window_state::StateFlags::VISIBLE,
                    )
                    .build(),
            )
            .plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _shortcut, event| {
                        // In-process global hotkey (X11 / Windows / macOS). On Wayland
                        // this won't fire — use `whatrust --toggle` via an OS shortcut.
                        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            window::toggle_active(app);
                        }
                    })
                    .build(),
            )
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ));
    }

    builder
        .manage(accounts::UnreadMap::default())
        .manage(accounts::ActiveAccount::new("wa-default".into()))
        .invoke_handler(tauri::generate_handler![
            commands::notify,
            commands::set_unread,
            commands::dlog,
            commands::get_settings,
            commands::set_settings,
            commands::open_settings,
            commands::list_accounts,
            commands::add_account,
            commands::remove_account,
            commands::rename_account,
            commands::open_account,
            commands::get_lock_status,
            commands::set_app_lock_password,
            commands::change_app_lock_password,
            commands::disable_app_lock,
            commands::set_app_lock_options,
            commands::set_biometric_enabled,
            commands::lock_app,
            commands::unlock_password,
            commands::unlock_biometric,
            commands::reset_app_lock,
        ])
        .setup(|app| {
            let handle = app.handle();

            // Start a fresh diagnostic log for this launch (issue #3): the only
            // way to see notification failures on a Windows GUI build with no
            // console. See dlog.rs.
            dlog::init();

            // Windows: register our AppUserModelID so WinRT toast notifications
            // actually render for the installed app (no-op elsewhere). Must run
            // before any account window can fire a notification. See aumid.rs.
            aumid::register(handle);

            let s = settings::load(handle);
            let args: Vec<String> = std::env::args().collect();
            let start_hidden = s.start_minimized || args.iter().any(|a| a == "--minimized");

            // Load accounts (seeds a single `default` on first run / corrupt file).
            let mut f = accounts::load(handle);

            // Backfill a persisted store_uuid for any non-default account missing one
            // (older state predating multi-account). Save only if something changed.
            let mut changed = false;
            for a in f.accounts.iter_mut() {
                if a.id != "default" && a.store_uuid.is_none() {
                    a.store_uuid = Some(accounts::gen_store_uuid());
                    changed = true;
                }
            }
            if changed {
                let _ = accounts::save(handle, &f);
            }

            // App lock: decide the initial state and whether to start hidden.
            let lock_cfg = applock::load(handle);
            let lock_on_launch = lock_cfg.is_active() && lock_cfg.lock_on_launch;
            handle.manage(lock::LockState::new(!lock_on_launch));
            let open_hidden = start_hidden || lock_on_launch;

            // Open every account window so each one receives messages/notifications.
            for a in &f.accounts {
                window::open_account_window(handle, a, open_hidden)?;
            }

            tray::setup(handle)?;
            tray::rebuild_menu(handle);
            let _ = settings::apply(handle, &s);

            if lock_on_launch && !start_hidden {
                lock::show_lock_window(handle);
            }

            // Idle auto-lock watcher. Always running; no-op unless the lock is active
            // with idle_secs > 0 and the app is currently unlocked.
            #[cfg(desktop)]
            {
                let idle_handle = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    let c = applock::load(&idle_handle);
                    if !c.is_active() || c.idle_secs == 0 {
                        continue;
                    }
                    if !lock::is_unlocked(&idle_handle) {
                        continue;
                    }
                    let idle_ok = user_idle::UserIdle::get_time()
                        .map(|t| t.as_seconds() >= c.idle_secs as u64)
                        .unwrap_or(false);
                    if idle_ok {
                        let h = idle_handle.clone();
                        let _ = idle_handle.run_on_main_thread(move || lock::lock_now(&h));
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building whatRust")
        .run(|_app_handle, _event| {
            // macOS: clicking the dock icon after hide-to-tray re-shows the window
            // (otherwise the app is only reachable via the menu-bar tray icon).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = &_event
            {
                if !*has_visible_windows {
                    window::show_main(_app_handle);
                }
            }
        });
}
