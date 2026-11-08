#!/usr/bin/env bash
# Installs the latest Ferrum Toolbox on macOS from GitHub Releases:
#   scripts/install-toolbox.sh [--repo owner/name]
#
# The release repo is private, so this uses the GitHub CLI: run
# `gh auth login` once first. Installs "Ferrum Toolbox.app" into
# /Applications, or ~/Applications when /Applications isn't writable.
set -euo pipefail

repo="Amygdal-doo/ferrum"
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) [ $# -ge 2 ] || { echo "usage: $0 [--repo owner/name]" >&2; exit 2; }; repo="$2"; shift 2 ;;
    *) echo "usage: $0 [--repo owner/name]" >&2; exit 2 ;;
  esac
done

[ "$(uname -s)" = "Darwin" ] || { echo "error: this installer is for macOS; on Linux use FerrumToolbox's tarball" >&2; exit 1; }
command -v gh >/dev/null || { echo "error: the GitHub CLI is required (brew install gh), then run: gh auth login" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "error: not signed in to GitHub. Run: gh auth login" >&2; exit 1; }

case "$(uname -m)" in
  arm64) arch="aarch64" ;;
  x86_64) arch="x86_64" ;;
  *) echo "error: unsupported CPU $(uname -m)" >&2; exit 1 ;;
esac

# Newest non-draft, non-prerelease toolbox-v* release.
tag="$(gh release list --repo "$repo" --limit 100 --exclude-drafts --exclude-pre-releases \
  --json tagName --jq '[.[].tagName | select(startswith("toolbox-v"))] | first // empty')"
[ -n "$tag" ] || { echo "error: no toolbox-v* release found in $repo" >&2; exit 1; }
version="${tag#toolbox-v}"
dmg="FerrumToolbox-$version-$arch.dmg"
echo "Installing Ferrum Toolbox $version ($arch) from $repo…"

work="$(mktemp -d)"
mount=""
cleanup() {
  [ -n "$mount" ] && hdiutil detach -quiet "$mount" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

gh release download "$tag" --repo "$repo" --dir "$work" --pattern "$dmg" --pattern "$dmg.sha256"
if [ -f "$work/$dmg.sha256" ]; then
  expected="$(awk '{print $1}' "$work/$dmg.sha256")"
  actual="$(shasum -a 256 "$work/$dmg" | awk '{print $1}')"
  [ "$expected" = "$actual" ] || { echo "error: checksum mismatch for $dmg" >&2; exit 1; }
fi

mount="$(mktemp -d)"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount" "$work/$dmg"

destination="/Applications"
[ -w "$destination" ] || { destination="$HOME/Applications"; mkdir -p "$destination"; }
rm -rf "$destination/Ferrum Toolbox.app"
cp -R "$mount/Ferrum Toolbox.app" "$destination/"
# Downloaded via gh, not a browser, but clear quarantine in case it was set.
xattr -dr com.apple.quarantine "$destination/Ferrum Toolbox.app" 2>/dev/null || true

echo "Installed: $destination/Ferrum Toolbox.app"
open "$destination/Ferrum Toolbox.app"
