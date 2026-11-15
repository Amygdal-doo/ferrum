#!/usr/bin/env bash
# Builds Ferrum apps for Linux in Docker on this machine:
#   scripts/build-linux.sh <amd64|arm64> <out-dir> <app>:<version> [...]
# Each app gets `<app>-<version>-<target>.tar.gz` (the binaries) and a .deb
# in <out-dir>/<app>-v<version>/. amd64 runs emulated on Apple Silicon
# (Rosetta in Docker Desktop keeps it reasonably fast). Cargo's registry, git
# checkouts (GPUI from upstream Zed) and target directory live in Docker
# volumes, so later builds are incremental.
set -euo pipefail

[ $# -ge 3 ] || { echo "usage: $0 <amd64|arm64> <out-dir> <app>:<version> [...]" >&2; exit 2; }
arch="$1"; out="$2"; shift 2
case "$arch" in
  amd64) target=x86_64-unknown-linux-gnu ;;
  arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "error: arch must be amd64 or arm64" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
toolchain="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$root/rust-toolchain.toml")"
image="ferrum-linux-build:$toolchain-$arch"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

docker build --platform "linux/$arch" --build-arg "RUST_TOOLCHAIN=$toolchain" \
  -t "$image" "$root/packaging/linux"

docker run --rm --platform "linux/$arch" \
  -v "$root:/src:ro" \
  -v "$out:/out" \
  -v "ferrum-cargo-registry-$arch:/usr/local/cargo/registry" \
  -v "ferrum-cargo-git-$arch:/usr/local/cargo/git" \
  -v "ferrum-target-$arch:/target" \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" \
  -e TARGET="$target" -e ARCH="$arch" \
  -w /src "$image" bash -euo pipefail -c '
    . scripts/lib/apps.sh
    for pair in "$@"; do
      app=${pair%%:*}; version=${pair#*:}
      ferrum_app "$app"
      packages=""
      case "$app" in
        term) packages="ferrumterm-gui ferrumterm-cli" ;;
        dock) packages="ferrumdock-desktop ferrumdock-cli" ;;
        launcher) packages="ferrumlauncher-desktop" ;;
        *) packages="ferrum${app}-desktop" ;;
      esac
      args=(); for p in $packages; do args+=(-p "$p"); done
      echo "=== $app $version ($TARGET)"
      FERRUM_VERSION="$version" cargo build --release --locked "${args[@]}"
      dir="/out/$app-v$version"; mkdir -p "$dir"
      staging="$(mktemp -d)"
      for b in $APP_BINARIES; do cp "/target/release/$b" "$staging/"; done
      tar -C "$staging" -czf "$dir/$app-$version-$TARGET.tar.gz" .
      scripts/package-deb.sh "$app" "$version" /target/release "$dir" "$ARCH" >/dev/null
      (cd "$dir" && for f in *"$TARGET"*.tar.gz *_"$ARCH".deb; do sha256sum "$f" > "$f.sha256"; done)
    done
  ' _ "$@"
