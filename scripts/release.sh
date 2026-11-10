#!/usr/bin/env bash
# Tags a Ferrum app release: scripts/release.sh <app-id> <version> [-m "notes"]
# Creates the annotated tag `<app-id>-v<version>` locally. Pushing it runs
# .github/workflows/release.yml, which builds and publishes the release that
# FerrumToolbox picks up. This script never pushes.
set -euo pipefail

usage() {
  echo "usage: $0 <ide|api|git|data|term|kit|dock|toolbox> <semver> [-m \"release notes\"]" >&2
  exit 2
}

[ $# -ge 2 ] || usage
app="$1"
version="$2"
shift 2
notes=""
while [ $# -gt 0 ]; do
  case "$1" in
    -m) [ $# -ge 2 ] || usage; notes="$2"; shift 2 ;;
    *) usage ;;
  esac
done

case "$app" in
  ide|api|git|data|term|kit|dock|toolbox) ;;
  *) echo "error: unknown app id '$app'" >&2; usage ;;
esac

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a semver version (e.g. 1.2.3 or 1.2.3-beta.1)" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)"
if [ -n "$(git status --porcelain)" ]; then
  echo "error: the working tree has uncommitted changes; commit or stash them first" >&2
  exit 1
fi

tag="${app}-v${version}"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "error: tag $tag already exists" >&2
  exit 1
fi

[ -n "$notes" ] || notes="$tag"
git tag -a "$tag" -m "$notes"
echo "Created tag $tag at $(git rev-parse --short HEAD)."
echo "Publish it with:"
echo "  git push origin $tag"
