# Ferrum

Monorepo for the Ferrum family of Rust developer tools. All desktop tools
render with [GPUI](https://gpui.rs), the UI framework behind Zed, and share
one look through `shared/ui`.

| Path | Product | Stack |
| --- | --- | --- |
| `editor/` | Ferrum Editor: the suite's code editor, lightweight and hackable, with Git, CI (GitHub Actions) and Logs pages | GPUI + `ferrum-ui` |
| `api/` | FerrumAPI: HTTP/API client | GPUI + `ferrum-ui` |
| `git/` | FerrumGIT: Git client | GPUI + `ferrum-ui` |
| `data/` | FerrumData: database IDE | GPUI + `ferrum-ui` |
| `term/` | FerrumTerm: terminal server, GUI and CLI | GPUI GUI, crossterm CLI |
| `kit/` | FerrumKit: developer utilities | GPUI + `ferrum-ui` |
| `dock/` | FerrumContainers: container manager | GPUI + `ferrum-ui` |
| `ports/` | FerrumPorts: listening ports and their processes | GPUI + `ferrum-ui` |
| `env/` | FerrumEnv: password-locked, shareable vault for project env variables | GPUI + `ferrum-ui` |
| `logs/` | Library only: `ferrumlogs-core` (log sources, parsing, filtering) behind Ferrum Editor's Logs page | Rust (no UI) |
| `ci/` | Library only: `ferrumci-core` (GitHub Actions runs, logs, dispatch, PR checks) behind Ferrum Editor's CI page | Rust (no UI) |
| `launcher/` | Ferrum Launcher (formerly Ferrum Toolbox): installs, updates and launches the apps | GPUI + `ferrum-ui` |
| `shared/ui` | `ferrum-ui`: shared shell, theme, widgets, icons and fonts | GPUI |
| `web/` | Marketing site | axum + maud (standalone package) |
| `ide/` | Library only: the Zed codebase (git submodule) that provides GPUI (Apache-2.0). Not a product: Ferrum IDE was dropped in favour of Ferrum Editor | GPUI (own Cargo workspace) |

## Layout

- The root `Cargo.toml` is a single workspace for `shared/`, `api/`, `git/`,
  `data/`, `term/`, `kit/`, `dock/`, `editor/`, `ports/`, `env/`, `logs/`,
  `ci/` and `launcher/`. It has one `Cargo.lock` and one toolchain
  (`rust-toolchain.toml`).
- `ide/` keeps its own workspace and toolchain so merges from upstream Zed
  stay easy. Nothing is built or released from it on its own. The other tools
  depend only on its `gpui` and `gpui_platform` crates (Apache-2.0) by path,
  so GPUI is upgraded once, in `ide/`, for every tool. Zed's GPL-3.0 crates
  (`ui`, `theme`, `menu`, `icons`, `assets`, `component`, ...) must never be
  linked: the apps ship closed source. `scripts/check-licenses.sh` enforces
  this (see **Licences** below).
- `web/` is excluded from the workspace because it is deployed on its own
  (`web/Dockerfile`, `web/railway.toml`).
- The root `[patch.crates-io]` mirrors the GPUI-relevant patches from
  `ide/Cargo.toml`. Keep them in sync when updating `ide/`.

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
  in Ferrum Editor, FerrumGIT or FerrumTerm. The data side is
  `ferrum_ui::project`.
- `Modal`, `Notice` / `render_notice`, `toolbar`, `status_bar`,
  `section_header`, `badge`, `kbd` and `empty_state`.
- `ferrum_ui::prelude::*` re-exports GPUI and the components
  (`Button`, `Label`, `ListItem`, `ContextMenu`, `PopoverMenu`, `TabBar`, …).
- `ferrum_ui::init_for_tests(cx)` installs the theme in headless GPUI tests.

Widget gallery: `cargo run -p ferrum-ui --example gallery`.

## Licences

The Ferrum apps are closed source. Everything they link must be under a
permissive licence (MIT, Apache-2.0, BSD, ISC, Zlib, OFL for fonts, ...):

- Run `scripts/check-licenses.sh` (also in CI next to `cargo lint`) before
  adding a dependency; it fails if any GPL / AGPL / LGPL-only crate is in the
  normal or build dependency graph of a binary under `*/apps/*`. Dual-licensed
  crates with a permissive option (e.g. `r-efi`, `self_cell`) pass.
- From `ide/`, only GPUI (`gpui`, `gpui_platform` and their Apache-2.0
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
cargo editor     # Ferrum Editor
cargo ports      # FerrumPorts
cargo env-ui     # FerrumEnv
cargo launcher   # Ferrum Launcher
cargo lint       # clippy, whole workspace, -D warnings
cargo test --workspace
scripts/check-licenses.sh   # no GPL-family crate in any shipped binary

# Website
cd web && cargo run
```

The first build compiles GPUI, which takes a few minutes. Dependencies build at
`opt-level = 2` even in dev, because GPUI is too slow unoptimized.

Each product folder keeps its own `README.md`, `docs/` and `LICENSE`.

## Releases

Each app is versioned on its own, through GitHub Releases tagged
`<app-id>-v<semver>` (for example `api-v0.2.0`):

```bash
scripts/release.sh api 0.2.0 -m "Release notes"
git push origin api-v0.2.0     # runs .github/workflows/release.yml
```

Ferrum Launcher reads these releases to install and update the apps; a new
app shows up in it once it has an entry in `catalog.json`. For
installing Ferrum on a machine (DMG, GitHub sign-in, Launchpad apps), see
**[INSTALL.md](INSTALL.md)**. See
`launcher/README.md` for the conventions, the catalog schema, the
`SUBMODULE_TOKEN` secret that the workflow needs, and the bridge release for
Ferrum Toolbox 0.5.0.
