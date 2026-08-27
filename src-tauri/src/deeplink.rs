//! Parsing for `whatsapp://` deep links (issue #23).
//!
//! Desktop entry points (the `xdg` open path on Linux, the single-instance
//! callback when the app is already running, and OS URL handlers on
//! Windows/macOS) hand us the full URL string. Everything here is pure so the
//! shape grammar is unit-tested; the platform glue lives in `lib.rs` and the
//! window layer.
//!
//! Supported shapes:
//!   whatsapp://send?phone=15551234567&text=hi
//!   whatsapp://send?text=hi            (no phone — share sheet, no chat target)
//!   whatsapp://chat/<jid>              (internal links; best effort)
//!   anything else                      -> parsed but unrecognised
//!
//! The returned phone is digits-only, normalized the way WhatsApp's wa.me links
//! expect (no `+`, no punctuation). A missing/empty phone is `None`, not an
//! error: `whatsapp://send?text=...` is a legitimate share link that opens the
//! app with a draft instead of a chat.

/// What a `whatsapp://` URL asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLink {
    /// `whatsapp://send?phone=...&text=...` — open a chat (optionally pre-filled).
    Send {
        /// E.164 digits only (country code included, no `+`). Absent for share links.
        phone: Option<String>,
        /// URL-decoded `text` query parameter, if any.
        text: Option<String>,
    },
    /// A `whatsapp://` URL we recognise as a WhatsApp link but don't implement.
    /// Opening the app (without navigating) is still the right response.
    Other,
}

/// True when this raw string is a `whatsapp://` URL (any casing of the scheme).
pub fn is_whatsapp_url(raw: &str) -> bool {
    // Byte-wise comparison: slicing a &str at a fixed offset can panic on a
    // multi-byte char boundary, but `eq_ignore_ascii_case` on bytes cannot.
    let b = raw.trim().as_bytes();
    let prefix = b"whatsapp://";
    b.len() >= prefix.len() && b[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// Split a query string into URL-decoded (key, value) pairs. '+' means space
/// (HTML form encoding, which wa.me links use). No allocation-heavy deps.
fn parse_query(query: &str) -> Vec<(String, String)> {
    fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' if i + 2 < bytes.len() + 1 && i + 2 < bytes.len() + 1 => {
                    let hex = bytes.get(i + 1..i + 3).and_then(|h| {
                        std::str::from_utf8(h)
                            .ok()
                            .and_then(|h| u8::from_str_radix(h, 16).ok())
                    });
                    match hex {
                        Some(b) => {
                            out.push(b);
                            i += 3;
                        }
                        None => {
                            out.push(b'%');
                            i += 1;
                        }
                    }
                }
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(pair), String::new()),
        })
        .collect()
}

/// Parse a raw `whatsapp://...` URL. Returns `None` for non-whatsapp URLs.
pub fn parse(raw: &str) -> Option<DeepLink> {
    let t = raw.trim();
    // Scheme matching is ASCII-case-insensitive; compare a short byte prefix so
    // a multi-byte first character can never panic on a non-char-boundary slice.
    let prefix_len = "whatsapp://".len();
    {
        let b = t.as_bytes();
        if b.len() < prefix_len || !b[..prefix_len].eq_ignore_ascii_case(b"whatsapp://") {
            return None;
        }
    }
    let rest = &t[prefix_len..];
    // Everything before the first '?' is the "host/path" part; xdg/WinRT may
    // hand us any letter-case for it.
    let (host, query) = match rest.split_once('?') {
        Some((h, q)) => (h, q),
        None => (rest, ""),
    };
    // whatsapp://send/... or whatsapp://send?... — the classic share/send intent.
    let first_seg = host
        .split(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_ascii_lowercase();
    if first_seg == "send" {
        let mut phone = None;
        let mut text = None;
        for (k, v) in parse_query(query) {
            match k.as_str() {
                "phone" | "to" | "abid" => {
                    // Keep digits only (country code included); WhatsApp Web's
                    // /send flow wants the bare international number.
                    let digits: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() {
                        phone = Some(digits);
                    }
                }
                "text" => {
                    if !v.is_empty() {
                        text = Some(v);
                    }
                }
                _ => {}
            }
        }
        return Some(DeepLink::Send { phone, text });
    }
    // Recognised scheme, unhandled action: still open the app.
    Some(DeepLink::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_whatsapp_urls_case_insensitively() {
        assert!(is_whatsapp_url("whatsapp://send?phone=123"));
        assert!(is_whatsapp_url("WHATSAPP://send"));
        assert!(is_whatsapp_url("  WhatsApp://send?text=hi  "));
        assert!(!is_whatsapp_url("https://web.whatsapp.com"));
        // Bare scheme with no target: recognised as a whatsapp URL (opens/focuses
        // the app) but parses to `Other`.
        assert!(is_whatsapp_url("whatsapp://"));
        assert!(!is_whatsapp_url(""));
    }

    #[test]
    fn parse_send_with_phone_and_text() {
        match parse("whatsapp://send?phone=+1 (555) 123-4567&text=Hello%20there") {
            Some(DeepLink::Send { phone, text }) => {
                assert_eq!(phone.as_deref(), Some("15551234567"));
                assert_eq!(text.as_deref(), Some("Hello there"));
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_send_text_only_is_valid_share_link() {
        match parse("whatsapp://send?text=check%20this") {
            Some(DeepLink::Send { phone, text }) => {
                assert_eq!(phone, None);
                assert_eq!(text.as_deref(), Some("check this"));
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_send_with_plus_encoding() {
        // Form-encoding: '+' is a space.
        match parse("whatsapp://send?text=a+b") {
            Some(DeepLink::Send { text, .. }) => assert_eq!(text.as_deref(), Some("a b")),
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_bare_send_opens_app_without_chat() {
        match parse("whatsapp://send") {
            Some(DeepLink::Send { phone, text }) => {
                assert_eq!(phone, None);
                assert_eq!(text, None);
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_host_case_insensitive_and_pathed() {
        assert!(matches!(
            parse("WHATSAPP://Send?phone=42"),
            Some(DeepLink::Send { .. })
        ));
        // Internal-style path links: recognised, opened as plain focus.
        assert!(matches!(
            parse("whatsapp://chat/12345678"),
            Some(DeepLink::Other)
        ));
    }

    #[test]
    fn parse_non_whatsapp_is_none() {
        assert!(parse("https://example.com").is_none());
        assert!(parse("whatsapp:send").is_none()); // missing '//'
        assert!(parse("").is_none());
    }

    #[test]
    fn malformed_percent_escape_does_not_panic() {
        match parse("whatsapp://send?text=100%") {
            Some(DeepLink::Send { text, .. }) => assert_eq!(text.as_deref(), Some("100%")),
            other => panic!("expected Send, got {other:?}"),
        }
    }
}
