//! Update engine: finds the newest GitHub release, downloads the asset that fits
//! how whatRust was installed, checks it against the release's `SHA256SUMS` and
//! applies it. No Tauri types here, so every path is unit-tested; the scheduling,
//! notifications, tray item and IPC live in `update_service.rs`.
//!
//! How an update is applied depends on the install type (see [`Method`]):
//!
//! | install type      | what happens                                                     |
//! |-------------------|------------------------------------------------------------------|
//! | AppImage          | new file next to `$APPIMAGE`, atomic rename over it, relaunch    |
//! | macOS `.app`      | `.app.tar.gz` unpacked beside the bundle, bundle swapped, relaunch |
//! | Windows NSIS      | setup run with `/P /UPDATE /R` (the template's update switches)  |
//! | Windows MSI       | `msiexec /i … /passive`                                          |
//! | `.deb`            | `pkexec dpkg -i` after the user clicks Install                   |
//! | Flatpak           | bundle saved in Downloads and opened through the OpenURI portal  |
//! | source build etc. | reported only                                                    |
//!
//! Nothing is replaced or launched before the download matched the release's
//! `SHA256SUMS`; a mismatch, a missing entry or a missing `SHA256SUMS` aborts and
//! leaves the installed copy as it was. `WHATRUST_UPDATE_FEED` points the checker
//! at another release JSON (a URL or a local file whose asset URLs may be local
//! paths); the tests and the local end-to-end run use it.

use crate::i18n::{tr, trf};
use semver::Version;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REPO: &str = "karem505/whatRust";
pub const FEED_ENV: &str = "WHATRUST_UPDATE_FEED";
/// Where the Flatpak fallback sends people who cannot use the bundle directly.
pub const FLATPAK_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/karem505/whatRust/master/install-flatpak.sh | sh";
const ASSET_PREFIX: &str = "https://github.com/karem505/whatRust/releases/download/";
const NOTES_LIMIT: usize = 60_000;

// ---- errors ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Network,
    Checksum,
    NoAsset,
    Io,
    Unsupported,
    Install,
    Cancelled,
}

/// A failure with a user-readable (already translated) message.
#[derive(Debug, Clone, PartialEq)]
pub struct UErr {
    pub kind: Kind,
    pub msg: String,
}

impl std::fmt::Display for UErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

fn err(kind: Kind, msg: impl Into<String>) -> UErr {
    UErr {
        kind,
        msg: msg.into(),
    }
}

type Res<T> = Result<T, UErr>;

// ---- where we run ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
    Macos,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Linux
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
    Other,
}

impl Arch {
    pub fn current() -> Arch {
        match std::env::consts::ARCH {
            "x86_64" => Arch::X64,
            "aarch64" => Arch::Arm64,
            _ => Arch::Other,
        }
    }
}

/// Everything install-type detection looks at, so tests can describe any machine.
#[derive(Debug, Clone)]
pub struct Env {
    pub os: Os,
    pub exe: PathBuf,
    /// `$APPIMAGE`
    pub appimage: Option<PathBuf>,
    /// `$FLATPAK_ID` set (or `/.flatpak-info` present)
    pub flatpak: bool,
    /// a `dpkg` is on this machine (Debian family)
    pub has_dpkg: bool,
}

impl Env {
    pub fn current() -> Env {
        let exe = std::env::current_exe().unwrap_or_default();
        // Replaced under our feet by a package upgrade: Linux reports `<path> (deleted)`.
        let exe = PathBuf::from(exe.to_string_lossy().trim_end_matches(" (deleted)"));
        Env {
            os: Os::current(),
            exe,
            appimage: std::env::var_os("APPIMAGE")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            flatpak: std::env::var_os("FLATPAK_ID").is_some_and(|v| !v.is_empty())
                || Path::new("/.flatpak-info").exists(),
            has_dpkg: Path::new("/usr/bin/dpkg").exists() || Path::new("/bin/dpkg").exists(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    /// the AppImage file to replace
    AppImage(PathBuf),
    Deb,
    Flatpak,
    /// Windows install made by the NSIS setup (`uninstall.exe` beside the exe)
    WindowsNsis,
    /// Windows install made by the MSI (under Program Files)
    WindowsMsi,
    /// the running `.app` bundle
    MacApp(PathBuf),
    /// run from a source checkout (`target/debug|release`)
    Source,
    /// installed by something we cannot drive (distro package other than .deb, portable copy…)
    Other,
}

/// A `/` separated, lower-cased view of a path, so Windows paths can be judged on any host.
fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn is_source_build(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.iter().enumerate().any(|(i, p)| {
        *p == "target"
            && parts[i + 1..parts.len().saturating_sub(1)]
                .iter()
                .take(2)
                .any(|q| *q == "debug" || *q == "release")
    })
}

/// The `.app` directory that contains `exe` (`…/whatRust.app/Contents/MacOS/whatrust`).
fn mac_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|a| {
            a.extension().is_some_and(|e| e == "app")
                && exe
                    .strip_prefix(a)
                    .is_ok_and(|rest| rest.starts_with("Contents/MacOS"))
        })
        .map(Path::to_path_buf)
}

pub fn detect(env: &Env) -> Method {
    if env.flatpak {
        return Method::Flatpak;
    }
    let path = norm(&env.exe);
    match env.os {
        Os::Linux => {
            if let Some(img) = env.appimage.as_ref().filter(|p| p.is_file()) {
                return Method::AppImage(img.clone());
            }
            if is_source_build(&path) {
                Method::Source
            } else if path.starts_with("/usr/") && env.has_dpkg {
                Method::Deb
            } else {
                Method::Other
            }
        }
        Os::Windows => {
            if is_source_build(&path) {
                Method::Source
            } else if env
                .exe
                .parent()
                .is_some_and(|d| d.join("uninstall.exe").is_file())
            {
                Method::WindowsNsis
            } else if path.to_ascii_lowercase().contains("/program files") {
                Method::WindowsMsi
            } else {
                Method::Other
            }
        }
        Os::Macos => match mac_bundle(&env.exe) {
            Some(b) if !is_source_build(&path) => Method::MacApp(b),
            _ if is_source_build(&path) => Method::Source,
            _ => Method::Other,
        },
    }
}

/// What a release asset is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    AppImage,
    Deb,
    Flatpak,
    Nsis,
    Msi,
    MacTar,
    MacDmg,
}

impl Method {
    pub fn id(&self) -> &'static str {
        match self {
            Method::AppImage(_) => "appimage",
            Method::Deb => "deb",
            Method::Flatpak => "flatpak",
            Method::WindowsNsis => "windows-nsis",
            Method::WindowsMsi => "windows-msi",
            Method::MacApp(_) => "macos",
            Method::Source => "source",
            Method::Other => "other",
        }
    }

    pub fn label(&self) -> String {
        match self {
            Method::AppImage(_) => "AppImage".into(),
            Method::Deb => ".deb".into(),
            Method::Flatpak => "Flatpak".into(),
            Method::WindowsNsis => tr("Instalador do Windows (NSIS)").into_owned(),
            Method::WindowsMsi => tr("Instalador do Windows (MSI)").into_owned(),
            Method::MacApp(_) => tr("Aplicativo do macOS").into_owned(),
            Method::Source => tr("Compilado do código-fonte").into_owned(),
            Method::Other => tr("Outro tipo de instalação").into_owned(),
        }
    }

    /// The asset this install type needs, or `None` when the app cannot update itself.
    /// `writable_bundle`: whether the macOS bundle's folder can be written.
    pub fn asset_kind(&self, writable_bundle: bool) -> Option<AssetKind> {
        match self {
            Method::AppImage(_) => Some(AssetKind::AppImage),
            Method::Deb => Some(AssetKind::Deb),
            Method::Flatpak => Some(AssetKind::Flatpak),
            Method::WindowsNsis => Some(AssetKind::Nsis),
            Method::WindowsMsi => Some(AssetKind::Msi),
            Method::MacApp(_) if writable_bundle => Some(AssetKind::MacTar),
            Method::MacApp(_) => Some(AssetKind::MacDmg),
            Method::Source | Method::Other => None,
        }
    }

    /// Needs the user to click (a password prompt or a managed installer).
    pub fn needs_password(&self) -> bool {
        matches!(self, Method::Deb | Method::WindowsMsi)
    }

    /// Whether "install updates automatically" may act on this type: in-place
    /// types install by themselves; Windows NSIS downloads and verifies by
    /// itself and runs the installer when the user clicks (it restarts the app);
    /// everything else never installs unasked.
    pub fn auto_install_default(&self) -> bool {
        matches!(
            self,
            Method::AppImage(_) | Method::MacApp(_) | Method::WindowsNsis
        )
    }

    /// What to tell people whose copy cannot update itself.
    pub fn how_to(&self) -> String {
        match self {
            Method::Source => tr("Compilado do código-fonte: atualize o código e compile de novo, ou instale uma versão publicada.").into_owned(),
            Method::Other => tr("Esta instalação não pode ser atualizada pelo whatRust. Baixe a nova versão na página de lançamentos ou use o gerenciador de pacotes.").into_owned(),
            _ => String::new(),
        }
    }
}

// ---- releases -------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub published: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// A GitHub release (REST API shape). Drafts, prereleases and tags that are not
/// plain `X.Y.Z` are ignored.
pub fn parse_release(v: &Value) -> Option<Release> {
    if v["draft"] == json!(true) || v["prerelease"] == json!(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    let version = Version::parse(tag.trim_start_matches('v')).ok()?;
    if !version.pre.is_empty() {
        return None;
    }
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Asset {
                        name: x["name"].as_str()?.to_string(),
                        url: x["browser_download_url"].as_str()?.to_string(),
                        size: x["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release {
        version,
        name: v["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(tag)
            .to_string(),
        notes: v["body"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(NOTES_LIMIT)
            .collect(),
        url: v["html_url"].as_str().unwrap_or_default().to_string(),
        published: v["published_at"].as_str().unwrap_or_default().to_string(),
        assets,
    })
}

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver")
}

/// The asset for `kind` on `arch`, chosen by file-name pattern (never by position).
pub fn find_asset(release: &Release, kind: AssetKind, arch: Arch) -> Option<&Asset> {
    let token = |x64: &'static str, arm: &'static str| match arch {
        Arch::X64 => Some(x64),
        Arch::Arm64 => Some(arm),
        Arch::Other => None,
    };
    let (suffix, arch_token): (&str, &str) = match kind {
        AssetKind::AppImage => (".appimage", token("_amd64", "_aarch64")?),
        AssetKind::Deb => (".deb", token("_amd64", "_arm64")?),
        AssetKind::Flatpak => (".flatpak", token("_x86_64", "_aarch64")?),
        AssetKind::Nsis => ("-setup.exe", token("_x64", "_arm64")?),
        AssetKind::Msi => (".msi", token("_x64", "_arm64")?),
        AssetKind::MacTar => (".app.tar.gz", token("_x64", "_aarch64")?),
        AssetKind::MacDmg => (".dmg", token("_x64", "_aarch64")?),
    };
    release.assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n.starts_with("whatrust") && n.ends_with(suffix) && n.contains(arch_token)
    })
}

/// The expected SHA-256 of `name` in a `sha256sum` listing (file names only,
/// optional `*` binary marker).
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.trim().split_once(char::is_whitespace)?;
        let ok = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        (ok && file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

// ---- network --------------------------------------------------------------------------------

/// The feed in use and whether it was overridden (tests, local end-to-end runs).
fn feed() -> (String, bool) {
    match std::env::var(FEED_ENV).ok().filter(|s| !s.is_empty()) {
        Some(f) => (f, true),
        None => (
            format!("https://api.github.com/repos/{REPO}/releases/latest"),
            false,
        ),
    }
}

/// A path or `file://` URL, else `None` (a real URL).
fn local_path(url: &str) -> Option<PathBuf> {
    if let Some(p) = url.strip_prefix("file://") {
        Some(PathBuf::from(p))
    } else if !url.contains("://") {
        Some(PathBuf::from(url))
    } else {
        None
    }
}

/// Real downloads only come from this repository's releases; a feed override
/// (explicitly set by whoever runs the app) may point anywhere.
pub fn asset_url_ok(url: &str, overridden: bool) -> bool {
    overridden || url.starts_with(ASSET_PREFIX)
}

/// How HTTP requests leave this machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Net {
    /// `socks5://host:port` or `http://host:port`: the proxy chosen in Settings → Network.
    pub proxy: Option<String>,
}

impl Net {
    /// Same proxy as the WhatsApp webviews. An invalid saved proxy fails closed:
    /// the update is not fetched directly behind the user's back.
    pub fn from_settings(s: &crate::settings::Settings) -> Res<Net> {
        match crate::proxy::url_for(s) {
            Ok(url) => Ok(Net {
                proxy: url.map(|u| u.as_str().trim_end_matches('/').to_string()),
            }),
            Err(e) => Err(err(Kind::Network, e)),
        }
    }

    fn agent(&self, body_limit: Duration) -> Res<ureq::Agent> {
        let mut b = ureq::Agent::config_builder()
            .user_agent(format!(
                "whatRust/{} (+https://github.com/{REPO})",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(body_limit));
        if let Some(p) = &self.proxy {
            let proxy = ureq::Proxy::new(p)
                .map_err(|e| err(Kind::Network, trf("Proxy inválido: {0}", &[&e])))?;
            b = b.proxy(Some(proxy));
        }
        Ok(b.build().into())
    }
}

fn net_err(e: impl std::fmt::Display) -> UErr {
    err(Kind::Network, trf("Falha de rede: {0}", &[&e]))
}

fn fetch_text(net: &Net, url: &str) -> Res<String> {
    if let Some(p) = local_path(url) {
        return std::fs::read_to_string(&p)
            .map_err(|e| err(Kind::Io, format!("{}: {e}", p.display())));
    }
    let mut resp = net
        .agent(Duration::from_secs(60))?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(net_err)?;
    resp.body_mut().read_to_string().map_err(net_err)
}

/// The newest published release.
pub fn fetch_latest(net: &Net) -> Res<Release> {
    let (url, _) = feed();
    let text = fetch_text(net, &url)?;
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        err(
            Kind::Network,
            trf("Resposta inesperada do GitHub: {0}", &[&e]),
        )
    })?;
    parse_release(&v).ok_or_else(|| err(Kind::NoAsset, tr("Nenhuma versão publicada encontrada.")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Download `url` into `dest` (created new), returning the SHA-256 of what was
/// written. Reports `(received, total)` as it goes.
fn download(
    net: &Net,
    url: &str,
    dest: &Path,
    total_hint: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Res<String> {
    let io = |e: std::io::Error| err(Kind::Io, format!("{}: {e}", dest.display()));
    let _ = std::fs::remove_file(dest);
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(io)?;
    let (mut reader, total): (Box<dyn Read>, u64) = match local_path(url) {
        Some(p) => {
            let f = std::fs::File::open(&p)
                .map_err(|e| err(Kind::Io, format!("{}: {e}", p.display())))?;
            let len = f.metadata().map(|m| m.len()).unwrap_or(total_hint);
            (Box::new(f), len)
        }
        None => {
            let resp = net
                .agent(Duration::from_secs(60 * 60))?
                .get(url)
                .call()
                .map_err(net_err)?;
            let len = resp
                .headers()
                .get("content-length")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(total_hint);
            (
                Box::new(resp.into_body().into_with_config().limit(u64::MAX).reader()),
                len,
            )
        }
    };
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut got = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| err(Kind::Network, trf("Download interrompido: {0}", &[&e])))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(io)?;
        hasher.update(&buf[..n]);
        got += n as u64;
        progress(got, total);
    }
    out.sync_all().map_err(io)?;
    if total > 0 && got != total {
        return Err(err(
            Kind::Network,
            trf("Download incompleto ({0} de {1} bytes).", &[&got, &total]),
        ));
    }
    Ok(hex(&hasher.finalize()))
}

/// The expected checksum of `name` from the release's `SHA256SUMS`. A release
/// without that file, or without an entry for `name`, cannot be installed.
fn expected_checksum(net: &Net, release: &Release, name: &str) -> Res<String> {
    let sums = release.asset("SHA256SUMS").ok_or_else(|| {
        err(
            Kind::Checksum,
            tr("Esta versão não tem o arquivo SHA256SUMS; nada foi alterado."),
        )
    })?;
    let text = fetch_text(net, &sums.url)?;
    checksum_for(&text, name).ok_or_else(|| {
        err(
            Kind::Checksum,
            trf("O SHA256SUMS não lista {0}; nada foi alterado.", &[&name]),
        )
    })
}

// ---- planning and fetching ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Dest {
    /// next to the file being replaced (same file system, so the rename is atomic)
    Beside(PathBuf),
    /// a fresh private temp directory
    Private,
    /// the user's Downloads folder
    Downloads(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub kind: AssetKind,
    pub asset: Asset,
    pub dest: Dest,
}

/// Is `dir` somewhere we can create files?
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".whatrust-write-test-{}", std::process::id()));
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Choose the asset and where it is downloaded for this install type.
pub fn plan(method: &Method, release: &Release, arch: Arch, downloads: &Path) -> Res<Plan> {
    let writable = match method {
        Method::MacApp(b) => b.parent().is_some_and(dir_writable),
        _ => true,
    };
    let kind = method
        .asset_kind(writable)
        .ok_or_else(|| err(Kind::Unsupported, method.how_to()))?;
    let asset = find_asset(release, kind, arch).ok_or_else(|| {
        err(
            Kind::NoAsset,
            trf(
                "Esta versão não tem um arquivo para o seu tipo de instalação ({0}).",
                &[&method.id()],
            ),
        )
    })?;
    let dest = match (method, kind) {
        (Method::AppImage(t), _) => Dest::Beside(
            t.parent()
                .ok_or_else(|| err(Kind::Io, tr("Caminho do AppImage inválido.")))?
                .to_path_buf(),
        ),
        (Method::MacApp(b), AssetKind::MacTar) => Dest::Beside(
            b.parent()
                .ok_or_else(|| err(Kind::Io, tr("Caminho do aplicativo inválido.")))?
                .to_path_buf(),
        ),
        (_, AssetKind::Flatpak | AssetKind::MacDmg) => Dest::Downloads(downloads.to_path_buf()),
        _ => Dest::Private,
    };
    Ok(Plan {
        kind,
        asset: asset.clone(),
        dest,
    })
}

/// A downloaded, verified file.
#[derive(Debug, Clone, PartialEq)]
pub struct Staged {
    pub path: PathBuf,
    pub name: String,
    pub kind: AssetKind,
    /// directory to delete when the file is no longer needed
    pub cleanup: Option<PathBuf>,
}

impl Staged {
    pub fn discard(&self) {
        let _ = std::fs::remove_file(&self.path);
        if let Some(d) = &self.cleanup {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

#[cfg(unix)]
fn private_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("whatrust-update-{}-{nanos}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

#[cfg(not(unix))]
fn private_dir() -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("whatrust-update-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

/// Remove private update folders a crashed or finished run left behind (older than a day).
pub fn clean_stale() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for e in rd.flatten() {
        let stale = e
            .file_name()
            .to_string_lossy()
            .starts_with("whatrust-update-")
            && e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(86_400));
        if stale {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Download the planned asset and verify it against `SHA256SUMS`. On any failure
/// every file this wrote is removed and nothing else was touched.
pub fn fetch(
    plan: &Plan,
    release: &Release,
    net: &Net,
    progress: &mut dyn FnMut(u64, u64),
) -> Res<Staged> {
    let (_, overridden) = feed();
    if !asset_url_ok(&plan.asset.url, overridden) {
        return Err(err(
            Kind::Unsupported,
            tr("O endereço do arquivo não pertence às versões oficiais do whatRust."),
        ));
    }
    // Ask for the checksum first: a release that cannot be verified is not downloaded at all.
    let want = expected_checksum(net, release, &plan.asset.name)?;
    let (dir, part, final_path, cleanup) = match &plan.dest {
        Dest::Beside(d) => (d.clone(), d.join(".whatrust-update.part"), None, None),
        Dest::Downloads(d) => (
            d.clone(),
            d.join(format!(".{}.part", plan.asset.name)),
            Some(d.join(&plan.asset.name)),
            None,
        ),
        Dest::Private => {
            let d = private_dir().map_err(|e| {
                err(
                    Kind::Io,
                    trf("Não foi possível criar uma pasta temporária: {0}", &[&e]),
                )
            })?;
            (d.clone(), d.join(&plan.asset.name), None, Some(d))
        }
    };
    if matches!(plan.dest, Dest::Downloads(_)) {
        let _ = std::fs::create_dir_all(&dir);
    }
    let cleanup_all = |part: &Path| {
        let _ = std::fs::remove_file(part);
        if let Some(c) = &cleanup {
            let _ = std::fs::remove_dir_all(c);
        }
    };
    let got = match download(net, &plan.asset.url, &part, plan.asset.size, progress) {
        Ok(h) => h,
        Err(e) => {
            cleanup_all(&part);
            return Err(e);
        }
    };
    if got != want {
        cleanup_all(&part);
        return Err(err(
            Kind::Checksum,
            trf(
                "{0} está danificado (a soma de verificação não confere); nada foi alterado.",
                &[&plan.asset.name],
            ),
        ));
    }
    let path = match final_path {
        Some(f) => {
            if let Err(e) = std::fs::rename(&part, &f) {
                cleanup_all(&part);
                return Err(err(Kind::Io, format!("{}: {e}", f.display())));
            }
            f
        }
        None => part,
    };
    Ok(Staged {
        path,
        name: plan.asset.name.clone(),
        kind: plan.kind,
        cleanup,
    })
}

// ---- applying -------------------------------------------------------------------------------

/// How to start the new version once this process has gone.
#[derive(Debug, Clone, PartialEq)]
pub enum Relaunch {
    Exec(PathBuf),
    MacOpen(PathBuf),
}

/// What happened after a successful apply.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// replaced in place: start this to run the new version
    Restart(Relaunch),
    /// an installer was started and replaces the app: quit now
    Handoff,
    /// a file was saved for the user to open (Flatpak bundle, macOS disk image)
    OpenFile(PathBuf),
    /// the file was saved but could not be installed automatically
    Manual { path: PathBuf, message: String },
}

/// Programs and folders `apply` uses, replaceable in tests.
#[derive(Debug, Clone)]
pub struct ApplyCtx {
    pub pkexec: String,
    pub msiexec: String,
    pub downloads: PathBuf,
    /// where a `.deb` install relaunches the app from
    pub exe: PathBuf,
}

impl ApplyCtx {
    pub fn current(downloads: PathBuf, exe: PathBuf) -> ApplyCtx {
        ApplyCtx {
            pkexec: "pkexec".into(),
            msiexec: "msiexec".into(),
            downloads,
            exe,
        }
    }
}

/// Move `new` over `target` atomically (a running program can be replaced this way on Unix).
fn replace_file(new: &Path, target: &Path) -> Res<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| err(Kind::Io, format!("{}: {e}", new.display())))?;
    }
    std::fs::rename(new, target).map_err(|e| {
        err(
            Kind::Install,
            trf(
                "Não foi possível substituir {0}: {1}",
                &[&target.display(), &e],
            ),
        )
    })
}

/// Swap `new_app` in for `bundle`, keeping the old bundle until the new one is in place.
fn swap_bundle(new_app: &Path, bundle: &Path) -> Res<()> {
    let parent = bundle
        .parent()
        .ok_or_else(|| err(Kind::Io, tr("Caminho do aplicativo inválido.")))?;
    let old = parent.join(format!(".whatrust-old-{}.app", std::process::id()));
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(bundle, &old).map_err(|e| {
        err(
            Kind::Install,
            trf(
                "Não foi possível substituir {0}: {1}",
                &[&bundle.display(), &e],
            ),
        )
    })?;
    if let Err(e) = std::fs::rename(new_app, bundle) {
        // Put the old one back: the user must never be left without an app.
        let back = std::fs::rename(&old, bundle);
        return Err(err(
            Kind::Install,
            trf(
                "Não foi possível instalar a nova versão: {0}{1}",
                &[
                    &e,
                    &if back.is_err() {
                        format!(" ({})", old.display())
                    } else {
                        String::new()
                    },
                ],
            ),
        ));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

fn apply_mac_tar(staged: &Staged, bundle: &Path) -> Res<Outcome> {
    let parent = bundle
        .parent()
        .ok_or_else(|| err(Kind::Io, tr("Caminho do aplicativo inválido.")))?;
    let work = parent.join(format!(".whatrust-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)
        .map_err(|e| err(Kind::Io, format!("{}: {e}", work.display())))?;
    let result = (|| {
        let st = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&staged.path)
            .arg("-C")
            .arg(&work)
            .stdin(std::process::Stdio::null())
            .status()
            .map_err(|e| err(Kind::Install, format!("tar: {e}")))?;
        if !st.success() {
            return Err(err(
                Kind::Install,
                tr("Não foi possível descompactar a atualização."),
            ));
        }
        let new_app = std::fs::read_dir(&work)
            .map_err(|e| err(Kind::Io, e.to_string()))?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.is_dir() && p.extension().is_some_and(|x| x == "app"))
            .ok_or_else(|| err(Kind::Install, tr("A atualização não contém um aplicativo.")))?;
        if !new_app.join("Contents/Info.plist").is_file()
            || !new_app.join("Contents/MacOS").is_dir()
        {
            return Err(err(
                Kind::Install,
                tr("A atualização não contém um aplicativo válido."),
            ));
        }
        swap_bundle(&new_app, bundle)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result?;
    // The downloaded copy must open without the "downloaded from the internet" gate.
    let _ = std::process::Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(bundle)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    Ok(Outcome::Restart(Relaunch::MacOpen(bundle.to_path_buf())))
}

fn apply_deb(staged: &Staged, ctx: &ApplyCtx) -> Res<Outcome> {
    let status = std::process::Command::new(&ctx.pkexec)
        .arg("dpkg")
        .arg("-i")
        .arg(&staged.path)
        .stdin(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {
            staged.discard();
            Ok(Outcome::Restart(Relaunch::Exec(ctx.exe.clone())))
        }
        // pkexec: 126 = the user dismissed the prompt, 127 = not authorized.
        Ok(s) if matches!(s.code(), Some(126) | Some(127)) => {
            staged.discard();
            Err(err(Kind::Cancelled, tr("A instalação foi cancelada.")))
        }
        Ok(s) => {
            staged.discard();
            Err(err(
                Kind::Install,
                trf("O dpkg não conseguiu instalar o pacote ({0}).", &[&s]),
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No pkexec: keep the verified package where the user can find it.
            let keep = ctx.downloads.join(&staged.name);
            let _ = std::fs::create_dir_all(&ctx.downloads);
            let path = if std::fs::copy(&staged.path, &keep).is_ok() {
                staged.discard();
                keep
            } else {
                staged.path.clone()
            };
            Ok(Outcome::Manual {
                message: trf(
                    "O pkexec não foi encontrado. O pacote verificado está em {0}; instale com: sudo dpkg -i {0}",
                    &[&path.display()],
                ),
                path,
            })
        }
        Err(e) => {
            staged.discard();
            Err(err(Kind::Install, format!("pkexec: {e}")))
        }
    }
}

/// Apply a verified download. On an `Err` the installed copy is untouched
/// (or, for a macOS swap that failed half way, restored).
pub fn apply(method: &Method, staged: &Staged, ctx: &ApplyCtx) -> Res<Outcome> {
    match (method, staged.kind) {
        (Method::AppImage(target), AssetKind::AppImage) => {
            replace_file(&staged.path, target).inspect_err(|_| staged.discard())?;
            Ok(Outcome::Restart(Relaunch::Exec(target.clone())))
        }
        (Method::MacApp(bundle), AssetKind::MacTar) => {
            let r = apply_mac_tar(staged, bundle);
            staged.discard();
            r
        }
        (_, AssetKind::MacDmg | AssetKind::Flatpak) => Ok(Outcome::OpenFile(staged.path.clone())),
        (Method::Deb, AssetKind::Deb) => apply_deb(staged, ctx),
        (Method::WindowsNsis, AssetKind::Nsis) => {
            // /P passive, /UPDATE keep shortcuts and settings (no uninstall page),
            // /R start the new version afterwards (the template adds --show).
            match std::process::Command::new(&staged.path)
                .args(["/P", "/UPDATE", "/R"])
                .spawn()
            {
                Ok(_) => Ok(Outcome::Handoff),
                Err(e) => {
                    staged.discard();
                    Err(err(
                        Kind::Install,
                        trf("Não foi possível iniciar o instalador: {0}", &[&e]),
                    ))
                }
            }
        }
        (Method::WindowsMsi, AssetKind::Msi) => {
            match std::process::Command::new(&ctx.msiexec)
                .arg("/i")
                .arg(&staged.path)
                .args(["/passive", "/norestart"])
                .spawn()
            {
                Ok(_) => Ok(Outcome::Handoff),
                Err(e) => {
                    staged.discard();
                    Err(err(
                        Kind::Install,
                        trf("Não foi possível iniciar o instalador: {0}", &[&e]),
                    ))
                }
            }
        }
        _ => Err(err(Kind::Unsupported, method.how_to())),
    }
}

/// Plan, download, verify and apply in one go (the service runs the steps separately
/// so it can show progress and defer the apply; tests and the e2e use this).
#[cfg(test)]
pub fn install(
    method: &Method,
    release: &Release,
    net: &Net,
    arch: Arch,
    ctx: &ApplyCtx,
    progress: &mut dyn FnMut(u64, u64),
) -> Res<Outcome> {
    let plan = plan(method, release, arch, &ctx.downloads)?;
    let staged = fetch(&plan, release, net, progress)?;
    apply(method, &staged, ctx)
}

/// The command that starts `what` once this process has exited (the
/// single-instance socket must be free by then).
pub fn relaunch_command(what: &Relaunch) -> std::process::Command {
    #[cfg(unix)]
    {
        let mut c = std::process::Command::new("sh");
        c.arg("-c");
        match what {
            Relaunch::Exec(p) => {
                c.arg("sleep 1; exec \"$0\"").arg(p);
            }
            Relaunch::MacOpen(p) => {
                c.arg("sleep 1; exec open \"$0\"").arg(p);
            }
        }
        // The old AppImage mount is about to disappear; the new one sets its own.
        for k in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            c.env_remove(k);
        }
        c
    }
    #[cfg(not(unix))]
    {
        match what {
            Relaunch::Exec(p) | Relaunch::MacOpen(p) => std::process::Command::new(p),
        }
    }
}

pub fn relaunch(what: &Relaunch) -> bool {
    relaunch_command(what)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, assets: &[(&str, &str)]) -> Value {
        json!({
            "tag_name": tag, "name": format!("whatRust {tag}"), "body": "**Fixes**\n- one",
            "html_url": "https://github.com/karem505/whatRust/releases/tag/x",
            "published_at": "2026-10-17T10:00:00Z",
            "assets": assets.iter().map(|(n, u)| json!({"name": n, "browser_download_url": u, "size": 0})).collect::<Vec<_>>()
        })
    }

    /// The asset names of a real release (v0.6.10), for one version.
    fn real_assets(v: &str) -> Vec<(String, String)> {
        [
            format!("whatRust_{v}_x64-setup.exe"),
            format!("whatRust_{v}_x64_en-US.msi"),
            format!("whatRust_{v}_aarch64.dmg"),
            "whatRust_aarch64.app.tar.gz".to_string(),
            format!("whatRust_{v}_amd64.deb"),
            format!("whatRust_{v}_amd64.AppImage"),
            "whatRust_x86_64.flatpak".to_string(),
            "whatRust_aarch64.flatpak".to_string(),
            "SHA256SUMS".to_string(),
        ]
        .into_iter()
        .map(|n| {
            let u = format!("{ASSET_PREFIX}v{v}/{n}");
            (n, u)
        })
        .collect()
    }

    fn real_release(v: &str) -> Release {
        let a = real_assets(v);
        let pairs: Vec<(&str, &str)> = a.iter().map(|(n, u)| (n.as_str(), u.as_str())).collect();
        parse_release(&release_json(&format!("v{v}"), &pairs)).unwrap()
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wr-upd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn no_progress() -> impl FnMut(u64, u64) {
        |_, _| {}
    }

    // ---- releases ----

    #[test]
    fn releases_parse_and_compare() {
        let r = parse_release(&release_json("v9.1.0", &[("SHA256SUMS", "x")])).unwrap();
        assert_eq!(r.version, Version::new(9, 1, 0));
        assert!(r.version > current_version());
        assert_eq!(r.assets.len(), 1);
        assert!(r.notes.contains("Fixes"));
        // 0.6.10 < 0.6.11 < 0.10.0 (numeric, not text, comparison)
        assert!(Version::new(0, 6, 10) < Version::new(0, 6, 11));
        assert!(Version::new(0, 6, 11) < Version::new(0, 10, 0));
        // a plain tag without the v works
        assert_eq!(
            parse_release(&release_json("1.2.3", &[])).unwrap().version,
            Version::new(1, 2, 3)
        );
    }

    #[test]
    fn drafts_prereleases_and_odd_tags_are_skipped() {
        let mut draft = release_json("v9.2.0", &[]);
        draft["draft"] = json!(true);
        assert!(parse_release(&draft).is_none());
        let mut pre = release_json("v9.2.0", &[]);
        pre["prerelease"] = json!(true);
        assert!(parse_release(&pre).is_none());
        assert!(parse_release(&release_json("v9.2.0-rc.1", &[])).is_none());
        assert!(parse_release(&json!({"tag_name": "nightly"})).is_none());
        assert!(parse_release(&json!({})).is_none());
    }

    #[test]
    fn assets_are_picked_by_name_pattern_for_every_platform_and_arch() {
        let r = real_release("1.2.3");
        let pick = |k, a| find_asset(&r, k, a).map(|x| x.name.as_str());
        assert_eq!(
            pick(AssetKind::AppImage, Arch::X64),
            Some("whatRust_1.2.3_amd64.AppImage")
        );
        assert_eq!(
            pick(AssetKind::Deb, Arch::X64),
            Some("whatRust_1.2.3_amd64.deb")
        );
        assert_eq!(
            pick(AssetKind::Nsis, Arch::X64),
            Some("whatRust_1.2.3_x64-setup.exe")
        );
        assert_eq!(
            pick(AssetKind::Msi, Arch::X64),
            Some("whatRust_1.2.3_x64_en-US.msi")
        );
        assert_eq!(
            pick(AssetKind::MacTar, Arch::Arm64),
            Some("whatRust_aarch64.app.tar.gz")
        );
        assert_eq!(
            pick(AssetKind::MacDmg, Arch::Arm64),
            Some("whatRust_1.2.3_aarch64.dmg")
        );
        assert_eq!(
            pick(AssetKind::Flatpak, Arch::X64),
            Some("whatRust_x86_64.flatpak")
        );
        assert_eq!(
            pick(AssetKind::Flatpak, Arch::Arm64),
            Some("whatRust_aarch64.flatpak")
        );
        // there is no build for these combinations: never fall back to another arch
        assert_eq!(pick(AssetKind::AppImage, Arch::Arm64), None);
        assert_eq!(pick(AssetKind::Deb, Arch::Arm64), None);
        assert_eq!(pick(AssetKind::Nsis, Arch::Arm64), None);
        assert_eq!(pick(AssetKind::MacTar, Arch::X64), None);
        assert_eq!(pick(AssetKind::MacDmg, Arch::X64), None);
        assert_eq!(pick(AssetKind::Flatpak, Arch::Other), None);
    }

    #[test]
    fn asset_choice_ignores_order_and_unrelated_files() {
        let mut r = real_release("1.2.3");
        r.assets.reverse();
        r.assets.insert(
            0,
            Asset {
                name: "other_amd64.deb".into(),
                url: "u".into(),
                size: 0,
            },
        );
        r.assets.insert(
            0,
            Asset {
                name: "whatRust_1.2.3_amd64.deb.sig".into(),
                url: "u".into(),
                size: 0,
            },
        );
        assert_eq!(
            find_asset(&r, AssetKind::Deb, Arch::X64).unwrap().name,
            "whatRust_1.2.3_amd64.deb"
        );
    }

    #[test]
    fn checksums_are_found_by_file_name() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!(
            "{a}  whatRust_1.0.0_amd64.AppImage\n{b} *whatRust_x86_64.flatpak\nnot-a-hash  junk\n"
        );
        assert_eq!(
            checksum_for(&sums, "whatRust_1.0.0_amd64.AppImage").as_deref(),
            Some(a.as_str())
        );
        assert_eq!(
            checksum_for(&sums, "whatRust_x86_64.flatpak"),
            Some(b.to_lowercase())
        );
        assert_eq!(checksum_for(&sums, "whatRust_1.0.0_amd64.deb"), None);
        assert_eq!(checksum_for(&sums, "junk"), None);
        // a prefix of a name is not a match
        assert_eq!(checksum_for(&sums, "whatRust_1.0.0_amd64"), None);
    }

    #[test]
    fn real_downloads_must_come_from_the_repository() {
        assert!(asset_url_ok(&format!("{ASSET_PREFIX}v1.0.0/x"), false));
        assert!(!asset_url_ok("https://evil.example/x", false));
        assert!(!asset_url_ok(
            "https://github.com/someone/else/releases/download/v1/x",
            false
        ));
        assert!(asset_url_ok("/tmp/local", true));
    }

    // ---- install type ----

    fn env(os: Os, exe: &str) -> Env {
        Env {
            os,
            exe: exe.into(),
            appimage: None,
            flatpak: false,
            has_dpkg: false,
        }
    }

    #[test]
    fn install_types_are_recognised() {
        let d = tempdir("detect");
        let img = d.join("whatRust.AppImage");
        std::fs::write(&img, b"x").unwrap();

        let mut e = env(Os::Linux, "/tmp/.mount_abc/usr/bin/whatrust");
        e.appimage = Some(img.clone());
        assert_eq!(detect(&e), Method::AppImage(img.clone()));
        // $APPIMAGE pointing at nothing is not trusted
        e.appimage = Some(d.join("missing.AppImage"));
        assert_eq!(detect(&e), Method::Other);
        // Flatpak wins over everything
        let mut e = env(Os::Linux, "/app/bin/whatrust");
        e.flatpak = true;
        assert_eq!(detect(&e), Method::Flatpak);
        // .deb: under /usr with dpkg; otherwise a package we cannot drive
        let mut e = env(Os::Linux, "/usr/bin/whatrust");
        e.has_dpkg = true;
        assert_eq!(detect(&e), Method::Deb);
        e.has_dpkg = false;
        assert_eq!(detect(&e), Method::Other);
        // source builds
        for p in [
            "/home/u/whatRust/src-tauri/target/debug/whatrust",
            "/home/u/whatRust/src-tauri/target/release/whatrust",
            "/home/u/.cache/t/target/x86_64-unknown-linux-gnu/release/whatrust",
        ] {
            assert_eq!(detect(&env(Os::Linux, p)), Method::Source, "{p}");
        }
        assert_eq!(
            detect(&env(Os::Linux, "/home/u/target-practice/bin/whatrust")),
            Method::Other
        );
    }

    #[test]
    fn windows_installs_are_told_apart() {
        let d = tempdir("detect-win");
        let nsis_dir = d.join("whatRust");
        std::fs::create_dir_all(&nsis_dir).unwrap();
        std::fs::write(nsis_dir.join("uninstall.exe"), b"x").unwrap();
        let exe = nsis_dir.join("whatrust.exe");
        assert_eq!(
            detect(&env(Os::Windows, &exe.to_string_lossy())),
            Method::WindowsNsis
        );
        assert_eq!(
            detect(&env(Os::Windows, r"C:\Program Files\whatRust\whatrust.exe")),
            Method::WindowsMsi
        );
        assert_eq!(
            detect(&env(Os::Windows, r"C:\Users\me\Downloads\whatrust.exe")),
            Method::Other
        );
        assert_eq!(
            detect(&env(
                Os::Windows,
                r"C:\src\whatRust\src-tauri\target\debug\whatrust.exe"
            )),
            Method::Source
        );
    }

    #[test]
    fn mac_bundles_are_found_from_the_executable() {
        assert_eq!(
            detect(&env(
                Os::Macos,
                "/Applications/whatRust.app/Contents/MacOS/whatrust"
            )),
            Method::MacApp("/Applications/whatRust.app".into())
        );
        assert_eq!(
            detect(&env(
                Os::Macos,
                "/Users/me/Apps/My.app/Contents/MacOS/whatrust"
            )),
            Method::MacApp("/Users/me/Apps/My.app".into())
        );
        assert_eq!(
            detect(&env(
                Os::Macos,
                "/Users/me/whatRust/src-tauri/target/debug/whatrust"
            )),
            Method::Source
        );
        assert_eq!(
            detect(&env(Os::Macos, "/usr/local/bin/whatrust")),
            Method::Other
        );
    }

    #[test]
    fn every_install_type_maps_to_one_asset_kind() {
        let k = |m: Method, w| m.asset_kind(w);
        assert_eq!(
            k(Method::AppImage("/a".into()), true),
            Some(AssetKind::AppImage)
        );
        assert_eq!(k(Method::Deb, true), Some(AssetKind::Deb));
        assert_eq!(k(Method::Flatpak, true), Some(AssetKind::Flatpak));
        assert_eq!(k(Method::WindowsNsis, true), Some(AssetKind::Nsis));
        assert_eq!(k(Method::WindowsMsi, true), Some(AssetKind::Msi));
        assert_eq!(
            k(Method::MacApp("/A.app".into()), true),
            Some(AssetKind::MacTar)
        );
        assert_eq!(
            k(Method::MacApp("/A.app".into()), false),
            Some(AssetKind::MacDmg)
        );
        assert_eq!(k(Method::Source, true), None);
        assert_eq!(k(Method::Other, true), None);
        // never automatic where a password is needed
        assert!(!Method::Deb.auto_install_default() && Method::Deb.needs_password());
        assert!(!Method::Flatpak.auto_install_default());
        assert!(Method::AppImage("/a".into()).auto_install_default());
    }

    #[test]
    fn plan_picks_destinations() {
        let r = real_release("1.2.3");
        let dl = PathBuf::from("/dl");
        let d = tempdir("plan");
        let img = d.join("w.AppImage");
        let p = plan(&Method::AppImage(img.clone()), &r, Arch::X64, &dl).unwrap();
        assert_eq!(p.dest, Dest::Beside(d.clone()));
        assert_eq!(p.asset.name, "whatRust_1.2.3_amd64.AppImage");
        assert_eq!(
            plan(&Method::Deb, &r, Arch::X64, &dl).unwrap().dest,
            Dest::Private
        );
        assert_eq!(
            plan(&Method::Flatpak, &r, Arch::X64, &dl).unwrap().dest,
            Dest::Downloads(dl.clone())
        );
        assert_eq!(
            plan(&Method::WindowsNsis, &r, Arch::X64, &dl).unwrap().dest,
            Dest::Private
        );
        // writable bundle: tarball swap; unwritable: the disk image goes to Downloads
        let bundle = d.join("whatRust.app");
        let p = plan(&Method::MacApp(bundle), &r, Arch::Arm64, &dl).unwrap();
        assert_eq!(
            (p.kind, p.dest),
            (AssetKind::MacTar, Dest::Beside(d.clone()))
        );
        let p = plan(
            &Method::MacApp("/nonexistent-dir/whatRust.app".into()),
            &r,
            Arch::Arm64,
            &dl,
        )
        .unwrap();
        assert_eq!((p.kind, p.dest), (AssetKind::MacDmg, Dest::Downloads(dl)));
        assert_eq!(
            plan(&Method::Source, &r, Arch::X64, Path::new("/dl"))
                .unwrap_err()
                .kind,
            Kind::Unsupported
        );
        assert_eq!(
            plan(&Method::Deb, &r, Arch::Arm64, Path::new("/dl"))
                .unwrap_err()
                .kind,
            Kind::NoAsset
        );
    }

    // ---- feed override ----

    /// Env vars are process-wide: tests that set them take this lock.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
        L.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn the_feed_override_reads_a_local_release_file() {
        let _g = env_lock();
        let d = tempdir("feed");
        let feed_file = d.join("release.json");
        std::fs::write(
            &feed_file,
            release_json(
                "v9.9.9",
                &[
                    ("whatRust_9.9.9_amd64.AppImage", "/local/path"),
                    ("SHA256SUMS", "/local/sums"),
                ],
            )
            .to_string(),
        )
        .unwrap();
        std::env::set_var(FEED_ENV, &feed_file);
        let (url, overridden) = feed();
        let rel = fetch_latest(&Net::default());
        std::env::remove_var(FEED_ENV);
        assert!(overridden);
        assert_eq!(url, feed_file.to_string_lossy());
        let rel = rel.unwrap();
        assert_eq!(rel.version, Version::new(9, 9, 9));
        assert_eq!(
            rel.asset("whatRust_9.9.9_amd64.AppImage").unwrap().url,
            "/local/path"
        );
        // without the override the real GitHub endpoint is used
        let (url, overridden) = feed();
        assert!(!overridden);
        assert_eq!(
            url,
            "https://api.github.com/repos/karem505/whatRust/releases/latest"
        );
    }

    // ---- AppImage ----

    struct Fixture {
        dir: PathBuf,
        target: PathBuf,
        name: String,
        sums: PathBuf,
        release: Release,
    }

    fn appimage_fixture(tag: &str, new_bytes: &[u8]) -> Fixture {
        let _ = tag;
        let dir = tempdir(tag);
        let target = dir.join("whatRust.AppImage");
        std::fs::write(&target, b"old").unwrap();
        let new = dir.join("dl-new.AppImage");
        std::fs::write(&new, new_bytes).unwrap();
        let name = "whatRust_9.0.0_amd64.AppImage".to_string();
        let sums = dir.join("SHA256SUMS");
        let release = parse_release(&release_json(
            "v9.0.0",
            &[
                (&name, &new.display().to_string()),
                ("SHA256SUMS", &sums.display().to_string()),
            ],
        ))
        .unwrap();
        Fixture {
            dir,
            target,
            name,
            sums,
            release,
        }
    }

    fn ctx(dir: &Path) -> ApplyCtx {
        ApplyCtx {
            pkexec: "pkexec".into(),
            msiexec: "msiexec".into(),
            downloads: dir.join("Downloads"),
            exe: dir.join("whatrust"),
        }
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".whatrust"))
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_is_replaced_only_when_the_checksum_matches() {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let f = appimage_fixture("appimage", b"new build");
        let m = Method::AppImage(f.target.clone());

        // wrong checksum: refused, nothing replaced, nothing left behind
        std::fs::write(&f.sums, format!("{}  {}\n", sha(b"something else"), f.name)).unwrap();
        let e = install(
            &m,
            &f.release,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut no_progress(),
        )
        .unwrap_err();
        assert_eq!(e.kind, Kind::Checksum, "{e}");
        assert_eq!(std::fs::read(&f.target).unwrap(), b"old");
        assert!(leftovers(&f.dir).is_empty(), "{:?}", leftovers(&f.dir));

        // right checksum: replaced, executable, no temp files, restart target
        std::fs::write(&f.sums, format!("{}  {}\n", sha(b"new build"), f.name)).unwrap();
        let mut seen = 0;
        let out = install(
            &m,
            &f.release,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut |got, _| seen = got,
        )
        .unwrap();
        std::env::remove_var(FEED_ENV);
        assert_eq!(out, Outcome::Restart(Relaunch::Exec(f.target.clone())));
        assert_eq!(std::fs::read(&f.target).unwrap(), b"new build");
        assert_eq!(seen, 9);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&f.target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(leftovers(&f.dir).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_sums_file_or_entry_aborts_the_update() {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let f = appimage_fixture("nosums", b"new build");
        let m = Method::AppImage(f.target.clone());
        // SHA256SUMS listed in the release but without our file
        std::fs::write(&f.sums, format!("{}  other.file\n", sha(b"new build"))).unwrap();
        let e = install(
            &m,
            &f.release,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut no_progress(),
        )
        .unwrap_err();
        assert_eq!(e.kind, Kind::Checksum);
        // a release without any SHA256SUMS asset
        let mut rel = f.release.clone();
        rel.assets.retain(|a| a.name != "SHA256SUMS");
        let e = install(
            &m,
            &rel,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut no_progress(),
        )
        .unwrap_err();
        std::env::remove_var(FEED_ENV);
        assert_eq!(e.kind, Kind::Checksum);
        assert_eq!(std::fs::read(&f.target).unwrap(), b"old");
        assert!(leftovers(&f.dir).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_download_leaves_no_temp_file_and_the_old_copy() {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let mut f = appimage_fixture("faildl", b"new build");
        std::fs::write(&f.sums, format!("{}  {}\n", sha(b"new build"), f.name)).unwrap();
        for a in &mut f.release.assets {
            if a.name == f.name {
                a.url = f.dir.join("does-not-exist").display().to_string();
            }
        }
        let m = Method::AppImage(f.target.clone());
        let e = install(
            &m,
            &f.release,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut no_progress(),
        )
        .unwrap_err();
        std::env::remove_var(FEED_ENV);
        assert_eq!(e.kind, Kind::Io);
        assert_eq!(std::fs::read(&f.target).unwrap(), b"old");
        assert!(leftovers(&f.dir).is_empty());
    }

    #[test]
    fn without_a_feed_override_a_foreign_asset_url_is_refused() {
        let _g = env_lock();
        std::env::remove_var(FEED_ENV);
        let f = appimage_fixture("foreign", b"new build");
        std::fs::write(&f.sums, format!("{}  {}\n", sha(b"new build"), f.name)).unwrap();
        let m = Method::AppImage(f.target.clone());
        let e = install(
            &m,
            &f.release,
            &Net::default(),
            Arch::X64,
            &ctx(&f.dir),
            &mut no_progress(),
        )
        .unwrap_err();
        assert_eq!(e.kind, Kind::Unsupported);
        assert_eq!(std::fs::read(&f.target).unwrap(), b"old");
    }

    // ---- Private downloads (installers) ----

    #[cfg(unix)]
    #[test]
    fn a_bad_installer_checksum_removes_the_private_folder() {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let d = tempdir("private");
        let name = "whatRust_9.0.1_x64-setup.exe";
        let setup = d.join("src-setup.exe");
        std::fs::write(&setup, b"installer").unwrap();
        let sums = d.join("SHA256SUMS");
        std::fs::write(&sums, format!("{}  {name}\n", sha(b"tampered"))).unwrap();
        let rel = parse_release(&release_json(
            "v9.0.1",
            &[
                (name, &setup.display().to_string()),
                ("SHA256SUMS", &sums.display().to_string()),
            ],
        ))
        .unwrap();
        let e = install(
            &Method::WindowsNsis,
            &rel,
            &Net::default(),
            Arch::X64,
            &ctx(&d),
            &mut no_progress(),
        )
        .unwrap_err();
        std::env::remove_var(FEED_ENV);
        assert_eq!(e.kind, Kind::Checksum);
        let left: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("whatrust-update-")
            })
            .filter(|e| e.path().join(name).exists())
            .collect();
        assert!(left.is_empty(), "the private download folder must be gone");
    }

    #[cfg(unix)]
    fn script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Download a "setup" through the whole pipeline, with `SHA256SUMS` matching it.
    #[cfg(unix)]
    fn staged_installer(tag: &str, name: &str, kind: AssetKind, body: &str) -> (PathBuf, Staged) {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let d = tempdir(tag);
        let src = d.join("src.bin");
        script(&src, body);
        let sums = d.join("SHA256SUMS");
        std::fs::write(
            &sums,
            format!("{}  {name}\n", sha(&std::fs::read(&src).unwrap())),
        )
        .unwrap();
        let rel = parse_release(&release_json(
            "v9.0.0",
            &[
                (name, &src.display().to_string()),
                ("SHA256SUMS", &sums.display().to_string()),
            ],
        ))
        .unwrap();
        let plan = Plan {
            kind,
            asset: rel.asset(name).unwrap().clone(),
            dest: Dest::Private,
        };
        let staged = fetch(&plan, &rel, &Net::default(), &mut no_progress()).unwrap();
        std::env::remove_var(FEED_ENV);
        // The "installer" is a shell script here, so it needs the exec bit a real .exe does not.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged.path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (d, staged)
    }

    #[cfg(unix)]
    #[test]
    fn the_nsis_setup_is_started_with_the_update_switches() {
        let d0 = tempdir("nsis-log");
        let log = d0.join("args.txt");
        let (d, staged) = staged_installer(
            "nsis",
            "whatRust_9.0.0_x64-setup.exe",
            AssetKind::Nsis,
            &format!("echo \"$@\" > {}", log.display()),
        );
        let out = apply(&Method::WindowsNsis, &staged, &ctx(&d)).unwrap();
        assert_eq!(out, Outcome::Handoff);
        for _ in 0..50 {
            if log.exists()
                && std::fs::metadata(&log)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            std::fs::read_to_string(&log).unwrap().trim(),
            "/P /UPDATE /R"
        );
        staged.discard();
    }

    #[cfg(unix)]
    #[test]
    fn an_installer_that_cannot_start_is_an_error_not_a_quit() {
        let (d, staged) = staged_installer(
            "nsis-bad",
            "whatRust_9.0.0_x64-setup.exe",
            AssetKind::Nsis,
            "exit 0",
        );
        // make it unlaunchable
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let e = apply(&Method::WindowsNsis, &staged, &ctx(&d)).unwrap_err();
        assert_eq!(e.kind, Kind::Install);
        // MSI: msiexec missing
        let (d, staged) = staged_installer(
            "msi-bad",
            "whatRust_9.0.0_x64_en-US.msi",
            AssetKind::Msi,
            "exit 0",
        );
        let mut c = ctx(&d);
        c.msiexec = "/nonexistent/msiexec".into();
        assert_eq!(
            apply(&Method::WindowsMsi, &staged, &c).unwrap_err().kind,
            Kind::Install
        );
    }

    #[cfg(unix)]
    #[test]
    fn msiexec_gets_the_passive_arguments() {
        let (d, staged) = staged_installer(
            "msi",
            "whatRust_9.0.0_x64_en-US.msi",
            AssetKind::Msi,
            "exit 0",
        );
        let log = d.join("msi-args.txt");
        let fake = d.join("fake-msiexec");
        script(&fake, &format!("echo \"$@\" > {}", log.display()));
        let mut c = ctx(&d);
        c.msiexec = fake.display().to_string();
        assert_eq!(
            apply(&Method::WindowsMsi, &staged, &c).unwrap(),
            Outcome::Handoff
        );
        for _ in 0..50 {
            if std::fs::metadata(&log)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let args = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            args.trim(),
            format!("/i {} /passive /norestart", staged.path.display())
        );
        staged.discard();
    }

    #[cfg(unix)]
    #[test]
    fn a_deb_is_installed_through_pkexec_dpkg_and_cleaned_up() {
        let (d, staged) =
            staged_installer("deb", "whatRust_9.0.0_amd64.deb", AssetKind::Deb, "exit 0");
        let log = d.join("deb-args.txt");
        let fake = d.join("fake-pkexec");
        script(&fake, &format!("echo \"$@\" > {}", log.display()));
        let mut c = ctx(&d);
        c.pkexec = fake.display().to_string();
        let out = apply(&Method::Deb, &staged, &c).unwrap();
        assert_eq!(out, Outcome::Restart(Relaunch::Exec(c.exe.clone())));
        assert_eq!(
            std::fs::read_to_string(&log).unwrap().trim(),
            format!("dpkg -i {}", staged.path.display())
        );
        assert!(
            !staged.path.exists(),
            "the package is removed after installing"
        );

        // cancelled prompt: nothing installed, file removed
        let (d, staged) = staged_installer(
            "deb-cancel",
            "whatRust_9.0.0_amd64.deb",
            AssetKind::Deb,
            "exit 0",
        );
        let fake = d.join("fake-pkexec");
        script(&fake, "exit 126");
        let mut c = ctx(&d);
        c.pkexec = fake.display().to_string();
        assert_eq!(
            apply(&Method::Deb, &staged, &c).unwrap_err().kind,
            Kind::Cancelled
        );
        assert!(!staged.path.exists());

        // dpkg failure
        let (d, staged) = staged_installer(
            "deb-fail",
            "whatRust_9.0.0_amd64.deb",
            AssetKind::Deb,
            "exit 0",
        );
        let fake = d.join("fake-pkexec");
        script(&fake, "exit 1");
        let mut c = ctx(&d);
        c.pkexec = fake.display().to_string();
        assert_eq!(
            apply(&Method::Deb, &staged, &c).unwrap_err().kind,
            Kind::Install
        );
    }

    #[cfg(unix)]
    #[test]
    fn without_pkexec_the_verified_deb_is_kept_for_the_user() {
        let (d, staged) = staged_installer(
            "deb-nopk",
            "whatRust_9.0.0_amd64.deb",
            AssetKind::Deb,
            "exit 0",
        );
        let mut c = ctx(&d);
        c.pkexec = "/nonexistent/pkexec".into();
        match apply(&Method::Deb, &staged, &c).unwrap() {
            Outcome::Manual { path, message } => {
                assert_eq!(path, c.downloads.join("whatRust_9.0.0_amd64.deb"));
                assert!(path.is_file());
                assert!(message.contains("dpkg -i"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- macOS ----

    #[cfg(unix)]
    fn fake_app(root: &Path, marker: &str) -> PathBuf {
        let app = root.join("whatRust.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/Info.plist"), b"<plist/>").unwrap();
        std::fs::write(app.join("Contents/MacOS/whatrust"), marker).unwrap();
        app
    }

    #[cfg(unix)]
    fn mac_staged(d: &Path, tar_args_root: &Path) -> Staged {
        let tgz = d.join("whatRust_aarch64.app.tar.gz");
        let st = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&tgz)
            .arg("-C")
            .arg(tar_args_root)
            .arg("whatRust.app")
            .status()
            .unwrap();
        assert!(st.success());
        Staged {
            path: tgz,
            name: "whatRust_aarch64.app.tar.gz".into(),
            kind: AssetKind::MacTar,
            cleanup: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_mac_bundle_is_swapped_and_the_old_one_removed() {
        let d = tempdir("mac");
        let apps = d.join("Applications");
        std::fs::create_dir_all(&apps).unwrap();
        let bundle = fake_app(&apps, "old");
        let src = d.join("src");
        std::fs::create_dir_all(&src).unwrap();
        fake_app(&src, "new");
        let staged = mac_staged(&d, &src);
        let out = apply(&Method::MacApp(bundle.clone()), &staged, &ctx(&d)).unwrap();
        assert_eq!(out, Outcome::Restart(Relaunch::MacOpen(bundle.clone())));
        assert_eq!(
            std::fs::read(bundle.join("Contents/MacOS/whatrust")).unwrap(),
            b"new"
        );
        let names: Vec<String> = std::fs::read_dir(&apps)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["whatRust.app"],
            "no old bundle or work folder left: {names:?}"
        );
        assert!(!staged.path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_mac_archive_keeps_the_current_bundle() {
        let d = tempdir("mac-bad");
        let apps = d.join("Applications");
        std::fs::create_dir_all(&apps).unwrap();
        let bundle = fake_app(&apps, "old");
        // an archive whose app has no Info.plist
        let src = d.join("src");
        let broken = src.join("whatRust.app/Contents/MacOS");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("whatrust"), "new").unwrap();
        let staged = mac_staged(&d, &src);
        let e = apply(&Method::MacApp(bundle.clone()), &staged, &ctx(&d)).unwrap_err();
        assert_eq!(e.kind, Kind::Install);
        assert_eq!(
            std::fs::read(bundle.join("Contents/MacOS/whatrust")).unwrap(),
            b"old"
        );
        // not an archive at all
        std::fs::write(&staged.path, b"garbage").unwrap();
        let staged = Staged {
            path: staged.path.clone(),
            ..staged
        };
        assert_eq!(
            apply(&Method::MacApp(bundle.clone()), &staged, &ctx(&d))
                .unwrap_err()
                .kind,
            Kind::Install
        );
        assert_eq!(
            std::fs::read(bundle.join("Contents/MacOS/whatrust")).unwrap(),
            b"old"
        );
        let names: Vec<String> = std::fs::read_dir(&apps)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["whatRust.app"], "{names:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_second_rename_puts_the_old_bundle_back() {
        let d = tempdir("mac-rollback");
        let bundle = fake_app(&d, "old");
        // the "new" app does not exist: the second rename fails after the first moved the old one
        let e = swap_bundle(&d.join("missing.app"), &bundle).unwrap_err();
        assert_eq!(e.kind, Kind::Install);
        assert_eq!(
            std::fs::read(bundle.join("Contents/MacOS/whatrust")).unwrap(),
            b"old"
        );
    }

    #[test]
    fn flatpak_and_dmg_downloads_are_handed_back_to_be_opened() {
        let staged = Staged {
            path: "/dl/whatRust_x86_64.flatpak".into(),
            name: "n".into(),
            kind: AssetKind::Flatpak,
            cleanup: None,
        };
        assert_eq!(
            apply(&Method::Flatpak, &staged, &ctx(Path::new("/tmp"))).unwrap(),
            Outcome::OpenFile("/dl/whatRust_x86_64.flatpak".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn flatpak_bundles_are_verified_and_saved_in_downloads() {
        let _g = env_lock();
        std::env::set_var(FEED_ENV, "/x");
        let d = tempdir("flatpak");
        let src = d.join("src.flatpak");
        std::fs::write(&src, b"bundle").unwrap();
        let sums = d.join("SHA256SUMS");
        let name = "whatRust_x86_64.flatpak";
        std::fs::write(&sums, format!("{}  {name}\n", sha(b"bundle"))).unwrap();
        let rel = parse_release(&release_json(
            "v9.0.0",
            &[
                (name, &src.display().to_string()),
                ("SHA256SUMS", &sums.display().to_string()),
            ],
        ))
        .unwrap();
        let downloads = d.join("Downloads");
        let out = install(
            &Method::Flatpak,
            &rel,
            &Net::default(),
            Arch::X64,
            &ctx(&d),
            &mut no_progress(),
        )
        .unwrap();
        assert_eq!(out, Outcome::OpenFile(downloads.join(name)));
        assert_eq!(std::fs::read(downloads.join(name)).unwrap(), b"bundle");
        assert!(
            leftovers(&downloads).is_empty() && std::fs::read_dir(&downloads).unwrap().count() == 1
        );
        // tampered: refused, nothing left in Downloads
        let _ = std::fs::remove_file(downloads.join(name));
        std::fs::write(&sums, format!("{}  {name}\n", sha(b"other"))).unwrap();
        let e = install(
            &Method::Flatpak,
            &rel,
            &Net::default(),
            Arch::X64,
            &ctx(&d),
            &mut no_progress(),
        )
        .unwrap_err();
        std::env::remove_var(FEED_ENV);
        assert_eq!(e.kind, Kind::Checksum);
        assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn relaunch_waits_for_the_single_instance_socket() {
        let c = relaunch_command(&Relaunch::Exec("/x/whatRust.AppImage".into()));
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(c.get_program(), "sh");
        assert_eq!(
            args,
            vec!["-c", "sleep 1; exec \"$0\"", "/x/whatRust.AppImage"]
        );
        let c = relaunch_command(&Relaunch::MacOpen("/Applications/whatRust.app".into()));
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[1], "sleep 1; exec open \"$0\"");
    }

    // ---- proxy ----

    fn settings_with_proxy(mode: &str, host: &str, port: u32) -> crate::settings::Settings {
        crate::settings::Settings {
            proxy_mode: mode.into(),
            proxy_host: host.into(),
            proxy_port: port,
            ..Default::default()
        }
    }

    #[test]
    fn the_proxy_setting_is_what_the_updater_uses() {
        assert_eq!(
            Net::from_settings(&settings_with_proxy("off", "", 0))
                .unwrap()
                .proxy,
            None
        );
        assert_eq!(
            Net::from_settings(&settings_with_proxy("socks5", "127.0.0.1", 1080))
                .unwrap()
                .proxy
                .as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
        assert_eq!(
            Net::from_settings(&settings_with_proxy("http", "Proxy.Example.com", 8080))
                .unwrap()
                .proxy
                .as_deref(),
            Some("http://proxy.example.com:8080")
        );
        // a saved proxy that no longer validates never turns into a direct connection
        assert_eq!(
            Net::from_settings(&settings_with_proxy("http", "bad host", 8080))
                .unwrap_err()
                .kind,
            Kind::Network
        );
        assert_eq!(
            Net::from_settings(&settings_with_proxy("socks5", "h", 0))
                .unwrap_err()
                .kind,
            Kind::Network
        );
    }

    /// Read from `s` until the end of an HTTP request head.
    fn read_head(s: &mut std::net::TcpStream) -> String {
        let mut seen = Vec::new();
        let mut buf = [0u8; 1024];
        while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&seen).into_owned()
    }

    /// A one-shot fake HTTP proxy. It accepts a `CONNECT` tunnel (what a client
    /// opens through an HTTP proxy), answers 200, then serves `body` to the
    /// request sent inside the tunnel. Both request heads are reported.
    fn fake_http_proxy(
        body: &'static str,
        tunnel: bool,
    ) -> (u16, std::sync::mpsc::Receiver<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut heads = vec![read_head(&mut s)];
            if !tunnel {
                let _ = s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
                tx.send(heads).unwrap();
                return;
            }
            let _ = s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
            heads.push(read_head(&mut s));
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = s.write_all(resp.as_bytes());
            tx.send(heads).unwrap();
        });
        (port, rx)
    }

    #[test]
    fn requests_go_through_the_configured_http_proxy_with_a_user_agent() {
        let _g = env_lock();
        let body = release_json("v9.9.9", &[]).to_string();
        let (port, rx) = fake_http_proxy(Box::leak(body.into_boxed_str()), true);
        // the "GitHub" host does not exist: only the proxy can answer for it
        std::env::set_var(FEED_ENV, "http://github.invalid/repos/x/releases/latest");
        let net = Net {
            proxy: Some(format!("http://127.0.0.1:{port}")),
        };
        let rel = fetch_latest(&net);
        std::env::remove_var(FEED_ENV);
        let heads = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the proxy saw a request");
        assert!(
            heads[0].starts_with("CONNECT github.invalid:80 "),
            "{heads:?}"
        );
        let inner = heads[1].to_ascii_lowercase();
        assert!(
            inner.starts_with("get /repos/x/releases/latest "),
            "{inner}"
        );
        assert!(inner.contains("user-agent: whatrust/"), "{inner}");
        assert!(
            inner.contains("accept: application/vnd.github+json"),
            "{inner}"
        );
        assert_eq!(rel.unwrap().version, Version::new(9, 9, 9));
    }

    #[test]
    fn https_requests_ask_the_proxy_to_connect_and_never_go_direct() {
        let _g = env_lock();
        let (port, rx) = fake_http_proxy("", false);
        std::env::set_var(
            FEED_ENV,
            "https://api.github.com/repos/karem505/whatRust/releases/latest",
        );
        let net = Net {
            proxy: Some(format!("http://127.0.0.1:{port}")),
        };
        let r = fetch_latest(&net);
        std::env::remove_var(FEED_ENV);
        let heads = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the proxy saw a request");
        assert!(
            heads[0].starts_with("CONNECT api.github.com:443 "),
            "{heads:?}"
        );
        assert_eq!(r.unwrap_err().kind, Kind::Network);
    }

    // ---- the real thing, opt-in ----

    /// Real download (network) through a feed file:
    /// `WR_E2E_FEED=<release json> cargo test --locked updater::tests::e2e -- --ignored --nocapture`
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn e2e_download_and_replace_from_a_feed() {
        let Ok(feed_file) = std::env::var("WR_E2E_FEED") else {
            return;
        };
        let _g = env_lock();
        std::env::set_var(FEED_ENV, feed_file);
        let rel = fetch_latest(&Net::default()).unwrap();
        let d = tempdir("e2e");
        let target = d.join("whatRust.AppImage");
        std::fs::write(&target, b"old").unwrap();
        let t = std::time::Instant::now();
        let mut last = 0;
        let out = install(
            &Method::AppImage(target.clone()),
            &rel,
            &Net::default(),
            Arch::X64,
            &ctx(&d),
            &mut |got, _| last = got,
        )
        .unwrap();
        assert_eq!(out, Outcome::Restart(Relaunch::Exec(target.clone())));
        let len = std::fs::metadata(&target).unwrap().len();
        assert_eq!(len, last);
        eprintln!(
            "downloaded, verified and replaced {len} bytes in {:?}",
            t.elapsed()
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
