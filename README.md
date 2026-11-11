# Ferrum

Monorepo for the Ferrum family of Rust developer tools. All desktop tools
render with [GPUI](https://gpui.rs), the UI framework behind Zed, and share
one look through `shared/ui`.

| Path | Product | Stack |
| --- | --- | --- |
| `ide/` | FerrumIDE: fork of Zed | GPUI (own Cargo workspace) |
| `api/` | FerrumAPI: HTTP/API client | GPUI + `ferrum-ui` |
| `git/` | FerrumGIT: Git client | GPUI + `ferrum-ui` |
| `data/` | FerrumData: database IDE | GPUI + `ferrum-ui` |
| `term/` | FerrumTerm: terminal server, GUI and CLI | GPUI GUI, crossterm CLI |
| `kit/` | FerrumKit: developer utilities | GPUI + `ferrum-ui` |
| `dock/` | FerrumDock: container manager | GPUI + `ferrum-ui` |
| `editor/` | Ferrum Editor: lightweight, hackable text editor (not the Zed fork) | GPUI + `ferrum-ui` |
| `launcher/` | Ferrum Launcher (formerly Ferrum Toolbox): installs, updates and launches the apps | GPUI + `ferrum-ui` |
| `shared/ui` | `ferrum-ui`: shared shell and widgets | GPUI + Zed `ui`/`theme` |
| `web/` | Marketing site | axum + maud (standalone package) |

## Layout

- The root `Cargo.toml` is a single workspace for `shared/`, `api/`, `git/`,
  `data/`, `term/`, `kit/`, `dock/`, `editor/` and `launcher/`. It has one `Cargo.lock` and one toolchain
  (`rust-toolchain.toml`).
- `ide/` keeps its own workspace and toolchain so merges from upstream Zed
  stay easy. The other tools depend on its `gpui`, `gpui_platform`, `ui`,
  `theme`, `menu` and `assets` crates by path, so GPUI is upgraded once, in
  `ide/`, for every tool.
- `web/` is excluded from the workspace because it is deployed on its own
  (`web/Dockerfile`, `web/railway.toml`).
- The root `[patch.crates-io]` mirrors the GPUI-relevant patches from
  `ide/Cargo.toml`. Keep them in sync when updating `ide/`.

## `ferrum-ui`

`shared/ui` contains:

- `ferrum_ui::run(AppConfig, build)` boots GPUI with FerrumIDE's fonts,
  icons and the One Dark theme, opens the main window and wires up Quit.
- `TextInput`: single-line, multi-line, masked and monospace editable
  text. It emits `Changed`, `Submit` and `Blurred`.
- `splitter` + `DraggedSplitter` for resizable panes.
- `Modal`, `Notice` / `render_notice`, `toolbar`, `status_bar`,
  `section_header`, `badge`, `kbd` and `empty_state`.
- `ferrum_ui::prelude::*` re-exports GPUI and Zed `ui` components
  (`Button`, `Label`, `ListItem`, `ContextMenu`, `PopoverMenu`, `TabBar`, …).

Widget gallery: `cargo run -p ferrum-ui --example gallery`.

## Commands

```bash
cargo api        # FerrumAPI
cargo git-ui     # FerrumGIT
cargo desktop    # FerrumData
cargo term       # FerrumTerm GUI
cargo fterm      # FerrumTerm CLI
cargo kit        # FerrumKit
cargo dock       # FerrumDock
cargo editor     # Ferrum Editor
cargo launcher   # Ferrum Launcher
cargo lint       # clippy, whole workspace, -D warnings
cargo test --workspace

# FerrumIDE (own workspace)
cd ide && cargo run
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
