# Contributing to whatRust

Thanks for helping. Bug reports, fixes and translations are all welcome. This page covers how to build whatRust, which checks a pull request has to pass, and how to send one.

## Reporting a bug

Open an [issue](https://github.com/karem505/whatRust/issues) with:

- the whatRust version (Settings → About whatRust) and how you installed it (Flatpak, AppImage, `.deb`/`.rpm`, `.exe`/`.msi`, `.dmg`, or built from source);
- your OS and desktop (for example Ubuntu 24.04 GNOME on Wayland, Windows 11 24H2);
- the steps that show the problem;
- the relevant lines of the log. The log never contains message text or file names:
  - Linux (Flatpak): `~/.var/app/io.github.karem505.whatRust/data/whatRust/logs/whatrust.log`
  - Linux (other installs) and macOS: `~/.local/share/whatRust/logs/whatrust.log` (or under `$XDG_DATA_HOME` when it is set)
  - Windows: `%LOCALAPPDATA%\whatRust\logs\whatrust.log`

Screenshots help, but crop out chats, names and phone numbers.

## Building

You need Rust (stable), the Tauri CLI and, on Linux, the WebKitGTK development packages.

```bash
# Linux build dependencies (Ubuntu/Debian), the same list CI installs
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libdbus-1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libhunspell-dev patchelf

cargo install tauri-cli --version "^2.0" --locked

cargo tauri dev      # run in development
cargo tauri build    # build installers for the current OS
```

Windows needs the Visual Studio C++ build tools and the WebView2 runtime (preinstalled on Windows 11). macOS needs the Xcode command line tools.

Code layout:

- `src-tauri/src/`: the Rust app (windows, accounts, notifications, downloads, settings).
- `src-tauri/resources/*.js`: scripts injected into WhatsApp Web (`bridge.js` for notifications, the unread count and file drops, `media.js` for WebKitGTK media fixes, `integration.js` for the in-page controls).
- `settings-ui/`: the Settings window.
- `flatpak/` and `io.github.karem505.whatRust.yml`: the Flatpak build.

## Checks every pull request must pass

`.github/workflows/check.yml` runs these on Linux, Windows and macOS. Run them locally before you push:

```bash
cd src-tauri
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
cd ..

# JavaScript tests (Node 20+, no dependencies)
node src-tauri/resources/bridge.test.mjs
node settings-ui/comboToAccelerator.test.mjs
node settings-ui/dialog.test.mjs
node settings-ui/i18n.test.mjs
node src-tauri/resources/notifications.test.mjs
node --test src-tauri/resources/privacy.test.mjs
node --test src-tauri/resources/upsell.test.mjs
node src-tauri/resources/media.test.mjs
node settings-ui/notes.test.mjs
```

Things that often fail only on the other operating systems:

- `clippy -D warnings` runs on all three systems. Code used only under `#[cfg(windows)]` or `#[cfg(target_os = "macos")]` is dead code elsewhere, so gate it with the same `cfg`.
- Every user-visible string needs an English entry. Rust strings go through `tr("...")` with their pair in `src-tauri/src/i18n.rs`; Settings strings go in `settings-ui/i18n.js`. A test fails when one is missing.
- A new Tauri command must be listed in `src-tauri/build.rs`, granted in the right file under `src-tauri/capabilities/`, and added to `src-tauri/src/ipc_acl_tests.rs`. WhatsApp's page (`main-remote`) must only get what it really needs.
- If `Cargo.lock` gains a new crate, regenerate `flatpak/cargo-sources.json` (see `flatpak/README.md`); the Flatpak build is offline and fails on a missing crate.

Pull requests from forks run CI after a maintainer approves the run.

## Pull requests

- Branch from `master`, one fix or feature per pull request.
- Describe what changed, why, and how you tested it, including on which OS. Say what you could not test (for example "not tried on macOS").
- New behaviour that changes what existing users see (a new setting, a new shortcut, anything that alters the page) should start off, or keep today's behaviour by default.
- Add or extend a test for the logic you change.
- Write `Refs #123` in the description; the issue is closed when the release that contains the fix is out.

## Translations

The interface is in Portuguese (the source strings) and English. To add a language, open an issue first so the language list and the fallback rules can be agreed on.

## License

By contributing you agree that your contribution is licensed under the project's [MIT License](LICENSE).
