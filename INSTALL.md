# Installing Ferrum

You install **Ferrum Toolbox** once. It then installs, updates and rolls back
every other Ferrum app (Ferrum IDE, FerrumAPI, FerrumGIT, FerrumData,
FerrumTerm). Every app is also published as a normal installer for each OS
(`.dmg`, `.exe`, `.deb`) if you'd rather install one by hand; see
[Install an app directly](#install-an-app-directly).

> **The releases are private.** They live in the private repo
> `Amygdal-doo/ferrum`, so you need a GitHub account with access to it, both
> to download the toolbox and for the toolbox to download apps.

## 1. Install Ferrum Toolbox

**Option A: disk image**

1. Sign in to GitHub in your browser and open the latest `toolbox-v…` release
   at <https://github.com/Amygdal-doo/ferrum/releases>.
2. Download `FerrumToolbox-<version>-aarch64.dmg` (Apple Silicon) or
   `FerrumToolbox-<version>-x86_64.dmg` (Intel).
3. Open it and drag **Ferrum Toolbox** onto **Applications**.
4. First launch: the app is ad-hoc signed, not notarized, so macOS blocks a
   plain double-click on downloaded copies. **Right-click → Open → Open**
   once, or run:
   ```bash
   xattr -dr com.apple.quarantine "/Applications/Ferrum Toolbox.app"
   ```

**Option B: script (uses the GitHub CLI)**

```bash
brew install gh        # if needed
gh auth login          # once; the repo is private
scripts/install-toolbox.sh
```

The script picks the latest `toolbox-v*` release for your CPU, checks the
`.sha256`, installs into `/Applications` (or `~/Applications` if that isn't
writable) and opens it. Files downloaded by `gh` aren't quarantined, so
there's no Gatekeeper prompt.

**Windows and Linux:** download `FerrumToolbox-<version>-x86_64-setup.exe`
(run it; SmartScreen: **More info → Run anyway**) or
`ferrum-toolbox_<version>_amd64.deb` (`sudo apt install ./ferrum-toolbox_<version>_amd64.deb`)
from the same release. See [Windows](#windows) and [Linux](#linux) below.

## 2. Sign in to GitHub (first launch)

If the toolbox finds no token, it shows **Sign in to GitHub**:

- **Use gh CLI:** works when `gh auth login` has been done. Nothing is
  stored; the toolbox asks `gh auth token` when it needs one.
- **Access token:** click **Create a token on GitHub**, make a fine-grained
  token for `Amygdal-doo` with **Contents: Read-only** on `ferrum`, paste
  it, then click **Save token**. It goes into the macOS Keychain (service
  `FerrumToolbox`), never into a file or a log.
- **Skip:** only useful if you point the toolbox at a public repo in
  Settings.

Lookup order on every check: Keychain, then `GITHUB_TOKEN`, then `gh auth
token`. Change or remove the token later in **Settings**.

## 3. Install apps

Select an app on the left and click **Install**. The toolbox:

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
   `ferrumdata`, `ferrumterm`, `fterm`, `zed`). Add that folder to your
   `PATH`:
   ```bash
   echo 'export PATH="$HOME/.ferrum/bin:$PATH"' >> ~/.zshrc
   ```

**Launch** opens the app through macOS (`open`), like double-clicking it.
Apps installed by the toolbox aren't quarantined, so they open without a
Gatekeeper prompt.

## 4. Updates and rollback

- The toolbox checks on startup and every 6 hours (configurable). Apps with a
  newer release get an **Update to vX** button.
- **Update** installs the new version and makes it active.
  `~/Applications/Ferrum/<App>.app` and `~/.ferrum/bin` switch over.
- **Rollback:** pick an older version in the versions list and click
  **Activate** (or **Install**, then **Activate**, if it isn't installed).
- The last 3 versions of each app are kept (Settings → *Versions to keep*; 0
  keeps all). The active version is never pruned.
- Turn on **Include prereleases** in Settings to see `-beta`/`-rc` builds.

## 5. Uninstall

- **One version:** click **Remove** in its row (activate another version
  first if it's the active one).
- **An app:** remove its last version. That also deletes
  `~/Applications/Ferrum/<App>.app` and its `~/.ferrum/bin` links.
- **Everything:**
  ```bash
  rm -rf "/Applications/Ferrum Toolbox.app" ~/Applications/Ferrum ~/.ferrum \
         ~/Library/Application\ Support/dev.Ferrum.FerrumToolbox
  security delete-generic-password -s FerrumToolbox   # saved token, if any
  ```

## Install an app directly

Each release (`<app>-v<version>` on the
[releases page](https://github.com/Amygdal-doo/ferrum/releases)) carries an
installer per OS. Apps installed this way don't update themselves; use the
toolbox for updates and rollback.

| OS | Download | Install | Uninstall |
| --- | --- | --- | --- |
| macOS (Apple Silicon / Intel) | `<App>-<version>-aarch64.dmg` / `…-x86_64.dmg`, e.g. `FerrumData-0.2.0-aarch64.dmg` | Open it and drag the app onto **Applications** | Drag the app from Applications to the Trash |
| Windows 10/11 (x64) | `<App>-<version>-x86_64-setup.exe`, e.g. `FerrumData-0.2.0-x86_64-setup.exe` | Run it. Installs for your user only (no admin) into `%LOCALAPPDATA%\Programs\Ferrum\<App>`, adds a Start Menu entry and optionally a desktop shortcut | **Settings → Apps → Installed apps → <App> → Uninstall** |
| Debian / Ubuntu (x64) | `<package>_<version>_amd64.deb`, e.g. `ferrumdata_0.2.0_amd64.deb` | `sudo apt install ./ferrumdata_0.2.0_amd64.deb`. Installs into `/opt/ferrum/<app>/`, puts the command on `PATH` (`/usr/bin`) and adds it to the app menu | `sudo apt remove ferrumdata` |

Package names on Linux: `ferrum-ide`, `ferrumapi`, `ferrumgit`, `ferrumdata`,
`ferrumterm`, `ferrum-toolbox`. Pre-release versions such as `0.2.0-beta.1`
become `0.2.0~beta.1` so apt orders them before the final release.

**First launch: nothing is code-signed yet.**

- **macOS:** the apps are ad-hoc signed, not notarized, so Gatekeeper blocks a
  plain double-click on a downloaded copy. **Right-click → Open → Open** once,
  or run `xattr -dr com.apple.quarantine "/Applications/<App>.app"`.
- **Windows:** SmartScreen shows "Windows protected your PC" for the setup
  file. Click **More info → Run anyway**.
- **Linux:** no prompt. GPUI needs a Vulkan driver (`mesa-vulkan-drivers` is
  recommended by the package).

**FerrumTerm is not available on Windows yet**: its server and clients talk
over Unix sockets.

## Where files live (macOS)

| What | Where |
| --- | --- |
| Toolbox | `/Applications/Ferrum Toolbox.app` |
| Active apps | `~/Applications/Ferrum/<App>.app` |
| All installed versions | `~/Library/Application Support/dev.Ferrum.FerrumToolbox/apps/<app>/<version>/<App>.app` |
| Settings and state | `…/dev.Ferrum.FerrumToolbox/state.json` (no token) |
| CLI links | `~/.ferrum/bin/` |
| Token | macOS Keychain, service `FerrumToolbox` |

## Linux

Install the toolbox with `sudo apt install ./ferrum-toolbox_<version>_amd64.deb`
(or run it from the `.tar.gz`). It downloads each app's
`<app>-<version>-x86_64-unknown-linux-gnu.tar.gz`, keeps versions under
`~/.local/share/ferrumtoolbox/apps/`, links the active one into
`~/.ferrum/bin` and adds a menu entry
(`~/.local/share/applications/ferrum-<app>.desktop` plus its icon).

## Windows

Install the toolbox with `FerrumToolbox-<version>-x86_64-setup.exe`. It
downloads each app's `<app>-<version>-x86_64-pc-windows-msvc.zip`, keeps
versions under `%LOCALAPPDATA%\Ferrum\FerrumToolbox\data\apps\`, writes
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
| `<app>-<version>-<target>.tar.gz` | macOS, Linux (binaries at the root) | toolbox (Linux), CLI |
| `<app>-<version>-<target>.app.tar.gz` | macOS: the `.app` | toolbox (macOS) |
| `<app>-<version>-x86_64-pc-windows-msvc.zip` | Windows: `.exe` files + `<app>.ico` | toolbox (Windows) |
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

```bash
cargo build --release -p ferrumdata-desktop
scripts/bundle-macos.sh data 0.3.0 aarch64-apple-darwin target/release dist
scripts/make-dmg.sh dist/FerrumData.app dist/FerrumData-0.3.0-aarch64.dmg FerrumData
```

The workflow needs the `SUBMODULE_TOKEN` secret (read access to
`Amygdal-doo/ferrum-ide`). See [toolbox/README.md](toolbox/README.md).

## Known limitations

- **Signing:** builds are ad-hoc signed only, with no Developer ID and no
  notarization. That's why the DMG needs right-click → Open the first time.
  Proper signing needs an Apple Developer account plus `codesign`/`notarytool`
  steps in the workflow.
- **Self-update:** the toolbox can install a newer version of itself (into
  `~/Applications/Ferrum/Ferrum Toolbox.app`), but it doesn't replace the
  copy in `/Applications` or restart. Quit it and open the new one.
- **Windows:** the builds have only been compiled in CI, not tested by hand
  yet. FerrumTerm has no Windows build. Ferrum IDE on Windows ships `zed.exe`
  only, without the extras Zed's own installer adds (bundled ConPTY, Explorer
  context menu, auto-update helper). Executables carry no embedded icon; the
  shortcuts use `<app>.ico`. SmartScreen warns because nothing is signed.
- **Linux:** only x86_64 `.deb` packages and tarballs (no arm64, RPM, Flatpak
  or AppImage).
