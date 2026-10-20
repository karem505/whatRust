//! Native integration smoke, debug-only, isolated identifier and synthetic accounts.
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Manager};
fn eval(app: &AppHandle, label: &str, script: &str) -> Result<String, String> {
    let v = app.get_webview(label).ok_or("missing webview")?;
    let (tx, rx) = std::sync::mpsc::channel();
    v.eval_with_callback(script.to_string(), move |s| {
        let _ = tx.send(s);
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(8))
        .map_err(|e| e.to_string())
}
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(8));
        let mut result: Vec<Value> = vec![];
        let run = (|| -> Result<(), String> {
            let f = crate::accounts::load(&app);
            let a = &f.accounts[0];
            let b = &f.accounts[1];
            let label = crate::accounts::window_label(&a.id);
            result.push(json!({"test":"two_live_profiles","pass":app.get_webview(&label).is_some()&&app.get_webview(&crate::accounts::window_label(&b.id)).is_some()}));
            // Reproduce the installed-app regression: the page reports no
            // anchor while loading/QR login, then mounts/unmounts its header.
            crate::sessions::activate(&app, &label);
            let login_script = "!!document.body && !document.getElementById('pane-side') && (/QR|scan|código|vincular|link devices|phone number|número de telefone|WhatsApp on your|WhatsApp no seu/i.test(document.body.innerText))";
            let mut login_ready = false;
            for _ in 0..40 {
                if eval(&app, &label, login_script)? == "true" {
                    login_ready = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
            result.push(json!({"test":"real_whatsapp_login_screen_without_linked_account","pass":login_ready}));
            for (step, mounted) in [false, true, false, true, false].into_iter().enumerate() {
                eval(&app, &label, &format!("window.__menuStep=-1;window.__TAURI__.core.invoke('report_integration',{{mounted:{mounted},detector:false}}).then(()=>window.__menuStep={step})"))?;
                for _ in 0..20 {
                    if eval(&app, &label, &format!("window.__menuStep==={step}"))? == "true" {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if eval(&app, &label, &format!("window.__menuStep==={step}"))? != "true" {
                    return Err("menu transition stalled".into());
                }
                let w = app.get_window("main").unwrap();
                let client = w.inner_size().map_err(|e| e.to_string())?;
                let bounds = app
                    .get_webview(&label)
                    .unwrap()
                    .bounds()
                    .map_err(|e| e.to_string())?;
                let scale = w.scale_factor().unwrap_or(1.);
                let origin = bounds.position.to_physical::<i32>(scale);
                let size = bounds.size.to_physical::<u32>(scale);
                if origin.y as u32 + size.height != client.height || size.width != client.width {
                    return Err(format!("webview bounds mismatch step={step} mounted={mounted} scale={scale} client={client:?} origin={origin:?} size={size:?}"));
                }
            }
            result.push(json!({"test":"login_header_transitions_do_not_deadlock","pass":true}));
            result.push(json!({"test":"physical_bounds_have_no_bottom_gap","pass":true}));
            // These profiles are isolated from the user's sessions. A logged-out
            // WhatsApp view must still allow opening the local account panel.
            result.push(json!({"test":"logged_out_profile_has_management_access","pass":app.get_webview("shell-main").is_some()&&crate::accounts::startup_account(&f).is_some()}));
            eval(&app, &label, "window.__whatrustSmokeToken='same-live-view'")?;
            crate::sessions::detach(&app, &a.id)?;
            result.push(json!({"test":"detach_without_recreate","pass":app.get_webview(&label).unwrap().window().label().starts_with("detached-")&&eval(&app,&label,"window.__whatrustSmokeToken==='same-live-view'")?=="true"}));
            crate::sessions::attach(&app, &a.id)?;
            result.push(json!({"test":"reattach_preserves_view","pass":app.get_webview(&label).unwrap().window().label()=="main"&&eval(&app,&label,"window.__whatrustSmokeToken==='same-live-view'")?=="true"}));
            crate::sessions::open_panel(&app, Some("main")).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_secs(2));
            result.push(json!({"test":"local_panel_same_window","pass":app.get_webview("settings").is_some_and(|v|v.window().label()=="main"&&crate::integration::trusted_local(&v))}));
            result.push(json!({"test":"local_panel_loaded","pass":eval(&app,"settings","document.querySelectorAll('.group').length>=7")?=="true"}));
            crate::sessions::close_panel(&app);
            crate::sessions::activate(&app, &label);
            eval(&app,&label,"window.__smokeDenied=null;window.__TAURI__.core.invoke('get_settings').then(()=>window.__smokeDenied=false,()=>window.__smokeDenied=true)")?;
            for _ in 0..20 {
                if eval(&app, &label, "window.__smokeDenied===true")? == "true" {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            result.push(json!({"test":"remote_cannot_read_settings","pass":eval(&app,&label,"window.__smokeDenied===true")?=="true"}));
            crate::sessions::close_panel(&app);
            app.get_window("main")
                .unwrap()
                .set_size(tauri::LogicalSize::new(900., 650.))
                .map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(500));
            let bounds = app
                .get_webview(&label)
                .unwrap()
                .bounds()
                .map_err(|e| e.to_string())?;
            result.push(json!({"test":"native_resize","pass":bounds.size.to_logical::<f64>(app.get_window("main").unwrap().scale_factor().unwrap_or(1.)).width>500.}));
            let w = app.get_window("main").unwrap();
            for (width, height) in [(560., 480.), (1100., 800.), (900., 650.)] {
                w.set_size(tauri::LogicalSize::new(width, height))
                    .map_err(|e| e.to_string())?;
                std::thread::sleep(Duration::from_millis(200));
                let scale = w.scale_factor().unwrap_or(1.);
                let client = w.inner_size().map_err(|e| e.to_string())?;
                let b = app
                    .get_webview(&label)
                    .unwrap()
                    .bounds()
                    .map_err(|e| e.to_string())?;
                let origin = b.position.to_physical::<i32>(scale);
                let size = b.size.to_physical::<u32>(scale);
                if size.width != client.width || origin.y as u32 + size.height != client.height {
                    return Err("resize leaves client gap".into());
                }
            }
            result.push(json!({"test":"repeated_resize_fills_client_area","pass":true}));
            w.maximize().map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(350));
            let scale = w.scale_factor().unwrap_or(1.);
            let client = w.inner_size().map_err(|e| e.to_string())?;
            let b = app
                .get_webview(&label)
                .unwrap()
                .bounds()
                .map_err(|e| e.to_string())?;
            let origin = b.position.to_physical::<i32>(scale);
            let size = b.size.to_physical::<u32>(scale);
            result.push(json!({"test":"maximized_view_has_no_client_gap","pass":size.width==client.width&&origin.y as u32+size.height==client.height}));
            w.unmaximize().map_err(|e| e.to_string())?;
            crate::notify::from_account(
                &app,
                &label,
                "Teste isolado do whatRust",
                "Verificação do envio ao Windows; nenhum contato recebeu mensagem.",
                None,
                crate::notify::Source::Test,
            );
            for _ in 0..20 {
                if app
                    .state::<crate::notify::Recent>()
                    .records()
                    .iter()
                    .any(|n| n.source == crate::notify::Source::Test && n.status != "pending")
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            result.push(json!({"test":"windows_notification_submission","pass":app.state::<crate::notify::Recent>().records().iter().any(|n|n.source==crate::notify::Source::Test && n.status=="submitted")}));
            crate::lock::lock_now(&app);
            std::thread::sleep(Duration::from_millis(300));
            result.push(json!({"test":"lock_hides_host","pass":!app.get_window("main").unwrap().is_visible().unwrap_or(true)&&!crate::lock::is_unlocked(&app)}));
            crate::lock::unlock(&app);
            result.push(json!({"test":"unlock_restores_host","pass":crate::lock::is_unlocked(&app)&&app.get_window("main").unwrap().is_visible().unwrap_or(false)}));
            Ok(())
        })();
        if let Err(e) = run {
            result.push(json!({"test":"native_smoke","pass":false,"error":e}));
        }
        let passed = result.iter().all(|r| r["pass"] == true);
        if let Some(path) = std::env::var_os("WHATRUST_SMOKE_REPORT") {
            let _ = std::fs::write(
                path,
                serde_json::to_string_pretty(&json!({"passed":passed,"checks":result})).unwrap(),
            );
        }
        app.exit(if passed { 0 } else { 1 });
    });
}
