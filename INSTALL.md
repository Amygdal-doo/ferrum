# Installing Ferrum

You install **Ferrum Toolbox** once. It then installs, updates and rolls back
every other Ferrum app (Ferrum IDE, FerrumAPI, FerrumGIT, FerrumData,
FerrumTerm). macOS is the main target; Linux gets plain tarballs.

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

There's no DMG or `.app`. Releases ship `<app>-<version>-x86_64-unknown-linux-gnu.tar.gz`
with the binaries at the root. Run FerrumToolbox from that tarball. It
installs apps under `~/.local/share/ferrumtoolbox/apps/` and links them in
`~/.ferrum/bin`.

## For maintainers: how builds get there

```bash
scripts/release.sh data 0.3.0 -m "Release notes"
git push origin data-v0.3.0
```

`.github/workflows/release.yml` builds the app for each target and uploads
these assets, each with a `.sha256`:

| Asset | Platform |
| --- | --- |
| `<app>-<version>-<target>.tar.gz` | all (binaries at the root) |
| `<app>-<version>-<target>.app.tar.gz` | macOS: the `.app`, preferred by the toolbox |
| `FerrumToolbox-<version>-<arch>.dmg` | macOS, toolbox only |

The bundles come from `scripts/bundle-macos.sh` (Info.plist, the
`brand/icons/<app>.icns` icon, ad-hoc signature), and the DMG from
`scripts/make-dmg.sh`. You can run both locally:

```bash
cargo build --release -p ferrumdata-desktop
scripts/bundle-macos.sh data 0.3.0 aarch64-apple-darwin target/release dist
scripts/make-dmg.sh "dist/Ferrum Toolbox.app" dist/FerrumToolbox-0.3.0-aarch64.dmg   # toolbox only
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
- **Platforms:** Windows has no builds.
