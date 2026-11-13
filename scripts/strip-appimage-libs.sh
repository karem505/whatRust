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
# - GnuTLS (issue #84): the bundled Ubuntu GnuTLS looks for CA certificates
#   only at Debian's /etc/ssl/certs/ca-certificates.crt (and through a p11-kit
#   module path that is Debian's too). openSUSE and Fedora keep them elsewhere,
#   so every HTTPS load failed with "Unacceptable TLS certificate", and
#   SSL_CERT_FILE (an OpenSSL variable) can't help. GnuTLS and its private
#   libraries move to usr/lib/gnutls-fallback, and an AppRun hook puts that
#   directory on the library path only when the system has no libgnutls.so.30;
#   otherwise the system's GnuTLS, built for the system's CA store, is used.
#   WHATRUST_BUNDLED_GNUTLS=1 forces the bundled copy.
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

# GnuTLS and the libraries only it (and the bundled hogweed) needs. libidn2 and
# libunistring stay: the bundled libpsl links them directly.
FALLBACK=usr/lib/gnutls-fallback
TLS_LIBS='libgnutls.so.30* libnettle.so.8* libhogweed.so.6* libtasn1.so.6* libp11-kit.so.0*'
HOOK=whatrust-system-gnutls.sh

cd "$work"
"$img" --appimage-extract >/dev/null
found=$(list_excluded squashfs-root)
if [ -n "$found" ]; then
  echo "strip-appimage-libs: removing from the bundle:"
  echo "$found" | sed "s|^squashfs-root/|  |"
  echo "$found" | while IFS= read -r f; do rm -f "$f"; done
fi

if [ ! -e "squashfs-root/apprun-hooks/$HOOK" ]; then
  grep -q '^source "$this_dir"/apprun-hooks/"linuxdeploy-plugin-gtk.sh"$' squashfs-root/AppRun \
    || { echo "strip-appimage-libs: unexpected AppRun, can't add the GnuTLS hook" >&2; exit 1; }
  mkdir -p "squashfs-root/$FALLBACK"
  for pat in $TLS_LIBS; do
    find squashfs-root/usr/lib -maxdepth 1 -name "$pat" \( -type f -o -type l \) \
      -exec mv {} "squashfs-root/$FALLBACK/" \;
  done
  [ -e "squashfs-root/$FALLBACK/libgnutls.so.30" ] \
    || { echo "strip-appimage-libs: no bundled libgnutls.so.30 found" >&2; exit 1; }
  echo "strip-appimage-libs: GnuTLS moved to $FALLBACK:"
  ls "squashfs-root/$FALLBACK" | sed 's/^/  /'
  cat > "squashfs-root/apprun-hooks/$HOOK" <<'HOOK_EOF'
# whatRust (issue #84): use the system's GnuTLS, which knows where this distro
# keeps its CA certificates. The bundled copy (built on Ubuntu) only looks at
# Debian's paths. It is used only when the system has no GnuTLS, or when
# WHATRUST_BUNDLED_GNUTLS=1.
whatrust_system_has() {
  for d in /usr/lib64 /usr/lib /lib64 /lib "/usr/lib/$(uname -m)-linux-gnu" "/lib/$(uname -m)-linux-gnu"; do
    [ -e "$d/$1" ] && return 0
  done
  return 1
}
if [ "${WHATRUST_BUNDLED_GNUTLS:-}" = 1 ] || ! whatrust_system_has libgnutls.so.30; then
  export LD_LIBRARY_PATH="$APPDIR/usr/lib/gnutls-fallback${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
unset -f whatrust_system_has
HOOK_EOF
  sed -i 's|^source "$this_dir"/apprun-hooks/"linuxdeploy-plugin-gtk.sh"$|&\nsource "$this_dir"/apprun-hooks/"'"$HOOK"'"|' squashfs-root/AppRun
fi

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
grep -q "apprun-hooks/\"$HOOK\"" squashfs-root/AppRun \
  || { echo "strip-appimage-libs: AppRun does not source $HOOK" >&2; exit 1; }
[ -e "squashfs-root/$FALLBACK/libgnutls.so.30" ] && [ ! -e squashfs-root/usr/lib/libgnutls.so.30 ] \
  || { echo "strip-appimage-libs: GnuTLS not moved to $FALLBACK" >&2; exit 1; }
cd "$work"
mv -f "$work/out.AppImage" "$img"
echo "strip-appimage-libs: repacked $(basename "$img")"
