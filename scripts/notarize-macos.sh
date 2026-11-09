#!/usr/bin/env bash
# Notarizes a Developer ID-signed .app or .dmg with Apple and staples the ticket,
# so Gatekeeper opens it without a warning, even offline:
#   scripts/notarize-macos.sh <path/to/App.app | path/to/file.dmg>
#
# Credentials come from one of:
#   FERRUM_NOTARY_PROFILE  a keychain profile made once with
#                          `xcrun notarytool store-credentials <name>` (local use)
#   FERRUM_NOTARY_KEY, FERRUM_NOTARY_KEY_ID, FERRUM_NOTARY_ISSUER
#                          an App Store Connect API key file, its key ID and
#                          issuer ID (CI)
set -euo pipefail

[ $# -eq 1 ] || { echo "usage: $0 <App.app | file.dmg>" >&2; exit 2; }
target="${1%/}"
[ -e "$target" ] || { echo "error: $target not found" >&2; exit 1; }

if [ -n "${FERRUM_NOTARY_PROFILE:-}" ]; then
  auth=(--keychain-profile "$FERRUM_NOTARY_PROFILE")
elif [ -n "${FERRUM_NOTARY_KEY:-}" ]; then
  auth=(--key "$FERRUM_NOTARY_KEY" --key-id "$FERRUM_NOTARY_KEY_ID" --issuer "$FERRUM_NOTARY_ISSUER")
else
  echo "error: set FERRUM_NOTARY_PROFILE or FERRUM_NOTARY_KEY/_KEY_ID/_ISSUER" >&2
  exit 1
fi

# notarytool takes a .dmg, .pkg or .zip; an .app is sent as a zip.
upload="$target"
if [[ "$target" == *.app ]]; then
  upload="$(mktemp -d)/$(basename "$target" .app).zip"
  ditto -c -k --keepParent "$target" "$upload"
fi

if ! xcrun notarytool submit "$upload" "${auth[@]}" --wait --timeout 30m; then
  echo "error: notarization failed; see: xcrun notarytool log <submission-id> ${auth[*]:0:2}" >&2
  exit 1
fi
xcrun stapler staple "$target"
if [[ "$target" == *.app ]]; then
  spctl --assess --type execute -v "$target"
else
  spctl --assess --type open --context context:primary-signature -v "$target"
fi
echo "$target"
