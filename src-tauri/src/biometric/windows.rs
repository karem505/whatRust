use super::Availability;
use tauri::Manager;
use windows::core::{factory, HSTRING};
// In windows-rs 0.61 the async operation types live in the windows-future crate,
// NOT windows::Foundation. `IAsyncOperation::get()` (blocking) is from there too.
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows_future::IAsyncOperation;
// IUserConsentVerifierInterop lives in Win32::System::WinRT, NOT Security::Credentials::UI:
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;

pub fn availability() -> Availability {
    match UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.get()) {
        Ok(UserConsentVerifierAvailability::Available) => Availability::Available,
        Ok(_) => Availability::NotConfigured,
        Err(_) => Availability::Unsupported,
    }
}

pub fn authenticate(app: &tauri::AppHandle, reason: &str) -> Result<bool, String> {
    // MUST query availability first, or RequestVerification hangs.
    let _ = UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.get());

    let win = app
        .get_window("lock")
        .or_else(|| app.windows().into_values().next())
        .ok_or("no window to host the Hello prompt")?;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;

    let interop: IUserConsentVerifierInterop =
        factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(|e| e.to_string())?;
    let msg = HSTRING::from(reason);
    let op: IAsyncOperation<UserConsentVerificationResult> =
        unsafe { interop.RequestVerificationForWindowAsync(hwnd, &msg) }
            .map_err(|e| e.to_string())?;
    match op.get() {
        Ok(UserConsentVerificationResult::Verified) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
