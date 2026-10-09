#!/bin/sh
# whatRust one-line installer — Linux & macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/karem505/whatRust/master/install.sh | sh
#
# Downloads the latest published GitHub release and installs it
# (AppImage on Linux, .app into /Applications on macOS). No build toolchain needed.
set -eu

REPO="karem505/whatRust"
APP="whatRust"
API="https://api.github.com/repos/${REPO}/releases/latest"

red() { printf '\033[31m%s\033[0m\n' "$*" >&2; }
grn() { printf '\033[32m%s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

fetch() {
  if have curl; then curl -fsSL "$1"
  elif have wget; then wget -qO- "$1"
  else red "This installer needs 'curl' or 'wget'."; exit 1
  fi
}
download() {
  if have curl; then curl -fL --progress-bar -o "$2" "$1"
  else wget -O "$2" "$1"
  fi
}
# First asset download URL matching regex $1 (empty if none).
pick() { printf '%s\n' "$ASSETS" | grep -iE "$1" | head -n1 || true; }

OS=$(uname -s)
ARCH=$(uname -m)
grn "Installing ${APP} (latest) for ${OS}/${ARCH}…"

JSON=$(fetch "$API") || { red "Couldn't reach the GitHub release. Is ${REPO} public with a published release?"; exit 1; }
ASSETS=$(printf '%s' "$JSON" | tr ',' '\n' | grep browser_download_url | sed -E 's/.*"(https:[^"]+)".*/\1/')
[ -n "$ASSETS" ] || { red "No release assets found — the release build may still be running. Try again shortly."; exit 1; }

case "$OS" in
  Linux)
    URL=$(pick '\.AppImage$')
    [ -n "$URL" ] || { red "No .AppImage asset in the latest release."; exit 1; }
    DEST="${HOME}/.local/bin"; mkdir -p "$DEST"
    OUT="${DEST}/${APP}.AppImage"
    grn "Downloading $(basename "$URL")…"
    download "$URL" "$OUT"
    chmod +x "$OUT"
    APPS="${HOME}/.local/share/applications"; mkdir -p "$APPS"
    cat > "${APPS}/whatrust.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=${APP}
Exec=${OUT} %u
Terminal=false
Categories=Network;InstantMessaging;
MimeType=x-scheme-handler/whatsapp;
EOF
    # Open whatsapp:// links ("Open app" buttons on websites) in whatRust, unless
    # the user already picked another handler.
    have update-desktop-database && update-desktop-database "$APPS" >/dev/null 2>&1 || true
    if have xdg-mime && [ -z "$(xdg-mime query default x-scheme-handler/whatsapp 2>/dev/null)" ]; then
      xdg-mime default whatrust.desktop x-scheme-handler/whatsapp >/dev/null 2>&1 || true
    fi
    grn "Installed to ${OUT}"
    grn "Launch '${APP}' from your app menu, or run: ${OUT}"
    grn "(If it won't start, your system may need FUSE: 'sudo apt install libfuse2', or run with --appimage-extract-and-run.)"
    ;;
  Darwin)
    case "$ARCH" in
      arm64|aarch64) PAT='aarch64|arm64' ;;
      *)             PAT='x64|x86_64|intel' ;;
    esac
    URL=$(pick "(${PAT}).*\.dmg$")
    [ -n "$URL" ] || URL=$(pick '\.dmg$')
    [ -n "$URL" ] || { red "No .dmg asset in the latest release."; exit 1; }
    TMP=$(mktemp -d); DMG="${TMP}/${APP}.dmg"
    grn "Downloading $(basename "$URL")…"
    download "$URL" "$DMG"
    MNT=$(hdiutil attach -nobrowse -readonly "$DMG" | grep '/Volumes/' | awk '{print $NF}' | tail -n1)
    BUNDLE=$(find "$MNT" -maxdepth 1 -name '*.app' | head -n1)
    [ -n "$BUNDLE" ] || { red "No .app inside the dmg."; hdiutil detach "$MNT" >/dev/null 2>&1 || true; exit 1; }
    NAME=$(basename "$BUNDLE")
    grn "Installing ${NAME} to /Applications…"
    rm -rf "/Applications/${NAME}"
    cp -R "$BUNDLE" /Applications/
    hdiutil detach "$MNT" >/dev/null 2>&1 || true
    rm -rf "$TMP"
    # Unsigned build: clear the quarantine flag so Gatekeeper doesn't block first launch.
    xattr -dr com.apple.quarantine "/Applications/${NAME}" 2>/dev/null || true
    grn "Installed to /Applications/${NAME}"
    grn "Launch it from Launchpad or /Applications."
    ;;
  *)
    red "Unsupported OS: ${OS}. See the README for manual install."
    exit 1
    ;;
esac
grn "Done. ✅"
