#!/usr/bin/env bash
# Wraps a built Ferrum app in a macOS .app bundle:
#   scripts/bundle-macos.sh <app-id> <version> <target> <binary-dir> <out-dir>
#
# <binary-dir> holds the release binaries (e.g. target/<target>/release).
# Produces <out-dir>/<App Name>.app with Contents/MacOS/<binaries>, an
# Info.plist and Contents/Resources/<app-id>.icns when brand/icons has one.
# Signs with the Developer ID identity in FERRUM_SIGN_IDENTITY (hardened
# runtime, secure timestamp, packaging/macos/ferrum.entitlements) when it is
# set, otherwise ad-hoc. Notarize afterwards with scripts/notarize-macos.sh.
# Prints the bundle path on success.
set -euo pipefail

usage() {
  echo "usage: $0 <editor|agentic|api|git|data|term|kit|dock|ports|env|launcher> <semver> <target> <binary-dir> <out-dir>" >&2
  exit 2
}

[ $# -eq 5 ] || usage
app="$1"
version="$2"
target="$3"
binary_dir="$4"
out_dir="$5"

case "$app" in
  api)     name="FerrumAPI";      binaries="ferrumapi" ;;
  git)     name="FerrumGIT";      binaries="ferrumgit" ;;
  data)    name="FerrumData";     binaries="ferrumdata" ;;
  term)    name="FerrumTerm";     binaries="ferrumterm fterm" ;;
  kit)     name="FerrumKit";      binaries="ferrumkit" ;;
  dock)    name="FerrumContainers"; binaries="ferrumdock" ;;
  editor)  name="Ferrum Editor";  binaries="ferrumeditor" ;;
  agentic) name="FerrumAgentic";  binaries="ferrumagentic" ;;
  ports)   name="FerrumPorts";    binaries="ferrumports" ;;
  env)     name="FerrumEnv";      binaries="ferrumenv" ;;
  launcher) name="Ferrum Launcher"; binaries="ferrum-launcher" ;;
  *) echo "error: unknown app id '$app'" >&2; usage ;;
esac

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a semver version" >&2
  exit 2
fi
case "$target" in
  *-apple-darwin) ;;
  *) echo "error: '$target' is not a macOS target" >&2; exit 2 ;;
esac

# CFBundle*Version must be numeric; prerelease/build parts go in FerrumVersion.
numeric_version="${version%%[-+]*}"
executable="${binaries%% *}"
root="$(cd "$(dirname "$0")/.." && pwd)"
icon="$root/brand/icons/$app.icns"

mkdir -p "$out_dir"
bundle="$out_dir/$name.app"
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"

for binary in $binaries; do
  if [ ! -f "$binary_dir/$binary" ]; then
    echo "error: $binary_dir/$binary not found (build it first)" >&2
    exit 1
  fi
  cp "$binary_dir/$binary" "$bundle/Contents/MacOS/$binary"
  chmod 755 "$bundle/Contents/MacOS/$binary"
done

icon_entry=""
if [ -f "$icon" ]; then
  cp "$icon" "$bundle/Contents/Resources/$app.icns"
  icon_entry="  <key>CFBundleIconFile</key>
  <string>$app</string>"
else
  echo "warning: $icon not found; bundling without an icon" >&2
fi

cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$name</string>
  <key>CFBundleName</key>
  <string>$name</string>
  <key>CFBundleExecutable</key>
  <string>$executable</string>
  <key>CFBundleIdentifier</key>
  <string>dev.ferrum.$app</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$numeric_version</string>
  <key>CFBundleVersion</key>
  <string>$numeric_version</string>
  <key>FerrumVersion</key>
  <string>$version</string>
  <key>FerrumTarget</key>
  <string>$target</string>
$icon_entry
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.15.7</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST

if command -v plutil >/dev/null; then
  plutil -lint -s "$bundle/Contents/Info.plist"
fi

if [ -n "${FERRUM_SIGN_IDENTITY:-}" ]; then
  # Sign the inner executables first; --deep is deprecated for signing.
  entitlements="$root/packaging/macos/ferrum.entitlements"
  for binary in $binaries; do
    codesign --force --options runtime --timestamp --entitlements "$entitlements" \
      --sign "$FERRUM_SIGN_IDENTITY" --identifier "dev.ferrum.$app.$binary" \
      "$bundle/Contents/MacOS/$binary"
  done
  codesign --force --options runtime --timestamp --entitlements "$entitlements" \
    --sign "$FERRUM_SIGN_IDENTITY" --identifier "dev.ferrum.$app" "$bundle"
  codesign --verify --strict "$bundle"
elif command -v codesign >/dev/null; then
  codesign --force --deep --sign - --identifier "dev.ferrum.$app" "$bundle" 2>/dev/null
fi

echo "$bundle"
