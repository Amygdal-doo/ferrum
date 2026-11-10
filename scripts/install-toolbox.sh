#!/usr/bin/env bash
# Ferrum Toolbox is now Ferrum Launcher: this name is kept so existing links
# and docs keep working. Runs scripts/install-launcher.sh with the same
# arguments, or the published installer when this file was piped to bash.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
if [ -n "$here" ] && [ -f "$here/install-launcher.sh" ]; then
  exec bash "$here/install-launcher.sh" "$@"
fi
curl -fsSL https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/install.sh | bash -s -- "$@"
