//! The unread count on the app's own icon, next to the tray's: the Dock badge
//! on macOS (issue #54) and a numbered overlay on the taskbar button on Windows
//! (issue #55). On Linux the count goes out as a Unity `LauncherEntry` D-Bus
//! signal (see `launcher`), next to the tray's.

use tauri::AppHandle;

/// Show `count` on the app icon; 0 removes the badge.
#[allow(unused_variables)]
pub fn set(app: &AppHandle, count: u32) {
    #[cfg(target_os = "linux")]
    launcher::set(count);
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        // The Dock tile belongs to the app, so any of its windows can set it.
        if let Some(w) = app.windows().into_values().next() {
            let _ = w.set_badge_count((count > 0).then_some(i64::from(count)));
        }
    }
    #[cfg(windows)]
    {
        use tauri::Manager;
        let rgba = (count > 0).then(|| overlay_rgba(count));
        // Each taskbar button carries its own overlay: the main window and any
        // account detached into a window of its own.
        for (label, w) in app.windows() {
            if label == "main" || label.starts_with("detached-") {
                let icon = rgba
                    .as_deref()
                    .map(|px| tauri::image::Image::new(px, SIZE, SIZE));
                let _ = w.set_overlay_icon(icon);
            }
        }
    }
}

/// The Unity `com.canonical.Unity.LauncherEntry` API: a broadcast signal that
/// KDE Plasma's task manager, Dash to Dock / Ubuntu Dock, Plank, Latte and
/// others turn into a count on the app's launcher icon.
#[cfg(any(target_os = "linux", test))]
mod launcher {
    /// Desktop file id (with `.desktop`) that the dock matches the signal to.
    /// - Flatpak: `<FLATPAK_ID>.desktop`.
    /// - AppImage: `whatrust.desktop`, the entry `install.sh` writes.
    /// - Otherwise (.deb): `whatRust.desktop`, named after Tauri's productName.
    pub(super) fn desktop_id(flatpak_id: Option<&str>, appimage: bool) -> String {
        match (flatpak_id, appimage) {
            (Some(id), _) => format!("{id}.desktop"),
            (None, true) => "whatrust.desktop".to_string(),
            (None, false) => "whatRust.desktop".to_string(),
        }
    }

    /// Stable object path for an app uri (FNV-1a, as a decimal number).
    pub(super) fn object_path(app_uri: &str) -> String {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in app_uri.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("/com/canonical/unity/launcherentry/{h}")
    }

    #[cfg(target_os = "linux")]
    pub(super) use bus::set;

    #[cfg(target_os = "linux")]
    mod bus {
        use std::collections::HashMap;
        use std::sync::mpsc::{channel, Sender};
        use std::sync::{Mutex, OnceLock};
        use zbus::zvariant::Value;

        static TX: OnceLock<Mutex<Sender<u32>>> = OnceLock::new();

        /// Queue `count` for the worker thread; never blocks the caller.
        pub fn set(count: u32) {
            let tx = TX.get_or_init(|| {
                let (tx, rx) = channel::<u32>();
                let spawned = std::thread::Builder::new()
                    .name("launcher-badge".into())
                    .spawn(move || worker(rx));
                if let Err(e) = spawned {
                    eprintln!("launcher badge: cannot start worker: {e}");
                }
                Mutex::new(tx)
            });
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(count);
            }
        }

        fn worker(rx: std::sync::mpsc::Receiver<u32>) {
            let uri = format!(
                "application://{}",
                super::desktop_id(
                    crate::settings::flatpak_id().as_deref(),
                    std::env::var_os("APPIMAGE").is_some()
                )
            );
            let path = super::object_path(&uri);
            let mut conn: Option<zbus::blocking::Connection> = None;
            let mut warned = false;
            let mut last = None;
            while let Ok(mut count) = rx.recv() {
                // Only the latest count matters.
                while let Ok(c) = rx.try_recv() {
                    count = c;
                }
                if last == Some(count) {
                    continue;
                }
                let result = emit(&mut conn, &path, &uri, count);
                match result {
                    Ok(()) => last = Some(count),
                    Err(e) => {
                        conn = None; // reconnect next time
                        if !warned {
                            warned = true;
                            eprintln!("launcher badge: {e}");
                        }
                    }
                }
            }
        }

        fn emit(
            conn: &mut Option<zbus::blocking::Connection>,
            path: &str,
            uri: &str,
            count: u32,
        ) -> zbus::Result<()> {
            if conn.is_none() {
                *conn = Some(zbus::blocking::Connection::session()?);
            }
            let mut props: HashMap<&str, Value> = HashMap::new();
            props.insert("count", Value::from(i64::from(count)));
            props.insert("count-visible", Value::from(count > 0));
            conn.as_ref().unwrap().emit_signal(
                None::<&str>,
                path,
                "com.canonical.Unity.LauncherEntry",
                "Update",
                &(uri, props),
            )
        }

        #[cfg(test)]
        mod tests {
            /// Manual: `dbus-monitor --session "interface='com.canonical.Unity.LauncherEntry'"`
            /// in another terminal, then `cargo test emits_on_session_bus -- --ignored`.
            #[test]
            #[ignore]
            fn emits_on_session_bus() {
                for n in [3, 12, 0] {
                    super::set(n);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_id_per_install_type() {
            let id = "io.github.karem505.whatRust";
            assert_eq!(desktop_id(Some(id), false), format!("{id}.desktop"));
            assert_eq!(desktop_id(Some(id), true), format!("{id}.desktop"));
            assert_eq!(desktop_id(None, true), "whatrust.desktop");
            assert_eq!(desktop_id(None, false), "whatRust.desktop");
        }

        #[test]
        fn object_path_is_valid_and_stable() {
            let p = object_path("application://whatRust.desktop");
            assert_eq!(p, object_path("application://whatRust.desktop"));
            assert_ne!(p, object_path("application://whatrust.desktop"));
            let tail = p
                .strip_prefix("/com/canonical/unity/launcherentry/")
                .unwrap();
            assert!(!tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}

/// Overlay edge in pixels. Windows draws the overlay in a corner of the
/// taskbar button and scales it to the display.
#[cfg(any(windows, test))]
const SIZE: u32 = 32;

/// A red disc with the count in white, "99+" above 99.
#[cfg(any(windows, test))]
fn overlay_rgba(count: u32) -> Vec<u8> {
    let text = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let mut px = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mut put = |x: u32, y: u32, rgba: [u8; 4]| {
        let i = ((y * SIZE + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&rgba);
    };

    // The disc, with a one-pixel soft edge.
    let c = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let alpha = (c - d).clamp(0.0, 1.0);
            if alpha > 0.0 {
                put(x, y, [0xE5, 0x39, 0x35, (alpha * 255.0) as u8]);
            }
        }
    }

    // The digits, from a 3x5 pixel font scaled to fit.
    let n = text.len() as u32;
    let scale = match n {
        1 => 4,
        2 => 3,
        _ => 2,
    };
    let (gw, gh) = (3 * scale, 5 * scale);
    let x0 = (SIZE - (n * gw + (n - 1) * scale)) / 2;
    let y0 = (SIZE - gh) / 2;
    for (i, ch) in text.chars().enumerate() {
        let gx = x0 + i as u32 * (gw + scale);
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..3 {
                if bits >> (2 - col) & 1 == 1 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            put(
                                gx + col * scale + dx,
                                y0 + row as u32 * scale + dy,
                                [0xFF, 0xFF, 0xFF, 0xFF],
                            );
                        }
                    }
                }
            }
        }
    }
    px
}

/// Rows of a 3x5 glyph, the high bit of the three on the left.
#[cfg(any(windows, test))]
fn glyph(ch: char) -> [u8; 5] {
    match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => [0; 5],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(px: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        px[i..i + 4].try_into().unwrap()
    }

    fn white_pixels(px: &[u8]) -> usize {
        px.chunks(4)
            .filter(|p| *p == [0xFF, 0xFF, 0xFF, 0xFF])
            .count()
    }

    #[test]
    fn every_count_renders_inside_the_disc() {
        for count in [1, 7, 10, 42, 99, 100, 5000, u32::MAX] {
            let px = overlay_rgba(count);
            assert_eq!(px.len(), (SIZE * SIZE * 4) as usize);
            // Corners are outside the disc, so transparent.
            for (x, y) in [(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
                assert_eq!(pixel(&px, x, y)[3], 0, "count {count} corner ({x},{y})");
            }
            assert!(white_pixels(&px) > 0, "count {count} has no digits");
        }
    }

    #[test]
    fn large_counts_show_99_plus() {
        assert_eq!(overlay_rgba(100), overlay_rgba(12345));
        assert_ne!(overlay_rgba(99), overlay_rgba(100));
    }

    #[test]
    fn the_glyph_is_drawn_in_white_on_red() {
        // "1" at scale 4: its stem's centre column runs down the middle.
        let px = overlay_rgba(1);
        assert_eq!(pixel(&px, SIZE / 2, SIZE / 2), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(pixel(&px, 4, SIZE / 2), [0xE5, 0x39, 0x35, 0xFF]);
    }
}
