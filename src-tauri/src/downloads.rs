//! Download feedback (issue #21): when a download finishes, the WhatsApp window
//! shows a small toast with the file name and **Open** / **Show in folder**
//! buttons — the browser pattern — or says that the download failed.
//!
//! The page never learns where a file was saved. Each finished download gets a
//! session-local id; the toast's buttons send that id back (`open_download` /
//! `reveal_download`), and only paths recorded here can be opened. A file type
//! that runs code when opened (installers, scripts, executables) is never
//! opened, only shown in its folder: the page could press "Open" by itself, and a
//! file someone sent in a chat must not be one call away from running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Downloads(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    next_id: u64,
    finished: HashMap<u64, PathBuf>,
    /// Destination chosen at request time, by URL. macOS reports no path when a
    /// download finishes, so the toast needs the one we picked.
    requested: HashMap<String, PathBuf>,
}

/// How many finished downloads stay openable from their toast.
const KEEP: usize = 200;

impl Downloads {
    pub fn requested(&self, url: &str, destination: &Path) {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if inner.requested.len() >= KEEP {
            inner.requested.clear();
        }
        inner
            .requested
            .insert(url.to_string(), destination.to_path_buf());
    }

    /// Record a finished download and return its id. `path` is what the engine
    /// reported; without it, the destination chosen at request time is used.
    pub fn finished(&self, url: &str, path: Option<PathBuf>) -> Option<(u64, PathBuf)> {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let requested = inner.requested.remove(url);
        let path = path.or(requested)?;
        inner.next_id += 1;
        let id = inner.next_id;
        if inner.finished.len() >= KEEP {
            if let Some(oldest) = inner.finished.keys().min().copied() {
                inner.finished.remove(&oldest);
            }
        }
        inner.finished.insert(id, path.clone());
        Some((id, path))
    }

    pub fn failed(&self, url: &str) {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.requested.remove(url);
    }

    pub fn path(&self, id: u64) -> Option<PathBuf> {
        let inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.finished.get(&id).cloned()
    }
}

/// File types that run code when "opened". These are shown in their folder
/// instead, where the user can still choose to run them.
pub fn is_risky(path: &Path) -> bool {
    const RISKY: &[&str] = &[
        // Windows
        "exe",
        "msi",
        "msix",
        "msixbundle",
        "appx",
        "appxbundle",
        "appinstaller",
        "bat",
        "cmd",
        "com",
        "cpl",
        "scr",
        "pif",
        "ps1",
        "psm1",
        "psd1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "hta",
        "lnk",
        "url",
        "reg",
        "dll",
        "sys",
        "inf",
        "msc",
        "msp",
        "scf",
        "gadget",
        "application",
        "settingcontent-ms",
        "iso",
        "img",
        "vhd",
        "vhdx",
        "jar",
        "jnlp",
        // Linux
        "sh",
        "bash",
        "zsh",
        "csh",
        "ksh",
        "run",
        "bin",
        "appimage",
        "desktop",
        "deb",
        "rpm",
        "flatpak",
        "flatpakref",
        "flatpakrepo",
        "snap",
        "so",
        "py",
        "pl",
        "rb",
        "elf",
        "out",
        // macOS
        "app",
        "command",
        "tool",
        "dmg",
        "pkg",
        "mpkg",
        "workflow",
        "terminal",
        "scpt",
        "applescript",
        "action",
        "dylib",
        "webloc",
        "inetloc",
        "fileloc",
    ];
    // No extension: on Unix that can be an executable (or a script with a shebang).
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return !cfg!(windows);
    };
    let ext = ext.to_ascii_lowercase();
    RISKY.contains(&ext.as_str())
}

/// The page call that shows the toast. Only the file *name* goes to the page.
pub fn toast_js(done: Option<(u64, &Path)>) -> String {
    let payload = match done {
        Some((id, path)) => serde_json::json!({
            "ok": true,
            "id": id,
            "name": path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "download".into()),
            "canOpen": !is_risky(path),
        }),
        None => serde_json::json!({ "ok": false }),
    };
    format!("window.__whatrustDownloadDone&&window.__whatrustDownloadDone({payload});")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_downloads_are_found_by_id_and_nothing_else() {
        let d = Downloads::default();
        let (a, pa) = d.finished("u1", Some("/dl/a.pdf".into())).unwrap();
        let (b, _) = d.finished("u2", Some("/dl/b.jpg".into())).unwrap();
        assert_ne!(a, b);
        assert_eq!(pa, PathBuf::from("/dl/a.pdf"));
        assert_eq!(d.path(a), Some("/dl/a.pdf".into()));
        assert_eq!(d.path(b), Some("/dl/b.jpg".into()));
        assert_eq!(d.path(999), None);
    }

    #[test]
    fn a_download_without_a_reported_path_uses_the_requested_destination() {
        let d = Downloads::default();
        d.requested("https://x/1", Path::new("/dl/video.mp4"));
        let (id, path) = d.finished("https://x/1", None).unwrap();
        assert_eq!(path, PathBuf::from("/dl/video.mp4"));
        assert_eq!(d.path(id), Some(path));
        // Unknown and failed downloads give nothing to open.
        assert!(d.finished("https://x/unknown", None).is_none());
        d.requested("https://x/2", Path::new("/dl/2.bin"));
        d.failed("https://x/2");
        assert!(d.finished("https://x/2", None).is_none());
    }

    #[test]
    fn the_record_is_bounded() {
        let d = Downloads::default();
        let (first, _) = d.finished("u", Some("/dl/0".into())).unwrap();
        for i in 1..=KEEP {
            d.finished("u", Some(format!("/dl/{i}").into())).unwrap();
        }
        assert_eq!(d.path(first), None, "the oldest entry is dropped");
        assert_eq!(d.0.lock().unwrap().finished.len(), KEEP);
    }

    #[test]
    fn programs_and_installers_are_never_opened() {
        for risky in [
            "setup.exe",
            "Invoice.PDF.exe",
            "tool.msi",
            "run.bat",
            "x.ps1",
            "a.js",
            "a.lnk",
            "app.AppImage",
            "install.sh",
            "pkg.deb",
            "x.desktop",
            "App.app",
            "Disk.dmg",
            "a.command",
            "a.jar",
        ] {
            assert!(is_risky(Path::new(risky)), "{risky}");
        }
        for fine in [
            "photo.jpg",
            "report.pdf",
            "song.mp3",
            "sheet.xlsx",
            "notes.txt",
            "a.zip",
        ] {
            assert!(!is_risky(Path::new(fine)), "{fine}");
        }
    }

    #[test]
    fn the_toast_gets_the_name_never_the_path() {
        let js = toast_js(Some((7, Path::new("/home/ada/Downloads/a \"b\".pdf"))));
        assert!(js.starts_with("window.__whatrustDownloadDone&&"));
        assert!(js.contains("\"id\":7"));
        assert!(js.contains(r#""name":"a \"b\".pdf""#), "{js}");
        assert!(js.contains("\"canOpen\":true"));
        assert!(!js.contains("/home/ada"), "{js}");
        assert!(toast_js(Some((8, Path::new("/dl/setup.exe")))).contains("\"canOpen\":false"));
        assert!(toast_js(None).contains("\"ok\":false"));
    }
}
