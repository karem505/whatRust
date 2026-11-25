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
        let path = add_sniffed_extension(path);
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

/// Whether the name the engine picked is no name at all (issue #92): WebKitGTK
/// gives `blob:` downloads an empty suggested name, so wry ends up with the
/// download folder itself, or with `" (1)"` after its de-dup step replaced the
/// folder's own name one level up.
#[cfg(any(not(windows), test))]
fn is_unnamed(destination: &Path) -> bool {
    let Some(name) = destination.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    let t = name.trim();
    let counter_only = t
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    t.is_empty() || counter_only || destination.is_dir()
}

/// `yyyymmdd-hhmmss` (UTC) for a Unix time, for generated file names.
#[cfg(any(not(windows), test))]
pub fn timestamp_name(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Issue #92: if `destination` carries no usable file name, replace it with
/// `<download_dir>/whatsapp-<stamp>` (never overwriting an existing file). The
/// extension is added later from the file's content (`add_sniffed_extension`),
/// as the engine reports no MIME type here.
#[cfg(any(not(windows), test))]
pub fn name_unnamed(destination: &mut PathBuf, download_dir: &Path, stamp: &str) {
    if !is_unnamed(destination) {
        return;
    }
    let mut candidate = download_dir.join(format!("whatsapp-{stamp}"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = download_dir.join(format!("whatsapp-{stamp} ({counter})"));
        counter += 1;
    }
    *destination = candidate;
}

/// File extension for well-known content, from its first bytes.
pub fn sniff_extension(head: &[u8]) -> Option<&'static str> {
    let starts = |m: &[u8]| head.starts_with(m);
    if starts(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("gif")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("webp")
    } else if starts(b"%PDF-") {
        Some("pdf")
    } else if head.len() >= 12 && &head[4..8] == b"ftyp" {
        Some("mp4")
    } else if starts(b"OggS") {
        Some("ogg")
    } else if starts(b"ID3") {
        Some("mp3")
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("webm")
    } else {
        None
    }
}

/// Spellings of one file type's extension. A name that already ends in one of
/// them can get the MIME type's own subtype appended on its way through the
/// page or the engine: `photo.jpg.jpeg`, `song.mp3.mpeg` (issue #123).
const EXTENSION_ALIASES: &[&[&str]] = &[
    &["jpg", "jpeg", "jpe", "jfif"],
    &["mp3", "mpeg", "mpga"],
    &["mpg", "mpeg"],
    &["tif", "tiff"],
    &["htm", "html"],
    &["m4a", "mp4"],
    &["m4v", "mp4"],
    &["mov", "quicktime"],
    &["3gp", "3gpp"],
    &["oga", "ogg"],
    &["txt", "plain"],
];

/// `name` without a trailing extension that only repeats the one before it in
/// another spelling (`IMG_1.jpg.jpeg` -> `IMG_1.jpg`, `a.MP3.mpeg` ->
/// `a.MP3`). Anything else, including two different types (`notes.txt.pdf`) or
/// a name that is only extensions, is returned unchanged.
pub fn drop_alias_extension(name: &str) -> &str {
    let Some((rest, last)) = name.rsplit_once('.') else {
        return name;
    };
    let Some((stem, previous)) = rest.rsplit_once('.') else {
        return name;
    };
    if stem.trim().is_empty() || previous.is_empty() || last.is_empty() {
        return name;
    }
    let (previous, last) = (previous.to_ascii_lowercase(), last.to_ascii_lowercase());
    let same_type = previous != last
        && EXTENSION_ALIASES
            .iter()
            .any(|group| group.contains(&previous.as_str()) && group.contains(&last.as_str()));
    if same_type {
        rest
    } else {
        name
    }
}

/// Apply [`drop_alias_extension`] to the file name of a download destination,
/// without taking the name of a file already there.
#[cfg(any(not(windows), test))]
pub fn fix_alias_extension(destination: &mut PathBuf) {
    let Some(name) = destination.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let fixed = drop_alias_extension(name);
    if fixed.len() == name.len() {
        return;
    }
    let fixed = fixed.to_string();
    destination.set_file_name(fixed);
    if destination.exists() {
        if let Some(dir) = destination.parent().map(Path::to_path_buf) {
            place_in(&dir, destination);
        }
    }
}

/// A finished download saved without any extension (a `blob:` download) gets
/// the one its content implies. Anything unrecognised, or any rename that
/// would overwrite a file, leaves the path alone.
fn add_sniffed_extension(path: PathBuf) -> PathBuf {
    use std::io::Read;
    if path.extension().is_some() {
        return path;
    }
    let mut head = [0u8; 16];
    let n = match std::fs::File::open(&path).and_then(|mut f| f.read(&mut head)) {
        Ok(n) => n,
        Err(_) => return path,
    };
    let Some(ext) = sniff_extension(&head[..n]) else {
        return path;
    };
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{ext}"));
    let renamed = PathBuf::from(name);
    if renamed.exists() || std::fs::rename(&path, &renamed).is_err() {
        return path;
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_alias_extension_is_dropped() {
        assert_eq!(
            drop_alias_extension("IMG_20261124_164441.jpg.jpeg"),
            "IMG_20261124_164441.jpg"
        );
        assert_eq!(drop_alias_extension("Bodies.mp3.mpeg"), "Bodies.mp3");
        assert_eq!(drop_alias_extension("scan.TIF.tiff"), "scan.TIF");
        assert_eq!(drop_alias_extension("page.htm.HTML"), "page.htm");
        assert_eq!(drop_alias_extension("voice.m4a.mp4"), "voice.m4a");
        assert_eq!(drop_alias_extension("a.b.jpeg.jpg"), "a.b.jpeg");
    }

    #[test]
    fn other_names_are_left_alone() {
        for name in [
            "photo.jpg",
            "photo.jpeg",
            "photo",
            "notes.txt.pdf",
            "archive.tar.gz",
            "same.pdf.pdf",
            ".jpg.jpeg",
            "x..jpeg",
            "trailing.jpg.",
            "Report v1.2.mp3",
        ] {
            assert_eq!(drop_alias_extension(name), name, "{name}");
        }
    }

    #[test]
    fn a_destination_loses_only_the_repeated_extension() {
        let mut p = PathBuf::from("/tmp/dl/IMG_1.jpg.jpeg");
        fix_alias_extension(&mut p);
        assert_eq!(p, PathBuf::from("/tmp/dl/IMG_1.jpg"));
        let mut q = PathBuf::from("/tmp/dl/clip.mp4");
        fix_alias_extension(&mut q);
        assert_eq!(q, PathBuf::from("/tmp/dl/clip.mp4"));
        // Never the name of a file that is already there.
        let d = scratch("alias");
        std::fs::write(d.join("IMG_2.jpg"), b"x").unwrap();
        let mut r = d.join("IMG_2.jpg.jpeg");
        fix_alias_extension(&mut r);
        assert_eq!(r, d.join("IMG_2 (1).jpg"));
        let _ = std::fs::remove_dir_all(&d);
    }

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

    #[test]
    fn unnamed_destinations_get_a_generated_name_inside_the_folder() {
        let d = scratch("unnamed");
        // wry: push("") onto the folder, then set_file_name(" (1)").
        for bad in [d.clone(), d.parent().unwrap().join(" (1)"), d.join("(2)")] {
            let mut dest = bad;
            name_unnamed(&mut dest, &d, "20261120-061700");
            assert_eq!(dest, d.join("whatsapp-20261120-061700"));
        }
        std::fs::write(d.join("whatsapp-20261120-061700"), b"x").unwrap();
        let mut dest = d.clone();
        name_unnamed(&mut dest, &d, "20261120-061700");
        assert_eq!(dest, d.join("whatsapp-20261120-061700 (1)"));
        // A real name is untouched.
        let mut real = d.join("photo.jpg");
        name_unnamed(&mut real, &d, "x");
        assert_eq!(real, d.join("photo.jpg"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn timestamps_are_utc_civil_time() {
        assert_eq!(timestamp_name(0), "19700101-000000");
        assert_eq!(timestamp_name(1_700_000_000), "20231114-221320");
        assert_eq!(timestamp_name(951_782_400 + 86_399), "20000229-235959");
    }

    #[test]
    fn extension_follows_the_content() {
        assert_eq!(sniff_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff_extension(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(sniff_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff_extension(b"\0\0\0\x18ftypmp42"), Some("mp4"));
        assert_eq!(sniff_extension(b"hello"), None);
        assert_eq!(sniff_extension(b""), None);
        let d = scratch("sniff");
        let f = d.join("whatsapp-1");
        std::fs::write(&f, b"GIF89a....").unwrap();
        assert_eq!(add_sniffed_extension(f), d.join("whatsapp-1.gif"));
        let unknown = d.join("blob");
        std::fs::write(&unknown, b"plain").unwrap();
        assert_eq!(add_sniffed_extension(unknown.clone()), unknown);
        let named = d.join("a.bin");
        std::fs::write(&named, b"GIF89a").unwrap();
        assert_eq!(add_sniffed_extension(named.clone()), named);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
