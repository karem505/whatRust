//! Network proxy for the WhatsApp webviews (issue #52).
//!
//! The webview engines take a proxy as a `socks5://host:port` or
//! `http://host:port` URL. Neither WebView2 (`--proxy-server`), WKWebView
//! (`proxyConfigurations`) nor WebKitGTK accepts credentials through that
//! route, so a user name or password is rejected rather than half-honoured.
//!
//! A saved configuration that no longer validates (hand-edited settings.json)
//! never silently falls back to a direct connection: the webviews get an
//! unroutable proxy instead, so the page fails to load.
//!
//! whatRust's own Rust code makes one kind of network request: the update check
//! and download (`updater.rs`, api.github.com and github.com). It reads the same
//! saved proxy settings through `url_for`, so it is routed the same way, and an
//! invalid saved proxy stops it instead of letting it connect directly. Notification
//! pictures are decoded from data the page already fetched.

use crate::i18n::tr;
use crate::settings::Settings;
use std::net::{Ipv4Addr, Ipv6Addr};
use tauri::Url;

pub const MODE_OFF: &str = "off";
pub const MODE_SOCKS5: &str = "socks5";
pub const MODE_HTTP: &str = "http";

/// Where a webview is pointed when the saved proxy config is invalid: nothing
/// listens on the discard port, so traffic fails instead of leaking.
const BLACKHOLE: &str = "socks5://127.0.0.1:9";

fn valid_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Validate a host (hostname, IPv4 or IPv6, optionally in brackets) and return
/// it in URL form (IPv6 in brackets).
pub fn normalize_host(raw: &str) -> Result<String, String> {
    let host = raw.trim();
    let bad = || {
        tr("Endereço do proxy inválido. Use um nome de host ou um endereço IP, sem http://, usuário ou senha.").into_owned()
    };
    if host.is_empty() {
        return Err(bad());
    }
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if unbracketed.contains(':') {
        return unbracketed
            .parse::<Ipv6Addr>()
            .map(|a| format!("[{a}]"))
            .map_err(|_| bad());
    }
    if host.starts_with('[') || host.ends_with(']') {
        return Err(bad());
    }
    if unbracketed.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return unbracketed
            .parse::<Ipv4Addr>()
            .map(|a| a.to_string())
            .map_err(|_| bad());
    }
    if valid_hostname(unbracketed) {
        Ok(unbracketed.to_ascii_lowercase())
    } else {
        Err(bad())
    }
}

/// The proxy URL for a settings value: `Ok(None)` when the proxy is off.
pub fn url_for(s: &Settings) -> Result<Option<Url>, String> {
    let scheme = match s.proxy_mode.as_str() {
        MODE_OFF => return Ok(None),
        MODE_SOCKS5 => "socks5",
        MODE_HTTP => "http",
        _ => return Err(tr("Tipo de proxy inválido.").into_owned()),
    };
    let host = normalize_host(&s.proxy_host)?;
    if !(1..=65535).contains(&s.proxy_port) {
        return Err(tr("A porta do proxy deve estar entre 1 e 65535.").into_owned());
    }
    Url::parse(&format!("{scheme}://{host}:{}", s.proxy_port))
        .map(Some)
        .map_err(|_| tr("Endereço do proxy inválido.").into_owned())
}

/// The URL to hand every account webview. Fail-closed (see module docs).
pub fn webview_url(s: &Settings) -> Option<Url> {
    match url_for(s) {
        Ok(url) => url,
        Err(e) => {
            crate::dlog::log(&format!(
                "proxy: saved configuration invalid ({e}); blocking"
            ));
            Some(Url::parse(BLACKHOLE).expect("valid url"))
        }
    }
}

static STARTUP: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Remember the proxy this run's webviews were created with. Called once at
/// startup, so the settings page can tell when a saved change still waits for
/// a restart.
pub fn note_startup(s: &Settings) {
    let _ = STARTUP.set(summary(s));
}

/// What the running webviews use (`""` when none).
pub fn in_use() -> String {
    STARTUP.get().cloned().unwrap_or_default()
}

/// Canonical form of a configuration, for comparing saved vs. running.
pub fn summary(s: &Settings) -> String {
    webview_url(s).map(|u| u.to_string()).unwrap_or_default()
}

/// Whether the webview engine can proxy at all. WKWebView's
/// `proxyConfigurations` data-store key exists from macOS 14; setting it on
/// an older system raises an Objective-C exception, so it is never attempted.
pub fn engine_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::menubar::supported()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Refuse to create an account webview that would have to bypass or crash on
/// a configured proxy.
pub fn check_platform(s: &Settings) -> tauri::Result<()> {
    if s.proxy_mode != MODE_OFF && !engine_supported() {
        return Err(tauri::Error::Io(std::io::Error::other(
            tr("O proxy exige macOS 14 (Sonoma) ou superior. Desative o proxy nas configurações.")
                .into_owned(),
        )));
    }
    Ok(())
}

/// Settings-save validation: refuse a proxy that would not work, and keep the
/// stored fields canonical (a host without brackets/case noise, no stale port
/// when off).
pub fn validate(s: &mut Settings) -> Result<(), String> {
    if s.proxy_mode == MODE_OFF {
        // Keep the host/port the user typed so toggling back on is painless,
        // but never let an invalid leftover stop the user saving.
        return Ok(());
    }
    if !engine_supported() {
        return Err(tr("O proxy exige macOS 14 (Sonoma) ou superior.").into_owned());
    }
    url_for(s)?;
    s.proxy_host = normalize_host(&s.proxy_host)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: &str, host: &str, port: u32) -> Settings {
        Settings {
            proxy_mode: mode.into(),
            proxy_host: host.into(),
            proxy_port: port,
            ..Settings::default()
        }
    }

    #[test]
    fn off_is_the_default_and_yields_no_url() {
        assert_eq!(url_for(&Settings::default()), Ok(None));
        assert_eq!(webview_url(&Settings::default()), None);
        // Leftover junk while off is ignored.
        assert_eq!(url_for(&cfg("off", "!!", 0)), Ok(None));
    }

    #[test]
    fn builds_socks5_and_http_urls() {
        let u = url_for(&cfg("socks5", "127.0.0.1", 1080)).unwrap().unwrap();
        assert_eq!(u.as_str(), "socks5://127.0.0.1:1080");
        let u = url_for(&cfg("http", "Proxy.Example.com", 8080))
            .unwrap()
            .unwrap();
        assert_eq!(u.as_str(), "http://proxy.example.com:8080/");
        assert_eq!(u.host().unwrap().to_string(), "proxy.example.com");
        assert_eq!(u.port(), Some(8080));
    }

    #[test]
    fn ipv6_is_bracketed() {
        for h in ["::1", "[::1]", " 2001:db8::1 "] {
            let u = url_for(&cfg("socks5", h, 1080)).unwrap().unwrap();
            assert!(u.host_str().unwrap().starts_with('['), "{h}");
            assert_eq!(u.port(), Some(1080));
        }
        assert_eq!(normalize_host("[::1]").unwrap(), "[::1]");
    }

    #[test]
    fn rejects_bad_hosts() {
        for h in [
            "",
            " ",
            "user:pw@host",
            "user@host",
            "http://host",
            "host:1080",
            "host/path",
            "host?x",
            "ho st",
            "-a.com",
            "a-.com",
            "a..com",
            ".a.com",
            "999.1.1.1",
            "1.2.3",
            "[1.2.3.4]",
            "[host]",
            "[::1",
            "::1]",
            "::g",
            "a_b.com",
            "höst.com",
            "ho\nst",
            "host#frag",
            "host\\x",
        ] {
            assert!(
                url_for(&cfg("http", h, 80)).is_err(),
                "{h:?} should be rejected"
            );
        }
        assert!(url_for(&cfg("http", &"a".repeat(64), 80)).is_err());
        assert!(url_for(&cfg("http", &format!("{}.com", "a.".repeat(130)), 80)).is_err());
    }

    #[test]
    fn rejects_bad_ports_and_modes() {
        for p in [0, 65536, 99999, u32::MAX] {
            assert!(url_for(&cfg("socks5", "localhost", p)).is_err(), "{p}");
        }
        assert!(url_for(&cfg("socks5", "localhost", 65535)).is_ok());
        assert!(url_for(&cfg("socks5", "localhost", 1)).is_ok());
        assert!(url_for(&cfg("socks4", "localhost", 1080)).is_err());
        assert!(url_for(&cfg("", "localhost", 1080)).is_err());
    }

    #[test]
    fn invalid_saved_config_fails_closed() {
        let u = webview_url(&cfg("socks5", "bad host", 1080)).unwrap();
        assert_eq!(u.as_str(), BLACKHOLE);
        let u = webview_url(&cfg("http", "localhost", 0)).unwrap();
        assert_eq!(u.as_str(), BLACKHOLE);
    }

    #[test]
    fn validate_canonicalises_and_refuses() {
        let mut s = cfg("socks5", " [::1] ", 1080);
        validate(&mut s).unwrap();
        assert_eq!(s.proxy_host, "[::1]");
        assert!(validate(&mut cfg("http", "", 80)).is_err());
        assert!(validate(&mut cfg("off", "", 0)).is_ok());
    }

    #[test]
    fn settings_json_defaults_to_off() {
        let s: Settings = serde_json::from_str(r#"{"autostart": true}"#).unwrap();
        assert_eq!(s.proxy_mode, MODE_OFF);
        assert_eq!(url_for(&s), Ok(None));
    }
}
