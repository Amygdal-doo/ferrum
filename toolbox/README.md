# FerrumToolbox

Installs, updates, switches between and launches the Ferrum apps (FerrumIDE,
FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, and itself), in the style of
JetBrains Toolbox. GPUI desktop app built on `ferrum-ui`.

```bash
cargo toolbox            # = cargo run -p ferrumtoolbox-desktop (binary: ferrum-toolbox)
```

End-user setup (DMG, GitHub sign-in, where apps land): see
[INSTALL.md](../INSTALL.md).

## Where versions come from

Every version of every app is a **GitHub Release** in one repository,
`Amygdal-doo/ferrum` by default (change it in Settings). The release *is*
the record: tag, version, date, notes and downloadable builds.

| Thing | Convention | Example |
| --- | --- | --- |
| Tag | `<app-id>-v<semver>` | `api-v0.2.0`, `term-v1.0.0-beta.1` |
| App ids | `ide`, `api`, `git`, `data`, `term`, `toolbox` | |
| Build | `<app-id>-<version>-<target>.tar.gz` with the binaries at the archive root | `api-0.2.0-aarch64-apple-darwin.tar.gz` |
| macOS bundle | `<app-id>-<version>-<target>.app.tar.gz` with `<App>.app` at the root; preferred on macOS | `api-0.2.0-aarch64-apple-darwin.app.tar.gz` |
| Windows build | `<app-id>-<version>-<target>.zip` with the `.exe` files and `<app-id>.ico` at the root; preferred on Windows | `api-0.2.0-x86_64-pc-windows-msvc.zip` |
| Installers (toolbox releases only; the other apps are installed through the toolbox) | `FerrumToolbox-<version>-<arch>.dmg`, `FerrumToolbox-<version>-x86_64-setup.exe`, `ferrum-toolbox_<version>_amd64.deb` | `FerrumToolbox-0.3.0-aarch64.dmg` |
| Checksum | `<asset>.sha256` | `api-0.2.0-aarch64-apple-darwin.tar.gz.sha256` |
| Targets | `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` (not for `term`) | |

Tags that don't match the pattern, and draft releases, are ignored.

### Release channels

Every release belongs to one channel, read from its version and GitHub's
prerelease flag:

| Channel | Rule | Example tags |
| --- | --- | --- |
| **Stable** | no semver prerelease part, not marked prerelease on GitHub | `data-v1.2.0` |
| **Beta** | `-beta.N`, `-rc.N` (or any other prerelease part), or marked prerelease on GitHub | `data-v1.3.0-beta.1`, `data-v1.3.0-rc.1` |
| **Alpha** | prerelease part starting with `alpha` | `data-v1.4.0-alpha.2` |

Each app has an **update channel**, set on its page: **Stable** (default)
offers only stable releases; **Beta** offers betas and stable releases, so a
stable release newer than the newest beta wins. Alpha builds are never
offered as updates, but they're listed and can be installed by hand. (Older
state files with "Include prereleases" turned on are migrated to Beta for
every app.)

### App status (`catalog.json`)

The status shown for each app (Planned, In development, Alpha, Beta,
Stable, Deprecated), its summary, highlights, roadmap and website come from
[`catalog.json`](../catalog.json) at the repo root. The toolbox fetches it
from the release repo's default branch on every update check (through the
contents API, with the same token) and caches it. A copy is compiled in as a
fallback, so the toolbox works before the file is pushed or when offline.

**To mark an app beta or stable, edit `catalog.json` on `main` and push.**
Every toolbox picks it up on its next check; no toolbox release is needed.

```json
{ "id": "data", "status": "beta", "summary": "…", "highlights": ["…"], "roadmap": ["…"], "homepage": "https://…" }
```

Valid statuses: `planned`, `in-development`, `alpha`, `beta`, `stable`,
`deprecated`. Entries with an unknown id or status are skipped, so a newer
catalog never breaks an older toolbox.

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
3. uploads `<build>.tar.gz` and, on macOS, the `.app.tar.gz` bundle from
   `scripts/bundle-macos.sh` (plus the toolbox DMG from `scripts/make-dmg.sh`),
   each with a `.sha256`.

The workflow needs a repository secret **`SUBMODULE_TOKEN`** with read access
to `Amygdal-doo/ferrum-ide`. Every app needs the `ide/` submodule because the
apps use its GPUI crates.

## Using the toolbox

- **Overview** (the start page): one row per app with its logo, status,
  installed version, latest stable, latest beta, last release date, total
  downloads (GitHub download counts, checksum files excluded) and the next
  action: **Install**, **Update**, **Open** or ✓ up to date. Click a row to
  open the app.
- **Releases:** every release of every app, newest first, with channel,
  date, the size of this platform's build and download count. Filter by app
  and channel; click a release to expand its notes.
- **Sidebar:** each app with its logo, status badge and one line:
  *v0.1.0 · active*, *v0.1.0 available*, *Update to vX* or *No releases yet*.
- **App page:** logo, status, summary, highlights and roadmap from the
  catalog, the **Stable | Beta** update channel, the **Launch**,
  **Install / Update to vX** and **Uninstall** buttons, then a list of every
  version with its channel, date, size, downloads and an active/installed
  marker, and **Install**, **Activate** or **Remove** buttons. Click a row to
  see that release's notes.
- **Downloads** show a progress bar with **Cancel**, and several apps can
  download at once.
- **Check for updates** runs on startup and then every *N* hours (Settings).
  A notice appears when new versions are found.
- **Activating** a version points `~/.ferrum/bin/<binary>` at it (for
  FerrumTerm, `fterm` too). On macOS it also publishes the version's bundle
  as `~/Applications/Ferrum/<App>.app`, an APFS clone rather than a
  symlink, because Launchpad and Spotlight skip symlinked apps. Add
  `~/.ferrum/bin` to your `PATH` to run `ferrumapi`, `ferrumgit`, … from a
  shell.
- **Launch** uses `open` on the `.app` on macOS; otherwise it starts the
  binary.
- **First launch without a token** shows *Sign in to GitHub*: use the gh CLI,
  paste a token (saved to the Keychain), or skip.
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
| Active app bundles (macOS) | `~/Applications/Ferrum/<App>.app` |
| PATH links | `~/.ferrum/bin/` |

On Linux the data lives under `~/.local/share/ferrumtoolbox/`, and the
active version of each app gets `~/.local/share/applications/ferrum-<app>.desktop`
plus an icon in `~/.local/share/icons/hicolor/256x256/apps/`. On Windows it
lives under `%LOCALAPPDATA%\Ferrum\FerrumToolbox\data\`; `~/.ferrum/bin`
holds `.cmd` launchers instead of symlinks, and the active version gets a
Start Menu shortcut under **Ferrum**. `state.json` is written atomically
(temp file + rename).

## Install safety

- When a release has a `.sha256` file, the download is checked against it,
  and a mismatch aborts the install.
- Archives are unpacked into a staging directory and moved into place with a
  rename, so a failed or cancelled install leaves nothing behind.
- Archive entries with absolute paths, `..`, or links that point outside the
  install directory abort the install. Zip archives may not contain links at
  all.

## Layout

- `crates/core` (`ferrumtoolbox-core`): catalog, GitHub client, installer and
  state store. It has no UI code and is unit-tested (`cargo test -p
  ferrumtoolbox-core`).
- `apps/desktop` (`ferrumtoolbox-desktop`): the GPUI app.

## Limitations

- Bundles and the DMG are ad-hoc signed, not notarized: a browser-downloaded
  DMG needs right-click → Open once. Apps the toolbox downloads aren't
  quarantined.
- FerrumToolbox can install new versions of itself (into
  `~/Applications/Ferrum`), but it doesn't replace `/Applications/Ferrum
  Toolbox.app` or restart.
- Windows builds are compiled in CI but haven't been tested by hand;
  FerrumTerm has no Windows build (Unix sockets). Start Menu shortcuts are
  created through PowerShell; if that fails the install still succeeds and
  the toolbox logs a warning.
