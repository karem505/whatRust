# Flatpak packaging

The Flatpak application ID is `io.github.karem505.whatRust`. A build-only `TAURI_CONFIG` override aligns the sandbox's internal GTK/application identity with that ID. The checked-in native identifier remains `com.karem.whatrust`, so existing native sessions are not logged out, while the Flatpak ID maps to the upstream GitHub repository.

## Build and install locally

Install Flathub's official Builder and fetch the pinned shared modules once:

```bash
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.flatpak.Builder
git submodule update --init --recursive flatpak/shared-modules
```

Generate the locked, offline Cargo sources after changing `src-tauri/Cargo.lock`:

```bash
git clone https://github.com/flatpak/flatpak-builder-tools /tmp/flatpak-builder-tools
git -C /tmp/flatpak-builder-tools checkout 737c0085912f9f7dabf9341d4608e2a77a51a73a
uv run /tmp/flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  src-tauri/Cargo.lock -o flatpak/cargo-sources.json
```

The manifest downloads the official Rust 1.89.0 toolchain archives before entering the build sandbox. The x86_64 and ARM64 URLs and SHA-256 checksums are pinned in the manifest; flatpak-builder consumes `cargo-sources.json` to materialize the vendored dependency tree, and Cargo remains network-disabled while compiling from that tree. This avoids depending on a mutable SDK-extension ref while keeping the source build reproducible.

Build and install the current checkout:

```bash
flatpak run --command=flathub-build org.flatpak.Builder --install \
  --disable-rofiles-fuse --default-branch=stable io.github.karem505.whatRust.yml
flatpak run io.github.karem505.whatRust
```

Validate metadata and the exported repository:

```bash
flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream \
  flatpak/io.github.karem505.whatRust.metainfo.xml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

## Permission rationale

- Network: required for `web.whatsapp.com`, and for the update check (`api.github.com`, `github.com`).
- Wayland/fallback X11, IPC, and DRI: WebKitGTK windowing and GPU rendering.
- PulseAudio: microphone input and audio playback.
- Read-only home: Tauri's native drag-and-drop API supplies host paths rather than File Transfer portal handles; Rust only opens paths the user drops.
- Downloads: WhatsApp downloads are saved to the standard Downloads directory.
- In-app updates: the sandbox cannot update itself and is deliberately not given `--talk-name=org.freedesktop.Flatpak` (that would let it run arbitrary host commands). Settings → Updates downloads the matching `whatRust_<arch>.flatpak` into the Downloads folder, verifies it against the release's `SHA256SUMS`, and opens it through the OpenURI portal (`xdg-open`) so the host's software center installs it; the one-line `install-flatpak.sh` command is shown as a fallback.
- Custom download folder: no extra permission. The folder is chosen with a GTK file chooser, which inside a Flatpak is the `org.freedesktop.portal.FileChooser` portal (GTK detects the sandbox itself). The portal exports the chosen folder into the sandbox through the document portal, where it is writable even though only `xdg-download` is granted. It needs `xdg-desktop-portal` and a backend on the host; without it the chooser cannot grant access and Settings refuses the folder as not writable. A folder that later disappears falls back to Downloads.
- Custom app icon: the chosen PNG is copied into the app's own config dir (sandbox data), so no file access is needed. Wayland compositors take the window icon from the `.desktop` file (the Flatpak's exported icon), so a custom icon only changes the window icon on X11.
- Autostart config: the optional “Launch at system startup” setting writes one desktop entry that re-enters the sandbox.
- Notification and StatusNotifier D-Bus names plus the narrow `xdg-run/tray-icon` export: native notifications and system tray.
- ScreenSaver D-Bus names: optional idle-time app locking.
- `com.canonical.Unity` D-Bus name: lets the unread-count signal (`LauncherEntry`) through the proxy to docks that show launcher badges (KDE Plasma, Dash to Dock, Ubuntu Dock).

Linux polkit biometric unlock is deliberately not exposed through the sandbox; Flatpak users retain the password lock.

## Distribution

Tag releases build native x86_64 and ARM64 Flatpak bundles in GitHub Actions and attach them to the matching GitHub release. `install-flatpak.sh` selects the host architecture and installs or updates the bundle user-wide.
