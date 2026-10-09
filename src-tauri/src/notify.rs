use tauri::AppHandle;

/// Show a native notification.
///
/// Windows raises the WinRT toast itself (the way ZapFast does) instead of going
/// through `tauri-plugin-notification`: the plugin fires the toast on a detached
/// task and throws the result away, so a failure there could never be seen
/// (issue #3). Here the result is logged, and clicking the toast brings whatRust
/// forward. No message content is logged (PII) — only that a toast was shown.
pub fn show(app: &AppHandle, title: &str, body: &str) {
    #[cfg(windows)]
    {
        windows_toast(app, title, body);
    }
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        let r = app.notification().builder().title(title).body(body).show();
        crate::dlog::log(&format!("notify::show dispatched (plugin returned {r:?})"));
    }
}

#[cfg(windows)]
fn windows_toast(app: &AppHandle, title: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    // The AUMID aumid.rs registers at startup, so the toast is attributed to
    // whatRust and allowed to render.
    let aumid = app.config().identifier.clone();
    let (app, title, body) = (app.clone(), title.to_string(), body.to_string());
    // WinRT calls can block briefly; keep them off the UI/IPC thread.
    std::thread::spawn(move || {
        let click = app.clone();
        let result = Toast::new(&aumid)
            .title(&title)
            .text1(&body)
            .on_activated(move |_| {
                let app = click.clone();
                let _ = click.run_on_main_thread(move || crate::window::show_main(&app));
                Ok(())
            })
            .show();
        match result {
            Ok(()) => crate::dlog::log("notify::show: toast shown"),
            Err(e) => crate::dlog::log(&format!("notify::show: toast FAILED: {e}")),
        }
    });
}
