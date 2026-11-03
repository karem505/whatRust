//! The unread count on the app's own icon, next to the tray's: the Dock badge
//! on macOS (issue #54) and a numbered overlay on the taskbar button on Windows
//! (issue #55). Linux keeps the count on the tray icon only.

use tauri::AppHandle;

/// Show `count` on the app icon; 0 removes the badge.
#[allow(unused_variables)]
pub fn set(app: &AppHandle, count: u32) {
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
