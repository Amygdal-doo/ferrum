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

## Editing
Everything is edited with the mouse: **Add** (toolbar or the Add tab), drag a card to another
place or layer, drag from a card's side handle onto another card to connect them (or
**Connect**, then click two cards), and the **Component** tab for the name, description,
look, deployment and connections. ⌫ or *Delete component* removes one.

Each edit is saved into `.architecture/system.json` at once, rewriting only the values that
changed: the file's indentation, key order and any fields FerrumSystem does not know stay as
they were. Opening a project writes nothing. ⌘Z / ⇧⌘Z (Edit › Undo / Redo, or the toolbar's
arrows) undo and redo the edits; typing in one field undoes as one step, and undo puts the file
back exactly as it was.

## Generating a diagram
**Generate from project files** (sidebar, the empty view, File › Generate from Project Files)
reads the project — never `node_modules`, `target`, `.git` and the like — and builds a
diagram, the same one every time for the same files:

- **docker-compose** (`docker-compose*.yml`, `compose.yaml`): a Docker container per service;
  its kind and technology from the image (`postgres:16` → relational database, PostgreSQL)
  or, for `build:`, from the package manifest there; replicas, the first port and the
  health check; connections from `depends_on`, `links` and addresses in `environment`
  (`postgres://db:5432`).
- **Kubernetes** (any YAML with `apiVersion` and `kind`): Deployments, StatefulSets,
  DaemonSets, Jobs and CronJobs as pods (replicas, `app` label, port, probes), Ingresses in
  front of the workloads their Services select, the namespace for the Runtime tab.
- **Package manifests**, when there is neither: `package.json`, `Cargo.toml`,
  `pyproject.toml` / `requirements.txt`, `go.mod`, `Gemfile`, `pom.xml` — one component per
  app, plus the datastores its dependencies talk to (`pg`, `redis`, `kafkajs`, `pika`…).

A project without a diagram gets this one as its `.architecture/system.json`; with a diagram,
only the components and connections it lacks are added, as one edit (⌘Z takes it back).

## Export
**Export** (the toolbar's download button, or File › Export ›) saves the whole diagram, laid
out as on the canvas on a white background with its name on top, as a **PNG** image (twice
the size, rendered with resvg in the system fonts), an **SVG** drawing or a vector **PDF**
(one page, the standard Helvetica fonts; written without a PDF library).

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
| ⌘Z / ⇧⌘Z | Undo / redo an edit of the diagram |
| Esc | Cancel connect / close dialog / clear selection |

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo system                                   # run the app
cargo system ~/code/shop                       # start on a project
perl -e 'alarm 900; exec @ARGV' cargo test -p ferrumsystem-desktop -p ferrumsystem-core
```
