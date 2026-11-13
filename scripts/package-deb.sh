#!/usr/bin/env bash
# Builds a Debian package for a Ferrum app:
#   scripts/package-deb.sh <app-id> <version> <binary-dir> <out-dir> [arch]
#
# Layout: binaries in /opt/ferrum/<app-id>/, symlinks in /usr/bin, a .desktop
# entry and hicolor icons from brand/png. Prints the .deb path.
#
# Uses dpkg-deb (and dpkg-shlibdeps for library Depends) when available, as on
# Ubuntu CI; elsewhere it assembles the ar archive itself so the layout can be
# checked on macOS.
set -euo pipefail

usage() {
  echo "usage: $0 <editor|api|git|data|term|kit|dock|ports|env|launcher> <semver> <binary-dir> <out-dir> [arch]" >&2
  exit 2
}

[ $# -ge 4 ] && [ $# -le 5 ] || usage
app="$1"
version="$2"
binary_dir="$3"
out_dir="$4"
arch="${5:-amd64}"

root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=lib/apps.sh
. "$root/scripts/lib/apps.sh"
ferrum_app "$app" || { echo "error: unknown app id '$app'" >&2; usage; }

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a semver version" >&2
  exit 2
fi
# Debian orders `~` before everything, so 1.0.0~beta.1 < 1.0.0 like semver.
deb_version="$(printf '%s' "$version" | tr '-' '~')"
maintainer="${DEB_MAINTAINER:-Ferrum maintainers <ferrum@users.noreply.github.com>}"
homepage="${FERRUM_HOMEPAGE:-https://tryferrum.dev}"
main_binary="${APP_BINARIES%% *}"

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
pkg="$staging/pkg"
install_dir="/opt/ferrum/$app"
mkdir -p "$pkg/DEBIAN" "$pkg$install_dir" "$pkg/usr/bin" "$pkg/usr/share/applications" \
  "$pkg/usr/share/doc/$DEB_PACKAGE"

for binary in $APP_BINARIES; do
  if [ ! -f "$binary_dir/$binary" ]; then
    echo "error: $binary_dir/$binary not found (build it first)" >&2
    exit 1
  fi
  install -m 755 "$binary_dir/$binary" "$pkg$install_dir/$binary"
  ln -s "$install_dir/$binary" "$pkg/usr/bin/$binary"
done

# Ferrum Toolbox's command name, kept for one release after the rename.
[ "$app" = "launcher" ] && ln -s "$install_dir/$main_binary" "$pkg/usr/bin/ferrum-toolbox"

for size in 16 32 64 128 256 512; do
  icon="$root/brand/png/$app-$size.png"
  if [ -f "$icon" ]; then
    mkdir -p "$pkg/usr/share/icons/hicolor/${size}x${size}/apps"
    install -m 644 "$icon" "$pkg/usr/share/icons/hicolor/${size}x${size}/apps/ferrum-$app.png"
  fi
done

cat > "$pkg/usr/share/applications/ferrum-$app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.5
Name=$APP_NAME
Comment=$APP_SUMMARY
Exec=$install_dir/$main_binary %U
Icon=ferrum-$app
Terminal=false
Categories=Development;
StartupWMClass=$main_binary
DESKTOP

license="$root/$app/LICENSE"
[ -f "$license" ] || license="$root/LICENSE"
{
  echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
  echo "Upstream-Name: $APP_NAME"
  echo "Source: https://github.com/Amygdal-doo/ferrum"
  echo
  if [ -f "$license" ]; then cat "$license"; else echo "All rights reserved."; fi
} > "$pkg/usr/share/doc/$DEB_PACKAGE/copyright"

# GPUI opens most of these with dlopen, so dpkg-shlibdeps can't see them.
depends="libc6, libgcc-s1, libfontconfig1, libfreetype6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libx11-xcb1, libxcb1, libvulkan1"
[ "$app" = "git" ] && depends="$depends, git"
if command -v dpkg-shlibdeps >/dev/null; then
  shlib_dir="$staging/shlibs"
  mkdir -p "$shlib_dir/debian"
  printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$DEB_PACKAGE" "$DEB_PACKAGE" > "$shlib_dir/debian/control"
  args=()
  for binary in $APP_BINARIES; do args+=("-e$pkg$install_dir/$binary"); done
  if detected="$(cd "$shlib_dir" && dpkg-shlibdeps -O "${args[@]}" 2>/dev/null)"; then
    detected="${detected#shlibs:Depends=}"
    [ -n "$detected" ] && depends="$detected, $depends"
  else
    echo "warning: dpkg-shlibdeps failed; using the baseline Depends" >&2
  fi
fi
# Keep the first mention of each package (detected entries carry versions).
depends="$(printf '%s\n' "$depends" | tr ',' '\n' | sed 's/^ *//;s/ *$//' | awk 'NF { split($1, p, "|"); if (!seen[p[1]]++) print }' | paste -sd, - | sed 's/,/, /g')"

installed_size="$(du -sk "$pkg" | cut -f1)"
# Ferrum Launcher was called Ferrum Toolbox (package ferrum-toolbox) until 0.6.0.
relations=""
[ "$app" = "launcher" ] && relations="Conflicts: ferrum-toolbox
Replaces: ferrum-toolbox
Provides: ferrum-toolbox
"
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: $DEB_PACKAGE
Version: $deb_version
Architecture: $arch
Maintainer: $maintainer
Installed-Size: $installed_size
Depends: $depends
${relations}Recommends: mesa-vulkan-drivers | vulkan-icd
Section: devel
Priority: optional
Homepage: $homepage
Description: $APP_SUMMARY
 $APP_NAME is part of Ferrum, a family of native developer tools written in
 Rust on GPUI. Installed under $install_dir with $main_binary on PATH.
CONTROL

for script in postinst postrm; do
  cat > "$pkg/DEBIAN/$script" <<'SCRIPT'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
fi
SCRIPT
  chmod 755 "$pkg/DEBIAN/$script"
done

mkdir -p "$out_dir"
out="$out_dir/${DEB_PACKAGE}_${deb_version}_${arch}.deb"
rm -f "$out"

if command -v dpkg-deb >/dev/null; then
  dpkg-deb --root-owner-group -Zxz --build "$pkg" "$out" >/dev/null
else
  # Portable fallback: control.tar.gz + data.tar.gz in an ar archive, owned by root.
  if tar --version 2>/dev/null | grep -q 'GNU tar'; then
    tar_owner=(--owner=0 --group=0 --numeric-owner)
  else
    tar_owner=(--uid 0 --gid 0 --uname root --gname root --no-mac-metadata --no-xattrs)
  fi
  export COPYFILE_DISABLE=1
  (cd "$pkg/DEBIAN" && tar "${tar_owner[@]}" -czf "$staging/control.tar.gz" ./*)
  (cd "$pkg" && tar "${tar_owner[@]}" -czf "$staging/data.tar.gz" --exclude=./DEBIAN .)
  printf '2.0\n' > "$staging/debian-binary"
  {
    printf '!<arch>\n'
    for member in debian-binary control.tar.gz data.tar.gz; do
      size="$(wc -c < "$staging/$member" | tr -d ' ')"
      printf '%-16s%-12s%-6s%-6s%-8s%-10s`\n' "$member" "$(date +%s)" 0 0 100644 "$size"
      cat "$staging/$member"
      [ $((size % 2)) -eq 1 ] && printf '\n'
    done
  } > "$out"
fi

echo "$out"
