# Installing Ferrum

You install **FerrumLauncher** once. It then installs, updates and rolls back
every other Ferrum app (FerrumIDE, FerrumEngineer, FerrumAPI, FerrumGIT,
FerrumData, FerrumTerm, FerrumKit, FerrumContainers, and apps published later).
Only the launcher has an installer (`.dmg`, `.exe`, `.deb`); the other apps are
installed through it and have no separate download.

FerrumLauncher (Ferrum Launcher until 2026-11) was called **Ferrum Toolbox** up to 0.5.0. See
[Coming from Ferrum Toolbox](#coming-from-ferrum-toolbox).

The Zed-based Ferrum IDE is no longer offered; FerrumIDE (formerly Ferrum Editor) is the suite's code editor
(`ferrumeditor path[:line[:col]]`). A copy of Ferrum IDE installed earlier is
listed in the launcher as **No longer offered**: it still launches and can be
uninstalled, but gets no updates.

Releases are public, in <https://github.com/Amygdal-doo/ferrum-releases>, so
no GitHub account is needed. Every app is in **alpha**.

## 1. Install FerrumLauncher

**Option A: Terminal**

```bash
curl -fsSL https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/install.sh | bash
```

It picks the newest `launcher-v*` release for your CPU, checks the `.sha256`,
installs into `/Applications` (or `~/Applications` if that isn't writable)
and opens it. Files downloaded with curl aren't quarantined, so macOS doesn't
show the "could not verify" dialog. The same script is
`scripts/install-launcher.sh` in this repo.

**Option B: disk image**

1. Download `FerrumLauncher-<version>-aarch64.dmg` (Apple Silicon) from the
   newest `launcher-v…` release at
   <https://github.com/Amygdal-doo/ferrum-releases/releases>.
2. Open it and drag **FerrumLauncher** onto **Applications**.
3. Open it. Releases are signed with the AMYGDAL d.o.o. Sarajevo Developer
   ID and notarized by Apple, so macOS opens them without a warning.

You do this once. The launcher updates itself in place afterwards, and
neither its updates nor the apps it installs are quarantined.

**Windows and Linux:** download `FerrumLauncher-<version>-x86_64-setup.exe`
(run it; SmartScreen: **More info → Run anyway**) or
`ferrum-launcher_<version>_amd64.deb` (`sudo apt install ./ferrum-launcher_<version>_amd64.deb`)
from the same release. See [Windows](#windows) and [Linux](#linux) below.

## 2. GitHub sign-in (only for a private repo)

The default public repo needs no sign-in. If you point the launcher at a
private repo in **Settings**, it asks for a token: **Use gh CLI** (after
`gh auth login`) or paste a fine-grained token with **Contents: Read-only**;
it goes into the macOS Keychain (service `FerrumLauncher`), never a file.

## 3. Install apps

Click an app in the overview, then **Install**. The launcher:

1. downloads the macOS build (`<app>-<version>-<target>.app.tar.gz`) with a
   progress bar you can cancel;
2. checks it against the published `.sha256`, and refuses to install on a
   mismatch;
3. unpacks it into its version store;
4. publishes the active version as **`~/Applications/Ferrum/<App>.app`**, so
   it appears in Launchpad and Spotlight. This is an APFS clone (`cp -c`),
   so it takes no extra disk space. It's a real bundle rather than a symlink
   because Launchpad ignores symlinked apps;
5. links its executables into **`~/.ferrum/bin`** (`ferrumapi`, `ferrumgit`,
   `ferrumdata`, `ferrumterm`, `fterm`, `ferrumkit`, `ferrumdock`,
   `ferrumeditor`).
   Add that folder to your
   `PATH`:
   ```bash
   echo 'export PATH="$HOME/.ferrum/bin:$PATH"' >> ~/.zshrc
   ```

   Turning on a shell command (below) does this for you.

**Shell commands.** Each app can be opened from a terminal: turn on its
command in the app's **Settings › System** (or on its page in the
launcher) and `fei .` opens the current folder in FerrumIDE, `feg .` in
FerrumGIT, `fet` FerrumTerm there, and so on. The launcher also installs one
command for all of them, `ferrum <app> [path]` (`ferrum ide .`,
`ferrum git`, `ferrum list`, `ferrum help`), so the short names are
optional. Short commands can be renamed or turned off at any time; the
scripts in `~/.ferrum/bin` change at once. The choices (and Show in Dock)
are kept in `~/.ferrum/preferences.json`, shared by the launcher and every
app. See the [README](README.md#shell-commands) for the full list.

**Launch** opens the app through macOS (`open`), like double-clicking it.
Apps installed by the launcher aren't quarantined, so they open without a
Gatekeeper prompt.

**New apps** appear on their own: when an app is added to the catalog, the
launcher shows it with a **New** badge and a one-time "New app: … is
available" banner with **Install**. No launcher update is needed.

## 4. Updates and rollback

- The launcher checks on startup and every 6 hours (configurable). Apps with a
  newer release get an **Update to vX** button.
- **The launcher itself:** when a newer launcher is released, the toolbar
  shows **Update Launcher to vX**. It downloads the new version, replaces
  `/Applications/FerrumLauncher.app` and restarts. No DMG, no Gatekeeper
  prompt.
  A copy still called `Ferrum Launcher.app` (the name before 2026-11) renames
  itself to `FerrumLauncher.app` after that update, replacing any other copy
  with the new name in the same folder, so one copy is left.
- **Uninstall an app:** **Uninstall…** on the app's page (or in its
  right-click menu) removes every installed version, its app and its
  command-line links. Its settings and data in `~/.ferrum` are kept unless
  you tick **Also remove settings and data**.
- **Update** installs the new version and makes it active.
  `~/Applications/Ferrum/<App>.app` and `~/.ferrum/bin` switch over.
- **Rollback:** pick an older version in the versions list and click
  **Activate** (or **Install**, then **Activate**, if it isn't installed).
- The last 3 versions of each app are kept (Settings → *Versions to keep*; 0
  keeps all). The active version is never pruned.
- **Update channels:** every app follows **Alpha** by default, since all
  Ferrum releases are alpha (`<app>-v<version>-alpha.N`). Switch an app to
  **Beta** or **Stable** on its page to only get those once they exist. Each
  channel also takes the more stable ones.

## 5. Uninstall

- **One version:** click **Remove** in its row (activate another version
  first if it's the active one).
- **An app:** remove its last version. That also deletes
  `~/Applications/Ferrum/<App>.app` and its `~/.ferrum/bin` links.
- **Everything:**
  ```bash
  rm -rf "/Applications/FerrumLauncher.app" ~/Applications/Ferrum ~/.ferrum \
         ~/Library/Application\ Support/dev.Ferrum.FerrumLauncher
  security delete-generic-password -s FerrumLauncher   # saved token, if any
  ```
  If you used Ferrum Toolbox before, also remove
  `~/Library/Application Support/dev.Ferrum.FerrumToolbox` (a link to the
  launcher's folder after the move) and run
  `security delete-generic-password -s FerrumToolbox`.

## Coming from Ferrum Toolbox

Ferrum Toolbox 0.5.0 updates itself to FerrumLauncher: click **Update
Toolbox to v0.6.0** as usual. Then, on the first start, the launcher

- renames `Ferrum Toolbox.app` to `FerrumLauncher.app` in the same folder
  and restarts from there (if the folder isn't writable it keeps running and
  says so; drag it to the new name yourself or reinstall);
- moves its data from `dev.Ferrum.FerrumToolbox` to `dev.Ferrum.FerrumLauncher`
  (installed apps, active versions, update channels, settings) and leaves a
  link at the old path;
- copies the GitHub token from the `FerrumToolbox` keychain entry to
  `FerrumLauncher` (macOS may ask once whether FerrumLauncher may read it);
- points `~/.ferrum/bin/ferrum-launcher` at itself and keeps
  `ferrum-toolbox` as a link for this release;
- shows FerrumKit, FerrumContainers and FerrumIDE, added after 0.5.0, as **New**.

Installed apps, `~/Applications/Ferrum` and the other `~/.ferrum/bin` links
keep working throughout. The `install.sh` one-liner installs the launcher
and moves an old `Ferrum Toolbox.app` to the Trash.

## First launch and platforms

**First launch.**

- **macOS:** released builds are signed with the AMYGDAL d.o.o. Sarajevo
  Developer ID and notarized by Apple, so they open without a warning. A build
  you make yourself without `FERRUM_SIGN_IDENTITY` is only ad-hoc signed; open
  it once with **System Settings → Privacy & Security → Open Anyway**, or run
  `xattr -dr com.apple.quarantine "/Applications/FerrumLauncher.app"`.
- **Windows:** SmartScreen shows "Windows protected your PC" for the setup
  file. Click **More info → Run anyway**.
- **Linux:** no prompt. GPUI needs a Vulkan driver (`mesa-vulkan-drivers` is
  recommended by the package).

**FerrumTerm is not available on Windows yet**: its server and clients talk
over Unix sockets.

## Where files live (macOS)

| What | Where |
| --- | --- |
| Launcher | `/Applications/FerrumLauncher.app` |
| Active apps | `~/Applications/Ferrum/<App>.app` |
| All installed versions | `~/Library/Application Support/dev.Ferrum.FerrumLauncher/apps/<app>/<version>/<App>.app` |
| Settings and state | `…/dev.Ferrum.FerrumLauncher/state.json` (no token) |
| Catalog logos | `…/dev.Ferrum.FerrumLauncher/logos/` |
| CLI links and shell commands | `~/.ferrum/bin/` |
| Shell commands, Show in Dock | `~/.ferrum/preferences.json` |
| Token | macOS Keychain, service `FerrumLauncher` |

## Linux

Install the launcher with `sudo apt install ./ferrum-launcher_<version>_amd64.deb`
(or run it from the `.tar.gz`); the package replaces `ferrum-toolbox`. It
downloads each app's `<app>-<version>-x86_64-unknown-linux-gnu.tar.gz`, keeps
versions under `~/.local/share/ferrumlauncher/apps/` (moved from
`~/.local/share/ferrumtoolbox/` on first start), links the active one into
`~/.ferrum/bin` and adds a menu entry
(`~/.local/share/applications/ferrum-<app>.desktop` plus its icon).

## Windows

Install the launcher with `FerrumLauncher-<version>-x86_64-setup.exe`. It
downloads each app's `<app>-<version>-x86_64-pc-windows-msvc.zip`, keeps
versions under `%LOCALAPPDATA%\Ferrum\FerrumLauncher\data\apps\` (moved from
`…\FerrumToolbox\data` on first start; uninstall the old Ferrum Toolbox from
**Apps & features** afterwards), writes
`.cmd` launchers into `%USERPROFILE%\.ferrum\bin` (add it to your `PATH` to
use the commands) and a Start Menu shortcut in **Ferrum** for the active
version. The token goes into Windows Credential Manager.

## For maintainers: how builds get there

```bash
scripts/release.sh data 0.3.0 -m "Release notes"
git push origin data-v0.3.0
```

`.github/workflows/release.yml` builds the app for each target and uploads
these assets, each with a `.sha256`:

| Asset | Platform | Used by |
| --- | --- | --- |
| `<app>-<version>-<target>.tar.gz` | macOS, Linux (binaries at the root) | launcher (Linux), CLI |
| `<app>-<version>-<target>.app.tar.gz` | macOS: the `.app` | launcher (macOS) |
| `<app>-<version>-x86_64-pc-windows-msvc.zip` | Windows: `.exe` files + `<app>.ico` | launcher (Windows) |
| `<App>-<version>-<arch>.dmg` | macOS | people |
| `<package>_<version>_amd64.deb` | Debian/Ubuntu | people |
| `<App>-<version>-x86_64-setup.exe` | Windows (Inno Setup, per-user) | people |

The scripts behind them run locally too:

| Script | Makes |
| --- | --- |
| `scripts/bundle-macos.sh <app> <version> <target> <binary-dir> <out-dir>` | `.app` (Info.plist, `brand/icons/<app>.icns`, ad-hoc signature) |
| `scripts/make-dmg.sh <App.app> <out.dmg> [volume]` | drag-to-install `.dmg` |
| `scripts/package-deb.sh <app> <version> <binary-dir> <out-dir>` | `.deb` (uses `dpkg-deb` when present; builds the archive itself elsewhere) |
| `scripts/package-windows.ps1 -App <app> -Version <v> -BinaryDir <dir> -OutDir <dir>` | Windows `.zip` + Inno Setup `.exe` (`packaging/windows/ferrum.iss`) |
| `scripts/package-bridge.sh <version> <assets-dir> [out-dir]` | the `toolbox-v<version>` assets Ferrum Toolbox 0.5.0 updates from (see [launcher/README.md](launcher/README.md)) |

```bash
cargo build --release -p ferrumdata-desktop
scripts/bundle-macos.sh data 0.3.0 aarch64-apple-darwin target/release dist
scripts/make-dmg.sh dist/FerrumData.app dist/FerrumData-0.3.0-aarch64.dmg FerrumData
```

The workflow needs the `RELEASES_TOKEN` secret (see the header of
`.github/workflows/release.yml`). GPUI is a pinned git dependency on upstream
Zed, so no other repository access is needed. See
[launcher/README.md](launcher/README.md).

## Known limitations

- **Signing:** released macOS builds are signed with the AMYGDAL d.o.o. Sarajevo Developer ID
  (hardened runtime, `packaging/macos/ferrum.entitlements`) and notarized by
  Apple, and opens without a warning.
  - Local releases: set `FERRUM_SIGN_IDENTITY="Developer ID Application: AMYGDAL
    d.o.o. Sarajevo (24XNX2CBH8)"` and `FERRUM_NOTARY_PROFILE=ferrum-notary`
    (created once with `xcrun notarytool store-credentials ferrum-notary`).
    `scripts/bundle-macos.sh` and `scripts/make-dmg.sh` sign;
    `scripts/notarize-macos.sh` notarizes and staples.
  - CI: add the repository secrets `MACOS_CERTIFICATE_P12` (base64 of the
    exported `.p12`), `MACOS_CERTIFICATE_PASSWORD`, `NOTARY_API_KEY` (contents
    of the App Store Connect `.p8`), `NOTARY_API_KEY_ID` and
    `NOTARY_API_ISSUER`. The release workflow signs and notarizes when they
    exist.
- **Self-update** on macOS needs write access to the folder the launcher is
  in (an admin user's `/Applications` is fine). On Windows and Linux a
  launcher installed with the setup.exe or the `.deb` downloads and opens the
  new installer (untested on real Windows and Linux machines, see
  [launcher/README.md](launcher/README.md#windows-and-linux-what-is-and-isnt-verified)).
  Otherwise the button opens the release page instead.
- **Windows:** the builds have only been compiled in CI, not tested by hand
  yet. FerrumTerm has no Windows build. Executables carry no embedded icon; the
  shortcuts use `<app>.ico`. SmartScreen warns because nothing is signed.
- **Linux:** only x86_64 `.deb` packages and tarballs (no arm64, RPM, Flatpak
  or AppImage).

## Building from source

Ferrum is open source under MIT OR Apache-2.0 (see [`LICENSE-MIT`](LICENSE-MIT)
and [`LICENSE-APACHE`](LICENSE-APACHE)). To build an app yourself instead of
installing a release, see [`CONTRIBUTING.md`](CONTRIBUTING.md): with
[rustup](https://rustup.rs) installed, `cargo run --release -p <package>`
from the repository root (for example `ferrumgit-desktop`) is enough; the first
build downloads and compiles GPUI from upstream Zed.
