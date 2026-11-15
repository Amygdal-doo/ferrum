# FerrumLauncher

Installs, updates, switches between and launches the Ferrum apps
(FerrumIDE, FerrumEngineer, FerrumSystem, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm,
FerrumKit, FerrumContainers, FerrumPorts, FerrumVariables, any app added to the catalog later, and itself), in the style of JetBrains
Toolbox.
GPUI desktop app built on `ferrum-ui`. Up to 0.5.0 it was called **Ferrum
Toolbox**; see [Migration from Ferrum Toolbox](#migration-from-ferrum-toolbox).

```bash
cargo launcher           # = cargo run -p ferrumlauncher-desktop (binary: ferrum-launcher)
```

End-user setup (DMG, GitHub sign-in, where apps land): see
[INSTALL.md](../INSTALL.md).

## Opening apps without the launcher window
Each app's page has three choices, all off by default:

| Choice | What it does |
| --- | --- |
| **Shell command** | Writes a small script in `~/.ferrum/bin` named as typed (suggested: "Fe" plus the logo's capital — `fei` FerrumIDE, `fen` FerrumEngineer, `fes` FerrumSystem, `fea` FerrumAPI, `feg` FerrumGIT, `fed` FerrumData, `fet` FerrumTerm, `fek` FerrumKit, `fec` FerrumContainers, `fep` FerrumPorts, `fev` FerrumVariables, `fel` the launcher). `fei .` opens FerrumIDE on the current folder (like `zed .`), `feg .` FerrumGIT on that repository, `fet` FerrumTerm in a session in the current folder. Names: 2–20 lowercase letters, digits or `-`, not another app's command (its chosen one, or its suggested one while it has none), not `ferrum` or an app's CLI link, and not a file in `~/.ferrum/bin` Ferrum didn't write. The same choice is in every app's **Settings › System**; both read and write `~/.ferrum/preferences.json` (below), so they always agree. The first command turned on also adds `~/.ferrum/bin` to PATH in `~/.zprofile` and `~/.zshrc` (`~/.bash_profile` / `~/.bashrc` for bash, `conf.d/ferrum.fish` for fish) with one marked line; FerrumTerm and FerrumIDE's terminal have it on PATH anyway. Scripts not written by Ferrum are never replaced or removed. Enter in the name field renames an enabled command. At every start the launcher rewrites the enabled commands' scripts and removes the Ferrum scripts no app uses any more (`State::sync_shell_commands`, `ferrum_shell::ops::sync`). The 2026-11 renames moved the suggested names once (`commands_revision`; `State::migrate`, `ferrum_shell::prefs::rename_defaults`): Ferrum Editor's `fee` became FerrumIDE's `fei`, and FerrumVibe's `fev` and FerrumEnv's `fen` swapped (FerrumEngineer `fen`, FerrumVariables `fev`); names the user chose, and moves onto a name another app already has, are left alone. |
| **Hide from Dock** (macOS) | Listed in `~/.ferrum/preferences.json`, which every Ferrum app reads when it starts (`ferrum_ui::dock`); the app then runs as an accessory app (no Dock icon, not in ⌘-Tab) and carries its menus in a menu bar icon. The same switch is **Show in Dock** in each app's Settings › System, which applies at once. The launcher switches at once and moves to the menu bar. |
| **Shortcut** (macOS) | A global shortcut (⌘, ⌃ or ⌥ plus a letter, digit, F-key or space) that opens or brings forward the app from anywhere, while the launcher runs. |

**Settings → Show FerrumLauncher in the menu bar** (macOS) adds a menu bar icon listing the
installed apps (with their shortcuts), Open FerrumLauncher and Quit. With it on, closing the
window only hides the launcher, so shortcuts keep working.

### `ferrum <app> [path]`

Besides the short commands, the launcher installs `~/.ferrum/bin/ferrum`, one
command for every app: `ferrum ide .`, `ferrum engineer`, `ferrum git`,
`ferrum term`, `ferrum api`, `ferrum containers`, `ferrum data`,
`ferrum variables`, `ferrum ports`, `ferrum kit`, `ferrum system`,
`ferrum launcher` (app ids and the suggested short names work too, e.g.
`ferrum editor`, `ferrum fei`). `ferrum list` prints each app, its short
command (`-` while off) and whether it's installed; `ferrum help` the usage.
It runs the app's CLI link exactly as its short command does, and is
regenerated whenever a short command changes. It is on unless turned off in
an app's Settings › System (`ferrum_command: false`).

### `~/.ferrum/preferences.json`

The source of truth for shell commands and Dock icons, written by the
launcher and every app (`shared/shell`, crate `ferrum-shell`):

```json
{
  "hide_from_dock": ["ferrumeditor"],
  "shell_commands": { "editor": { "name": "fei", "enabled": true },
                      "git": { "name": "g", "enabled": false } },
  "commands_revision": 1,
  "ferrum_command": true
}
```

An app without an entry has its suggested name, turned off. On its first
start with this file the launcher moves its own choices there from
`state.json` (`Settings::app_prefs`, with its `commands_revision`); an app
that gets there first takes over the scripts it finds in `~/.ferrum/bin`.
After that `app_prefs` only mirrors the file (re-read whenever the launcher
window becomes active). Unknown fields are kept.

## Where versions come from

Every version of every app is a **GitHub Release** in one repository,
`Amygdal-doo/ferrum-releases` by default (change it in Settings). The release
*is* the record: tag, version, date, notes and downloadable builds.

| Thing | Convention | Example |
| --- | --- | --- |
| Tag | `<app-id>-v<semver>` | `api-v0.2.0`, `term-v1.0.0-beta.1` |
| App ids | 1–32 of `a-z`, `0-9`, `_`, starting with a letter (no `-`). Built in: `editor`, `api`, `git`, `data`, `term`, `kit`, `dock`, `launcher`; others come from the catalog. Retired: `ide` (see below) | |
| Build | `<app-id>-<version>-<target>.tar.gz` with the binaries at the archive root | `api-0.2.0-aarch64-apple-darwin.tar.gz` |
| macOS bundle | `<app-id>-<version>-<target>.app.tar.gz` with `<App>.app` at the root; preferred on macOS | `api-0.2.0-aarch64-apple-darwin.app.tar.gz` |
| Windows build | `<app-id>-<version>-<target>.zip` with the `.exe` files and `<app-id>.ico` at the root; preferred on Windows | `api-0.2.0-x86_64-pc-windows-msvc.zip` |
| Installers (launcher releases only; the other apps are installed through the launcher) | `FerrumLauncher-<version>-<arch>.dmg`, `FerrumLauncher-<version>-x86_64-setup.exe`, `ferrum-launcher_<version>_amd64.deb` | `FerrumLauncher-0.6.0-aarch64.dmg` |
| Checksum | `<asset>.sha256` | `api-0.2.0-aarch64-apple-darwin.tar.gz.sha256` |
| Targets | `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` (not for `term`) | |

Tags that don't match the pattern, and draft releases, are ignored. Releases
of ids this build doesn't know are kept; the catalog decides whether the app
is shown. `toolbox-v*` tags read as the launcher (its former id): the
launcher's own update check takes the newest of `launcher-v*` and
`toolbox-v*`, keeps one release per version (the `launcher-v*` one when both
exist) and only offers versions newer than the running one, so the bridge
release is never offered as a downgrade.

### Release channels

Every release belongs to one channel, read from its version and GitHub's
prerelease flag:

| Channel | Rule | Example tags |
| --- | --- | --- |
| **Stable** | no semver prerelease part, not marked prerelease on GitHub | `data-v1.2.0` |
| **Beta** | `-beta.N`, `-rc.N` (or any other prerelease part), or marked prerelease on GitHub | `data-v1.3.0-beta.1`, `data-v1.3.0-rc.1` |
| **Alpha** | prerelease part starting with `alpha` | `data-v1.4.0-alpha.2` |

Each app has an **update channel**, set on its page: **Stable** offers only
stable releases; **Beta** offers betas and stable releases; **Alpha**
(default) offers everything. A channel always takes a more stable release
when it's newer. Every Ferrum app is in alpha today, so the launcher moves
every app to Alpha once (`settings.channels_revision`); a channel picked
afterwards is kept. (Older state files with "Include prereleases" turned on
were first migrated to Beta.)

## The catalog (`catalog.json`)

Which apps exist, and their status (Planned, In development, Alpha, Beta,
Stable, Deprecated), summary, highlights, roadmap, website and install
metadata come from [`catalog.json`](../catalog.json) at the repo root. The
launcher fetches it from the release repo's default branch on every update
check (through the contents API, with the same token) and caches it in
`state.json`. A copy is compiled in, and the launcher also has a built-in
table for the apps it was built with, so it works before the file is pushed,
offline, or with an old catalog.

**Editing `catalog.json` on `main` and pushing it is enough** to change an
app's status or metadata, or to add an app: every launcher picks it up on its
next check; no launcher release is needed.

```json
{
  "id": "forge",
  "name": "FerrumForge",
  "status": "alpha",
  "summary": "Build runner for Rust workspaces.",
  "description": "Runs and caches cargo builds.",
  "binary": "ferrumforge",
  "extra_binaries": ["fforge"],
  "bundle_name": "FerrumForge",
  "icon": "Hammer",
  "logo": "https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/logos/forge.png",
  "order": 80,
  "highlights": ["…"],
  "roadmap": ["…"],
  "homepage": "https://…"
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | the tag/asset id (see above) |
| `status` | yes | `planned`, `in-development`, `alpha`, `beta`, `stable` or `deprecated` |
| `summary` | yes | one line on the overview and the app page |
| `highlights`, `roadmap`, `homepage` | no | app page |
| `name` | for new apps | display name |
| `description` | no | desktop entries and shortcuts; defaults to `summary` |
| `binary` | for new apps | main executable at the archive root or in `<bundle_name>.app/Contents/MacOS/` |
| `extra_binaries` | no | more executables that get `~/.ferrum/bin` links (`fterm`) |
| `bundle_name` | for new apps | macOS bundle name without `.app`, published as `~/Applications/Ferrum/<bundle_name>.app` |
| `former_names` | no | names the app was published under before a rename (`["FerrumDock"]` for FerrumContainers): a `<name>.app` copy in `~/Applications/Ferrum` or a `<name>.lnk` Start Menu shortcut with one is replaced by the current name's, at activation and at every start. Added to the built-in list; Launcher 0.8.1 and older ignore it |
| `icon` | no | a `ferrum_ui::components::IconName` (`ToolHammer` or `tool_hammer`); unknown names show a generic icon |
| `logo` | no | `https://` URL of a PNG (≤ 1 MiB), downloaded once and cached; falls back to the built-in logo, then the icon |
| `order` | no | position on the overview, ascending (built-ins use 10–70; default 1000) |

Values in the catalog override the built-in table. An app the launcher
doesn't know is installable, launchable and updatable like a built-in one
once its entry has `name`, `binary` and `bundle_name`; entries without them,
or with an invalid id, unknown status or unsafe file name, are skipped with a
log line. Unknown fields and malformed optional fields are ignored, so neither
an older nor a newer catalog breaks a launcher. Ferrum Toolbox 0.5.0 also
reads this file; it skips the `launcher` entry and ignores the new fields.

**Renaming an app** (FerrumDock → FerrumContainers): change `name` and
`bundle_name`, list the old bundle name in `former_names`, and keep `id`,
`binary`, the tags and the data directories, so installs update in place and
shell commands, Hide from Dock (keyed by binary) and shortcuts (keyed by id)
carry over. Launcher 0.8.1 reads such a catalog fine: it shows the new name,
launches the installed version from its install folder until it's updated,
and publishes the update as `<new>.app`, but leaves the `<old>.app` copy (and
on Windows the old `.lnk`) behind; later launchers remove it on their first
start.

**New apps:** the launcher remembers which apps the user has seen
(`settings.seen_apps` in `state.json`). An app that appears for the first
time gets a one-time "New app: <name> is available" banner with **Install**,
and a **New** badge on its overview row until it is opened or installed.

**Discontinued apps:** an installed app that is no longer offered (it left
the catalog, or the launcher retired it: Ferrum IDE, id `ide`, dropped in
favour of FerrumIDE) is not removed or hidden. It stays on the overview
below the offered apps with a **No longer offered** badge: Launch and
Uninstall keep working, but it gets no updates or new installs and never
counts as a new app. Retired ids are ignored in any catalog, including copies
cached by older launchers, and their install layout (Ferrum IDE: binary
`zed`, bundle `Ferrum IDE.app`) is kept in `catalog.rs` so an installed copy
can still be launched and removed.

**Logos:** release time must copy each app's `brand/png/<id>-256.png` to
`logos/<id>.png` on `main` of `Amygdal-doo/ferrum-releases`, where the `logo`
URLs point. Until then the built-in apps show their compiled-in logos.

**Adding an app:** publish its releases with the usual tags and asset names
(add it to `scripts/lib/apps.sh`, `scripts/bundle-macos.sh`,
`scripts/package-windows.ps1` and the workflow's `case`), add its logo to the
releases repo, then add its catalog entry with full metadata. Add it to the
built-in table in `crates/core/src/catalog.rs` with the next launcher release
so offline launchers know it too.

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
2. builds the app for the three targets;
3. uploads `<build>.tar.gz` and, on macOS, the `.app.tar.gz` bundle from
   `scripts/bundle-macos.sh` (plus the launcher DMG from `scripts/make-dmg.sh`),
   each with a `.sha256`.

The workflow needs a repository secret **`RELEASES_TOKEN`** with Contents: write
on the releases repository. GPUI comes from upstream Zed as a pinned git
dependency, so no other repository access is needed.

### Bridge release for Ferrum Toolbox 0.5.0

Ferrum Toolbox 0.5.0 (the version users have) only looks at `toolbox-v*`
releases. When **Update Toolbox to vX** is clicked it

1. downloads `toolbox-<ver>-<target>.app.tar.gz` (macOS prefers the bundle
   archive; it falls back to `toolbox-<ver>-<target>.tar.gz`) and checks its
   `.sha256`;
2. unpacks it and accepts it only if `ferrum-toolbox` is at the archive root
   or in the single `.app`'s `Contents/MacOS/`;
3. takes the single `*.app` at the root, whatever its name, copies it over
   the running `Ferrum Toolbox.app` (same path, via a detached script) and
   opens that path.

So each launcher version that should reach 0.5.0 users is also published as
`toolbox-v<ver>`, with the launcher's macOS bundle archives plus one root
entry `ferrum-toolbox -> Ferrum Launcher.app/Contents/MacOS/ferrum-launcher`
(a symlink, appended to the original tar, so the signed and notarized bundle
is untouched). 0.5.0 installs that, swaps `Ferrum Launcher.app` into
`…/Ferrum Toolbox.app` and opens it; the launcher then renames itself (see
below). `scripts/package-bridge.sh` makes these assets; the core test
`bridge_assets_satisfy_toolbox_0_5_0` runs the script and installs its output
with 0.5.0's rules.

Steps for 0.6.0 (repeat for later versions while 0.5.0 users remain):

```bash
scripts/release.sh launcher 0.6.0 -m "Ferrum Toolbox is now Ferrum Launcher"
git push origin launcher-v0.6.0          # CI publishes launcher-v0.6.0

# once CI is done:
gh release download launcher-v0.6.0 --repo Amygdal-doo/ferrum-releases \
  --dir dist --pattern 'launcher-0.6.0-*-apple-darwin.app.tar.gz*'
scripts/package-bridge.sh 0.6.0 dist dist/bridge
gh release create toolbox-v0.6.0 --repo Amygdal-doo/ferrum-releases --target main \
  --title "Ferrum Toolbox is now Ferrum Launcher" \
  --notes "Updates Ferrum Toolbox to Ferrum Launcher 0.6.0. Windows and Linux: install from launcher-v0.6.0." \
  dist/bridge/*
```

Do not mark it prerelease, and don't push a `toolbox-v*` tag to this repo
(the workflow refuses it). Also, on `main` of the releases repo, replace
`install.sh` with `scripts/install-launcher.sh` (it installs from `launcher-v*`,
falling back to `toolbox-v*`), add `logos/<id>.png`, and push the new
`catalog.json`. Windows and Linux builds of 0.5.0 can't update themselves;
their "Update" button opens the `toolbox-v0.6.0` release page, whose notes
point to the launcher installers.

## Migration from Ferrum Toolbox

On every start (each step is a no-op once done):

- **Data:** if the launcher has no `state.json` and Ferrum Toolbox's data
  directory has one, that directory is renamed to the launcher's (or copied
  when the rename fails, keeping the original); into an existing launcher
  directory it is merged entry by entry, `state.json` last, never replacing
  anything. On Unix the old path becomes a link to the new one. Launcher
  versions 0.5.0 installed (`apps/toolbox/`) move to `apps/launcher/`. Then
  the CLI links, Linux desktop entries and Windows shortcuts of every active
  app are pointed at the new directory.
- **State:** `toolbox` keys, releases and catalog entries read as `launcher`;
  settings, installed and active versions, channels and `channels_revision`
  are kept. A state file without the launcher's `format` marker came from the
  toolbox: FerrumKit, FerrumContainers and FerrumIDE, added after 0.5.0, are
  announced as new.
- **Token:** once, the GitHub token under keychain service `FerrumToolbox` is
  copied to `FerrumLauncher` unless that already has one; the old entry is
  kept.
- **Self-rename:** started from `Ferrum Toolbox.app` (after the bridge
  update), the launcher points `~/.ferrum/bin/ferrum-launcher` and, for this
  release, `ferrum-toolbox` at the renamed bundle (when the toolbox had a CLI
  link) and replaces a per-user `ferrum-toolbox.desktop`. It then starts a
  detached script and quits; the script waits for it to exit, renames the
  bundle to `Ferrum Launcher.app` in the same folder (an existing copy is
  replaced, and restored if the move fails) and opens it. If the folder isn't
  writable the launcher keeps running and shows a one-line notice; a failed
  move reopens the old bundle with `--ferrum-rename-failed`, so it doesn't
  loop. A plain `ferrum-toolbox` binary is renamed in place to
  `ferrum-launcher`, with the old name left as a link.
- **FerrumLauncher (2026-11):** the bundle is now `FerrumLauncher.app`
  (catalog `former_names`: `Ferrum Launcher`; binary and data unchanged). A
  self-update from a `Ferrum Launcher.app` copy swaps the new version in under
  the old name, as always; the new version then renames itself the same way
  (`self_rename::plan` treats every former name like `Ferrum Toolbox`), so an
  existing `FerrumLauncher.app` in that folder is replaced and only one copy
  is left.

## Using the launcher

- **Overview** (the start page): two sections, **Apps** and **Tools** (the
  catalog's `group`, `"apps"` or `"tools"`; missing or unknown reads as apps),
  then the launcher itself; one row per app, in catalog order, with
  its logo, a **New** badge for apps that just appeared, status, installed
  version, latest stable, latest alpha (or beta), last release date, total
  downloads (GitHub download counts, checksum files excluded) and the next
  action: **Install**, **Update**, **Open** or ✓ up to date. Click a row to
  open the app. There is no sidebar: the overview is the whole window.
- **App page** (**All apps** goes back): logo, status, summary, highlights and roadmap from the
  catalog, the **Stable | Beta | Alpha** update channel, the **Launch**,
  **Install / Update to vX** and **Uninstall** buttons, then a list of every
  version with its channel, date, size, downloads and an active/installed
  marker, and **Install**, **Activate** or **Remove** buttons. Click a row to
  see that release's notes.
- **Uninstall** (the app page's red button, or **Uninstall…** in a row's
  right-click menu on the overview and in the menu bar panel) asks first, then
  removes every installed version, the `~/Applications/Ferrum` copy (or the
  desktop entry / Start Menu shortcut) and the CLI links
  (`State::uninstall_app`). Settings and data in `~/.ferrum/<id>` and
  `~/.ferrum/<binary>` are kept unless **Also remove settings and data** is
  ticked. The launcher can't uninstall itself.
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
`repo`). The launcher looks for one in this order:

1. the OS keychain (service `FerrumLauncher`), which you set in **Settings**;
2. the `GITHUB_TOKEN` environment variable;
3. `gh auth token`, if the GitHub CLI is installed and logged in.

Without a token, only public repositories work, and GitHub's anonymous rate
limit (60 requests/hour) applies. The token is never written to `state.json`
or to logs. The status bar shows which source is in use.

## Local data

| What | Where (macOS) |
| --- | --- |
| State (settings, installed versions, cached releases and catalog, seen apps) | `~/Library/Application Support/dev.Ferrum.FerrumLauncher/state.json` |
| Installed versions | `…/dev.Ferrum.FerrumLauncher/apps/<app-id>/<version>/` |
| In-progress downloads | `…/dev.Ferrum.FerrumLauncher/downloads/` |
| Catalog logos | `…/dev.Ferrum.FerrumLauncher/logos/<app-id>-<url hash>.png` |
| Active app bundles (macOS) | `~/Applications/Ferrum/<App>.app` |
| PATH links | `~/.ferrum/bin/` |

On Linux the data lives under `~/.local/share/ferrumlauncher/`, and the
active version of each app gets `~/.local/share/applications/ferrum-<app>.desktop`
plus an icon in `~/.local/share/icons/hicolor/256x256/apps/`. On Windows it
lives under `%LOCALAPPDATA%\Ferrum\FerrumLauncher\data\`; `~/.ferrum/bin`
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

- `crates/core` (`ferrumlauncher-core`): catalog, GitHub client, installer,
  state store, self-update, and the migration from Ferrum Toolbox
  (`migrate.rs`, `self_rename.rs`, `auth::migrate_token`). It has no UI code
  and is unit-tested (`cargo test -p ferrumlauncher-core`).
- `apps/desktop` (`ferrumlauncher-desktop`): the GPUI app.

## Limitations

- Released bundles and the DMG are signed with the Developer ID and
  notarized. Local builds without `FERRUM_SIGN_IDENTITY` are ad-hoc signed and
  need right-click → Open once. Apps the launcher downloads aren't
  quarantined.
- Self-update (**Update Launcher to vX** in the toolbar) only works for a
  macOS `.app` in a folder the user can write to (an admin's
  `/Applications`, or `~/Applications`). A detached script waits for the
  launcher to quit, swaps the bundle (rolling back if the move fails) and
  reopens it. Elsewhere the button opens the release page instead.
- Windows builds are compiled in CI but haven't been tested by hand;
  FerrumTerm has no Windows build (Unix sockets). Start Menu shortcuts are
  created through PowerShell; if that fails the install still succeeds and
  the launcher logs a warning.
