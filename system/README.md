# FerrumSystem (FeS)

Draw a system's architecture: layers, components on any cloud, connections, APIs, sockets,
data, flows (with a step-by-step walkthrough), reliability notes, deployment and live pods.
It is a GPUI desktop app (`ferrumsystem-desktop`, binary `ferrumsystem`) on the shared
`ferrum-ui` crate, with the model, catalogue, demos and storage in `ferrumsystem-core`
(`crates/core`). A project's diagram is kept in `.architecture/system.json` inside the
project folder (custom icons in `.architecture/icons/`).

> **Status: alpha.** "Tested" means unit tests plus headless GPUI tests of the window
> (`TestAppContext`, no window shown). See `apps/desktop/src/tests.rs`.

## Window
- **Left sidebar**: window controls, the project strip (⌥⌘O switcher), *This project*
  (its diagram, or **Create architecture** when it has none) and the **Layers** of the
  project's diagram. The demos are no longer listed here, and a demo's layers are not shown.
- **Diagram card**: header (name, tagline, a *Demo* badge for demos, the tools toggle and the
  ◨ panel toggle), the toolbar (cloud switch, Connect, Add) and the canvas.
- **Right panel**: Add, Overview, Component, APIs, Sockets, Data, Flows, Reliability, Deploy,
  Runtime.

Both side panes fold away and the canvas takes their room:

| | Sidebar (left) | Panel (right) |
|---|---|---|
| Button | ◧ in the title row | ◨ at the right end of the header |
| Shortcut | ⌘B | ⌥⌘B |
| Menu | View › Toggle Sidebar | View › Toggle Panel |

## What opens at start
1. `ferrumsystem <folder>` opens that folder; otherwise the project open last.
2. If the project has a diagram, it is shown.
3. Otherwise, **at the very first start only**, the banking demo is shown.
4. After that the window shows an empty view: **New diagram** (⌘N; creates the open
   project's `.architecture/`, or asks for a folder first), **Open…** (⌘O) and *Open the demo*.

The demos are always available from the menu bar: **File › Open Demo ›** (Nova Bank — core
banking, Atlas Knowledge — RAG assistant, Swift Courier — delivery tracking) and
**Help › Open Demo** (Nova Bank). Edits to a demo stay
in memory; *Start from this demo* (sidebar, when the project has no diagram) saves it as the
project's diagram.

## Preferences
`~/.ferrum/ferrumsystem/ui.json` (`$FERRUM_HOME` overrides `~/.ferrum`):

```json
{ "demo_seen": true, "sidebar_open": true, "panel_open": false, "last_project": "/path/to/project" }
```

A missing or broken file reads as the defaults (a first run: the demo is shown). Delete the
file to see the demo open by itself again.

## Shortcuts
| Keys | Action |
|---|---|
| ⌘N / ⌘O | New diagram / Open project folder |
| ⌥⌘O | Switch project |
| ⌘B / ⌥⌘B | Toggle sidebar / right panel |
| ⌘= / ⌘- / ⌘0 | Zoom in / out / fit |
| ⌫ | Delete the selected component |
| Esc | Cancel connect / close dialog / clear selection |

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo system                                   # run the app
cargo system ~/code/shop                       # start on a project
perl -e 'alarm 900; exec @ARGV' cargo test -p ferrumsystem-desktop -p ferrumsystem-core
```
