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

/// Extensions that are opened directly. Anything else (programs, scripts,
/// macro-enabled Office files, HTML/SVG, shortcuts, disk images, unknown types)
/// is shown in its folder instead, where the user can still choose to run it.
/// An allowlist, so a type nobody thought of fails safe.
const SAFE_TO_OPEN: &[&str] = &[
    // documents
    "pdf", "txt", "md", "csv", "rtf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods",
    "odp", "vcf", // images
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "avif", "tif", "tiff",
    // audio
    "mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "flac", "amr", // video
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "3gp", // archives
    "zip", "7z", "rar", "gz", "tar",
];

/// Whether "Open" must be replaced by "Show in folder" for this file.
pub fn is_risky(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return true;
    };
    let ext = ext.to_ascii_lowercase();
    !SAFE_TO_OPEN.contains(&ext.as_str())
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

/// Check a folder offered as the download destination: it must exist, be a
/// directory and accept a new file. The probe file is removed again.
pub fn validate_dir(dir: &Path) -> Result<(), String> {
    use crate::i18n::tr;
    if !dir.is_dir() {
        return Err(tr("A pasta não existe.").into_owned());
    }
    let probe = dir.join(format!(".whatrust-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(_) => Err(tr("Não é possível gravar nessa pasta.").into_owned()),
    }
}

/// The configured download folder if it is still usable. If it has vanished or
/// stopped being writable the caller falls back to the default Downloads
/// folder; that is logged (no path: dlog keeps personal data out).
pub fn configured_dir(configured: Option<&str>) -> Option<PathBuf> {
    let dir = PathBuf::from(configured?);
    match validate_dir(&dir) {
        Ok(()) => Some(dir),
        Err(_) => {
            crate::dlog::log(
                "download: custom folder unusable, using the default Downloads folder",
            );
            None
        }
    }
}

/// Move a destination chosen by the engine (which names files against the
/// default folder) into `dir`, keeping the file name and the engine's
/// `name (1).ext` scheme so nothing already there is overwritten.
#[cfg(any(not(windows), test))]
pub fn place_in(dir: &Path, destination: &mut PathBuf) {
    let full = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let (base, ext) = full
        .split_once('.')
        .map(|(b, e)| (b.to_string(), format!(".{e}")))
        .unwrap_or((full.clone(), String::new()));
    let mut candidate = dir.join(&full);
    let mut counter = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{base} ({counter}){ext}"));
        counter += 1;
    }
    *destination = candidate;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("whatrust-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn validate_dir_accepts_a_writable_folder_and_leaves_nothing_behind() {
        let d = scratch("ok");
        assert!(validate_dir(&d).is_ok());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn validate_dir_rejects_missing_and_file_paths() {
        let d = scratch("bad");
        assert!(validate_dir(&d.join("nope")).is_err());
        let f = d.join("file.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(validate_dir(&f).is_err());
        assert_eq!(configured_dir(Some(f.to_str().unwrap())), None);
        assert_eq!(configured_dir(None), None);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn validate_dir_rejects_a_read_only_folder() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("ro");
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores permission bits; only assert when they are enforced.
        if std::fs::write(d.join("t"), b"").is_err() {
            assert!(validate_dir(&d).is_err());
        }
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn place_in_keeps_the_name_and_never_overwrites() {
        let d = scratch("place");
        let mut dest = PathBuf::from("/home/u/Downloads/report.final.pdf");
        place_in(&d, &mut dest);
        assert_eq!(dest, d.join("report.final.pdf"));
        std::fs::write(&dest, b"1").unwrap();
        let mut again = PathBuf::from("/home/u/Downloads/report.final.pdf");
        place_in(&d, &mut again);
        assert_eq!(again, d.join("report (1).final.pdf"));
        let mut bare = PathBuf::from("/x/notes");
        place_in(&d, &mut bare);
        assert_eq!(bare, d.join("notes"));
        std::fs::remove_dir_all(&d).unwrap();
    }

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
            "macro.docm",
            "macro.xlsm",
            "page.html",
            "image.svg",
            "help.chm",
            "noextension",
            "weird.zzz",
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
