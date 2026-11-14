#!/usr/bin/env bash
# Builds and publishes Ferrum releases from this Mac, without CI:
#   scripts/release-local.sh <out-dir> <app>:<version> [...]
# For each app: the Apple Silicon and Intel builds (`.tar.gz`, `.app.tar.gz`
# for Ferrum Launcher, and a `.dmg`), plus the Linux files that
# scripts/build-linux.sh put in <out-dir>/<app>-v<version>/ (run it first,
# with the same <out-dir>). Then it creates the GitHub release for the tag
# `<app>-v<version>` (made with scripts/release.sh) and uploads everything.
#   TARGETS   Mac targets to build (default: both)
#   REPO      release repository (default: Amygdal-doo/ferrum-releases)
set -euo pipefail

[ $# -ge 2 ] || { echo "usage: $0 <out-dir> <app>:<version> [...]" >&2; exit 2; }
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source=lib/apps.sh
. scripts/lib/apps.sh
TARGETS="${TARGETS:-aarch64-apple-darwin x86_64-apple-darwin}"
REPO="${REPO:-Amygdal-doo/ferrum-releases}"
mkdir -p "$1"
out="$(cd "$1" && pwd)"
shift

for pair in "$@"; do
  app=${pair%%:*}; version=${pair#*:}; tag="$app-v$version"
  ferrum_app "$app"
  case "$app" in
    term) packages="ferrumterm-gui ferrumterm-cli" ;;
    dock) packages="ferrumdock-desktop ferrumdock-cli" ;;
    launcher) packages="ferrumlauncher-desktop" ;;
    *) packages="ferrum${app}-desktop" ;;
  esac
  args=(); for p in $packages; do args+=(-p "$p"); done
  dir="$out/$tag"
  mkdir -p "$dir"
  for target in $TARGETS; do
    echo "=== $tag: $target"
    FERRUM_VERSION="$version" cargo build --release --locked --target "$target" "${args[@]}"
    target_dir="$root/target/$target/release"
    staging="$(mktemp -d)"
    for b in $APP_BINARIES; do cp "$target_dir/$b" "$staging/"; done
    tar -C "$staging" -czf "$dir/$app-$version-$target.tar.gz" .
    bundles="$(mktemp -d)"
    bundle="$(scripts/bundle-macos.sh "$app" "$version" "$target" "$target_dir" "$bundles")"
    tar -C "$bundles" -czf "$dir/$app-$version-$target.app.tar.gz" "$(basename "$bundle")"
    scripts/make-dmg.sh "$bundle" "$dir/${APP_FILE_NAME}-${version}-${target%%-*}.dmg" "$APP_NAME" >/dev/null
    rm -rf "$staging" "$bundles"
  done
  (cd "$dir" && for f in *; do
    case "$f" in *.sha256) ;; *) shasum -a 256 "$f" > "$f.sha256" ;; esac
  done)

  echo "=== $tag: publish"
  notes="$(git tag -l --format='%(contents)' "$tag")"
  [ -n "$notes" ] || notes="$tag"
  # A new release stays a draft until every file is up, so Ferrum Launcher
  # never offers a version whose build is still uploading. Not marked
  # pre-release: the launcher reads the channel from the version.
  if ! gh release view "$tag" --repo "$REPO" >/dev/null 2>&1; then
    gh release create "$tag" --repo "$REPO" --target main --title "$APP_NAME $version" \
      --notes "- $notes" --draft
  fi
  gh release upload "$tag" --repo "$REPO" --clobber "$dir"/*
  gh release edit "$tag" --repo "$REPO" --draft=false >/dev/null
  echo "=== $tag: done"
done
