//! Windows 11: tint the window border with the shown account's colour, so with
//! several accounts it is clear at a glance which one is open. With a single
//! account the system border is kept. Older Windows ignores the attribute.

use crate::accounts;
use tauri::{AppHandle, Window};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};

/// Muted, distinct colours as COLORREF (0x00BBGGRR), one per account by order.
const PALETTE: [u32; 6] = [
    0x0061AA1D, // green  #1DAA61
    0x00F6823B, // blue   #3B82F6
    0x000B9EF5, // amber  #F59E0B
    0x00F755A8, // purple #A855F7
    0x00A6B814, // teal   #14B8A6
    0x004444EF, // red    #EF4444
];
/// DWMWA_COLOR_DEFAULT: hand the border back to the system.
const SYSTEM: u32 = 0xFFFF_FFFF;

/// Colour for the account at `position` (by order) among `count` accounts.
fn colour(position: Option<usize>, count: usize) -> u32 {
    match position {
        Some(i) if count > 1 => PALETTE[i % PALETTE.len()],
        _ => SYSTEM,
    }
}

pub fn apply(app: &AppHandle, window: &Window, label: &str) {
    let mut list = accounts::load(app).accounts;
    list.sort_by_key(|a| a.order);
    let shown = accounts::id_from_label(label);
    let position = list.iter().position(|a| Some(a.id.as_str()) == shown);
    let value = colour(position, list.len());
    let Ok(hwnd) = window.hwnd() else { return };
    // SAFETY: a valid top-level HWND and a u32 COLORREF of the size passed.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&value as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_account_keeps_the_system_border() {
        assert_eq!(colour(Some(0), 1), SYSTEM);
        assert_eq!(colour(None, 3), SYSTEM);
    }

    #[test]
    fn accounts_get_distinct_colours_and_wrap() {
        assert_eq!(colour(Some(0), 2), PALETTE[0]);
        assert_ne!(colour(Some(0), 2), colour(Some(1), 2));
        assert_eq!(colour(Some(6), 7), PALETTE[0]);
    }
}
