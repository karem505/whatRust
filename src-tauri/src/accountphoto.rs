//! Per-account profile photo (issue #80), shown in the settings account list.
//!
//! The settings page decodes the picked PNG/JPEG (the webview can read both,
//! so no image-decoding crate is needed here), crops it to a square, scales it
//! to 96x96 and sends the PNG bytes. This module re-validates them with the same
//! checks as the app icon and stores them as `account-photos/<id>.png` in the
//! config dir. Photos are removed with their account.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const DIR: &str = "account-photos";

fn path_for(app: &AppHandle, id: &str) -> Option<PathBuf> {
    // Account ids are `default` / `acct-N`; refuse anything that could leave the folder.
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join(DIR).join(format!("{id}.png")))
}

/// Validate and store the photo for an account.
pub fn set(app: &AppHandle, id: &str, png: &[u8]) -> Result<(), String> {
    crate::appicon::validate(png)?;
    let path = path_for(app, id).ok_or("invalid account id")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("png.tmp");
    std::fs::write(&tmp, png).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete the stored photo, if any.
pub fn clear(app: &AppHandle, id: &str) {
    if let Some(path) = path_for(app, id) {
        let _ = std::fs::remove_file(path);
    }
}

/// The photo as an image (the window icon of this account's own window, issue
/// #54), if there is a valid one.
pub fn image(app: &AppHandle, id: &str) -> Option<tauri::image::Image<'static>> {
    let bytes = std::fs::read(path_for(app, id)?).ok()?;
    crate::appicon::validate(&bytes).ok()?;
    tauri::image::Image::from_bytes(&bytes).ok()
}

/// The photo as a `data:` URL for the settings page, if there is a valid one.
pub fn data_url(app: &AppHandle, id: &str) -> Option<String> {
    let bytes = std::fs::read(path_for(app, id)?).ok()?;
    crate::appicon::validate(&bytes).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
