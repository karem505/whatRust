//! Hands links and downloaded files to the operating system: a web link opens in
//! the default browser (issue #30), a download opens in its default application
//! or is shown in its folder (issue #21).
//!
//! The webview never does this on its own. A link WhatsApp opens in a new tab
//! (`target="_blank"`, `window.open`) arrives as a new-window request, which the
//! system webview drops when nobody answers it — so clicking a link did nothing on
//! Windows and macOS. `window.rs` answers it and sends external links here.
//!
//! Each launch is judged by its result (pattern borrowed from ZapFast's opener):
//! a launcher that exits non-zero, or that is missing, moves on to the next way of
//! opening, and when nothing works the caller learns why. Launching blocks for up
//! to [`LAUNCH_GRACE`], so the public functions run on their own thread and report
//! failures as a toast instead of freezing the window.

use std::path::{Path, PathBuf};

/// Schemes we hand to the system. Everything else (javascript:, file:, data:,
/// blob:, custom app schemes a page might try to launch) is refused: a remote
/// page must never be able to start arbitrary local handlers through us.
pub fn is_openable_external(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto" | "tel")
}

/// Open a web link in the default browser, off the calling thread. Failures are
/// logged (scheme only — never the URL, which may carry chat content) and shown
/// as a toast so a dead click is never silent.
pub fn open_url(app: &tauri::AppHandle, url: tauri::Url) {
    if !is_openable_external(&url) {
        crate::dlog::log(&format!("opener: refused scheme {}", url.scheme()));
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let result = first_success(&attempts_url(url.as_str()));
        crate::dlog::log(&format!(
            "opener: open {} link: {}",
            url.scheme(),
            if result.is_ok() { "ok" } else { "FAILED" }
        ));
        if let Err(e) = result {
            crate::dlog::log(&format!("opener: {e}"));
            crate::notify::show(
                &app,
                "Couldn't open the link",
                "No browser could be started. Copy the link and open it yourself.",
            );
        }
    });
}

/// Open a file in its default application, falling back to showing it in its
/// folder. Off the calling thread; a failure becomes a toast.
pub fn open_file(app: &tauri::AppHandle, path: PathBuf) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut attempts = attempts_file(&path);
        attempts.extend(attempts_reveal(&path));
        report(&app, "open", first_success(&attempts));
    });
}

/// Show a file selected in the file manager (or at least its folder).
pub fn reveal_file(app: &tauri::AppHandle, path: PathBuf) {
    let app = app.clone();
    std::thread::spawn(move || {
        report(&app, "reveal", first_success(&attempts_reveal(&path)));
    });
}

fn report(app: &tauri::AppHandle, what: &str, result: Result<(), String>) {
    match result {
        Ok(()) => crate::dlog::log(&format!("opener: {what} file: ok")),
        Err(e) => {
            crate::dlog::log(&format!("opener: {what} file FAILED: {e}"));
            crate::notify::show(
                app,
                "Couldn't open the file",
                "Open it from your Downloads folder instead.",
            );
        }
    }
}

type Attempt<'a> = Box<dyn Fn() -> Result<(), String> + 'a>;

/// Runs the attempts in order and stops at the first that works; if none does,
/// the last error explains why.
fn first_success(attempts: &[Attempt<'_>]) -> Result<(), String> {
    let mut last = String::from("no way to open it was found");
    for attempt in attempts {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// How long a launcher gets to fail. One still running by then is taken to have
/// started the program: some launchers run it in the foreground.
#[cfg(not(windows))]
const LAUNCH_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Start a launcher and wait up to [`LAUNCH_GRACE`] for its verdict.
#[cfg(not(windows))]
fn launch(command: &mut std::process::Command) -> Result<(), String> {
    use std::process::Stdio;
    use std::time::Instant;
    let program = command.get_program().to_string_lossy().into_owned();
    #[cfg(target_os = "linux")]
    for (key, value) in host_env_overrides(std::env::vars_os()) {
        match value {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let deadline = Instant::now() + LAUNCH_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{program} failed ({status})")),
            Ok(None) if Instant::now() >= deadline => {
                // Still running: reap it whenever it ends so it leaves no zombie.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(format!("{program}: {e}")),
        }
    }
}

/// Environment fixes for programs we launch from an AppImage.
///
/// The AppImage's AppRun points `LD_LIBRARY_PATH`, `XDG_DATA_DIRS`, the GTK/GIO
/// module paths and friends into the mounted bundle and forces `GDK_BACKEND=x11`.
/// A browser started with that environment loads our bundled (older) libraries
/// and can crash or render nothing — the same family of bug as issue #22. So for
/// every variable that points into `$APPDIR`, drop those entries (removing the
/// variable when nothing is left), and let the host pick its own GDK backend.
/// Returns `(name, Some(new value) | None = remove)`; empty outside an AppImage.
#[cfg(target_os = "linux")]
fn host_env_overrides<I>(vars: I) -> Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>
where
    I: IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
{
    use std::ffi::OsString;
    let vars: Vec<(OsString, OsString)> = vars.into_iter().collect();
    let appdir = vars
        .iter()
        .find(|(k, _)| k == "APPDIR")
        .map(|(_, v)| v.to_string_lossy().trim_end_matches('/').to_string())
        .filter(|d| !d.is_empty());
    let in_appimage = vars.iter().any(|(k, _)| k == "APPIMAGE");
    let Some(appdir) = appdir.filter(|_| in_appimage) else {
        return Vec::new();
    };
    let inside = |p: &str| p == appdir || p.starts_with(&format!("{appdir}/"));
    let mut out = Vec::new();
    for (key, value) in &vars {
        if key == "APPDIR" || key == "APPIMAGE" || key == "ARGV0" || key == "OWD" {
            continue;
        }
        if key == "GDK_BACKEND" {
            out.push((key.clone(), None));
            continue;
        }
        let value = value.to_string_lossy();
        if !value.contains(appdir.as_str()) {
            continue;
        }
        let kept: Vec<&str> = value
            .split(':')
            .filter(|p| !p.is_empty() && !inside(p))
            .collect();
        if kept.is_empty() {
            out.push((key.clone(), None));
        } else {
            out.push((key.clone(), Some(OsString::from(kept.join(":")))));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn attempts_url(url: &str) -> Vec<Attempt<'_>> {
    use std::process::Command;
    vec![
        // xdg-open is the portable entry point (inside Flatpak it forwards to the
        // OpenURI portal); gio covers systems without xdg-utils.
        Box::new(move || launch(Command::new("xdg-open").arg(url))),
        Box::new(move || launch(Command::new("gio").arg("open").arg(url))),
    ]
}

#[cfg(target_os = "linux")]
fn attempts_file(path: &Path) -> Vec<Attempt<'_>> {
    use std::process::Command;
    vec![
        Box::new(move || launch(Command::new("xdg-open").arg(path))),
        Box::new(move || launch(Command::new("gio").arg("open").arg(path))),
    ]
}

#[cfg(target_os = "linux")]
fn attempts_reveal(path: &Path) -> Vec<Attempt<'_>> {
    use std::process::Command;
    let mut attempts: Vec<Attempt<'_>> = vec![Box::new(move || show_items(path))];
    if let Some(folder) = path.parent() {
        attempts.push(Box::new(move || {
            launch(Command::new("xdg-open").arg(folder))
        }));
    }
    attempts
}

/// Ask the file manager to show the file selected in its folder.
#[cfg(target_os = "linux")]
fn show_items(path: &Path) -> Result<(), String> {
    let uri = file_uri(path);
    let connection = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    connection
        .call_method(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            Some("org.freedesktop.FileManager1"),
            "ShowItems",
            &(vec![uri.as_str()], ""),
        )
        .map(|_| ())
        .map_err(|e| format!("the file manager: {e}"))
}

/// A `file://` URI for an absolute path, percent-encoding every byte outside the
/// characters a path segment may carry as they are.
#[cfg(any(target_os = "linux", test))]
fn file_uri(path: &Path) -> String {
    use std::fmt::Write;
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(*byte));
            }
            _ => {
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    uri
}

#[cfg(target_os = "macos")]
fn attempts_url(url: &str) -> Vec<Attempt<'_>> {
    use std::process::Command;
    vec![Box::new(move || {
        launch(Command::new("/usr/bin/open").arg(url))
    })]
}

#[cfg(target_os = "macos")]
fn attempts_file(path: &Path) -> Vec<Attempt<'_>> {
    use std::process::Command;
    vec![Box::new(move || {
        launch(Command::new("/usr/bin/open").arg(path))
    })]
}

#[cfg(target_os = "macos")]
fn attempts_reveal(path: &Path) -> Vec<Attempt<'_>> {
    use std::process::Command;
    vec![Box::new(move || {
        launch(Command::new("/usr/bin/open").arg("-R").arg(path))
    })]
}

#[cfg(windows)]
fn attempts_url(url: &str) -> Vec<Attempt<'_>> {
    vec![Box::new(move || win::shell_open(std::ffi::OsStr::new(url)))]
}

#[cfg(windows)]
fn attempts_file(path: &Path) -> Vec<Attempt<'_>> {
    vec![Box::new(move || win::shell_open(path.as_os_str()))]
}

#[cfg(windows)]
fn attempts_reveal(path: &Path) -> Vec<Attempt<'_>> {
    let mut attempts: Vec<Attempt<'_>> = vec![Box::new(move || win::explorer_select(path))];
    if let Some(folder) = path.parent() {
        attempts.push(Box::new(move || win::shell_open(folder.as_os_str())));
    }
    attempts
}

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// `ShellExecuteW(open)`: the default browser for a URL, the default
    /// application for a file. Unlike `cmd /c start`, nothing in the target is
    /// interpreted by a shell (a `&` in a URL is just a character).
    pub fn shell_open(target: &OsStr) -> Result<(), String> {
        shell_execute(target, None)
    }

    /// Explorer with the file selected.
    pub fn explorer_select(path: &Path) -> Result<(), String> {
        let mut args = std::ffi::OsString::from("/select,\"");
        args.push(path.as_os_str());
        args.push("\"");
        shell_execute(OsStr::new("explorer.exe"), Some(&args))
    }

    fn shell_execute(file: &OsStr, params: Option<&OsStr>) -> Result<(), String> {
        let op = wide(OsStr::new("open"));
        let file = wide(file);
        let params = params.map(wide);
        // Shell extensions may need COM on the calling thread (this is a worker
        // thread of our own); harmless when it was already initialized.
        let com =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        let inst = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(file.as_ptr()),
                params
                    .as_ref()
                    .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if com.is_ok() {
            unsafe { CoUninitialize() };
        }
        // ShellExecute returns a value greater than 32 on success.
        let code = inst.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!("ShellExecuteW failed ({code})"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn only_web_mail_and_phone_links_leave_the_app() {
        for ok in [
            "https://github.com",
            "http://example.com/a?b=c",
            "mailto:a@b.c",
            "tel:+123",
        ] {
            assert!(is_openable_external(&ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "blob:https://web.whatsapp.com/1234",
            "data:text/html,hi",
            "ms-settings:privacy",
            "smb://host/share",
        ] {
            assert!(!is_openable_external(&bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn the_first_attempt_that_works_ends_the_chain() {
        let tried = Cell::new(0);
        let attempts: Vec<Attempt<'_>> = vec![
            Box::new(|| {
                tried.set(tried.get() + 1);
                Err("no handler".into())
            }),
            Box::new(|| {
                tried.set(tried.get() + 1);
                Ok(())
            }),
            Box::new(|| {
                tried.set(tried.get() + 1);
                Ok(())
            }),
        ];
        assert_eq!(first_success(&attempts), Ok(()));
        assert_eq!(tried.get(), 2, "the fallback ran and nothing after it");
    }

    #[test]
    fn when_nothing_works_the_last_reason_is_reported() {
        let attempts: Vec<Attempt<'_>> = vec![
            Box::new(|| Err("xdg-open failed".into())),
            Box::new(|| Err("no file manager".into())),
        ];
        assert_eq!(first_success(&attempts), Err("no file manager".into()));
        assert!(first_success(&[]).is_err());
    }

    #[test]
    fn file_uris_escape_what_a_path_segment_cannot_carry() {
        assert_eq!(
            file_uri(Path::new("/home/ada/Downloads/photo.jpg")),
            "file:///home/ada/Downloads/photo.jpg"
        );
        assert_eq!(
            file_uri(Path::new("/home/José Ω/a#b%c.pdf")),
            "file:///home/Jos%C3%A9%20%CE%A9/a%23b%25c.pdf"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_launcher_is_judged_by_its_exit() {
        use std::process::Command;
        assert_eq!(launch(Command::new("sh").args(["-c", "exit 0"])), Ok(()));
        let failed = launch(Command::new("sh").args(["-c", "exit 4"])).unwrap_err();
        assert!(failed.starts_with("sh failed"), "{failed}");
        let missing = launch(&mut Command::new("whatrust-no-such-launcher")).unwrap_err();
        assert!(
            missing.starts_with("whatrust-no-such-launcher: "),
            "{missing}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_paths_are_scrubbed_from_launched_programs() {
        use std::ffi::OsString;
        let env = |pairs: &[(&str, &str)]| -> Vec<(OsString, OsString)> {
            pairs
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v)))
                .collect()
        };
        // Outside an AppImage nothing changes.
        assert!(host_env_overrides(env(&[("LD_LIBRARY_PATH", "/opt/lib")])).is_empty());

        let mut out = host_env_overrides(env(&[
            ("APPIMAGE", "/home/a/whatRust.AppImage"),
            ("APPDIR", "/tmp/.mount_whatRx/"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_whatRx/usr/lib"),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_whatRx/usr/share:/usr/local/share:/usr/share",
            ),
            ("GDK_BACKEND", "x11"),
            ("HOME", "/home/a"),
            ("PATH", "/usr/bin:/bin"),
        ]));
        out.sort();
        assert_eq!(
            out,
            vec![
                (OsString::from("GDK_BACKEND"), None),
                (OsString::from("LD_LIBRARY_PATH"), None),
                (
                    OsString::from("XDG_DATA_DIRS"),
                    Some(OsString::from("/usr/local/share:/usr/share"))
                ),
            ]
        );
    }
}
