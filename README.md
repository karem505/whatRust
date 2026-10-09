<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="whatRust app icon — a lightweight WhatsApp Web desktop client">
</p>

# whatRust — Lightweight WhatsApp Web Desktop Client (Rust + Tauri)

**whatRust is a free, open-source, lightweight desktop client for WhatsApp Web that runs on Linux, Windows, and macOS — a lean, native alternative to the official Electron-based WhatsApp Desktop app, built with Rust and Tauri v2. It runs multiple WhatsApp accounts at once, each in its own window with a fully isolated login.**

![Latest release](https://img.shields.io/github/v/release/karem505/whatRust?label=release)
![License: MIT](https://img.shields.io/github/license/karem505/whatRust)
![Platforms: Linux, Windows, macOS](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows%20%7C%20macOS-informational)
![Built with Rust and Tauri v2](https://img.shields.io/badge/built%20with-Rust%20%2B%20Tauri%20v2-orange)
![GitHub stars](https://img.shields.io/github/stars/karem505/whatRust?style=social)

> **Unofficial, independent project** — not affiliated with, endorsed by, or sponsored by WhatsApp or Meta. whatRust simply loads the official `web.whatsapp.com` interface in a native system webview.

## Contents

- [What is whatRust?](#what-is-whatrust)
- [Why whatRust? A lean, native WhatsApp Desktop alternative](#why-whatrust-a-lean-native-whatsapp-desktop-alternative)
- [Features](#features)
- [Run multiple WhatsApp accounts](#run-multiple-whatsapp-accounts)
- [Lock the app (optional)](#lock-the-app-optional)
- [whatRust vs the official WhatsApp Desktop (Electron)](#whatrust-vs-the-official-whatsapp-desktop-electron)
- [Requirements](#requirements)
- [Installation](#installation)
- [Getting started](#getting-started)
- [FAQ](#faq)
- [Limitations](#limitations)
- [Contributing](#contributing)
- [Disclaimer](#disclaimer)
- [License](#license)

## What is whatRust?

whatRust is an open-source **WhatsApp Web desktop client** for Linux, Windows, and macOS. It wraps the official `web.whatsapp.com` in your operating system's native webview and adds the desktop conveniences a browser tab can't — a system tray, native notifications, persistent login, global shortcuts, and microphone/camera access for voice messages and calls.

It is an **unofficial, open-source WhatsApp client** and a practical **WhatsApp Desktop alternative** for people who want a fast, native app with minimal overhead instead of the heavier Electron-based official build. It is not affiliated with WhatsApp or Meta.

## Why whatRust? A lean, native WhatsApp Desktop alternative

**whatRust's native app shell is small — typically around 90 MB** — because it reuses the webview that already ships with your OS instead of bundling its own browser engine the way Electron apps do. That gives it a much lighter baseline than the official Electron-based WhatsApp Desktop, which ships an entire Chromium runtime on top of the same WhatsApp Web page.

The official WhatsApp Desktop app is built on Electron, which packs an entire Chromium browser inside every app. whatRust instead renders WhatsApp Web through the OS-native webview — **WebKitGTK** on Linux, **WebView2** on Windows, and **WKWebView** on macOS — via [Tauri v2](https://tauri.app). The result is a fast, native WhatsApp desktop app with a small footprint of its own.

> **A fair caveat on total memory:** your overall RAM use is dominated by **WhatsApp Web itself** and grows with how many chats, groups, and media you keep open — commonly a few hundred MB up to ~1 GB for busy accounts. That cost is roughly the same in any browser-based client (whatRust, the official app, or a plain Chrome tab); whatRust's advantage is the lean native shell, not lighter web content.

## Features

- **Multiple WhatsApp accounts** — run several numbers at once, each in its own window with a fully isolated login (add, rename, and remove accounts)
- **Optional app lock** — password (Argon2id) or biometric (Windows Hello / Touch ID / Linux polkit); locks on launch, on demand, on hide-to-tray, or after idle
- **System tray** icon with **close-to-tray** and an **unread message badge**
- **Native OS notifications** for new messages
- **Display size** — a Smaller / Small / Default / Big page zoom in Settings, so the chat list stops crowding out the conversation on a high-DPI screen
- **Persistent login** — scan the QR code once, stay signed in across restarts
- **Voice messages** everywhere, plus **voice & video calls** on macOS (WhatsApp Web doesn't offer calls on Windows, and most Linux distros build WebKitGTK without WebRTC — see the FAQ)
- **Drag and drop files and images** — drop a photo, video, or document straight onto a chat to attach it
- **Links open in your browser** — web, email, and phone links in chats go to your default apps; WhatsApp links (`wa.me`, group invites) stay in the app
- **Opens `whatsapp://` links** — "Open app" / click-to-chat buttons on websites open the chat in whatRust (registered by the installers, the `.deb`, the Flatpak, and the one-line AppImage install)
- **Download feedback** — a small toast confirms each download with **Open** and **Show in folder** buttons (programs and installers are only ever shown in their folder, never run)
- **Launch at startup** (auto-start), optional
- **Global keyboard shortcut** to show/hide the window (default `Ctrl/Cmd+Shift+W`; record your own by pressing the keys in Settings). On **Wayland**, bind `whatrust --toggle` to a system shortcut instead — see the FAQ.
- **Single instance** — relaunching focuses the running window; `whatrust --toggle` from a second launch shows/hides it
- **Remembers window size and position**
- **One-line install** on every platform
- **Cross-platform**: Linux, Windows, and macOS from one Rust + Tauri codebase

## Run multiple WhatsApp accounts

whatRust runs **multiple WhatsApp accounts at the same time** — each account opens in its **own window** with a **completely isolated session** (separate cookies, local storage, and IndexedDB), so you can stay signed in to several numbers at once without them interfering.

- **Add an account** in **Settings → Accounts** with the **+ Add** button, then scan the new QR code; you can **rename** or **remove** accounts there too.
- Each account keeps its **own login, unread badge, and notifications**; the tray shows a **combined unread count** and a one-click switcher for every account.
- Your **first (default) account keeps its existing login** when you upgrade — no need to re-scan.

> **macOS note:** running **multiple accounts requires macOS 14 (Sonoma) or later**, where the system webview supports isolated data stores. On macOS 12–13 whatRust runs a single account. Linux and Windows have no such limit.

## Lock the app (optional)

whatRust can require a password — or a fingerprint / Windows Hello / Touch ID where your
OS supports it — before showing your chats. Enable it under **Settings → Security**.

- **Password** works on every platform (Argon2id, stored locally).
- **Biometric** is an optional shortcut: Windows Hello on Windows, Touch ID on any Mac
  that has it, and the system fingerprint dialog on Linux where polkit/`pam_fprintd` is
  configured (native `.deb` install only; AppImage and Flatpak use the password).
- Lock **on launch**, **on demand** (tray → *Lock now*), **when hidden to the tray**, or
  **after an idle timeout** — each toggleable in Settings.
- Forgot your password? **Reset** from the lock screen logs out all accounts and clears
  the lock (you'll re-scan the QR). There is no backdoor.

> **What the lock does and doesn't do:** it controls who can open whatRust's windows. It
> does **not** encrypt your data on disk — your WhatsApp session stays readable to other
> software running as your user, locked or not (the same posture as Signal Desktop). For
> at-rest protection, use full-disk encryption (FileVault / BitLocker / LUKS).

## whatRust vs the official WhatsApp Desktop (Electron)

| Feature | whatRust | Official WhatsApp Desktop (Electron) |
|---|---|---|
| App-shell memory overhead | Lean native shell (~90 MB), no bundled browser | Heavier — bundles a full Chromium + Node runtime |
| Total RAM (with WhatsApp Web loaded) | Dominated by WhatsApp Web (similar across clients) | Dominated by WhatsApp Web **+** Electron runtime |
| Rendering engine | OS-native webview (WebKitGTK / WebView2 / WKWebView) | Bundled Chromium (Electron) |
| Built with | Rust + Tauri v2 | Electron (Chromium + Node.js) |
| Open source | ✅ Yes | ❌ No |
| Native Linux app | ✅ Yes | ⚠️ Limited |
| Windows / macOS | ✅ Yes | ✅ Yes |
| System tray + close to tray | ✅ Yes | ⚠️ Partial |
| Unread message badge | ✅ Yes | ✅ Yes |
| Native notifications | ✅ Yes | ✅ Yes |
| Voice messages (mic/camera) | ✅ Yes | ✅ Yes |
| Voice & video calls | ⚠️ macOS only (see the FAQ) | ✅ Yes |
| Multiple accounts (isolated sessions) | ✅ Yes | ❌ No |
| Optional app lock (password + biometric) | ✅ Yes | ❌ No |
| Global show/hide shortcut | ✅ Yes | ❌ No |
| Launch at startup | ✅ Yes | ✅ Yes |
| Affiliated with Meta | ❌ No (unofficial) | ✅ Yes |

## Requirements

| OS | Webview engine | Notes |
|---|---|---|
| **Linux** | WebKitGTK | Requires WebKitGTK **≥ 2.46.1** (older versions hang WhatsApp's QR login). AppImage may need `libfuse2`. |
| **Windows 10/11** | WebView2 | Uses the Evergreen WebView2 runtime (preinstalled on Windows 11). |
| **macOS** | WKWebView | macOS 12.1+ (SharedArrayBuffer for video upload needs Safari/WebKit 15.2; running **multiple accounts** needs macOS 14+); the current build is Apple Silicon (arm64). |

## Installation

**Linux Flatpak (x86_64 or ARM64)** — one line:
```bash
curl -fsSL https://raw.githubusercontent.com/karem505/whatRust/master/install-flatpak.sh | sh
```
The Flatpak uses the GNOME runtime from Flathub, while the whatRust bundle itself is
published with each [GitHub release](https://github.com/karem505/whatRust/releases/latest).
It keeps its own sandboxed WhatsApp login, separate from a `.deb` or AppImage install.

**Linux AppImage / macOS** — one line:
```bash
curl -fsSL https://raw.githubusercontent.com/karem505/whatRust/master/install.sh | sh
```
Installs the AppImage to `~/.local/bin` on Linux (with an application-menu entry), or the `.dmg` app into `/Applications` on macOS (Apple Silicon). The macOS build is unsigned — if it warns on first launch, right-click the app → **Open**.

**Windows** — one line (PowerShell):
```powershell
irm https://raw.githubusercontent.com/karem505/whatRust/master/install.ps1 | iex
```
Downloads and runs the latest NSIS installer (`.exe`); an `.msi` is also available on the release page.

**Manual download** — grab a `.flatpak`, `.AppImage`/`.deb`, `.dmg`, `.exe`, or `.msi` from the [latest release](https://github.com/karem505/whatRust/releases/latest).

<details>
<summary><b>Build from source</b> (Rust + Cargo + Tauri CLI)</summary>

```bash
# Linux build dependencies (Ubuntu/Debian)
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libhunspell-dev patchelf

cargo install tauri-cli --version "^2.0" --locked

cargo tauri dev      # run in development
cargo tauri build    # build installers for the current OS
cd src-tauri && cargo test   # run the unit tests
```
</details>

## Getting started

1. Launch whatRust. The WhatsApp Web QR screen appears.
2. On your phone, open **WhatsApp → Linked Devices → Link a Device** and scan the QR code.
3. You're in. Login persists, so you won't need to scan again next launch.
4. Closing the window hides whatRust to the tray (toggle this in Settings). Use the tray icon or the global shortcut to bring it back.

## FAQ

### What is whatRust?
whatRust is a free, open-source, lightweight WhatsApp Web desktop client built with Rust and Tauri v2 for Linux, Windows, and macOS.

### Is whatRust an official WhatsApp app?
No. whatRust is unofficial and independent — not affiliated with WhatsApp or Meta. It loads the official `web.whatsapp.com` in a native webview.

### How is whatRust different from the official WhatsApp Desktop app?
whatRust uses your OS's native webview instead of bundling a Chromium engine (as Electron does), which makes it considerably lighter. See the [comparison table](#whatrust-vs-the-official-whatsapp-desktop-electron).

### How much RAM does whatRust use?
whatRust's own native shell is small — around 90 MB. Your **total** memory use is mostly WhatsApp Web's own footprint and scales with how many chats and how much media you keep open — commonly a few hundred MB up to ~1 GB for busy accounts, similar to WhatsApp Web in a browser tab.

### Is whatRust lighter than the official WhatsApp Desktop app?
Its native shell is lighter because it doesn't bundle a Chromium browser engine the way the Electron-based official app does, so it has less baseline overhead. The WhatsApp Web content itself uses a similar amount in either app.

### Which operating systems does whatRust support?
Linux (WebKitGTK), Windows 10/11 (WebView2), and macOS 12.1+ (WKWebView).

### Is whatRust free and open source?
Yes — whatRust is free and open source under the MIT License. The source is on [GitHub](https://github.com/karem505/whatRust).

### Do voice messages, voice calls, and video calls work in whatRust?
Voice messages work on every platform — whatRust grants the webview microphone and camera access. Voice and video **calls** depend on the platform:

- **macOS:** yes. whatRust presents itself as the Safari that ships with your macOS — which is exactly what WKWebView is — so WhatsApp Web uses its Safari calling path.
- **Windows:** no. WhatsApp Web doesn't offer calls to Windows browsers at all — it shows *"Make calls with the Windows app"* even in Chrome or Edge — and whatRust won't disguise your OS to get around Meta's decision. Use the official WhatsApp app for calls on Windows.
- **Linux:** most distributions build WebKitGTK **without** WebRTC, so WhatsApp correctly reports that calling isn't supported. This is an engine limitation, not a permissions problem — if your distro ships a WebRTC-enabled WebKitGTK, calls light up automatically.

### Can I use multiple WhatsApp accounts in whatRust?
Yes. whatRust supports **multiple WhatsApp accounts** running at the same time — each opens in its own window with a fully isolated session, so different numbers stay logged in independently. Add, rename, and remove accounts from **Settings → Accounts**. On macOS this requires macOS 14 or later; Linux and Windows have no limit.

### Does whatRust have an app lock?

Yes. You can set a password under **Settings → Security** to require authentication before whatRust shows your chats. Biometric unlock (Windows Hello, Touch ID, or Linux polkit with an enrolled fingerprint) is an optional shortcut where the OS supports it. The lock controls window access — it does not encrypt data on disk (same posture as Signal Desktop). For at-rest protection use full-disk encryption.

### The global show/hide shortcut doesn't work on my Linux desktop (Wayland)

Wayland blocks apps from grabbing global hotkeys, so whatRust's built-in shortcut can't fire on a Wayland session (GNOME, KDE Plasma's Wayland, etc.) — this is a platform limitation, not specific to whatRust. The reliable approach is to let your desktop own the keybinding and have it toggle whatRust:

1. **Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts → +** (GNOME; KDE has an equivalent under *Custom Shortcuts*)
2. Name it `whatRust`, set the command to `whatrust --toggle`, and assign your key (e.g. `Ctrl+Shift+W`).

`whatrust --toggle` shows the window if it's hidden and hides it if it's visible — a true toggle that works on Wayland. For the Flatpak, bind `flatpak run io.github.karem505.whatRust --toggle` instead. (The built-in shortcut still works on X11, Windows, and macOS.)

### Does whatRust support the system tray and close-to-tray?
Yes. It adds a system tray icon with an unread-message badge, can close to the tray, and forwards new messages to native OS notifications.

### Do I have to log in every time I open whatRust?
No. Login is persistent — scan the QR code once via Linked Devices and you stay signed in across restarts.

### Is whatRust safe? Does it read my messages?
whatRust only loads the official `web.whatsapp.com` in a native webview and adds no message-handling layer of its own. It requests only the webview, microphone, and camera access that WhatsApp Web itself needs, and it is open source, so the code can be audited.

### My WhatsApp account went "under review" after linking whatRust — why?
whatRust doesn't talk to WhatsApp's servers itself: it shows the official `web.whatsapp.com` page, exactly as a browser tab would, and linking it is the same Linked Devices flow as WhatsApp Web. Account reviews are decided by WhatsApp's own automated systems, which whatRust can't see or influence. If your account is flagged, appeal through the WhatsApp app on your phone.

## Limitations

- **Windows unread count**: Windows tray icons ignore text labels, so the unread *number* appears only in the hover tooltip (the icon still switches to a badged variant). macOS and Linux show the count.
- **Notification click** focuses the window on Windows only — elsewhere use the tray icon or the global shortcut.
- **macOS** builds are unsigned and currently Apple Silicon (arm64) only.
- **macOS drag-and-drop from Photos.app** (and other apps that "promise" files rather than providing real paths, e.g. dragging an image straight out of a browser) isn't supported by the system webview layer — whatRust shows a hint instead of silently ignoring the drop. Drag from Finder, or use the attach (+) button.
- **Drag-and-drop limits**: up to 200 files per drop, 500 MB per file, and 500 MB per drop total. Base64 transport is decoded incrementally while files stream in, but the page must still hold the final `File` data in memory. Larger batches can always be sent via WhatsApp's attach (+) → Document picker, which is not routed through these limits. Skipped files are reported in a notification.
- **Multiple accounts on macOS** require macOS 14+ (older macOS runs a single account); Linux and Windows are unrestricted.
- **Flatpak file access**: native drag-and-drop needs read-only access to your home
  files because Tauri does not yet route OS drops through the File Transfer portal.
  whatRust writes only to Downloads, its sandbox data, and its optional autostart entry.
  The wrapped WhatsApp page has no general filesystem API.

## Contributing

Contributions to this open-source WhatsApp client are welcome — open an issue or a pull request on [GitHub](https://github.com/karem505/whatRust).

## Disclaimer

whatRust is an unofficial, independent project. It is **not affiliated with, endorsed by, or sponsored by WhatsApp LLC or Meta Platforms, Inc.** "WhatsApp" is a trademark of its respective owner. whatRust only loads the official `web.whatsapp.com` interface in a native webview and does not modify or intercept WhatsApp's services.

## License

Released under the [MIT License](LICENSE).

## Built with

[Rust](https://www.rust-lang.org/) · [Tauri v2](https://tauri.app/) · [WhatsApp Web](https://web.whatsapp.com/)
