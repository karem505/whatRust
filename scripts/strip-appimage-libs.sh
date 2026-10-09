#!/bin/sh
# Remove host-only libraries and broken GIO modules from a built AppImage and
# repack it in place.
#
#   scripts/strip-appimage-libs.sh path/to/whatRust_x.y.z_amd64.AppImage
#
# Why:
# - libwayland-* (issue #22): the AppImage is built on Ubuntu 22.04 and the
#   bundler copies that system's libwayland into the bundle. AppRun puts the
#   bundle's lib dir first on LD_LIBRARY_PATH, so on a current distro Mesa's
#   libEGL_mesa (which needs wayland >= 1.23's `wl_fixes_interface`) fails to load
#   against the old copy: WebKit's web process aborts with "Could not create
#   default EGL display: EGL_BAD_PARAMETER" and the window stays blank. These
#   libraries are on the AppImage project's excludelist for exactly this reason.
# - GIO proxy modules (found testing v0.6.5): newer Tauri bundlers also pack
#   glib-networking's environment and libproxy proxy-resolver modules. Whenever
#   HTTP(S)_PROXY-style variables are set, WebKit's network process crashes on
#   them and every page shows "WebKit encountered an internal error". v0.6.3 and
#   earlier never bundled them; without them WebKit behaves as it did there.
#
# Directories are also given normal 0755 modes: extracting with the AppImage
# runtime leaves them 0700, and a repack would bake that in.
#
# Needs: appimagetool on PATH (or $APPIMAGETOOL). Set APPIMAGE_EXTRACT_AND_RUN=1
# where FUSE is unavailable (CI runners).
set -eu

img=${1:?usage: $0 path/to/app.AppImage}
img=$(cd "$(dirname "$img")" && pwd)/$(basename "$img")
tool=${APPIMAGETOOL:-appimagetool}
# Files that must not ship in the bundle (name patterns, matched anywhere).
EXCLUDE='libwayland-client.so* libwayland-cursor.so* libwayland-egl.so* libwayland-server.so*
libgioenvironmentproxy.so libgiolibproxy.so libproxy.so*'

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

list_excluded() {
  for pat in $EXCLUDE; do
    find "$1" -name "$pat" \( -type f -o -type l \)
  done
}

cd "$work"
"$img" --appimage-extract >/dev/null
found=$(list_excluded squashfs-root)
if [ -z "$found" ]; then
  echo "strip-appimage-libs: nothing to remove in $(basename "$img")"
  exit 0
fi
echo "strip-appimage-libs: removing from the bundle:"
echo "$found" | sed "s|^squashfs-root/|  |"
echo "$found" | while IFS= read -r f; do rm -f "$f"; done

# GIO lists its modules in giomodule.cache; drop entries for removed modules so
# it doesn't try (and warn about failing) to load them.
find squashfs-root -name giomodule.cache | while IFS= read -r cache; do
  grep -v -e '^libgioenvironmentproxy\.so:' -e '^libgiolibproxy\.so:' "$cache" > "$cache.new" || true
  mv "$cache.new" "$cache"
done

find squashfs-root -type d -exec chmod 755 {} +

# Repack. ARCH is read from the AppDir's binaries when unset, but say it outright.
ARCH=${ARCH:-$(uname -m)} "$tool" --no-appstream squashfs-root "$work/out.AppImage" >/dev/null
chmod +x "$work/out.AppImage"

# Verify the result before replacing the original.
mkdir verify && cd verify
"$work/out.AppImage" --appimage-extract >/dev/null
left=$(list_excluded squashfs-root)
[ -x squashfs-root/AppRun ] || { echo "strip-appimage-libs: repacked image has no AppRun" >&2; exit 1; }
[ -z "$left" ] || { echo "strip-appimage-libs: still bundled: $left" >&2; exit 1; }
cd "$work"
mv -f "$work/out.AppImage" "$img"
echo "strip-appimage-libs: repacked $(basename "$img")"
