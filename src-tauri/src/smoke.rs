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
fn visible(app: &AppHandle, label: &str) -> Result<bool, String> {
    let v = app.get_webview(label).ok_or("missing view")?;
    let (tx, rx) = std::sync::mpsc::channel();
    v.with_webview(move |platform| {
        let mut value = windows::core::BOOL::default();
        let result = unsafe { platform.controller().IsVisible(&mut value) }
            .map(|_| value.as_bool())
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(4))
        .map_err(|e| e.to_string())?
}
fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if condition() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn fills_client(app: &AppHandle, label: &str) -> bool {
    let handle = app.clone();
    let label = label.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let result = (|| {
            let w = handle.get_window("main")?;
            let scale = w.scale_factor().ok()?;
            let client = w.inner_size().ok()?;
            let bounds = handle.get_webview(&label)?.bounds().ok()?;
            let origin = bounds.position.to_physical::<i32>(scale);
            let size = bounds.size.to_physical::<u32>(scale);
            Some(size.width == client.width && origin.y as u32 + size.height == client.height)
        })();
        let _ = tx.send(result == Some(true));
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false)
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
            let startup_hidden = crate::settings::start_hidden(
                crate::settings::load(&app).start_minimized,
                &std::env::args().collect::<Vec<_>>(),
            );
            result.push(json!({"test":"startup_visibility_matches_explicit_foreground_request","pass":app.get_window("main").unwrap().is_visible().unwrap_or(false)!=startup_hidden}));
            crate::dlog::log("smoke: two_live_profiles");
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
            crate::dlog::log("smoke: real_whatsapp_login_screen_without_linked_account");
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
            crate::dlog::log("smoke: login_header_transitions_do_not_deadlock");
            result.push(json!({"test":"login_header_transitions_do_not_deadlock","pass":true}));
            crate::dlog::log("smoke: physical_bounds_have_no_bottom_gap");
            result.push(json!({"test":"physical_bounds_have_no_bottom_gap","pass":true}));
            // These profiles are isolated from the user's sessions. A logged-out
            // WhatsApp view must still allow opening the local account panel.
            crate::dlog::log("smoke: logged_out_profile_has_management_access");
            result.push(json!({"test":"logged_out_profile_has_management_access","pass":app.get_webview("shell-main").is_some()&&crate::accounts::startup_account(&f).is_some()}));
            eval(&app, &label, "window.__whatrustSmokeToken='same-live-view'")?;
            crate::sessions::detach(&app, &a.id)?;
            crate::dlog::log("smoke: detach_without_recreate");
            result.push(json!({"test":"detach_without_recreate","pass":app.get_webview(&label).unwrap().window().label().starts_with("detached-")&&eval(&app,&label,"window.__whatrustSmokeToken==='same-live-view'")?=="true"}));
            crate::sessions::attach(&app, &a.id)?;
            crate::dlog::log("smoke: reattach_preserves_view");
            result.push(json!({"test":"reattach_preserves_view","pass":app.get_webview(&label).unwrap().window().label()=="main"&&eval(&app,&label,"window.__whatrustSmokeToken==='same-live-view'")?=="true"}));
            let second_label = crate::accounts::window_label(&b.id);
            crate::sessions::activate(&app, &label);
            eval(
                &app,
                &second_label,
                "window.__whatrustCloseSmokeToken='preserve-close-view'",
            )?;
            crate::sessions::detach(&app, &b.id)?;
            app.get_window(&format!("detached-{}", b.id))
                .ok_or("missing detached window")?
                .close()
                .map_err(|e| e.to_string())?;
            let returned_to_main_without_stealing_selection = wait_until(|| {
                crate::sessions::selected(&app, "main").as_deref() == Some(label.as_str())
                    && app
                        .get_webview(&second_label)
                        .is_some_and(|view| view.window().label() == "main")
                    && visible(&app, &label).unwrap_or(false)
                    && !visible(&app, &second_label).unwrap_or(true)
            });
            result.push(json!({"test":"closing_detached_window_preserves_main_account","pass":returned_to_main_without_stealing_selection&&eval(&app,&second_label,"window.__whatrustCloseSmokeToken==='preserve-close-view'")?=="true"}));
            for cycle in 0..12 {
                crate::sessions::schedule_layout(&app, "main");
                crate::sessions::detach(&app, &b.id)?;
                crate::sessions::schedule_layout(&app, &format!("detached-{}", b.id));
                crate::sessions::attach(&app, &b.id)?;
                let target = crate::accounts::window_label(&b.id);
                if eval(&app, &target, "2+2")? != "4" {
                    return Err(format!("reparent/layout circular wait at cycle {cycle}"));
                }
            }
            result.push(json!({"test":"reparent_with_pending_layout_12_cycles","pass":true}));
            crate::sessions::activate(&app, &label);
            crate::sessions::open_panel(&app, Some("main")).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_secs(2));
            crate::dlog::log("smoke: local_panel_same_window");
            result.push(json!({"test":"local_panel_same_window","pass":app.get_webview("settings").is_some_and(|v|v.window().label()=="main"&&crate::integration::trusted_local(&v))}));
            crate::dlog::log("smoke: local_panel_loaded");
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
            crate::dlog::log("smoke: remote_cannot_read_settings");
            result.push(json!({"test":"remote_cannot_read_settings","pass":eval(&app,&label,"window.__smokeDenied===true")?=="true"}));
            crate::sessions::close_panel(&app);
            app.get_window("main")
                .unwrap()
                .set_size(tauri::LogicalSize::new(900., 650.))
                .map_err(|e| e.to_string())?;
            result.push(
                json!({"test":"native_resize","pass":wait_until(||fills_client(&app,&label))}),
            );
            let w = app.get_window("main").unwrap();
            for (width, height) in [(560., 480.), (1100., 800.), (900., 650.)] {
                w.set_size(tauri::LogicalSize::new(width, height))
                    .map_err(|e| e.to_string())?;
                if !wait_until(|| fills_client(&app, &label)) {
                    return Err("resize did not fill client area".into());
                }
            }
            crate::dlog::log("smoke: repeated_resize_fills_client_area");
            result.push(json!({"test":"repeated_resize_fills_client_area","pass":true}));
            w.maximize().map_err(|e| e.to_string())?;
            crate::dlog::log("smoke: maximized_view_has_no_client_gap");
            result.push(json!({"test":"maximized_view_has_no_client_gap","pass":wait_until(||fills_client(&app,&label))}));
            w.unmaximize().map_err(|e| e.to_string())?;
            // Concurrent callers must finish on exactly one authoritative view.
            for _ in 0..6 {
                let h1 = app.clone();
                let l1 = label.clone();
                let h2 = app.clone();
                let l2 = crate::accounts::window_label(&f.accounts[1].id);
                let first = std::thread::spawn(move || crate::sessions::activate(&h1, &l1));
                let second = std::thread::spawn(move || crate::sessions::activate(&h2, &l2));
                first.join().map_err(|_| "activation worker failed")?;
                second.join().map_err(|_| "activation worker failed")?;
            }
            crate::sessions::activate(&app, &label);
            eval(&app, &label, "true")?; // UI queue barrier after the final request.
            let global_active = app
                .state::<crate::accounts::ActiveAccount>()
                .lock()
                .unwrap()
                .clone();
            crate::dlog::log("smoke: rapid_account_switch_identity_consistent");
            result.push(json!({"test":"rapid_account_switch_identity_consistent","pass":crate::sessions::selected(&app,"main").as_deref()==Some(label.as_str())&&global_active==label&&app.get_window("main").unwrap().title().map_err(|e|e.to_string())?.ends_with(&a.name)}));
            crate::dlog::log("smoke: rapid_account_switch_exactly_one_visible");
            result.push(json!({"test":"rapid_account_switch_exactly_one_visible","pass":visible(&app,&label)?&&!visible(&app,&crate::accounts::window_label(&f.accounts[1].id))?}));
            for (test, target, hide) in [
                (
                    "first_switch_background_viewport",
                    crate::accounts::window_label(&f.accounts[1].id),
                    false,
                ),
                ("single_activation_restores_viewport", label.clone(), true),
            ] {
                let w = app.get_window("main").unwrap();
                if hide {
                    w.hide().map_err(|e| e.to_string())?;
                }
                crate::sessions::activate(&app, &target);
                let expected = w
                    .inner_size()
                    .map_err(|e| e.to_string())?
                    .to_logical::<f64>(w.scale_factor().unwrap_or(1.))
                    .width;
                let script = format!("innerWidth>={}", expected - 2.);
                let mut full = false;
                for _ in 0..20 {
                    if eval(&app, &target, &script)? == "true" {
                        full = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                result.push(json!({"test":test,"pass":full}));
            }
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
            crate::dlog::log("smoke: windows_notification_submission");
            result.push(json!({"test":"windows_notification_submission","pass":app.state::<crate::notify::Recent>().records().iter().any(|n|n.source==crate::notify::Source::Test && n.status=="submitted")}));
            // Exercise the real page constructor, not just the Rust test command:
            // plugin JS replacement used to silently invoke forbidden IPC here.
            let host = app.get_webview(&label).unwrap().window();
            host.minimize().map_err(|e| e.to_string())?;
            result.push(json!({"test":"minimized_account_is_not_foreground","pass":wait_until(||host.is_minimized().unwrap_or(false))&&!crate::sessions::account_focused(&app,&label)}));
            result.push(json!({"test":"minimized_document_is_hidden","pass":wait_until(||eval(&app,&label,"document.hidden").ok().as_deref()==Some("true"))}));
            let context=eval(&app,&label,"JSON.stringify({hidden:document.hidden,focused:document.hasFocus(),preferNative:window.__whatrustPreferNative,permission:Notification.permission})")?;
            crate::dlog::log(&format!("smoke: minimized browser context {context}"));
            for (test, hide) in [
                ("page_notification_while_minimized", false),
                ("page_notification_while_hidden_to_tray", true),
            ] {
                if hide {
                    host.hide().map_err(|e| e.to_string())?;
                    crate::sessions::layout(&app, host.label());
                }
                let previous = app
                    .state::<crate::notify::Recent>()
                    .records()
                    .iter()
                    .filter(|n| n.source == crate::notify::Source::WebView2)
                    .count();
                eval(&app,&label,"new Notification('Teste isolado do whatRust',{body:'Verificação da rota do navegador com janela oculta.'});true")?;
                let delivered = wait_until(|| {
                    app.state::<crate::notify::Recent>()
                        .records()
                        .iter()
                        .filter(|n| {
                            n.source == crate::notify::Source::WebView2
                                && matches!(
                                    n.status.as_str(),
                                    "submitted" | "dismissed" | "activated"
                                )
                        })
                        .count()
                        > previous
                });
                result.push(json!({"test":test,"pass":delivered}));
            }
            let page_before = app
                .state::<crate::notify::Recent>()
                .records()
                .iter()
                .filter(|n| n.source == crate::notify::Source::Page)
                .count();
            eval(&app,&label,"ServiceWorkerRegistration.prototype.showNotification.call({},'Teste da rota de registro',{body:'Verificação isolada do encaminhamento ao aplicativo.'});true")?;
            result.push(json!({"test":"registration_notification_while_hidden_reaches_own_service","pass":wait_until(||app.state::<crate::notify::Recent>().records().iter().filter(|n|n.source==crate::notify::Source::Page&&matches!(n.status.as_str(),"submitted"|"dismissed"|"activated")).count()>page_before)}));
            let s = crate::settings::load(&app);
            let mut off = s.clone();
            off.notifications = false;
            crate::settings::save(&app, &off).map_err(|e| e.to_string())?;
            eval(
                &app,
                &label,
                "new Notification('Teste sem aviso',{body:'Desativado nas preferências.'});true",
            )?;
            result.push(json!({"test":"page_constructor_respects_notifications_disabled","pass":wait_until(||app.state::<crate::notify::Recent>().records().iter().any(|n|n.source==crate::notify::Source::WebView2&&n.reason.as_deref()==Some("notifications_disabled")))}));
            crate::settings::save(&app, &s).map_err(|e| e.to_string())?;
            crate::sessions::activate(&app, &label);
            result.push(json!({"test":"restored_document_is_visible","pass":wait_until(||eval(&app,&label,"!document.hidden").ok().as_deref()==Some("true"))}));
            for step in 0..12 {
                eval(&app,&label,"window.__closeStress=new Notification('Teste de encerramento',{body:'Evento sintético para verificar a interface.'});window.__closeStress.close();window.__TAURI__.core.invoke('set_unread',{title:'WhatsApp',settled:true});true")?;
                crate::sessions::activate(&app, &crate::accounts::window_label(&b.id));
                crate::sessions::open_panel(&app, Some("main")).map_err(|e| e.to_string())?;
                crate::sessions::close_panel(&app);
                crate::sessions::activate(&app, &label);
                if eval(&app, &label, "1+1")? != "2" {
                    return Err(format!("UI stalled after notification close, cycle {step}"));
                }
            }
            result.push(json!({"test":"notification_close_with_account_panel_navigation_12_cycles","pass":true}));
            let panel = app.get_webview("settings").ok_or("missing local panel")?;
            crate::dlog::log("smoke: password_validation");
            result.push(json!({"test":"password_validation","pass":tauri::async_runtime::block_on(crate::commands::set_app_lock_password(panel.clone(),app.clone(),"abc".into(),"abc".into())).is_err()&&tauri::async_runtime::block_on(crate::commands::set_app_lock_password(panel.clone(),app.clone(),"test-pass".into(),"mismatch".into())).is_err()}));
            tauri::async_runtime::block_on(crate::commands::set_app_lock_password(
                panel.clone(),
                app.clone(),
                "smoke-old".into(),
                "smoke-old".into(),
            ))?;
            crate::dlog::log("smoke: password_stored_as_argon2id");
            result.push(json!({"test":"password_stored_as_argon2id","pass":crate::applock::load(&app).password_phc.is_some_and(|p|p.starts_with("$argon2id$")&&!p.contains("smoke-old"))}));
            crate::dlog::log("smoke: password_change_requires_current");
            result.push(json!({"test":"password_change_requires_current","pass":tauri::async_runtime::block_on(crate::commands::change_app_lock_password(panel.clone(),app.clone(),"wrong".into(),"smoke-new".into(),"smoke-new".into())).is_err()}));
            tauri::async_runtime::block_on(crate::commands::change_app_lock_password(
                panel.clone(),
                app.clone(),
                "smoke-old".into(),
                "smoke-new".into(),
                "smoke-new".into(),
            ))?;
            crate::commands::set_app_lock_options(panel.clone(), app.clone(), true, true, 120)?;
            let config = crate::applock::load(&app);
            crate::dlog::log("smoke: lock_preferences_persist");
            result.push(json!({"test":"lock_preferences_persist","pass":config.lock_on_launch&&config.lock_on_hide&&config.idle_secs==120}));
            crate::sessions::detach(&app, &b.id)?;
            crate::lock::lock_now(&app);
            if !wait_until(|| !crate::lock::is_unlocked(&app) && app.get_webview("lock").is_some())
            {
                return Err("lock screen did not open".into());
            }
            crate::dlog::log("smoke: lock_hides_all_hosts");
            result.push(json!({"test":"lock_hides_all_hosts","pass":!app.get_window("main").unwrap().is_visible().unwrap_or(true)&&!app.get_window(&format!("detached-{}",b.id)).unwrap().is_visible().unwrap_or(true)}));
            crate::lock::lock_now(&app); // Repeated lock must preserve the hidden-host list.
            let target = crate::accounts::window_label(&b.id);
            crate::sessions::activate(&app, &target);
            crate::sessions::open_panel(&app, Some("main")).map_err(|e| e.to_string())?;
            eval(&app, &label, "true")?;
            crate::dlog::log("smoke: locked_activation_and_panel_do_not_reveal_accounts");
            result.push(json!({"test":"locked_activation_and_panel_do_not_reveal_accounts","pass":!app.get_window("main").unwrap().is_visible().unwrap_or(true)&&!app.get_window(&format!("detached-{}",b.id)).unwrap().is_visible().unwrap_or(true)}));
            if !wait_until(|| {
                eval(&app, "lock", "!!document.getElementById('unlock')")
                    .ok()
                    .as_deref()
                    == Some("true")
            }) {
                return Err("lock UI not ready".into());
            }
            let lock_view = app.get_webview("lock").ok_or("missing lock view")?;
            result.push(json!({"test":"disabled_biometric_cannot_unlock","pass":crate::commands::unlock_biometric(lock_view,app.clone()).is_err()&&!crate::lock::is_unlocked(&app)}));
            result.push(json!({"test":"remote_cannot_unlock","pass":tauri::async_runtime::block_on(crate::commands::unlock_password(app.get_webview(&label).unwrap(),app.clone(),"smoke-new".into())).is_err()&&!crate::lock::is_unlocked(&app)}));
            eval(&app,"lock","document.getElementById('password').value='wrong';document.getElementById('unlock').click();true")?;
            let wrong = wait_until(|| {
                eval(
                    &app,
                    "lock",
                    "document.getElementById('error').textContent.includes('Senha incorreta')",
                )
                .ok()
                .as_deref()
                    == Some("true")
            });
            crate::dlog::log("smoke: wrong_password_keeps_locked_portuguese_feedback");
            result.push(json!({"test":"wrong_password_keeps_locked_portuguese_feedback","pass":wrong&&!crate::lock::is_unlocked(&app)}));
            eval(&app,"lock","document.getElementById('password').value='smoke-new';document.getElementById('unlock').click();true")?;
            let unlocked = wait_until(|| {
                crate::lock::is_unlocked(&app)
                    && app
                        .get_window(&format!("detached-{}", b.id))
                        .unwrap()
                        .is_visible()
                        .unwrap_or(false)
            });
            let pending = wait_until(|| {
                *app.state::<crate::accounts::ActiveAccount>()
                    .lock()
                    .unwrap()
                    == target
            });
            crate::dlog::log("smoke: correct_password_restores_pending_account");
            result.push(json!({"test":"correct_password_restores_pending_account","pass":unlocked&&pending}));
            crate::dlog::log("smoke: reattach after authentication begin");
            crate::sessions::attach(&app, &b.id)?;
            crate::dlog::log("smoke: reattach after authentication end");
            crate::dlog::log("smoke: close to tray begin");
            app.get_window("main")
                .unwrap()
                .close()
                .map_err(|e| e.to_string())?;
            crate::dlog::log("smoke: close_to_tray_locks_when_enabled");
            result.push(json!({"test":"close_to_tray_locks_when_enabled","pass":wait_until(||!crate::lock::is_unlocked(&app)&&!app.get_window("main").unwrap().is_visible().unwrap_or(true))}));
            std::thread::sleep(Duration::from_millis(300));
            result.push(json!({"test":"hide_lock_does_not_prompt_until_reopen","pass":app.get_window("lock").is_none_or(|w|!w.is_visible().unwrap_or(false))}));
            let before = app.state::<crate::notify::Recent>().records().len();
            eval(
                &app,
                &label,
                "new Notification('Teste bloqueado',{body:'Não deve aparecer no Windows.'});true",
            )?;
            result.push(json!({"test":"incoming_browser_notification_suppressed_while_locked","pass":wait_until(||app.state::<crate::notify::Recent>().records().len()>before)&&app.state::<crate::notify::Recent>().records().first().is_some_and(|n|n.status=="suppressed"&&n.reason.as_deref()==Some("locked"))}));
            result.push(json!({"test":"lock_removes_existing_windows_notification_history","pass":wait_until(||{
                use windows::{core::HSTRING,UI::Notifications::ToastNotificationManager};
                ToastNotificationManager::History().and_then(|h|h.GetHistoryWithId(&HSTRING::from(&app.config().identifier))).and_then(|h|h.Size()).is_ok_and(|size|size==0)
            })}));
            crate::window::show_main(&app);
            result.push(json!({"test":"reopen_locked_app_prompts_without_revealing_account","pass":wait_until(||app.get_window("lock").is_some_and(|w|w.is_visible().unwrap_or(false)))&&!app.get_window("main").unwrap().is_visible().unwrap_or(true)}));
            crate::lock::unlock(&app);
            if !wait_until(|| crate::lock::is_unlocked(&app)) {
                return Err("unlock did not complete".into());
            }
            let visual_count = app
                .state::<crate::notify::Recent>()
                .records()
                .iter()
                .filter(|n| n.source == crate::notify::Source::Visual)
                .count();
            crate::notify::visual_signal(&app, &label, "delayed-before-lock", true, Some(false));
            crate::lock::lock_on_hide(&app);
            if !wait_until(|| !crate::lock::is_unlocked(&app)) {
                return Err("silent lock did not complete".into());
            }
            crate::lock::unlock(&app);
            if !wait_until(|| crate::lock::is_unlocked(&app)) {
                return Err("unlock did not complete".into());
            }
            std::thread::sleep(Duration::from_millis(2100));
            result.push(json!({"test":"delayed_notification_not_replayed_after_lock_unlock","pass":app.state::<crate::notify::Recent>().records().iter().filter(|n|n.source==crate::notify::Source::Visual).count()==visual_count}));
            let stale = app
                .state::<crate::notify::Recent>()
                .epoch()
                .saturating_sub(1);
            crate::commands::notify(
                app.get_webview(&label).unwrap(),
                app.clone(),
                "Evento antigo".into(),
                "Conversão de ícone atrasada".into(),
                None,
                Some(stale),
            );
            result.push(json!({"test":"stale_page_icon_notification_not_replayed_after_unlock","pass":app.state::<crate::notify::Recent>().records().first().is_some_and(|n|n.status=="suppressed"&&n.reason.as_deref()==Some("locked"))}));
            crate::dlog::log("smoke: disable_lock_requires_current_password");
            result.push(json!({"test":"disable_lock_requires_current_password","pass":tauri::async_runtime::block_on(crate::commands::disable_app_lock(panel.clone(),app.clone(),"wrong".into())).is_err()}));
            tauri::async_runtime::block_on(crate::commands::disable_app_lock(
                panel,
                app.clone(),
                "smoke-new".into(),
            ))?;
            crate::dlog::log("smoke: disable_clears_hash_and_biometrics");
            result.push(json!({"test":"disable_clears_hash_and_biometrics","pass":crate::applock::load(&app)==crate::applock::AppLockConfig::default()}));

            // Isolated, finite stall validates the diagnostic path itself.
            let (tx, rx) = std::sync::mpsc::channel();
            app.run_on_main_thread(move || {
                let _health = crate::ui_health::enter("synthetic stall test");
                std::thread::sleep(Duration::from_millis(7200));
                let _ = tx.send(());
            })
            .map_err(|e| e.to_string())?;
            let recovered = rx.recv_timeout(Duration::from_secs(12)).is_ok();
            let log = std::env::var_os("WHATRUST_SMOKE_REPORT")
                .map(std::path::PathBuf::from)
                .map(|p| p.with_extension("log"));
            result.push(json!({"test":"ui_heartbeat_detects_stall_and_recovers","pass":recovered&&wait_until(||log.as_ref().and_then(|p|std::fs::read_to_string(p).ok()).is_some_and(|s|s.contains("ui: heartbeat delayed;")&&s.contains("ui: heartbeat recovered")))}));

            Ok(())
        })();
        if let Err(e) = run {
            crate::dlog::log("smoke: native_smoke");
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
