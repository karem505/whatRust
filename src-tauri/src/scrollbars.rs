//! macOS: force overlay (auto-fading) scrollers process-wide.
//!
//! WKWebView does not paint scrollbars from web CSS: its page scrollers are
//! native AppKit `NSScroller`s, so `::-webkit-scrollbar` rules injected into
//! WhatsApp Web are ignored there. Instead `NSScroller.preferredScrollerStyle`
//! is resolved from the `AppleShowScrollBars` preference: with the system set
//! to "Always" (or "Automatic" with a mouse attached) macOS draws persistent,
//! legacy scrollers — the thick white bar down the side of both WhatsApp
//! panes. Writing `WhenScrolling` into *this app's own* defaults domain
//! (standardUserDefaults persists to the application domain, not the global
//! one) makes every scroller in the process — including WKWebView page
//! scrollers — the thin, auto-fading overlay style, while the user's
//! system-wide preference stays untouched for every other app.
//!
//! This must run before any window/webview exists: AppKit resolves and caches
//! `preferredScrollerStyle` lazily, and a local defaults write does not post
//! the distributed settings-changed notification that would make it re-resolve.
//! Called at the very top of `run()` for that reason.
//!
//! Best-effort only: a failure to read/write defaults must never stop launch.

/// The preference `NSScroller` resolves its style from, and the value that
/// selects overlay scrollers.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const SCROLL_BARS_KEY: &str = "AppleShowScrollBars";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const OVERLAY_VALUE: &str = "WhenScrolling";

/// Does the resolved (merged-domain) preference already give overlay
/// scrollers? Anything else — "Always", "Automatic", or unset — can leave
/// WKWebView with a persistent legacy scroller, so the app-domain override
/// is written.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn overlay_needed(current: Option<&str>) -> bool {
    current != Some(OVERLAY_VALUE)
}

/// Force overlay scrollers for this process (macOS only). Returns whether
/// the override was written (false = overlay was already selected, or not
/// macOS). The caller logs the outcome after `dlog::init()`, because this
/// runs before the diagnostic log is (re)created and any line written here
/// would be truncated away.
#[cfg(target_os = "macos")]
pub fn force_overlay_scrollers() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};

    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(SCROLL_BARS_KEY);
    let current = defaults.stringForKey(&key).map(|s| s.to_string());
    if !overlay_needed(current.as_deref()) {
        return false;
    }
    // SAFETY: key/value are valid for the call; NSUserDefaults copies them.
    unsafe {
        defaults.setObject_forKey(Some(&NSString::from_str(OVERLAY_VALUE)), &key);
    }
    true
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub fn force_overlay_scrollers() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::overlay_needed;

    #[test]
    fn only_when_scrolling_needs_no_override() {
        assert!(!overlay_needed(Some("WhenScrolling")));
    }

    #[test]
    fn always_automatic_and_unset_all_get_the_override() {
        assert!(overlay_needed(Some("Always")));
        assert!(overlay_needed(Some("Automatic")));
        assert!(overlay_needed(None));
        assert!(overlay_needed(Some("something-else")));
    }
}
