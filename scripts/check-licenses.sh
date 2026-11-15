#!/usr/bin/env bash
# Fails if a GPL-family crate (GPL / AGPL / LGPL with no permissive
# alternative) is in the normal or build dependency graph of any shipped
# binary. Ferrum is MIT OR Apache-2.0, so copyleft crates (e.g. Zed's GPL
# `ui`, `theme`, `menu`, `icons`, `assets`, ...; only its Apache-2.0 GPUI
# crates are used) must stay out.
#
#   scripts/check-licenses.sh            # all binaries of */apps/*
#   scripts/check-licenses.sh -v         # also list every crate checked
#
# Dev-dependencies are ignored (they are not linked into shipped binaries).
set -euo pipefail

cd "$(dirname "$0")/.."
VERBOSE="${1:-}"

cargo metadata --format-version 1 --locked 2>/dev/null >"${TMPDIR:-/tmp}/ferrum-metadata.json" ||
    cargo metadata --format-version 1 >"${TMPDIR:-/tmp}/ferrum-metadata.json"

python3 -I - "${TMPDIR:-/tmp}/ferrum-metadata.json" "$VERBOSE" <<'PY'
import json, re, sys

meta = json.load(open(sys.argv[1]))
verbose = sys.argv[2] == "-v"
packages = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = meta["workspace_root"]

def is_copyleft_only(expr):
    """True when every alternative of an SPDX expression is GPL-family."""
    if not expr:
        return False
    alternatives = re.split(r"\s+OR\s+|/", expr.replace("(", " ").replace(")", " "))
    return all(re.search(r"\b(A|L)?GPL", alt) for alt in alternatives if alt.strip())

def shipped_binaries():
    for p in packages.values():
        rel = p["manifest_path"][len(root) + 1:]
        if not re.match(r"[^/]+/apps/[^/]+/Cargo.toml$", rel):
            continue
        for t in p["targets"]:
            if "bin" in t["kind"]:
                yield p, t["name"]

def closure(pkg_id):
    """Package ids reachable through normal and build dependencies."""
    seen, stack = set(), [pkg_id]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            kinds = {k["kind"] for k in dep["dep_kinds"]}
            if kinds & {None, "build"}:
                stack.append(dep["pkg"])
    return seen

failures = 0
checked = set()
for pkg, binary in sorted(shipped_binaries(), key=lambda pb: pb[1]):
    bad = []
    for dep_id in closure(pkg["id"]):
        dep = packages[dep_id]
        checked.add(dep_id)
        if is_copyleft_only(dep.get("license")):
            bad.append(f'{dep["name"]} {dep["version"]} ({dep["license"]})')
    if bad:
        failures += len(bad)
        print(f"FAIL {binary} ({pkg['name']}): {len(bad)} GPL-family crate(s)")
        for line in sorted(bad):
            print(f"    {line}")
    else:
        print(f"ok   {binary} ({pkg['name']})")

if verbose:
    for dep_id in sorted(checked, key=lambda i: packages[i]["name"]):
        dep = packages[dep_id]
        print(f'    {dep["name"]} {dep["version"]}: {dep.get("license") or dep.get("license_file") or "(workspace / unlicensed)"}')

if failures:
    print(f"\n{failures} GPL-family dependency edge(s) in shipped binaries.", file=sys.stderr)
    sys.exit(1)
print("\nNo GPL-family crates in any shipped binary.")
PY
