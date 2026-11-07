//! A custom app icon (issue #54): one PNG for the whole app, chosen in Settings.
//!
//! The chosen file is validated and *copied* into the config dir, so the user's
//! original can move or vanish. The icon is applied at startup, when a window is
//! built, and at once when the setting changes:
//! - macOS: the Dock icon (`NSApplication.applicationIconImage`). The Dock
//!   unread badge (`badge.rs`) is the dock *tile's* badge label, drawn over
//!   whatever icon is shown, so both keep working together.
//! - Windows: the icon of every whatRust window, which is what the taskbar and
//!   Alt-Tab show. The taskbar overlay badge is a separate per-window overlay.
//! - Linux: the window icon. Wayland compositors ignore it and show the
//!   `.desktop` file's icon instead, so there only X11 sessions change.
//!
//! The tray / menu-bar icon is not touched. Per-account icons are out of scope.

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::{AppHandle, Manager};

const DEFAULT_ICON: &[u8] = include_bytes!("../icons/128x128.png");
const FILE_NAME: &str = "app-icon.png";
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_PIXELS: u32 = 1024;
pub const MIN_PIXELS: u32 = 32;

/// What was last applied: `Some(custom)`. Lets settings saves that do not touch
/// the icon (zoom, language…) skip re-decoding the PNG.
static APPLIED: Mutex<Option<bool>> = Mutex::new(None);

fn custom_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(FILE_NAME))
}

/// PNG width and height from the IHDR chunk, read before any decoding so a huge
/// image is refused without being inflated.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// Accept only a PNG that decodes, is square, and is of sane size.
pub fn validate(bytes: &[u8]) -> Result<(), String> {
    use crate::i18n::{tr, trf};
    if bytes.len() > MAX_BYTES {
        return Err(tr("A imagem tem mais de 5 MB.").into_owned());
    }
    let (w, h) =
        png_size(bytes).ok_or_else(|| tr("O arquivo não é uma imagem PNG.").into_owned())?;
    if w != h {
        return Err(tr("A imagem precisa ser quadrada.").into_owned());
    }
    if w > MAX_PIXELS {
        return Err(trf(
            "A imagem é grande demais (máximo {0} px).",
            &[&MAX_PIXELS],
        ));
    }
    if w < MIN_PIXELS {
        return Err(trf(
            "A imagem é pequena demais (mínimo {0} px).",
            &[&MIN_PIXELS],
        ));
    }
    Image::from_bytes(bytes).map_err(|_| tr("Não foi possível ler a imagem PNG.").into_owned())?;
    Ok(())
}

/// The stored custom PNG, if it is still there and valid.
fn custom_bytes(app: &AppHandle) -> Option<Vec<u8>> {
    let bytes = std::fs::read(custom_path(app)?).ok()?;
    match validate(&bytes) {
        Ok(()) => Some(bytes),
        Err(_) => {
            crate::dlog::log("app icon: stored custom icon is invalid, using the default");
            None
        }
    }
}

/// Bytes of the icon the settings ask for; the default when there is no usable
/// custom one.
fn wanted_bytes(app: &AppHandle, custom: bool) -> Vec<u8> {
    if custom {
        if let Some(b) = custom_bytes(app) {
            return b;
        }
    }
    DEFAULT_ICON.to_vec()
}

/// The icon a newly built window should carry.
pub fn current(app: &AppHandle) -> Image<'static> {
    let custom = crate::settings::load(app).custom_icon;
    Image::from_bytes(&wanted_bytes(app, custom))
        .or_else(|_| Image::from_bytes(DEFAULT_ICON))
        .expect("the bundled icon decodes")
}

/// Put the wanted icon on every window (and the Dock on macOS). `force` redoes
/// it even when the custom/default choice has not changed (a new PNG).
pub fn apply(app: &AppHandle, custom: bool, force: bool) {
    {
        let mut applied = APPLIED.lock().unwrap_or_else(|e| e.into_inner());
        if !force && *applied == Some(custom) {
            return;
        }
        *applied = Some(custom);
    }
    let bytes = wanted_bytes(app, custom);
    let Ok(image) = Image::from_bytes(&bytes) else {
        return;
    };
    for window in app.windows().into_values() {
        let _ = window.set_icon(image.clone());
    }
    #[cfg(target_os = "macos")]
    set_dock_icon(app, custom.then_some(bytes));
}

/// Dock icon from PNG bytes; `None` restores the bundle's own icon.
#[cfg(target_os = "macos")]
fn set_dock_icon(app: &AppHandle, png: Option<Vec<u8>>) {
    use objc2::runtime::{AnyClass, AnyObject};
    let _ = app.run_on_main_thread(move || unsafe {
        let ns_app: *mut AnyObject = objc2::msg_send![
            AnyClass::get(c"NSApplication").expect("NSApplication class"),
            sharedApplication
        ];
        let image: *mut AnyObject = match &png {
            Some(bytes) => {
                let data: *mut AnyObject = objc2::msg_send![
                    AnyClass::get(c"NSData").expect("NSData class"),
                    dataWithBytes: bytes.as_ptr().cast::<std::ffi::c_void>(),
                    length: bytes.len()
                ];
                let alloc: *mut AnyObject =
                    objc2::msg_send![AnyClass::get(c"NSImage").expect("NSImage class"), alloc];
                objc2::msg_send![alloc, initWithData: data]
            }
            None => std::ptr::null_mut(),
        };
        let _: () = objc2::msg_send![ns_app, setApplicationIconImage: image];
        if !image.is_null() {
            // The app keeps its own reference.
            let _: () = objc2::msg_send![image, release];
        }
    });
}

/// Store a validated PNG as the app icon and apply it.
pub fn set_custom(app: &AppHandle, bytes: &[u8]) -> Result<(), String> {
    validate(bytes)?;
    let path = custom_path(app).ok_or("no config dir")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("png.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete the stored copy.
pub fn clear_custom(app: &AppHandle) {
    if let Some(path) = custom_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b
    }

    #[test]
    fn the_bundled_icon_is_valid() {
        assert_eq!(validate(DEFAULT_ICON), Ok(()));
    }

    #[test]
    fn rejects_non_png_oversize_nonsquare_and_extreme_sizes() {
        assert!(validate(b"GIF89a....................").is_err());
        assert!(validate(b"").is_err());
        assert!(validate(&vec![0u8; MAX_BYTES + 1]).is_err());
        assert!(validate(&header(64, 32)).is_err(), "not square");
        assert!(validate(&header(2048, 2048)).is_err(), "too large");
        assert!(validate(&header(16, 16)).is_err(), "too small");
    }

    #[test]
    fn a_truncated_png_with_a_sane_header_does_not_decode() {
        assert!(validate(&header(256, 256)).is_err());
    }

    #[test]
    fn reads_the_size_from_the_header() {
        assert_eq!(png_size(&header(300, 200)), Some((300, 200)));
        assert_eq!(png_size(DEFAULT_ICON), Some((128, 128)));
    }
}
