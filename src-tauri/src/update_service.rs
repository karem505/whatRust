//! Scheduling, status and UI plumbing around the update engine (`updater.rs`):
//! a background check shortly after start and then every six hours, a native
//! notification and a tray item when a newer release exists, optional automatic
//! installation, and the status the Settings → Updates page shows.
//!
//! Two switches in Settings control it: "Check for updates automatically" (on
//! by default) and "Install updates automatically" (on by default, but it only
//! acts where no password is needed: AppImage and macOS bundles are replaced in
//! place and take effect on the next start; a Windows installer is downloaded
//! and verified in the background and runs when the user clicks "Restart and
//! install", because it restarts the app).
//!
//! The only hosts contacted are api.github.com (the release feed) and github.com
//! (the assets), through the proxy chosen in Settings → Network.

use crate::i18n::{tr, trf};
use crate::updater::{self, Kind, Method, Outcome, Relaunch, Release, Staged};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// With `WHATRUST_UPDATE_FEED` set (tests, local end-to-end runs) nobody waits a minute.
const FIRST_CHECK_DELAY_OVERRIDDEN: Duration = Duration::from_secs(3);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const TICK: Duration = Duration::from_secs(5 * 60);

// ---- persisted bookkeeping --------------------------------------------------------------------

/// Small file next to `settings.json`, kept apart so a settings save from the UI can
/// never overwrite it.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Persisted {
    last_check_ms: i64,
    /// the newest version the user was already told about
    notified: String,
}

fn state_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("update-state.json"))
}

fn load_persisted(app: &AppHandle) -> Persisted {
    use crate::jsonfile::Loaded;
    let Some(p) = state_path(app) else {
        return Persisted::default();
    };
    match crate::jsonfile::read::<Persisted>(&p) {
        Loaded::Ok(s) | Loaded::Recovered(s) => s,
        _ => Persisted::default(),
    }
}

fn save_persisted(app: &AppHandle, s: &Persisted) {
    if let Some(p) = state_path(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::jsonfile::save(&p, s);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- shared state -----------------------------------------------------------------------------

#[derive(Default)]
struct Status {
    /// idle | checking | current | available | downloading | staged | installing | ready | handoff | opened | error
    state: &'static str,
    latest: Option<Release>,
    received: u64,
    total: u64,
    error: String,
    restart: Option<Relaunch>,
    staged: Option<Staged>,
    /// a file saved for the user (Flatpak bundle, disk image, .deb without pkexec)
    opened: String,
    note: String,
}

fn status() -> &'static Mutex<Status> {
    static S: OnceLock<Mutex<Status>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Status {
            state: "idle",
            ..Default::default()
        })
    })
}

fn set(f: impl FnOnce(&mut Status)) {
    f(&mut crate::lock::guard(status()));
}

static BUSY: AtomicBool = AtomicBool::new(false);

/// How this copy was installed (decided once; a package upgrade replaces the
/// executable on disk, so it must not be re-detected later).
pub fn method() -> &'static Method {
    static M: OnceLock<Method> = OnceLock::new();
    M.get_or_init(|| updater::detect(&updater::Env::current()))
}

/// The version of a newer release that was found, if any.
pub fn available_version() -> Option<String> {
    let s = crate::lock::guard(status());
    match s.state {
        "available" | "downloading" | "staged" | "error" => s
            .latest
            .as_ref()
            .filter(|r| r.version > updater::current_version())
            .map(|r| r.version.to_string()),
        _ => None,
    }
}

fn downloads_dir(app: &AppHandle) -> std::path::PathBuf {
    let s = crate::settings::load(app);
    crate::downloads::configured_dir(s.download_dir.as_deref())
        .or_else(|| app.path().download_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
}

// ---- status for the Updates page ----------------------------------------------------------------

pub fn status_json(app: &AppHandle) -> Value {
    let p = load_persisted(app);
    let m = method();
    let s = crate::lock::guard(status());
    let kind_available = s
        .latest
        .as_ref()
        .map(|r| updater::plan(m, r, updater::Arch::current(), std::path::Path::new("")).is_ok());
    json!({
        "current": env!("CARGO_PKG_VERSION"),
        "state": s.state,
        "newer": s.latest.as_ref().is_some_and(|r| r.version > updater::current_version()),
        "latest": s.latest.as_ref().map(|r| json!({
            "version": r.version.to_string(),
            "name": r.name,
            "notes": r.notes,
            "url": r.url,
            "published": r.published,
        })),
        "received": s.received,
        "total": s.total,
        "error": s.error,
        "restart": s.restart.is_some(),
        "staged": s.staged.is_some(),
        "opened": s.opened,
        "note": s.note,
        "lastCheck": p.last_check_ms,
        "method": {
            "id": m.id(),
            "label": m.label(),
            "canInstall": m.asset_kind(true).is_some(),
            "assetAvailable": kind_available.unwrap_or(true),
            "needsPassword": m.needs_password(),
            "autoInstall": m.auto_install_default(),
            "howTo": m.how_to(),
            "flatpakCommand": if matches!(m, Method::Flatpak) { updater::FLATPAK_COMMAND } else { "" },
        },
        "releasesUrl": format!("https://github.com/{}/releases", updater::REPO),
    })
}

/// Tell the Updates page (and the tray) that something changed.
fn publish(app: &AppHandle) {
    let _ = app.emit("whatrust-update-status", status_json(app));
    let a = app.clone();
    let _ = app.run_on_main_thread(move || crate::tray::rebuild_menu(&a));
}

// ---- checking -----------------------------------------------------------------------------------

/// Ask GitHub for the newest release. Returns the newer release, if any.
fn check(app: &AppHandle) -> Option<Release> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return None;
    }
    let keep = crate::lock::guard(status()).state;
    let busy_with_install = matches!(keep, "downloading" | "staged" | "ready" | "handoff");
    set(|s| {
        if !busy_with_install {
            s.state = "checking";
            s.error.clear();
        }
    });
    publish(app);
    let result = crate::updater::Net::from_settings(&crate::settings::load(app))
        .and_then(|net| updater::fetch_latest(&net));
    let mut p = load_persisted(app);
    p.last_check_ms = now_ms();
    let mut found = None;
    match result {
        Ok(rel) => {
            let newer = rel.version > updater::current_version();
            crate::dlog::log(&format!(
                "update: check ok, latest {} ({})",
                rel.version,
                if newer { "newer" } else { "current" }
            ));
            if newer {
                found = Some(rel.clone());
            }
            set(|s| {
                if !busy_with_install {
                    s.state = if newer { "available" } else { "current" };
                }
                s.latest = Some(rel);
            });
        }
        Err(e) => {
            crate::dlog::log(&format!("update: check failed ({:?})", e.kind));
            set(|s| {
                if !busy_with_install {
                    s.state = "error";
                    s.error = e.msg;
                }
            });
        }
    }
    save_persisted(app, &p);
    BUSY.store(false, Ordering::SeqCst);
    publish(app);
    found
}

/// Tell the user once per version.
fn announce(app: &AppHandle, rel: &Release) {
    let mut p = load_persisted(app);
    if p.notified == rel.version.to_string() {
        return;
    }
    p.notified = rel.version.to_string();
    save_persisted(app, &p);
    crate::notify::show_update(
        app,
        &trf("whatRust {0} está disponível", &[&rel.version]),
        &tr("Abra Configurações → Atualizações para ver as novidades e instalar."),
    );
}

/// "Check now": runs on its own thread.
pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(rel) = check(&app) {
            announce(&app, &rel);
        }
    });
}

// ---- installing ---------------------------------------------------------------------------------

fn quit(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    crate::menubar::restore_all(app);
    app.exit(0);
}

fn fail(app: &AppHandle, e: updater::UErr) {
    crate::dlog::log(&format!("update: install failed ({:?})", e.kind));
    set(|s| {
        s.state = if e.kind == Kind::Cancelled {
            "available"
        } else {
            "error"
        };
        s.error = e.msg;
        s.staged = None;
    });
    BUSY.store(false, Ordering::SeqCst);
    publish(app);
}

fn finish(app: &AppHandle, rel: &Release, outcome: Outcome, auto: bool) {
    crate::dlog::log(&format!("update: installed {}", rel.version));
    match outcome {
        Outcome::Restart(r) => {
            set(|s| {
                s.state = "ready";
                s.restart = Some(r);
                s.staged = None;
            });
            if auto {
                crate::notify::show_update(
                    app,
                    &trf("whatRust {0} foi instalado", &[&rel.version]),
                    &tr("Reinicie o whatRust para usar a nova versão (Configurações → Atualizações)."),
                );
            }
        }
        Outcome::Handoff => {
            set(|s| {
                s.state = "handoff";
                s.staged = None;
            });
            publish(app);
            BUSY.store(false, Ordering::SeqCst);
            // Let the installer take over: it closes whatRust itself if we are still
            // running, then starts the new version.
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                quit(&app);
            });
            return;
        }
        Outcome::OpenFile(path) => {
            set(|s| {
                s.state = "opened";
                s.opened = path.display().to_string();
                s.staged = None;
            });
            crate::opener::open_file(app, path);
        }
        Outcome::Manual { path, message } => {
            set(|s| {
                s.state = "opened";
                s.opened = path.display().to_string();
                s.note = message;
                s.staged = None;
            });
            crate::opener::reveal_file(app, path);
        }
    }
    BUSY.store(false, Ordering::SeqCst);
    publish(app);
}

/// Download, verify and (unless `auto` on a type that restarts the app) apply
/// the release found by the last check.
fn run_install(app: &AppHandle, auto: bool) {
    let Some(rel) = crate::lock::guard(status()).latest.clone() else {
        return;
    };
    if rel.version <= updater::current_version() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    set(|s| {
        s.state = "downloading";
        s.received = 0;
        s.total = 0;
        s.error.clear();
        s.note.clear();
        s.opened.clear();
    });
    publish(app);
    let m = method();
    let prepared = updater::Net::from_settings(&crate::settings::load(app)).and_then(|net| {
        let downloads = downloads_dir(app);
        let plan = updater::plan(m, &rel, updater::Arch::current(), &downloads)?;
        let mut last = std::time::Instant::now();
        let handle = app.clone();
        let mut progress = |got: u64, total: u64| {
            set(|s| {
                s.received = got;
                s.total = total;
            });
            if last.elapsed() > Duration::from_millis(200) {
                last = std::time::Instant::now();
                publish(&handle);
            }
        };
        updater::fetch(&plan, &rel, &net, &mut progress)
    });
    let staged = match prepared {
        Ok(s) => s,
        Err(e) => return fail(app, e),
    };
    // A Windows installer restarts the app, so unattended runs stop after the
    // verified download and wait for the click.
    if auto && matches!(m, Method::WindowsNsis) {
        set(|s| {
            s.state = "staged";
            s.staged = Some(staged);
        });
        BUSY.store(false, Ordering::SeqCst);
        publish(app);
        crate::notify::show_update(
            app,
            &trf("whatRust {0} está pronto para instalar", &[&rel.version]),
            &tr("Abra Configurações → Atualizações e clique em Reiniciar e instalar."),
        );
        return;
    }
    apply_staged(app, &rel, staged, auto);
}

fn apply_staged(app: &AppHandle, rel: &Release, staged: Staged, auto: bool) {
    // `Env::current().exe` survives a package upgrade replacing the file (" (deleted)" is stripped).
    let ctx = updater::ApplyCtx::current(downloads_dir(app), updater::Env::current().exe);
    set(|s| s.state = "installing");
    publish(app);
    match updater::apply(method(), &staged, &ctx) {
        Ok(o) => finish(app, rel, o, auto),
        Err(e) => fail(app, e),
    }
}

/// "Download and install" (or, with a verified installer waiting, "Restart and install").
pub fn install_now(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let waiting = {
            let mut s = crate::lock::guard(status());
            if s.state == "staged" {
                s.staged.take().zip(s.latest.clone())
            } else {
                None
            }
        };
        match waiting {
            Some((staged, rel)) if !BUSY.swap(true, Ordering::SeqCst) => {
                apply_staged(&app, &rel, staged, false)
            }
            Some(_) => {}
            None => run_install(&app, false),
        }
    });
}

/// Restart into the version that was replaced in place.
pub fn restart(app: &AppHandle) -> Result<(), String> {
    let plan = crate::lock::guard(status()).restart.clone();
    let Some(plan) = plan else {
        return Err(tr("Não há atualização instalada para reiniciar.").into_owned());
    };
    if !updater::relaunch(&plan) {
        return Err(tr("Não foi possível reiniciar o whatRust. Abra-o manualmente.").into_owned());
    }
    quit(app);
    Ok(())
}

// ---- opening the page -----------------------------------------------------------------------------

/// Settings → Updates (tray item, notification click).
pub fn open_updates(app: &AppHandle) {
    crate::window::open_settings_window(app);
    let app = app.clone();
    std::thread::spawn(move || {
        // The page may still be loading; clicking the nav link is idempotent.
        for ms in [300u64, 700, 1200, 2000, 3500] {
            std::thread::sleep(Duration::from_millis(ms));
            if let Some(v) = app.get_webview("settings") {
                let _ = v.eval(
                    "(function(){var a=document.querySelector('a[href=\"#sec-atualizacoes\"]');if(a)a.click();})()",
                );
            }
        }
    });
}

/// Release notes are opened from a link we vouch for.
pub fn release_page_url(app: &AppHandle) -> Option<tauri::Url> {
    let url = crate::lock::guard(status())
        .latest
        .as_ref()
        .map(|r| r.url.clone())
        .filter(|u| u.starts_with(&format!("https://github.com/{}/", updater::REPO)))
        .unwrap_or_else(|| format!("https://github.com/{}/releases", updater::REPO));
    let _ = app;
    tauri::Url::parse(&url).ok()
}

// ---- background loop ------------------------------------------------------------------------------

/// Whether a background check is due: never checked this run, or six hours ago.
fn due(last: Option<std::time::Instant>) -> bool {
    last.is_none_or(|t| t.elapsed() >= CHECK_EVERY)
}

fn first_delay(feed_overridden: bool) -> Duration {
    if feed_overridden {
        FIRST_CHECK_DELAY_OVERRIDDEN
    } else {
        FIRST_CHECK_DELAY
    }
}

pub fn start(app: &AppHandle) {
    updater::clean_stale();
    method(); // decide while the executable is still the one we started from
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            std::thread::sleep(first_delay(
                std::env::var_os(updater::FEED_ENV).is_some_and(|v| !v.is_empty()),
            ));
            let mut last: Option<std::time::Instant> = None;
            loop {
                let s = crate::settings::load(&app);
                if s.auto_update_check && due(last) {
                    last = Some(std::time::Instant::now());
                    if let Some(rel) = check(&app) {
                        announce(&app, &rel);
                        if s.auto_update_install && method().auto_install_default() {
                            run_install(&app, true);
                        }
                    }
                }
                std::thread::sleep(TICK);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_background_check_is_due_at_first_and_every_six_hours() {
        assert!(due(None));
        assert!(!due(Some(std::time::Instant::now())));
        let long_ago = std::time::Instant::now().checked_sub(CHECK_EVERY + Duration::from_secs(1));
        if let Some(t) = long_ago {
            assert!(due(Some(t)));
        }
        assert_eq!(CHECK_EVERY, Duration::from_secs(21_600));
        assert_eq!(first_delay(false), Duration::from_secs(60));
        assert_eq!(first_delay(true), Duration::from_secs(3));
    }

    #[test]
    fn persisted_state_tolerates_missing_fields() {
        let p: Persisted = serde_json::from_str("{}").unwrap();
        assert_eq!(p.last_check_ms, 0);
        assert_eq!(p.notified, "");
    }
}
