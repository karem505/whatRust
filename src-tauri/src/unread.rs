use std::sync::atomic::{AtomicBool, Ordering};

/// Settings → "Show unread counts" (issue #100). Display code goes through
/// [`visible`], so with the setting off no count reaches the tray, the dock or
/// taskbar badge, the window title or the app's own account lists. The counts
/// themselves are still tracked; notifications do not depend on this flag.
static SHOW: AtomicBool = AtomicBool::new(true);

/// Store the setting. Returns true when the value changed.
pub fn set_show(show: bool) -> bool {
    SHOW.swap(show, Ordering::Relaxed) != show
}

/// The count to display for `count` unread chats: zero while counts are hidden.
pub fn visible(count: u32) -> u32 {
    if SHOW.load(Ordering::Relaxed) {
        count
    } else {
        0
    }
}

/// Parse the unread count from a WhatsApp Web document title like "(3) WhatsApp".
/// Returns 0 when there is no leading "(n)" group.
pub fn parse_unread(title: &str) -> u32 {
    let t = title.trim_start();
    let Some(rest) = t.strip_prefix('(') else {
        return 0;
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{parse_unread, set_show, visible};

    #[test]
    fn hidden_counts_display_as_zero() {
        // One test owns the global flag, so parallel tests cannot race on it.
        set_show(false);
        assert_eq!(visible(7), 0);
        assert!(set_show(true), "turning it back on is a change");
        assert_eq!(visible(7), 7);
        assert!(!set_show(true), "no change the second time");
    }

    #[test]
    fn no_parentheses_is_zero() {
        assert_eq!(parse_unread("WhatsApp"), 0);
    }

    #[test]
    fn simple_count() {
        assert_eq!(parse_unread("(3) WhatsApp"), 3);
    }

    #[test]
    fn leading_whitespace_ok() {
        assert_eq!(parse_unread("  (12) WhatsApp"), 12);
    }

    #[test]
    fn plus_suffix_takes_digits() {
        assert_eq!(parse_unread("(99+) WhatsApp"), 99);
    }

    #[test]
    fn zero_count() {
        assert_eq!(parse_unread("(0) WhatsApp"), 0);
    }

    #[test]
    fn malformed_is_zero() {
        assert_eq!(parse_unread("(abc) WhatsApp"), 0);
        assert_eq!(parse_unread(""), 0);
        assert_eq!(parse_unread("()"), 0);
    }
}
