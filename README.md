# Ferrum

Monorepo for the Ferrum family of Rust developer tools. All desktop tools
render with [GPUI](https://gpui.rs), the UI framework behind Zed, and share
one look through `shared/ui`.

| Path | Product | Stack |
| --- | --- | --- |
| `editor/` | FerrumIDE: the suite's code editor, lightweight and hackable, with Git, CI (GitHub Actions) and Logs pages | GPUI + `ferrum-ui` |
| `vibe/` | FerrumEngineer (formerly FerrumVibe): FerrumChat, FerrumGIT and FerrumTerm side by side in one window for agentic development | GPUI + `ferrum-ui` |
| `agent/` | Library only: FerrumChat (`ferrumagent-core`, `ferrumagent-ui`), the agentic coding chat embedded in FerrumIDE and FerrumEngineer | GPUI + `ferrum-ui` |
| `api/` | FerrumAPI: HTTP/API client | GPUI + `ferrum-ui` |
| `git/` | FerrumGIT: Git client | GPUI + `ferrum-ui` |
| `data/` | FerrumData: database IDE | GPUI + `ferrum-ui` |
| `term/` | FerrumTerm: terminal server, GUI and CLI | GPUI GUI, crossterm CLI |
| `kit/` | FerrumKit: developer utilities | GPUI + `ferrum-ui` |
| `dock/` | FerrumContainers: container manager | GPUI + `ferrum-ui` |
| `ports/` | FerrumPorts: listening ports and their processes | GPUI + `ferrum-ui` |
| `env/` | FerrumVariables: password-locked, shareable vault for project env variables | GPUI + `ferrum-ui` |
| `logs/` | Library only: `ferrumlogs-core` (log sources, parsing, filtering) behind FerrumIDE's Logs page | Rust (no UI) |
| `ci/` | Library only: `ferrumci-core` (GitHub Actions runs, logs, dispatch, PR checks) behind FerrumIDE's CI page | Rust (no UI) |
| `launcher/` | FerrumLauncher (formerly Ferrum Toolbox): installs, updates and launches the apps | GPUI + `ferrum-ui` |
| `shared/ui` | `ferrum-ui`: shared shell, theme, widgets, icons and fonts | GPUI |
| `shared/shell` | `ferrum-shell`: shell commands (`fei .`, `ferrum ide .`) and `~/.ferrum/preferences.json`, shared by the launcher and every app | Rust (no UI) |
| `web/` | Marketing site | axum + maud (standalone package) |

## Layout

- The root `Cargo.toml` is a single workspace for `shared/`, `api/`, `git/`,
  `data/`, `term/`, `kit/`, `dock/`, `editor/`, `ports/`, `env/`, `logs/`,
  `ci/`, `agent/`, `agentic/` and `launcher/`. It has one `Cargo.lock` and one toolchain
  (`rust-toolchain.toml`).
- GPUI comes from upstream [Zed](https://github.com/zed-industries/zed) as a
  git dependency pinned to one commit (`gpui`, `gpui_platform`, Apache-2.0;
  see `[workspace.dependencies]` in `Cargo.toml`), so GPUI is upgraded once,
  by bumping `rev`, for every tool. Cargo downloads it on the first build.
  Zed's GPL-3.0 / AGPL-3.0 crates (`ui`, `theme`, `menu`, `icons`, `assets`,
  `component`, ...) must never be linked or copied: Ferrum is MIT OR
  Apache-2.0. `scripts/check-licenses.sh` enforces this (see **Licence**
  below).
- `web/` is excluded from the workspace because it is deployed on its own
  (`web/Dockerfile`, `web/railway.toml`).
- The root `[patch.crates-io]` mirrors the GPUI-relevant patches from Zed's
  `Cargo.toml` at the pinned `rev` (a git dependency's own patches are not
  applied). Keep them in sync when bumping GPUI.

## `ferrum-ui`

`shared/ui` contains:

- `ferrum_ui::run(AppConfig, build)` boots GPUI with the Ferrum fonts,
  icons and the Ferrum Dark theme, opens the main window and wires up Quit.
- `ferrum_ui::theme`: `Theme` (colour slots in `colors()`, `status()`,
  `players()`, `syntax()`), `ActiveTheme` (`cx.theme()`), `GlobalTheme`,
  the built-in `dark()` / `light()` themes and the font settings
  (`ThemeSettings`, `theme_settings(cx)`).
- `ferrum_ui::components`: `Button`, `IconButton`, `Label`, `LoadingLabel`,
  `Headline`, `Icon` / `IconName`, `Checkbox`, `Divider`, `ProgressBar`,
  `ListItem`, `Tab` / `TabBar`, `Tooltip`, `ContextMenu`, `PopoverMenu`,
  `right_click_menu`, `DropdownMenu`, the builder traits (`Clickable`,
  `Disableable`, `Toggleable`, `ButtonCommon`, `LabelCommon`, ...) and
  `h_flex` / `v_flex`, `StyledTypography`.
- `ferrum_ui::menu`: the `Cancel` / `Confirm` / `SelectNext` /
  `SelectPrevious` actions.
- `ferrum_ui::assets`: Lucide icons and the IBM Plex Sans / Lilex fonts,
  embedded at compile time.
- `TextInput`: single-line, multi-line, masked and monospace editable
  text. It emits `Changed`, `Submit` and `Blurred`.
- `splitter` + `DraggedSplitter` for resizable panes.
- `ProjectBar`: the strip at the top of the windows with the current project
  (name, path, git branch), a switcher over the recent projects shared by
  every app (`~/.ferrum/projects.json`, ⌥⌘O) and buttons that open the project
  in FerrumIDE, FerrumGIT or FerrumTerm. The data side is
  `ferrum_ui::project`.
- `Modal`, `Notice` / `render_notice`, `toolbar`, `status_bar`,
  `section_header`, `badge`, `kbd` and `empty_state`.
- Onboarding and Settings, shared by every app. Each app describes itself
  with `AppInfo` (passed to `AppConfig::app`): the keys worth knowing,
  macOS permissions, where users migrate from, notification events and
  its own settings pages.
  - `onboarding`: the first-launch walkthrough. It covers welcome, look and
    feel, integrations and permissions, agent-driven migration and the
    keys worth knowing. Finished apps are listed in
    `~/.ferrum/onboarding.json`. `FERRUM_ONBOARDING=1` shows it again; so
    does the Show Onboarding… menu item.
  - `settings`: the Settings window (⌘,). It has the Interface, Themes,
    Shortcuts, Integrations and Notifications pages plus the app's own
    (FerrumTerm adds Terminal and Remote Hosts). Pages are built from
    `settings::widgets`. `FERRUM_OPEN_SETTINGS=<page>` opens it at start.
  - `appearance`: colour scheme (Auto / Light / Dark), dark and light
    theme, interface style (Solid / Gradient / Glass), toolbar density and
    text size. Stored in
    `~/.ferrum/appearance.json` and followed live by every running app.
    `theme::presets` holds the built-in themes.
  - `surface`: the window backdrop for the interface style, and the pane
    cards (`pane_frame`, `pane_header`, `gap_splitter`) that FerrumIDE,
    FerrumEngineer and FerrumTerm lay out on it.
  - `notify`: in-app toasts, system notifications, Dock badge and sound,
    switchable per event, stored in `~/.ferrum/<binary>/notifications.json`.
  - `keymap`: shortcuts recorded in Settings, stored in
    `~/.ferrum/<binary>/keymap.json`.
  - `integrations`: coding-agent detection, the Ferrum agent skill, the
    migration prompt and permission checks.
  - `prefs`: JSON files under `~/.ferrum` (`$FERRUM_HOME` overrides).
- `ferrum_ui::prelude::*` re-exports GPUI and the components
  (`Button`, `Label`, `ListItem`, `ContextMenu`, `PopoverMenu`, `TabBar`, …).
- `ferrum_ui::init_for_tests(cx)` installs the theme in headless GPUI tests.

Widget gallery: `cargo run -p ferrum-ui --example gallery`.

## Licence

Ferrum is dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT licence ([`LICENSE-MIT`](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option. This covers everything in this repository except the
third-party works listed in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md);
the workspace crates declare `license = "MIT OR Apache-2.0"` through
`[workspace.package]`.

Everything the apps link must stay under a permissive licence (MIT,
Apache-2.0, BSD, ISC, Zlib, OFL for fonts, ...) so the binaries can be
distributed under these terms:

- Run `scripts/check-licenses.sh` (also in CI next to `cargo lint`) before
  adding a dependency; it fails if any GPL / AGPL / LGPL-only crate is in the
  normal or build dependency graph of a binary under `*/apps/*`. Dual-licensed
  crates with a permissive option (e.g. `r-efi`, `self_cell`) pass.
- From Zed, only GPUI (`gpui`, `gpui_platform` and their Apache-2.0
  internals) may be used. Do not copy code from Zed's GPL crates.
- Bundled third-party assets and their licences are listed in
  [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## Commands

```bash
cargo api        # FerrumAPI
cargo git-ui     # FerrumGIT
cargo desktop    # FerrumData
cargo term       # FerrumTerm GUI
cargo fterm      # FerrumTerm CLI
cargo kit        # FerrumKit
cargo dock       # FerrumContainers
cargo editor     # FerrumIDE
cargo ports      # FerrumPorts
cargo env-ui     # FerrumVariables
cargo vibe       # FerrumEngineer (cargo vibe <folder>)
cargo launcher   # FerrumLauncher
cargo lint       # clippy, whole workspace, -D warnings
cargo test --workspace
scripts/check-licenses.sh   # no GPL-family crate in any shipped binary

# Website
cd web && cargo run
```

The first build compiles GPUI, which takes a few minutes. Dependencies build at
`opt-level = 2` even in dev, because GPUI is too slow unoptimized.

Each product folder keeps its own `README.md` and `docs/`; the licence is
the root one.

## Shell commands

Every app can be opened from a terminal, like `zed .`. Turn its command on
in the app's **Settings › System** (or on its page in FerrumLauncher; both
show the same choice):

| App | Short command | Long form |
| --- | --- | --- |
| FerrumIDE | `fei` | `ferrum ide` |
| FerrumEngineer | `fen` | `ferrum engineer` |
| FerrumGIT | `feg` | `ferrum git` |
| FerrumTerm | `fet` | `ferrum term` |
| FerrumAPI | `fea` | `ferrum api` |
| FerrumContainers | `fec` | `ferrum containers` |
| FerrumData | `fed` | `ferrum data` |
| FerrumVariables | `fev` | `ferrum variables` |
| FerrumPorts | `fep` | `ferrum ports` |
| FerrumKit | `fek` | `ferrum kit` |
| FerrumSystem | `fes` | `ferrum system` |
| FerrumLauncher | `fel` | `ferrum launcher` |

```bash
fei .                 # this folder in FerrumIDE
ferrum ide .          # the same, without remembering `fei`
ferrum git ~/code/x   # FerrumGIT on a repository
fet                   # FerrumTerm in this folder
ferrum list           # every app, its short command and whether it's installed
ferrum help
```

- Short commands can be renamed (2–20 lowercase letters, digits or `-`; not
  another app's command or a file in `~/.ferrum/bin` Ferrum didn't write) and
  turned on or off; the change takes effect at once.
- `ferrum` is installed by the launcher and is on unless turned off in
  Settings › System. It also accepts app ids and short commands
  (`ferrum editor`, `ferrum fei`).
- The commands are small scripts in `~/.ferrum/bin`, which the first command
  turned on adds to `PATH` (one marked line in `~/.zprofile` and `~/.zshrc`,
  or bash's / fish's files); **Settings › System › Add to PATH** does it too.
- The choices live in `~/.ferrum/preferences.json` (`shell_commands`,
  `ferrum_command`, `hide_from_dock`), read and written by the launcher and
  every app (`shared/shell`).
- **Settings › System › Show in Dock** (macOS) runs the app without a Dock
  icon; its menus then move to an icon in the menu bar.

## Contributing

Contributions are welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for how
to build, test and send a pull request, [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)
for the community rules and [`SECURITY.md`](SECURITY.md) for reporting
vulnerabilities privately.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in Ferrum by you, as defined in the Apache-2.0
licence, shall be dual-licensed as above, without any additional terms or
conditions.

## Releases

Each app is versioned on its own, through GitHub Releases tagged
`<app-id>-v<semver>` (for example `api-v0.2.0`):

```bash
scripts/release.sh api 0.2.0 -m "Release notes"
git push origin api-v0.2.0     # runs .github/workflows/release.yml
```

FerrumLauncher reads these releases to install and update the apps; a new
app shows up in it once it has an entry in `catalog.json`. For
installing Ferrum on a machine (DMG, GitHub sign-in, Launchpad apps), see
**[INSTALL.md](INSTALL.md)**. See
`launcher/README.md` for the conventions, the catalog schema, the secrets
that the workflow needs, and the bridge release for Ferrum Toolbox 0.5.0.
