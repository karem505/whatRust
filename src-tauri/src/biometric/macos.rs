use super::Availability;
use block2::RcBlock;
use objc2_foundation::NSString;
use objc2_local_authentication::{LAContext, LAPolicy};
use std::sync::mpsc;

pub fn availability() -> Availability {
    let ctx = unsafe { LAContext::new() };
    let can =
        unsafe { ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics) };
    match can {
        Ok(()) => Availability::Available,
        Err(_) => Availability::NotConfigured,
    }
}

pub fn authenticate(_app: &tauri::AppHandle, reason: &str) -> Result<bool, String> {
    // Fresh context per call (LAContext caches a prior success).
    let ctx = unsafe { LAContext::new() };
    let fallback = crate::i18n::tr("Desbloquear o whatRust");
    let reason = if reason.is_empty() {
        fallback.as_ref()
    } else {
        reason
    };
    let ns_reason = NSString::from_str(reason);

    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(
        move |success: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {
            let _ = tx.send(success.as_bool());
        },
    );

    unsafe {
        // DeviceOwnerAuthentication = Touch ID OR the login password as fallback.
        ctx.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthentication,
            &ns_reason,
            &block,
        );
    }

    // The reply block fires on a background thread; block until it does (with a cap).
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .map_err(|_| crate::i18n::tr("O tempo para confirmar a biometria terminou.").into_owned())
}
