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

- Network: required for `web.whatsapp.com`.
- Wayland/fallback X11, IPC, and DRI: WebKitGTK windowing and GPU rendering.
- PulseAudio: microphone input and audio playback.
- Read-only home: Tauri's native drag-and-drop API supplies host paths rather than File Transfer portal handles; Rust only opens paths the user drops.
- Downloads: WhatsApp downloads are saved to the standard Downloads directory.
- Autostart config: the optional “Launch at system startup” setting writes one desktop entry that re-enters the sandbox.
- Notification and StatusNotifier D-Bus names plus the narrow `xdg-run/tray-icon` export: native notifications and system tray.
- ScreenSaver D-Bus names: optional idle-time app locking.
- `com.canonical.Unity` D-Bus name: lets the unread-count signal (`LauncherEntry`) through the proxy to docks that show launcher badges (KDE Plasma, Dash to Dock, Ubuntu Dock).

Linux polkit biometric unlock is deliberately not exposed through the sandbox; Flatpak users retain the password lock.

## Distribution

Tag releases build native x86_64 and ARM64 Flatpak bundles in GitHub Actions and attach them to the matching GitHub release. `install-flatpak.sh` selects the host architecture and installs or updates the bundle user-wide.
