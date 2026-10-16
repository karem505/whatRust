//! Windows-only: register the app's AppUserModelID (AUMID) at runtime so WinRT
//! toast notifications actually render for the installed app (issue #3).
//!
//! Every toast `notify.rs` raises carries the bundle identifier
//! (`com.karem.whatrust`) as its `System.AppUserModel.ID`.
//! Windows only renders a toast whose AUMID is *registered* on the system. The
//! NSIS/MSI installers do tag their Start-Menu shortcut with the AUMID, but
//! that registration is fragile: the Desktop shortcut carries no AUMID, a
//! per-user vs per-machine path mismatch or a regenerated shortcut can drop the
//! property, and a raw-exe run has none at all. When the AUMID is unregistered
//! the WinRT call fails *silently*, so no notification ever appears.
//!
//! Registering the AUMID under HKCU on every launch makes toast delivery
//! self-sufficient regardless of installer or launch path. Both steps below are
//! per-user (no admin), idempotent, and best-effort: a failure only means
//! toasts may not render, so we log and never panic or block startup.
//!
//! The AUMID is read from the live Tauri config `identifier`, i.e. the exact
//! value `notify.rs` passes to `Toast::new`, so the two can never drift apart.

use tauri::AppHandle;

/// No-op on every platform except Windows.
#[allow(unused_variables)]
pub fn register(app: &AppHandle) {
    #[cfg(windows)]
    {
        let config = app.config();
        let aumid = &config.identifier;
        let display = config.product_name.as_deref().unwrap_or("whatRust");
        let icon = app
            .path()
            .app_local_data_dir()
            .ok()
            .and_then(|dir| write_icon(&dir));
        win::register(aumid, display, icon.as_deref());
    }
}

/// The logo Windows shows in the toast header (issue #40). The registry's
/// `IconUri` must name an image file, so the bundled PNG is written next to the
/// app's data; it is rewritten only when its bytes differ.
#[cfg(windows)]
fn write_icon(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    const ICON: &[u8] = include_bytes!("../icons/128x128.png");
    let path = dir.join("notification-logo.png");
    if std::fs::read(&path).ok().as_deref() != Some(ICON) {
        let written = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, ICON));
        if let Err(e) = written {
            crate::dlog::log(&format!("aumid: could not write the toast logo: {e}"));
            return None;
        }
    }
    Some(path)
}

#[cfg(windows)]
mod win {
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    /// UTF-16, NUL-terminated — suitable for `PCWSTR` args and `REG_SZ` data.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn register(aumid: &str, display_name: &str, icon: Option<&Path>) {
        // Step 1 — the registry entry is what makes the Action Center render
        // the toast for an unpackaged desktop app.
        match write_registry(aumid, display_name, icon) {
            Ok(()) => crate::dlog::log(&format!(
                "aumid: HKCU\\Software\\Classes\\AppUserModelId\\{aumid} registered"
            )),
            Err(e) => crate::dlog::log(&format!("aumid: registry registration FAILED: {e:?}")),
        }
        // Step 2 — pin this process to the AUMID (taskbar grouping + toast
        // attribution). Harmless if it fails; we just log.
        let id = wide(aumid);
        match unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(id.as_ptr())) } {
            Ok(()) => crate::dlog::log(&format!(
                "aumid: SetCurrentProcessExplicitAppUserModelID({aumid}) ok"
            )),
            Err(e) => crate::dlog::log(&format!(
                "aumid: SetCurrentProcessExplicitAppUserModelID FAILED: {e:?}"
            )),
        }
    }

    /// Writes `HKCU\Software\Classes\AppUserModelId\<AUMID>` with a `DisplayName`
    /// and, when the logo was written, an `IconUri` (both REG_SZ). Idempotent —
    /// overwritten on every launch, which also self-heals a Start-Menu shortcut
    /// that lost its AUMID property.
    fn write_registry(
        aumid: &str,
        display_name: &str,
        icon: Option<&Path>,
    ) -> windows::core::Result<()> {
        let subkey = wide(&format!("Software\\Classes\\AppUserModelId\\{aumid}"));
        let mut hkey = HKEY(std::ptr::null_mut());
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut hkey,
                None,
            )
            .ok()?;
        }

        let mut set = set_sz(hkey, "DisplayName", display_name);
        if let (Ok(()), Some(icon)) = (&set, icon) {
            set = set_sz(hkey, "IconUri", &icon.to_string_lossy());
        }
        // Always close the key, then surface any set error.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        set
    }

    /// One REG_SZ value; its data is the NUL-terminated UTF-16 bytes of `value`.
    fn set_sz(hkey: HKEY, name: &str, value: &str) -> windows::core::Result<()> {
        let name = wide(name);
        let data = wide(value);
        let bytes = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(&data[..]))
        };
        unsafe { RegSetValueExW(hkey, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) }.ok()
    }
}
