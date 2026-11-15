#!/usr/bin/env bash
# Turns Ferrum Launcher's macOS release assets into the `toolbox-v<version>`
# bridge assets that Ferrum Toolbox 0.5.0 updates itself from:
#   scripts/package-bridge.sh <version> <assets-dir> [out-dir]
#
# <assets-dir> holds launcher-<version>-<target>.app.tar.gz (and its .sha256)
# for aarch64-apple-darwin and/or x86_64-apple-darwin, as published on the
# launcher-v<version> release. For each one this writes
#   <out-dir>/toolbox-<version>-<target>.app.tar.gz   (+ .sha256)
# which is the launcher archive with one extra root entry: a symlink
#   ferrum-toolbox -> FerrumLauncher.app/Contents/MacOS/ferrum-launcher
# (the bundle's name is read from the archive: `Ferrum Launcher.app` before
# the 2026-11 rename, `FerrumLauncher.app` after it)
#
# Why: Toolbox 0.5.0 only looks at toolbox-v* releases, downloads
# toolbox-<version>-<target>.app.tar.gz, checks its .sha256, and accepts the
# archive when `ferrum-toolbox` exists at its root or in the single .app's
# Contents/MacOS. It then swaps that single .app (whatever its name) into the
# place of the running "Ferrum Toolbox.app" and opens it; the launcher then
# renames itself to "FerrumLauncher.app". The entry is appended to the
# original tar, so the signed, notarized bundle stays byte-for-byte the same.
#
# Upload the result as a release tagged toolbox-v<version> (see
# launcher/README.md, "Bridge release for Ferrum Toolbox 0.5.0").
set -euo pipefail

usage() {
  echo "usage: $0 <semver> <assets-dir> [out-dir]" >&2
  exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
version="$1"
assets="$2"
out="${3:-$assets/bridge}"

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a semver version" >&2
  exit 2
fi
[ -d "$assets" ] || { echo "error: '$assets' is not a directory" >&2; exit 2; }

binary="ferrum-launcher"
legacy_binary="ferrum-toolbox"

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"
made=0

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  source="$assets/launcher-$version-$target.app.tar.gz"
  [ -f "$source" ] || { echo "skipping $target: $(basename "$source") not found" >&2; continue; }

  if [ -f "$source.sha256" ]; then
    expected="$(awk '{print $1}' "$source.sha256")"
    [ "$expected" = "$(sha256 "$source")" ] || { echo "error: checksum mismatch for $source" >&2; exit 1; }
  else
    echo "warning: no $(basename "$source").sha256; not verified" >&2
  fi

  # The launcher archive must hold exactly one bundle, with the launcher binary.
  listing="$(tar -tzf "$source")"
  bundles="$(printf '%s\n' "$listing" | sed -nE "s#^(\./)?([^/]+\.app)/Contents/MacOS/$binary\$#\2#p" | sort -u)"
  if [ -z "$bundles" ] || [ "$(printf '%s\n' "$bundles" | wc -l)" -ne 1 ]; then
    echo "error: $source needs exactly one <name>.app/Contents/MacOS/$binary" >&2
    exit 1
  fi
  bundle="$bundles"
  if printf '%s\n' "$listing" | sed 's|^\./||' | grep -q "^$legacy_binary\$"; then
    echo "error: $source already has a root $legacy_binary entry" >&2
    exit 1
  fi

  link_dir="$work/$target"
  mkdir -p "$link_dir"
  ln -s "$bundle/Contents/MacOS/$binary" "$link_dir/$legacy_binary"

  name="toolbox-$version-$target.app.tar.gz"
  gzip -dc "$source" > "$work/$target.tar"
  # COPYFILE_DISABLE keeps macOS tar from adding AppleDouble (._*) entries.
  COPYFILE_DISABLE=1 tar -rf "$work/$target.tar" -C "$link_dir" "$legacy_binary"
  gzip -9n -c "$work/$target.tar" > "$out/$name"
  printf '%s  %s\n' "$(sha256 "$out/$name")" "$name" > "$out/$name.sha256"
  echo "$out/$name"
  echo "$out/$name.sha256"
  made=$((made + 1))
done

[ "$made" -gt 0 ] || { echo "error: no launcher-$version-*-apple-darwin.app.tar.gz in $assets" >&2; exit 1; }
