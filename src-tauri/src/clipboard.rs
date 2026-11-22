//! Pasting files and images from the clipboard into WhatsApp on Linux.
//!
//! WebKitGTK hides both kinds of clipboard content from the page:
//!
//! - **An image** (a screenshot, "Copy image" in a viewer or browser): the
//!   paste event's `clipboardData` is empty, with no types and no files.
//! - **Files copied in a file manager** (a PDF, a video, several files): the
//!   paste event says `text/uri-list`, but `getData` returns `""` and `files`
//!   is empty, so the page cannot tell which files they are.
//!
//! `navigator.clipboard.read()` is refused too, even with WebKit's
//! clipboard-access setting on, so WhatsApp Web never gets anything to attach.
//!
//! bridge.js spots such a paste and calls [`paste_clipboard`]. Rust can see the
//! system clipboard through GTK:
//!
//! - Copied files are streamed into the page through the drag-and-drop
//!   pipeline (`window::stream_pasted_files`), so they get the same type
//!   routing, size caps and error toasts as a drop.
//! - An image comes back as base64 PNG in the response; the page attaches it
//!   as `image.png`.
//!
//! Windows and macOS webviews deliver pasted files and images to the page
//! themselves, so bridge.js never needs this there; the command finds nothing.

use std::path::PathBuf;
use tauri::{AppHandle, Webview};

/// Largest PNG handed back to the page. It travels through IPC in one piece, as
/// base64 a third larger, and the page holds it in memory, so keep it bounded.
/// A 4K screenshot encodes to a few MB.
const MAX_PASTE_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// How long to wait for the app that owns the clipboard to hand over its data.
/// An owner that has hung must not leave the page waiting for ever.
#[cfg(target_os = "linux")]
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the system clipboard holds, as far as a paste into WhatsApp cares.
/// Only the Linux reader ever finds files or an image.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum Clip {
    /// Nothing usable: plain text, an empty clipboard, or a failed read.
    Nothing,
    /// Local files copied in a file manager.
    Files(Vec<PathBuf>),
    /// An image, already encoded as PNG.
    Image(Vec<u8>),
}

/// The local files named by a clipboard URI list. Only `file://` URIs become
/// paths; anything else (an `https://` link copied with the files, a
/// `trash://` or network location with no local path) is left out.
#[cfg(target_os = "linux")]
fn local_paths<S: AsRef<str>>(uris: &[S]) -> Vec<PathBuf> {
    uris.iter()
        .filter(|uri| uri.as_ref().starts_with("file://"))
        .filter_map(|uri| gtk::glib::filename_from_uri(uri.as_ref()).ok())
        .map(|(path, _host)| path)
        .collect()
}

/// Whether an image is small enough to hand to the page.
fn image_fits(png: &[u8]) -> bool {
    png.len() <= MAX_PASTE_IMAGE_BYTES
}

/// Paste whatever the system clipboard holds into the calling WhatsApp page.
///
/// Returns a clipboard image as base64 PNG, or `""` when there is no image.
/// Copied files do not come back here: they are streamed into the page like a
/// drop, and the answer is `""`.
///
/// Why base64 and not raw bytes (`ipc::Response`): a remote page like
/// web.whatsapp.com talks to Rust over Tauri's postMessage IPC, which turns raw
/// bytes into a JSON array of numbers, several times the size and not an
/// `ArrayBuffer`. Base64 arrives the same way on every transport and is what
/// the drop pipeline already uses.
///
/// Only the WhatsApp page may call this (`main-remote` capability plus the
/// origin check below). The page cannot prove a real paste happened, so the
/// read is limited to the moments a paste is possible at all: the app is
/// unlocked and the calling account is the one in front, where the user just
/// pressed Ctrl+V. That matches what a browser gives a page during a paste. The
/// page never names a file: Rust reads the list from the clipboard itself, so
/// only files the user copied can ever be sent.
#[tauri::command]
pub async fn paste_clipboard(window: Webview, app: AppHandle) -> Result<String, String> {
    crate::integration::remote(&window)?;
    crate::lock::require_unlocked(&app)?;
    if !crate::commands::account_in_front(&app, &window) {
        return Err("not in front".into());
    }

    let empty = || Ok(String::new());
    match read_clipboard(&app).await {
        Clip::Files(paths) => {
            // Counts and sizes only: the log never records names or content.
            crate::dlog::log(&format!("paste: {} copied file(s)", paths.len()));
            // Streaming evals into the page blocks, so it runs off the async
            // workers; the command returns at once and the page attaches the
            // files when the stream commits.
            // ponytail: this bypasses the window's FIFO drop worker, so a paste and
            // a drag made at the same instant may commit in either order. The page
            // still attaches them one at a time; route through the worker if order
            // ever matters.
            tauri::async_runtime::spawn_blocking(move || {
                crate::window::stream_pasted_files(&window, &paths)
            });
            empty()
        }
        Clip::Image(png) if image_fits(&png) => {
            crate::dlog::log(&format!("paste: clipboard image, {} bytes", png.len()));
            let mut b64 = String::new();
            crate::window::base64_encode_into(&mut b64, &png);
            Ok(b64)
        }
        Clip::Image(png) => {
            crate::dlog::log(&format!(
                "paste: clipboard image too large, {} bytes",
                png.len()
            ));
            crate::notify::show(
                &app,
                crate::i18n::tr("Imagem não colada").as_ref(),
                crate::i18n::tr(
                    "A imagem da área de transferência é grande demais. Salve-a e anexe o arquivo.",
                )
                .as_ref(),
            );
            empty()
        }
        Clip::Nothing => empty(),
    }
}

/// Read the system clipboard: copied files first, then an image.
///
/// Files win because a file manager's copy names the original file, with its
/// real name and format, while an image viewer may offer both and the file is
/// the better thing to send.
///
/// GTK may only be used on the main thread, so the requests are posted there.
/// `request_uris` and `request_image` are asynchronous: GTK fetches the data
/// from the clipboard owner (another process) and calls back later, so the UI
/// keeps running meanwhile. The result comes back over a channel, awaited on a
/// blocking-pool thread so no async worker sits idle.
#[cfg(target_os = "linux")]
async fn read_clipboard(app: &AppHandle) -> Clip {
    let (tx, rx) = std::sync::mpsc::channel();
    let posted = app.run_on_main_thread(move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        clipboard.request_uris(move |clipboard, uris| {
            let paths = local_paths(uris);
            if !paths.is_empty() {
                let _ = tx.send(Clip::Files(paths));
                return;
            }
            clipboard.request_image(move |_, image| {
                // GTK has already decoded whatever format the owner offered
                // (PNG, JPEG, BMP...), so re-encoding gives the page one type.
                let png = image.and_then(|pixbuf| pixbuf.save_to_bufferv("png", &[]).ok());
                let _ = tx.send(
                    png.filter(|b| !b.is_empty())
                        .map_or(Clip::Nothing, Clip::Image),
                );
            });
        });
    });
    if posted.is_err() {
        return Clip::Nothing;
    }
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(READ_TIMEOUT).unwrap_or(Clip::Nothing)
    })
    .await
    .unwrap_or(Clip::Nothing)
}

/// The other webviews pass pasted files and images to the page directly.
#[cfg(not(target_os = "linux"))]
async fn read_clipboard(_app: &AppHandle) -> Clip {
    Clip::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn file_uris_become_decoded_local_paths() {
        let uris = [
            "file:///home/me/Documents/report%20final.pdf",
            "file:///home/me/Videos/clip.mp4",
        ];
        assert_eq!(
            local_paths(&uris),
            vec![
                PathBuf::from("/home/me/Documents/report final.pdf"),
                PathBuf::from("/home/me/Videos/clip.mp4"),
            ]
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn uris_without_a_local_file_are_left_out() {
        let uris = [
            "https://example.com/photo.jpg",
            "trash:///old.pdf",
            "file:relative-is-not-a-uri",
            "",
        ];
        assert!(local_paths(&uris).is_empty());
    }

    #[test]
    fn an_image_up_to_the_cap_fits_and_one_byte_more_does_not() {
        assert!(image_fits(&[]));
        assert!(image_fits(&vec![0u8; MAX_PASTE_IMAGE_BYTES]));
        assert!(!image_fits(&vec![0u8; MAX_PASTE_IMAGE_BYTES + 1]));
    }
}
