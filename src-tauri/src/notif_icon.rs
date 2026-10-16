//! The sender's picture on a message notification.
//!
//! bridge.js hands over WhatsApp's notification icon already scaled down and
//! re-encoded as a small PNG, base64 in the `notify` call. Native toasts take
//! an image by file path, so it is written to the app's cache directory under a
//! name derived from its bytes (the same picture is written once) and files
//! older than a day are removed again. Nothing about the picture is logged.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager};

/// Anything larger is not the 96px PNG bridge.js produces.
const MAX_BYTES: usize = 256 * 1024;
/// How long a picture stays on disk. Toasts kept in the notification centre
/// lose their picture after this, which is preferable to keeping avatars around.
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Decode and validate the picture, then store it for the toast. `None` when
/// anything is off; the toast is then shown without a picture.
pub fn store(app: &AppHandle, b64: &str) -> Option<PathBuf> {
    let png = decode_png(b64)?;
    let dir = app.path().app_cache_dir().ok()?.join("notification-icons");
    std::fs::create_dir_all(&dir).ok()?;
    prune(&dir, SystemTime::now());
    let path = dir.join(format!("{:016x}.png", fnv1a(&png)));
    if !path.exists() {
        std::fs::write(&path, &png).ok()?;
    }
    Some(path)
}

/// Base64 to bytes, accepted only if it is a PNG of a sensible size.
fn decode_png(b64: &str) -> Option<Vec<u8>> {
    if b64.len() > MAX_BYTES * 4 / 3 + 4 {
        return None;
    }
    let bytes = base64_decode(b64)?;
    (bytes.len() <= MAX_BYTES && bytes.starts_with(PNG_MAGIC)).then_some(bytes)
}

/// Remove pictures older than `KEEP`.
fn prune(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > KEEP);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Standard base64 (with or without padding). `None` on any other character.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let s = s.trim_end_matches('=').as_bytes();
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_padding_length() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg").unwrap(), b"foob");
        assert_eq!(base64_decode("+/+/").unwrap(), [0xfb, 0xff, 0xbf]);
        assert!(base64_decode("Zm9v!").is_none());
        assert!(base64_decode("Z").is_none());
    }

    #[test]
    fn only_a_png_of_sensible_size_is_accepted() {
        let png = [PNG_MAGIC, b"rest"].concat();
        let b64 = |b: &[u8]| {
            // Tiny encoder for the test only.
            const T: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in b.chunks(3) {
                let n = (c[0] as u32) << 16
                    | (*c.get(1).unwrap_or(&0) as u32) << 8
                    | *c.get(2).unwrap_or(&0) as u32;
                for i in 0..=c.len() {
                    s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                }
            }
            s
        };
        assert_eq!(decode_png(&b64(&png)).unwrap(), png);
        assert!(decode_png(&b64(b"\xff\xd8\xff a jpeg")).is_none());
        assert!(decode_png(&"A".repeat(MAX_BYTES * 2)).is_none());
    }

    #[test]
    fn the_same_picture_gets_the_same_name() {
        assert_eq!(fnv1a(b"avatar"), fnv1a(b"avatar"));
        assert_ne!(fnv1a(b"avatar"), fnv1a(b"avatars"));
    }

    #[test]
    fn pruning_removes_only_old_pictures() {
        let dir = std::env::temp_dir().join(format!("whatrust-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), b"x").unwrap();
        prune(&dir, SystemTime::now());
        assert!(dir.join("a.png").exists(), "a fresh picture stays");
        prune(&dir, SystemTime::now() + KEEP + Duration::from_secs(60));
        assert!(!dir.join("a.png").exists(), "a day-old picture is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
