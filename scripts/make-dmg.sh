#!/usr/bin/env bash
# Packs a .app into a drag-to-install disk image:
#   scripts/make-dmg.sh <path/to/App.app> <out.dmg> [volume name]
#
# The image holds the app and an /Applications shortcut (UDZO, compressed).
# The image is signed with FERRUM_SIGN_IDENTITY when it is set; notarize it
# afterwards with scripts/notarize-macos.sh.
set -euo pipefail

usage() {
  echo "usage: $0 <path/to/App.app> <out.dmg> [volume name]" >&2
  exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
app="${1%/}"
out="$2"
[ -d "$app" ] && [[ "$app" == *.app ]] || { echo "error: '$app' is not a .app bundle" >&2; exit 2; }
volume="${3:-$(basename "$app" .app)}"

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"

mkdir -p "$(dirname "$out")"
rm -f "$out"
hdiutil create -quiet -volname "$volume" -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$out"
if [ -n "${FERRUM_SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$FERRUM_SIGN_IDENTITY" "$out"
fi
echo "$out"
