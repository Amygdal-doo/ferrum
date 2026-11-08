# FerrumToolbox

Installs, updates, switches between and launches the Ferrum apps (FerrumIDE,
FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, and itself), in the style of
JetBrains Toolbox. GPUI desktop app built on `ferrum-ui`.

```bash
cargo toolbox            # = cargo run -p ferrumtoolbox-desktop (binary: ferrum-toolbox)
```

## Where versions come from

Every version of every app is a **GitHub Release** in one repository,
`Amygdal-doo/ferrum` by default (change it in Settings). The release *is*
the record: tag, version, date, notes and downloadable builds.

| Thing | Convention | Example |
| --- | --- | --- |
| Tag | `<app-id>-v<semver>` | `api-v0.2.0`, `term-v1.0.0-beta.1` |
| App ids | `ide`, `api`, `git`, `data`, `term`, `toolbox` | |
| Build | `<app-id>-<version>-<target>.tar.gz` with the binary at the archive root | `api-0.2.0-aarch64-apple-darwin.tar.gz` |
| Checksum | `<build>.sha256` | `api-0.2.0-aarch64-apple-darwin.tar.gz.sha256` |
| Targets | `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` | |

Tags that don't match the pattern, and draft releases, are ignored. A
version with a semver prerelease part (`-beta.1`), or a release marked as a
prerelease, only shows up when "Include prereleases" is on.

### Publishing a version

```bash
scripts/release.sh api 0.2.0 -m "Faster sends, new auth editor"
git push origin api-v0.2.0
```

`scripts/release.sh` checks the app id, the version and that the tree is
clean, then creates the annotated tag. It never pushes. Pushing the tag runs
`.github/workflows/release.yml`, which:

1. creates the release, with the tag message as notes (or GitHub's
   auto-generated notes when there is no message);
2. builds the app for the three targets (`ide` builds inside `ide/`);
3. uploads `<build>.tar.gz` and `<build>.sha256` to the release.

The workflow needs a repository secret **`SUBMODULE_TOKEN`** with read access
to `Amygdal-doo/ferrum-ide`. Every app needs the `ide/` submodule because the
apps use its GPUI crates.

## Using the toolbox

- **Left panel:** every app, with its active version (or "Not installed") and
  an *Update vX* badge when a newer release exists.
- **Right panel:** the app's **Launch**, **Install / Update to vX** and
  **Uninstall** buttons, then a list of every version. Each row shows the
  date, the size of this platform's build and an active/installed marker, and
  has **Install**, **Activate** or **Remove** buttons. Click a row to see that
  release's notes.
- **Downloads** show a progress bar with **Cancel**, and several apps can
  download at once.
- **Check for updates** runs on startup and then every *N* hours (Settings).
  A notice appears when new versions are found.
- **Activating** a version points `~/.ferrum/bin/<binary>` at it. Add
  `~/.ferrum/bin` to your `PATH` to run `ferrumapi`, `ferrumgit`, … from a
  shell.
- After an install, older versions beyond **Versions to keep** (default 3,
  including the active one; 0 keeps all) are removed. The active version is
  never pruned.

## Authentication

Private repositories need a GitHub token with read access to the repo's
contents (a fine-grained token with *Contents: read*, or a classic token with
`repo`). The toolbox looks for one in this order:

1. the OS keychain (service `FerrumToolbox`), which you set in **Settings**;
2. the `GITHUB_TOKEN` environment variable;
3. `gh auth token`, if the GitHub CLI is installed and logged in.

Without a token, only public repositories work, and GitHub's anonymous rate
limit (60 requests/hour) applies. The token is never written to `state.json`
or to logs. The status bar shows which source is in use.

## Local data

| What | Where (macOS) |
| --- | --- |
| State (settings, installed versions, cached releases) | `~/Library/Application Support/dev.Ferrum.FerrumToolbox/state.json` |
| Installed versions | `…/dev.Ferrum.FerrumToolbox/apps/<app-id>/<version>/` |
| In-progress downloads | `…/dev.Ferrum.FerrumToolbox/downloads/` |
| PATH links | `~/.ferrum/bin/` |

On Linux the data lives under `~/.local/share/ferrumtoolbox/`. `state.json`
is written atomically (temp file + rename).

## Install safety

- When a release has a `.sha256` file, the download is checked against it,
  and a mismatch aborts the install.
- Archives are unpacked into a staging directory and moved into place with a
  rename, so a failed or cancelled install leaves nothing behind.
- Archive entries with absolute paths, `..`, or links that point outside the
  install directory abort the install.

## Layout

- `crates/core` (`ferrumtoolbox-core`): catalog, GitHub client, installer and
  state store. It has no UI code and is unit-tested (`cargo test -p
  ferrumtoolbox-core`).
- `apps/desktop` (`ferrumtoolbox-desktop`): the GPUI app.

## Limitations

- The apps ship as plain binaries in a `.tar.gz`, not as signed `.app`
  bundles or `.dmg`s. macOS Gatekeeper may need the quarantine flag cleared
  for unsigned builds.
- FerrumToolbox can install new versions of itself, but it doesn't restart
  into them; launch the new version from `~/.ferrum/bin/ferrum-toolbox`.
- Windows has no published builds.
