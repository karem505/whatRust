mod accounts;
mod applock;
mod aumid;
mod biometric;
mod commands;
mod dlog;
mod downloads;
mod integration;
mod jsonfile;
mod links;
mod lock;
mod notif_icon;
mod notify;
mod opener;
#[cfg(windows)]
mod sessions;
mod settings;
#[cfg(all(windows, debug_assertions))]
mod smoke;
mod tray;
#[cfg(windows)]
mod ui_health;
mod unread;
mod window;

// Linux only: the capability files are the same on every OS, and a second
// `generate_context!` clashes with the embedded Info.plist on macOS, while the
// mock-runtime test binary lacks the Common Controls manifest on Windows.
#[cfg(all(test, target_os = "linux"))]
mod ipc_acl_tests;

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

    let smoke_mode =
        cfg!(debug_assertions) && std::env::var("WHATRUST_SMOKE").as_deref() == Ok("1");
    let mut context = tauri::generate_context!();
    if smoke_mode {
        context.config_mut().identifier = "com.karem.whatrust.smoke".into();
    }
    let mut builder = tauri::Builder::default();

    // single-instance MUST be registered first.
    #[cfg(desktop)]
    {
        let single_instance = tauri_plugin_single_instance::Builder::<tauri::Wry>::new().callback(
            |app, args, _cwd| {
                // `whatrust --toggle` (bind it to an OS keyboard shortcut — the reliable
                // global-hotkey path on Wayland, where in-process X11 grabs don't fire)
                // toggles the active window. Otherwise a 2nd launch raises it, except an
                // autostart relaunch carrying --minimized (stay hidden in the tray).
                // Both toggle_active and show_main defer to the lock screen when locked,
                // so neither can bypass the app lock.
                if args.iter().any(|a| a == "--settings") {
                    window::open_settings_window(app);
                } else if args.iter().any(|a| a == "--toggle") {
                    window::toggle_active(app);
                } else if let Some(link) = links::link_from_args(&args) {
                    // A whatsapp:// (or wa.me) link handed to a second launch by
                    // the OS scheme handler (issue #23): open it in the running copy.
                    links::open_from_outside(app, &link);
                } else if !settings::start_hidden(
                    cfg!(windows) && settings::load(app).start_minimized,
                    &args,
                ) {
                    window::show_main(app);
                }
            },
        );

        // Flatpak only permits a sandbox to own D-Bus names under its exported
        // application ID. Native packages keep using the stable Tauri identifier.
        #[cfg(target_os = "linux")]
        let single_instance = {
            let mut single_instance = single_instance;
            if let Some(id) = settings::flatpak_id() {
                single_instance = single_instance.dbus_id(id);
            }
            single_instance
        };

        if !smoke_mode {
            builder = builder.plugin(single_instance.build());
        }
    }

    // The Windows toast service is owned by notify.rs. This plugin injects a
    // replacement window.Notification that bypasses that account/lock-aware
    // service and invokes its own forbidden remote IPC command instead.
    #[cfg(not(windows))]
    {
        builder = builder.plugin(tauri_plugin_notification::init());
    }

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
                Some(vec![if cfg!(windows) {
                    "--autostart"
                } else {
                    "--minimized"
                }]),
            ));
    }

    #[cfg(windows)]
    let builder = builder.manage(sessions::Sessions::default());

    builder
        .manage(accounts::UnreadMap::default())
        .manage(downloads::Downloads::default())
        .manage(notify::Recent::default())
        .manage(integration::Integration::default())
        .manage(accounts::ActiveAccount::new("wa-default".into()))
        .invoke_handler(tauri::generate_handler![
            integration::integration_state,
            integration::report_integration,
            integration::report_message_signal,
            integration::open_app_panel,
            integration::close_app_panel,
            integration::detach_account,
            integration::attach_account,
            integration::reorder_accounts,
            integration::notification_diagnostics,
            integration::test_notification,
            integration::open_notification_settings,
            commands::notify,
            commands::set_unread,
            commands::dlog,
            commands::open_download,
            commands::reveal_download,
            commands::switcher_accounts,
            commands::switch_account,
            commands::get_settings,
            commands::set_settings,
            commands::open_settings,
            commands::list_accounts,
            commands::add_account,
            commands::remove_account,
            commands::rename_account,
            commands::open_account,
            commands::set_default_account,
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
        .setup(move |app| {
            let handle = app.handle();

            // Start a fresh diagnostic log for this launch (issue #3): the only
            // way to see notification failures on a Windows GUI build with no
            // console. See dlog.rs.
            dlog::init();

            // Windows: register our AppUserModelID so WinRT toast notifications
            // actually render for the installed app (no-op elsewhere). Must run
            // before any account window can fire a notification. See aumid.rs.
            aumid::register(handle);

            // A reset only runs while no WebViews hold the session stores open.
            // Failure aborts startup before reading accounts or creating a view.
            applock::complete_pending_reset(handle).map_err(std::io::Error::other)?;
            let mut s = settings::load(handle);
            if smoke_mode {
                s.start_minimized = true;
                settings::save(handle, &s)?;
            }
            let args: Vec<String> = std::env::args().collect();
            let start_hidden = settings::start_hidden(s.start_minimized, &args);

            // Load accounts (seeds a single `default` on first run / corrupt file).
            let mut f = accounts::load(handle);
            dlog::log("startup: account configuration loaded");
            if smoke_mode {
                if f.accounts.len() == 1 {
                    accounts::add(&mut f, "Conta de teste B");
                }
                accounts::save(handle, &f)?;
                applock::save(handle, &applock::AppLockConfig::default())?;
            }

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
            let lock_on_launch = lock_cfg.should_lock_on_launch();
            handle.manage(lock::LockState::new(!lock_on_launch));
            let open_hidden = start_hidden || lock_on_launch;

            let startup_label =
                accounts::startup_account(&f).map(|account| accounts::window_label(&account.id));
            if let Some(label) = &startup_label {
                *crate::lock::guard(&handle.state::<accounts::ActiveAccount>()) = label.clone();
            }

            // Build windows while Tauri is still on its UI thread. On Windows a
            // WebView2 controller is created lazily for a hidden window, so the
            // preferred account must be the one made visible here. This keeps
            // every other account connected in the background and guarantees the
            // selected startup account is the first usable window.
            for account in &f.accounts {
                let is_startup = startup_label
                    .as_deref()
                    .is_some_and(|label| label == accounts::window_label(&account.id));
                window::open_account_window(handle, account, open_hidden || !is_startup)?;
            }
            dlog::log("startup: account windows built");
            #[cfg(windows)]
            ui_health::start(handle);
            tray::setup(handle)?;
            // Let setup return before accessing tray/window APIs again. This
            // avoids waiting on the Windows UI loop while it is being created.
            let startup_handle = handle.clone();
            let open_settings = !start_hidden && args.iter().any(|a| a == "--settings");
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let ui_handle = startup_handle.clone();
                let _ = startup_handle.run_on_main_thread(move || {
                    tray::rebuild_menu(&ui_handle);
                    let _ = settings::apply(&ui_handle, &s);
                    if lock_on_launch && !start_hidden {
                        lock::show_lock_window(&ui_handle);
                    } else if open_settings {
                        window::open_settings_window(&ui_handle);
                    }
                });
            });

            // Launched by the OS to open a whatsapp:// link (issue #23). The
            // account window is already loading; point it at the chat instead.
            if let Some(link) = links::link_from_args(&args) {
                links::open_from_outside(handle, &link);
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
                        .map(|t| c.should_lock_when_idle(t.as_seconds()))
                        .unwrap_or(false);
                    if idle_ok {
                        let h = idle_handle.clone();
                        let _ = idle_handle.run_on_main_thread(move || lock::lock_now(&h));
                    }
                });
            }

            #[cfg(all(windows, debug_assertions))]
            if smoke_mode {
                smoke::start(handle);
            }
            Ok(())
        })
        .build(context)
        .expect("error while building whatRust")
        .run(|_app_handle, _event| {
            // macOS delivers scheme links (whatsapp://…, issue #23) as an Apple
            // event rather than a command-line argument, at launch or later.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &_event {
                for url in urls {
                    if url.scheme() == "whatsapp" || links::whatsapp_web_url(url.as_str()).is_some()
                    {
                        links::open_from_outside(_app_handle, url.as_str());
                    }
                }
            }

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
