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

/// The icon of one account's own window (issue #54): the account's profile
/// photo when it has one, so its taskbar entry and Alt+Tab tile tell accounts
/// apart; otherwise the app icon. macOS has no per-window icons (the Dock icon
/// is app-wide), so there it is always the app icon.
pub fn for_account(app: &AppHandle, id: &str) -> Image<'static> {
    if cfg!(target_os = "macos") {
        return current(app);
    }
    crate::accountphoto::image(app, id).unwrap_or_else(|| current(app))
}

/// `window.set_icon`, plus the Windows taskbar icon (see [`set_taskbar_icon`]).
pub fn set_window_icon(window: &tauri::Window, image: &Image<'_>) {
    let _ = window.set_icon(image.clone());
    set_taskbar_icon(window, image);
}

/// Windows (issue #117): the taskbar button shows a window's ICON_BIG, but
/// tauri's `icon()` / `set_icon()` go to tao's `set_window_icon`, which sets
/// ICON_SMALL only. With no big icon and no class icon the taskbar falls back
/// to the shell's blank-window icon. Put the same image on as the big icon.
/// Call it after building a window and whenever its icon changes; elsewhere
/// it does nothing.
pub fn set_taskbar_icon(window: &tauri::Window, image: &Image<'_>) {
    #[cfg(windows)]
    {
        let (w, h) = (image.width(), image.height());
        let owned = Image::new_owned(image.rgba().to_vec(), w, h);
        let target = window.clone();
        let _ = window.run_on_main_thread(move || big_icon::set(&target, &owned));
    }
    #[cfg(not(windows))]
    let _ = (window, image);
}

/// The ICON_BIG half of [`set_taskbar_icon`].
#[cfg(windows)]
mod big_icon {
    use std::sync::Mutex;
    use tauri::image::Image;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIcon, DestroyIcon, SendMessageW, HICON, ICON_BIG, WM_SETICON,
    };

    /// The big icon we last gave each window (HWND, HICON), freed once
    /// replaced. A few entries: one per window that ever had an icon set.
    static OURS: Mutex<Vec<(isize, isize)>> = Mutex::new(Vec::new());

    /// Main thread only.
    pub fn set(window: &tauri::Window, image: &Image<'_>) {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let Some((and_mask, bgra)) =
            super::icon_planes(image.rgba(), image.width(), image.height())
        else {
            return;
        };
        // SAFETY: both buffers cover width x height pixels, laid out the way
        // tao builds its ICON_SMALL (32-bit BGRA plus a mask).
        let Ok(icon) = (unsafe {
            CreateIcon(
                None,
                image.width() as i32,
                image.height() as i32,
                1,
                32,
                and_mask.as_ptr(),
                bgra.as_ptr(),
            )
        }) else {
            return;
        };
        // SAFETY: a live top-level window of this process, on its own thread.
        unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
        let key = hwnd.0 as isize;
        let mut ours = OURS.lock().unwrap_or_else(|e| e.into_inner());
        match ours.iter_mut().find(|(h, _)| *h == key) {
            Some(slot) => {
                let old = std::mem::replace(&mut slot.1, icon.0 as isize);
                // SAFETY: an icon we created and the window no longer uses.
                unsafe {
                    let _ = DestroyIcon(HICON(old as _));
                }
            }
            None => ours.push((key, icon.0 as isize)),
        }
    }
}

/// RGBA pixels as the two planes `CreateIcon` takes for a 32-bit icon: the
/// AND mask (one byte per pixel, the inverted alpha, as tao does) and the
/// colour plane in BGRA order. `None` when the buffer does not match the size.
#[cfg_attr(not(windows), allow(dead_code))]
fn icon_planes(rgba: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, Vec<u8>)> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0 || rgba.len() != pixels.checked_mul(4)? {
        return None;
    }
    let mut bgra = rgba.to_vec();
    let mut and_mask = Vec::with_capacity(pixels);
    for px in bgra.as_chunks_mut::<4>().0 {
        and_mask.push(px[3].wrapping_sub(u8::MAX));
        px.swap(0, 2);
    }
    Some((and_mask, bgra))
}

/// Re-apply one account's window icon (after its photo changed). Covers the
/// per-account window and a window detached from the single window.
pub fn apply_account(app: &AppHandle, id: &str) {
    if cfg!(target_os = "macos") {
        return;
    }
    let icon = for_account(app, id);
    for label in [crate::accounts::window_label(id), format!("detached-{id}")] {
        if let Some(window) = app.get_window(&label) {
            set_window_icon(&window, &icon);
        }
    }
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
        set_window_icon(&window, &image);
    }
    // Accounts with a photo keep it on their own window.
    for account in crate::accounts::load(app).accounts {
        apply_account(app, &account.id);
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
    fn taskbar_icon_planes_are_bgra_with_an_inverted_alpha_mask() {
        // Two pixels: opaque red, transparent blue.
        let rgba = [255, 0, 0, 255, 0, 0, 255, 0];
        let (mask, bgra) = icon_planes(&rgba, 2, 1).unwrap();
        assert_eq!(bgra, [0, 0, 255, 255, 255, 0, 0, 0]);
        assert_eq!(mask, [0, 1]);
        // The bundled icon converts at its real size.
        let image = Image::from_bytes(DEFAULT_ICON).unwrap();
        let (mask, bgra) = icon_planes(image.rgba(), image.width(), image.height()).unwrap();
        assert_eq!(mask.len() as u32, image.width() * image.height());
        assert_eq!(bgra.len(), image.rgba().len());
    }

    #[test]
    fn taskbar_icon_planes_refuse_a_size_mismatch() {
        assert!(icon_planes(&[0; 8], 2, 2).is_none());
        assert!(icon_planes(&[], 0, 0).is_none());
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
