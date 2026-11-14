#!/usr/bin/env bash
# Renders assets/img/og/<slug>.png (1200x630) for every app from template.html
# with headless Chrome. Run from web/: tools/og/render.sh
# Keep the rows in sync with src/products.rs (name, kind, a short tagline, logo, accent).
set -euo pipefail
cd "$(dirname "$0")/../.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
OUT=assets/img/og
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT"
ROOT="$(pwd)"

while IFS='|' read -r slug name kind logo accent tagline; do
  [ -z "$slug" ] && continue
  html="$TMP/$slug.html"
  python3 - "$ROOT" "$slug" "$name" "$kind" "$logo" "$accent" "$tagline" "$html" <<'PY'
import html, sys
root, slug, name, kind, logo, accent, tagline, out = sys.argv[1:]
t = open(f"{root}/tools/og/template.html").read()
for k, v in {
    "{{NAME}}": html.escape(name), "{{KIND}}": html.escape(kind),
    "{{SIZE}}": "118" if len(name) <= 13 else "96",
    "{{TAGLINE}}": html.escape(tagline), "{{ACCENT}}": accent,
    "{{LOGO}}": f"file://{root}/assets/img/brand/{logo}.svg",
    "{{MARK}}": f"file://{root}/assets/img/brand/ferrum.svg",
}.items():
    t = t.replace(k, v)
open(out, "w").write(t)
PY
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
    --window-size=1200,630 --virtual-time-budget=4000 \
    --screenshot="$ROOT/$OUT/$slug.png" "file://$html" >/dev/null 2>&1
  echo "$OUT/$slug.png"
done <<'ROWS'
ferrum-editor|Ferrum Editor|Code editor|editor|#c5dc3a|A small, hackable code editor written from scratch in Rust.
ferrumvibe|FerrumVibe|Vibe coding|vibe|#4CD97B|FerrumChat, FerrumGIT and FerrumTerm side by side on one project.
ferrum-system|Ferrum System|Architecture diagrams|system|#2DD4BF|Layers, services on any cloud, data, flows and live pods.
ferrumapi|FerrumAPI|API client|api|#3fb8a0|A native, local-first API client. No account, no webview.
ferrumgit|FerrumGIT|Git client|git|#f0b43c|Commit graph, line staging and per-project SSH profiles.
ferrumdata|FerrumData|Database IDE|data|#a07cf0|A local-first database IDE. PostgreSQL today.
ferrumterm|FerrumTerm|Terminal|term|#7cc85a|A GPU terminal whose sessions live in a server.
ferrumkit|FerrumKit|Developer utilities|kit|#d46be0|JSON, JWT, Base64, regex, hashes and timestamps.
ferrumcontainers|FerrumContainers|Container manager|dock|#38bdf8|A native window on Docker Desktop, OrbStack, Colima and Podman.
ferrumports|FerrumPorts|Port inspector|ports|#5b8def|Every listening port with its process and project.
ferrumenv|FerrumEnv|Environment vault|env|#f2665a|A password-locked vault for your environment variables.
ferrum-launcher|Ferrum Launcher|App manager|launcher|#e85a7e|Installs, updates and rolls back every Ferrum app.
ROWS
