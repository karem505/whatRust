//! The sender's picture on toasts for WebView2 `NotificationReceived` events.
//!
//! On Windows, WhatsApp's own `Notification` object is preferred over the
//! page-side shim (it keeps click/close semantics), so the toast is built from
//! the WebView2 event, which carries only title, body and an `IconUri`. A
//! `data:image/png` URI is read directly; a `blob:` or `https:` one cannot be
//! fetched from Rust, so bridge.js converts it (as on the page path) and sends
//! it with `notification_icon`. This broker pairs the two by (webview, title,
//! body) whichever comes first, and guarantees the toast is released exactly
//! once: by the matching picture, or by the deadline without one.
//!
//! Platform-independent so the pairing is unit-tested everywhere.

#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a toast waits for the page's picture (bridge.js ICON_WAIT_MS is 1.5 s).
pub const WAIT: Duration = Duration::from_millis(1600);
/// How long a picture that arrived before its event is kept.
const EARLY_KEEP: Duration = Duration::from_secs(5);
const MAX_PENDING: usize = 64;
const MAX_EARLY: usize = 32;
/// Largest base64 payload accepted (a 256 KiB PNG), matching notif_icon.
pub const MAX_ICON_B64: usize = 256 * 1024 * 4 / 3 + 8;
const PNG_DATA_URI: &str = "data:image/png;base64,";

/// What delivery needs once the picture question is settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub label: String,
    pub title: String,
    pub body: String,
    pub identity: String,
}

type Key = (String, String, String);

fn key(label: &str, title: &str, body: &str) -> Key {
    // Same limits `from_event` applies, so a long text still pairs.
    (
        label.to_string(),
        title.chars().take(256).collect(),
        body.chars().take(4096).collect(),
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum Arrival {
    /// Deliver now, with this picture if any.
    Ready(Held, Option<String>),
    /// Held until `icon_arrived` or `expire(token)`.
    Wait(u64),
}

#[derive(Default)]
struct Inner {
    next: u64,
    pending: Vec<(u64, Key, Held)>,
    early: Vec<(Key, Option<String>, Instant)>,
}

#[derive(Default)]
pub struct Broker {
    inner: Mutex<Inner>,
}

impl Broker {
    /// A native notification arrived and needs the page's picture.
    pub fn native_arrived(&self, held: Held, now: Instant) -> Arrival {
        let mut i = crate::lock::guard(&self.inner);
        i.early
            .retain(|(_, _, at)| now.saturating_duration_since(*at) <= EARLY_KEEP);
        let k = key(&held.label, &held.title, &held.body);
        if let Some(at) = i.early.iter().position(|(ek, _, _)| *ek == k) {
            let (_, icon, _) = i.early.remove(at);
            return Arrival::Ready(held, icon);
        }
        if i.pending.len() >= MAX_PENDING {
            return Arrival::Ready(held, None);
        }
        i.next += 1;
        let token = i.next;
        i.pending.push((token, k, held));
        Arrival::Wait(token)
    }

    /// The page's picture (`None`: it has none). Releases the oldest matching
    /// held notification, or is kept briefly for one that has not arrived yet.
    pub fn icon_arrived(
        &self,
        label: &str,
        title: &str,
        body: &str,
        icon: Option<String>,
        now: Instant,
    ) -> Option<(Held, Option<String>)> {
        let mut i = crate::lock::guard(&self.inner);
        let k = key(label, title, body);
        if let Some(at) = i.pending.iter().position(|(_, pk, _)| *pk == k) {
            let (_, _, held) = i.pending.remove(at);
            return Some((held, icon));
        }
        i.early
            .retain(|(_, _, at)| now.saturating_duration_since(*at) <= EARLY_KEEP);
        if i.early.len() >= MAX_EARLY {
            i.early.remove(0);
        }
        i.early.push((k, icon, now));
        None
    }

    /// Deadline reached: `Some` only if nothing released it first.
    pub fn expire(&self, token: u64) -> Option<Held> {
        let mut i = crate::lock::guard(&self.inner);
        let at = i.pending.iter().position(|(t, _, _)| *t == token)?;
        Some(i.pending.remove(at).2)
    }
}

/// What the event's `IconUri` tells us.
#[derive(Debug, PartialEq, Eq)]
pub enum IconUri {
    /// No picture at all: deliver now.
    None,
    /// A PNG data URI: its base64 payload (validated later by notif_icon).
    Png(String),
    /// Anything else (blob:, https:, other formats): ask the page.
    NeedsPage,
}

pub fn classify(uri: &str) -> IconUri {
    let uri = uri.trim();
    if uri.is_empty() {
        IconUri::None
    } else if let Some(payload) = uri.strip_prefix(PNG_DATA_URI) {
        if payload.len() <= MAX_ICON_B64 {
            IconUri::Png(payload.to_string())
        } else {
            IconUri::None
        }
    } else {
        IconUri::NeedsPage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(title: &str, body: &str, id: &str) -> Held {
        Held {
            label: "wa-a".into(),
            title: title.into(),
            body: body.into(),
            identity: id.into(),
        }
    }

    #[test]
    fn native_first_then_icon_releases_once_with_picture() {
        let b = Broker::default();
        let now = Instant::now();
        let Arrival::Wait(token) = b.native_arrived(held("Ann", "hi", "e1"), now) else {
            panic!("should wait");
        };
        let (h, icon) = b
            .icon_arrived("wa-a", "Ann", "hi", Some("PNG".into()), now)
            .expect("released");
        assert_eq!(h.identity, "e1");
        assert_eq!(icon.as_deref(), Some("PNG"));
        assert!(b.expire(token).is_none(), "deadline must not deliver again");
        assert!(
            b.icon_arrived("wa-a", "Ann", "hi", Some("PNG".into()), now)
                .is_none(),
            "a repeated picture finds nothing to release"
        );
    }

    #[test]
    fn icon_first_is_used_by_the_late_event() {
        let b = Broker::default();
        let now = Instant::now();
        assert!(b
            .icon_arrived("wa-a", "Ann", "hi", Some("PNG".into()), now)
            .is_none());
        assert_eq!(
            b.native_arrived(held("Ann", "hi", "e1"), now),
            Arrival::Ready(held("Ann", "hi", "e1"), Some("PNG".into()))
        );
        // Consumed: the next identical event waits again.
        assert!(matches!(
            b.native_arrived(held("Ann", "hi", "e2"), now),
            Arrival::Wait(_)
        ));
    }

    #[test]
    fn page_without_a_picture_releases_immediately_without_one() {
        let b = Broker::default();
        let now = Instant::now();
        b.native_arrived(held("Ann", "hi", "e1"), now);
        let (_, icon) = b.icon_arrived("wa-a", "Ann", "hi", None, now).unwrap();
        assert!(icon.is_none());
    }

    #[test]
    fn timeout_releases_without_picture_and_a_late_icon_is_ignored() {
        let b = Broker::default();
        let now = Instant::now();
        let Arrival::Wait(token) = b.native_arrived(held("Ann", "hi", "e1"), now) else {
            panic!()
        };
        assert_eq!(b.expire(token).unwrap().identity, "e1");
        assert!(b.expire(token).is_none(), "released once only");
        // The late picture is parked, never delivering anything by itself.
        assert!(b
            .icon_arrived("wa-a", "Ann", "hi", Some("PNG".into()), now)
            .is_none());
    }

    #[test]
    fn mismatched_title_body_or_webview_do_not_pair() {
        let b = Broker::default();
        let now = Instant::now();
        let Arrival::Wait(token) = b.native_arrived(held("Ann", "hi", "e1"), now) else {
            panic!()
        };
        assert!(b
            .icon_arrived("wa-a", "Bob", "hi", Some("X".into()), now)
            .is_none());
        assert!(b
            .icon_arrived("wa-a", "Ann", "yo", Some("X".into()), now)
            .is_none());
        assert!(b
            .icon_arrived("wa-b", "Ann", "hi", Some("X".into()), now)
            .is_none());
        assert_eq!(b.expire(token).unwrap().identity, "e1");
    }

    #[test]
    fn identical_messages_pair_in_order() {
        let b = Broker::default();
        let now = Instant::now();
        b.native_arrived(held("Ann", "hi", "e1"), now);
        b.native_arrived(held("Ann", "hi", "e2"), now);
        let (h1, _) = b.icon_arrived("wa-a", "Ann", "hi", None, now).unwrap();
        let (h2, _) = b.icon_arrived("wa-a", "Ann", "hi", None, now).unwrap();
        assert_eq!((h1.identity.as_str(), h2.identity.as_str()), ("e1", "e2"));
    }

    #[test]
    fn stale_early_icons_are_forgotten_and_bounds_hold() {
        let b = Broker::default();
        let now = Instant::now();
        b.icon_arrived("wa-a", "Ann", "hi", Some("OLD".into()), now);
        let later = now + EARLY_KEEP + Duration::from_secs(1);
        assert!(matches!(
            b.native_arrived(held("Ann", "hi", "e1"), later),
            Arrival::Wait(_)
        ));
        for n in 0..(MAX_PENDING + 5) {
            b.native_arrived(held(&format!("t{n}"), "b", "e"), later);
        }
        assert!(matches!(
            b.native_arrived(held("overflow", "b", "e"), later),
            Arrival::Ready(_, None)
        ));
        for n in 0..(MAX_EARLY * 2) {
            b.icon_arrived("wa-a", &format!("x{n}"), "b", None, later);
        }
        assert!(crate::lock::guard(&b.inner).early.len() <= MAX_EARLY);
    }

    #[test]
    fn long_texts_pair_on_the_truncated_form() {
        let b = Broker::default();
        let now = Instant::now();
        let long = "é".repeat(5000);
        b.native_arrived(held("T", &long, "e1"), now);
        assert!(b.icon_arrived("wa-a", "T", &long, None, now).is_some());
    }

    #[test]
    fn icon_uri_classification() {
        assert_eq!(classify(""), IconUri::None);
        assert_eq!(
            classify("data:image/png;base64,QUJD"),
            IconUri::Png("QUJD".into())
        );
        assert_eq!(
            classify("blob:https://web.whatsapp.com/x"),
            IconUri::NeedsPage
        );
        assert_eq!(
            classify("https://pps.whatsapp.net/a.jpg"),
            IconUri::NeedsPage
        );
        assert_eq!(classify("data:image/jpeg;base64,QUJD"), IconUri::NeedsPage);
        let big = format!("{PNG_DATA_URI}{}", "A".repeat(MAX_ICON_B64 + 1));
        assert_eq!(classify(&big), IconUri::None);
    }
}
