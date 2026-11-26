/// Parse the unread count from a WhatsApp Web document title like "(3) WhatsApp".
/// Returns 0 when there is no leading "(n)" group.
pub fn parse_unread(title: &str) -> u32 {
    // Right-to-left locales can put invisible direction marks before the "(".
    let t = title.trim_start_matches(|c: char| {
        c.is_whitespace()
            || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
    });
    let Some(rest) = t.strip_prefix('(') else {
        return 0;
    };
    // "1,234" or "1.234" style grouping: skip separators between digits.
    let mut digits = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if matches!(c, ',' | '.' | '\u{202f}' | '\u{a0}')
            && !digits.is_empty()
            && chars.peek().is_some_and(|n| n.is_ascii_digit())
        {
            continue;
        } else {
            break;
        }
    }
    digits.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::parse_unread;

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
    fn direction_marks_before_the_count_are_ignored() {
        assert_eq!(parse_unread("\u{200e}(4) WhatsApp"), 4);
        assert_eq!(parse_unread("\u{200f} \u{202b}(2) WhatsApp"), 2);
    }

    #[test]
    fn grouped_thousands_are_one_number() {
        assert_eq!(parse_unread("(1,234) WhatsApp"), 1234);
        assert_eq!(parse_unread("(1.234) WhatsApp"), 1234);
        assert_eq!(parse_unread("(5, x) WhatsApp"), 5);
    }

    #[test]
    fn cleared_title_is_zero() {
        assert_eq!(parse_unread("WhatsApp"), 0);
        assert_eq!(parse_unread("\u{200e}WhatsApp"), 0);
    }

    #[test]
    fn malformed_is_zero() {
        assert_eq!(parse_unread("(abc) WhatsApp"), 0);
        assert_eq!(parse_unread(""), 0);
        assert_eq!(parse_unread("()"), 0);
    }
}
