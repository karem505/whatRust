//! WhatsApp links that whatRust opens itself instead of handing to a browser.
//!
//! - `whatsapp://send?phone=…&text=…` and `whatsapp://chat?code=…` — the scheme
//!   "Open app" buttons on websites use. The packages register whatRust as its
//!   handler (issue #23); the OS then starts us (or, through single-instance,
//!   the running copy) with the link as an argument, or on macOS sends an
//!   open-URL event.
//! - `https://wa.me/<number>`, `https://api.whatsapp.com/send?phone=…` and
//!   `https://chat.whatsapp.com/<invite>` — click-to-chat and group-invite links,
//!   which a browser would redirect to WhatsApp Web anyway. Clicked inside a chat
//!   they stay in the app instead of opening a browser tab.
//!
//! Each maps to the WhatsApp Web URL a browser lands on for the same link, which
//! WhatsApp Web itself turns into "open this chat" / "join this group".

use tauri::{AppHandle, Manager, Url};

const WEB: &str = "https://web.whatsapp.com";

/// The WhatsApp Web page for a WhatsApp link, or `None` when `raw` isn't one we
/// know how to open in-app (callers then fall back to just showing the window,
/// or to the system browser for ordinary web links).
pub fn whatsapp_web_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    let host = url.host_str().map(str::to_ascii_lowercase);
    match (url.scheme(), host.as_deref()) {
        // whatsapp://send?phone=… — the host is the action.
        ("whatsapp", Some("send")) => send_url(&url, None),
        ("whatsapp", Some("chat")) => invite_url(query(&url, "code")?),
        ("http" | "https", Some("wa.me" | "www.wa.me")) => {
            let mut segments = url.path_segments()?.filter(|s| !s.is_empty());
            let first = segments.next();
            // wa.me/message/<id> (business short links) and anything deeper
            // need WhatsApp's own resolver: leave them to the browser.
            match (first, segments.next()) {
                (None, _) => send_url(&url, None),
                (Some(phone), None) => send_url(&url, Some(phone)),
                _ => None,
            }
        }
        ("http" | "https", Some("api.whatsapp.com" | "whatsapp.com" | "www.whatsapp.com"))
            if url.path().trim_end_matches('/') == "/send" =>
        {
            send_url(&url, None)
        }
        ("http" | "https", Some("chat.whatsapp.com")) => {
            let mut segments = url.path_segments()?.filter(|s| !s.is_empty());
            match (segments.next(), segments.next()) {
                (Some(code), None) => invite_url(code.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The first command-line argument that is a WhatsApp link (`whatsapp:` or one
/// of the click-to-chat web links). Plain flags like `--minimized` are skipped.
pub fn link_from_args<S: AsRef<str>>(args: &[S]) -> Option<String> {
    args.iter()
        .map(AsRef::as_ref)
        .skip(1) // argv[0] is the program
        .find(|a| {
            a.get(..9)
                .is_some_and(|p| p.eq_ignore_ascii_case("whatsapp:"))
                || whatsapp_web_url(a).is_some()
        })
        .map(str::to_string)
}

/// Open a WhatsApp link from outside the app (command line, single-instance
/// relaunch, macOS open-URL event): load the chat in the active account's window
/// and bring it forward. A `whatsapp:` link we can't map (e.g. a bare
/// `whatsapp://`) still brings the window forward. The link itself is never
/// logged — it can carry a phone number and a pre-filled message.
pub fn open_from_outside(app: &AppHandle, raw: &str) {
    match whatsapp_web_url(raw) {
        Some(target) => {
            crate::dlog::log("links: opening a WhatsApp link in-app");
            if let Some(win) = active_account_window(app) {
                let _ = win.navigate(target);
            }
        }
        None => crate::dlog::log("links: unrecognized WhatsApp link, focusing only"),
    }
    // Defers to the lock screen when locked, like every other "reveal" path.
    crate::window::show_main(app);
}

fn active_account_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(active) = app.try_state::<crate::accounts::ActiveAccount>() {
        let label = active.lock().unwrap().clone();
        if let Some(w) = app.get_webview_window(&label) {
            return Some(w);
        }
    }
    app.webview_windows()
        .into_iter()
        .find(|(label, _)| label.starts_with("wa-"))
        .map(|(_, w)| w)
}

fn query(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

/// `…/send?phone=<digits>&text=<text>`. The phone comes from the path (wa.me)
/// or the `phone` parameter; only its digits are kept. A link with neither still
/// opens WhatsApp Web's "share to…" picker when it has text.
fn send_url(url: &Url, path_phone: Option<&str>) -> Option<Url> {
    let phone: String = path_phone
        .map(str::to_string)
        .or_else(|| query(url, "phone"))
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let text = query(url, "text");
    if phone.is_empty() && text.is_none() {
        return Some(Url::parse(&format!("{WEB}/")).expect("valid url"));
    }
    let mut out = Url::parse(&format!("{WEB}/send")).expect("valid url");
    {
        let mut q = out.query_pairs_mut();
        if !phone.is_empty() {
            q.append_pair("phone", &phone);
        }
        if let Some(text) = text {
            q.append_pair("text", &text);
        }
    }
    Some(out)
}

/// `…/accept?code=<invite>` — WhatsApp Web's group-invite page. Invite codes are
/// alphanumeric; anything else is rejected rather than passed through.
fn invite_url(code: String) -> Option<Url> {
    if code.is_empty() || code.len() > 64 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Url::parse(&format!("{WEB}/accept?code={code}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(raw: &str) -> Option<String> {
        whatsapp_web_url(raw).map(|u| u.to_string())
    }

    #[test]
    fn whatsapp_send_links_open_the_chat() {
        assert_eq!(
            map("whatsapp://send?phone=+1 (555) 123-4567&text=Hello there"),
            Some("https://web.whatsapp.com/send?phone=15551234567&text=Hello+there".into())
        );
        assert_eq!(
            map("WHATSAPP://send/?phone=4412345"),
            Some("https://web.whatsapp.com/send?phone=4412345".into())
        );
        // Text only: WhatsApp Web asks which chat to send it to.
        assert_eq!(
            map("whatsapp://send?text=hi%20%26%20bye"),
            Some("https://web.whatsapp.com/send?text=hi+%26+bye".into())
        );
        assert_eq!(
            map("whatsapp://send"),
            Some("https://web.whatsapp.com/".into())
        );
    }

    #[test]
    fn click_to_chat_web_links_stay_in_the_app() {
        assert_eq!(
            map("https://wa.me/15551234567?text=I%27m%20interested"),
            Some("https://web.whatsapp.com/send?phone=15551234567&text=I%27m+interested".into())
        );
        assert_eq!(
            map("https://api.whatsapp.com/send?phone=15551234567"),
            Some("https://web.whatsapp.com/send?phone=15551234567".into())
        );
        assert_eq!(
            map("https://api.whatsapp.com/send/?phone=15551234567&text=x&type=phone_number&app_absent=0"),
            Some("https://web.whatsapp.com/send?phone=15551234567&text=x".into())
        );
        // wa.me business short links need WhatsApp's resolver: browser.
        assert_eq!(map("https://wa.me/message/ABCDEF123"), None);
    }

    #[test]
    fn group_invites_open_the_join_page() {
        assert_eq!(
            map("https://chat.whatsapp.com/AbCdEf1234567890XyZ"),
            Some("https://web.whatsapp.com/accept?code=AbCdEf1234567890XyZ".into())
        );
        assert_eq!(
            map("whatsapp://chat?code=AbCdEf1234567890XyZ"),
            Some("https://web.whatsapp.com/accept?code=AbCdEf1234567890XyZ".into())
        );
        // An invite code that isn't one can't smuggle a path or query in.
        assert_eq!(map("whatsapp://chat?code=../../logout"), None);
        assert_eq!(map("https://chat.whatsapp.com/abc/def"), None);
    }

    #[test]
    fn ordinary_links_are_not_whatsapp_links() {
        for raw in [
            "https://github.com/karem505/whatRust",
            "https://web.whatsapp.com/",
            "https://www.whatsapp.com/download",
            "mailto:someone@example.com",
            "whatsapp://unknown-action",
            "not a url",
        ] {
            assert_eq!(map(raw), None, "{raw}");
        }
    }

    #[test]
    fn the_link_is_found_among_launch_arguments() {
        assert_eq!(
            link_from_args(&["whatrust", "--minimized", "whatsapp://send?phone=1"]),
            Some("whatsapp://send?phone=1".into())
        );
        assert_eq!(
            link_from_args(&["/usr/bin/whatrust", "https://wa.me/123"]),
            Some("https://wa.me/123".into())
        );
        // A bare scheme link still counts: it should bring the window forward.
        assert_eq!(
            link_from_args(&["whatrust", "whatsapp://"]),
            Some("whatsapp://".into())
        );
        assert_eq!(link_from_args(&["whatrust", "--toggle"]), None);
        // argv[0] is never taken for a link.
        assert_eq!(link_from_args(&["whatsapp://send?phone=1"]), None);
    }
}
