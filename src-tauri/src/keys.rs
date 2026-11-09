//! macOS keyboard fix (issue #73): arrow keys inserted tofu glyphs into the
//! WhatsApp composer instead of moving the caret.
//!
//! WKWebView hands a key it did not act on (an arrow with nowhere to go, Home,
//! End, ...) up the responder chain. Tao's content view (`TaoView`) answers
//! `keyDown:` by running `interpretKeyEvents:` on itself, and AppKit delivers
//! the resulting `insertText:` with the key's raw function-key character
//! (U+F700..U+F8FF, or a C0 control) to the focused text input, which is the
//! page's editor. Nothing in the app needs that path: `TaoView` is not a text
//! input here, the webview is, so it is switched off for that one class.

/// Make `TaoView` ignore `interpretKeyEvents:`. Idempotent and cheap: call it
/// after any tao window is built (the class is registered with the first
/// window); until the class exists it does nothing and retries next time.
#[cfg(target_os = "macos")]
pub fn install() {
    use objc2::ffi::class_addMethod;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use std::sync::atomic::{AtomicBool, Ordering};

    extern "C" fn ignore(_this: *mut AnyObject, _cmd: Sel, _events: *mut AnyObject) {}

    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.load(Ordering::Acquire) {
        return;
    }
    {
        let Some(class) = AnyClass::get(c"TaoView") else {
            crate::dlog::log("keys: TaoView class not found yet; key fix deferred");
            return;
        };
        DONE.store(true, Ordering::Release);
        // SAFETY: `ignore` matches the `v@:@` signature registered for it, and
        // adding a method to a class we do not own only overrides the
        // inherited `interpretKeyEvents:` for that class.
        let added = unsafe {
            let imp: Imp = std::mem::transmute::<
                extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
                Imp,
            >(ignore);
            class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                objc2::sel!(interpretKeyEvents:),
                imp,
                c"v@:@".as_ptr(),
            )
        };
        crate::dlog::log(if added.as_bool() {
            "keys: TaoView no longer interprets key events"
        } else {
            "keys: TaoView already overrides interpretKeyEvents:"
        });
    }
}
