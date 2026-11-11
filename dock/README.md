# FerrumDock

A fast, native container manager written in Rust, meant as a lighter alternative to
Docker Desktop's dashboard. It is a GPUI desktop app (the UI framework of Zed, from the
`ide/` submodule) built on the shared `ferrum-ui` crate. FerrumDock **does not ship an
engine**. It talks to the Docker-compatible engine you already run (Docker Desktop, OrbStack,
Colima, or Podman with the Docker API) over the Docker Engine API. It is part of the Ferrum
monorepo, alongside [FerrumAPI](../api), [FerrumGIT](../git), [FerrumData](../data) and
[FerrumTerm](../term), and uses the same theme and components.

> **Status: alpha.** FerrumDock is also Ferrum's container tool for projects: the planned
> separate "FerrumContainers" app was folded into it. The "What works" table is the whole
> feature list. Assume nothing outside it works. "Tested" means automated tests. Engine-side
> logic is unit-tested against an in-memory fake engine. Read-only calls (version, Docker
> contexts, containers, stats, inspect, logs, images, image history, volumes with sizes,
> networks, `docker compose ps`, compose files of a project folder) were also checked against
> a real Docker Desktop 29.4.1 engine (API 1.54, Compose 5.1.3) with an `#[ignore]`d
> integration test. **No call that changes anything (start/stop/remove/create/run/tag/pull/
> prune/compose up/down) has run against a real engine**, `docker exec` (shell detection and
> open shell) has never run, and the GUI has **never been launched**. It compiles and passes
> clippy, and that is all that is known about the UI so far.

## What works today
| Area | State |
|------|-------|
| Member of the Ferrum workspace (edition 2024, toolchain 1.99.1, workspace lints) | ✅ |
| Engine detection: `DOCKER_HOST` (`unix://`, `tcp://`, bare socket path), then the **current Docker context**, then `~/.docker/run/docker.sock`, `~/.orbstack/run/docker.sock`, `~/.colima/default/docker.sock`, `/var/run/docker.sock`, then the other Docker contexts. The first one that answers `/version` wins, so a stale socket is skipped | ✅ unit-tested (ordering, dedupe, stale socket fallback, context ordering). Live check connected to Docker Desktop through its `desktop-linux` context |
| Docker contexts: read from `$DOCKER_CONFIG` or `~/.docker` (`$DOCKER_CONTEXT`, else `currentContext` in `config.json`, and `contexts/meta/*/meta.json`). `ssh://` and other unsupported hosts are listed as "not supported" and skipped. **Engine… picker** in the toolbar lists every engine present; the picked one is tried first on every reconnect (for the session). The connected context's name is shown in the toolbar | ⚠️ parsing and loading unit-tested (temp folder). Live check read the real contexts. Picker UI compiled only |
| Engine version and platform in the toolbar, endpoint and API version in the status bar | ⚠️ data live-checked. UI compiled only |
| No-engine screen ("Start Docker Desktop, OrbStack or Colima, then Retry") with the error, a Retry button, and a quiet auto-retry every ~6 s. Losing the engine mid-session returns to this screen | ⚠️ compiled only |
| Containers list: name, image, status, ports, CPU % and memory (polled every 2 s from `/stats`, CPU computed like `docker stats`, page cache excluded from memory), grouped by `com.docker.compose.project`, collapsible groups, filter (name, image, project, service, id, state) | ✅ grouping, filtering, port and size formatting, and the CPU/memory maths are unit-tested. Live check listed real compose projects with stats. UI compiled only |
| Container actions: start, stop, restart, pause / resume, remove (confirm dialog. Running containers are force-removed, and the dialog says so) | ⚠️ policy plus fake-engine behaviour tested. bollard calls compiled only (never run against a real engine) |
| Open a published port in the browser (`:8080` buttons. `0.0.0.0` / `::` become `localhost`, and port 443 opens as https) | ⚠️ URL building unit-tested. Click compiled only |
| Container detail, Logs tab: live follow (last 2 000 lines, then streaming, 20 000-line buffer), case-insensitive search filter, timestamps toggle, ANSI colours rendered (16 colours, bold, dim) or stripped, stderr marked. Logs restart when a stopped container comes back up | ✅ line splitting, timestamp parsing, ANSI parsing/stripping, and search are unit-tested. Live check read real logs. UI compiled only |
| Container detail, Inspect tab: overview, environment, mounts, networks, labels | ⚠️ conversion unit-tested. Live check inspected a real container. UI compiled only |
| **Open shell**: `docker -H <same endpoint> exec -it <id> <shell>` opens in FerrumTerm when it is installed, otherwise in Terminal.app (see below) | ⚠️ the launch plan and quoting are unit-tested. Launching has never been run |
| Images: tags, short id, size, age, containers using each one, total size and dangling count. Pull by reference with a progress bar (layers and bytes. A missing tag means `latest`, never "all tags"). Remove (confirm). Prune dangling (confirm, reports reclaimed space) | ⚠️ reference parsing, progress aggregation, and prune messages are unit-tested against the fake. Listing live-checked. Pull, remove, and prune never run for real |
| Volumes: name, compose project, driver, size and in-use count (from `/system/df?type=volume`, both the old and new response layouts), remove (confirm, disabled while in use), prune unused (confirm, deletes named volumes too, shows reclaimed space) | ⚠️ size parsing unit-tested. Listing with sizes live-checked. Remove and prune never run for real |
| Networks: name, project, driver, scope, built-in and internal markers, remove (confirm, disabled for `bridge`/`host`/`none`), prune unused (confirm) | ⚠️ listing live-checked. Remove and prune never run for real |
| Compose projects: **Up / Restart / Down** per project through `docker compose -p <project>` (plus `--project-directory` and `-f` from the compose labels when those files still exist). Down asks for confirmation. Runs with `DOCKER_HOST` set to the connected engine | ⚠️ argument building unit-tested. Never run |
| **Project bar** (`ferrum_ui::ProjectBar`, shared with the other Ferrum apps): recent projects switcher (⌥⌘O, forwarded from the root view), Open Folder…, open in Editor/Git/Term. `ferrumdock <folder>` opens a folder at launch. Opening a project records it in `~/.ferrum/projects.json` like the other apps | ⚠️ compiled only |
| Project scope: with a project open, the list shows only containers whose compose working-dir label is the folder, whose compose file lies in it, or whose compose project name matches the folder's compose files (`name:`) or the folder name normalized like compose. "Show all" / "This project only" toggles it; the section bar counts the project's containers | ✅ matching and the scoped rows are unit-tested. Live check matched the real `data` project to its folder. UI compiled only |
| Compose files in the project root (`compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml`), parsed for `name:` and the service names. A file whose project has **no containers yet** is listed at the top with **Up** (`docker compose -p <name> --project-directory <root> -f <file> up -d`); unreadable YAML is shown and Up is disabled | ⚠️ detection, naming and argument building unit-tested. Live check read a real compose file. Up from a file never run |
| Compose project panel (**Logs** / **ps** on each project group): combined logs of every service, interleaved as lines arrive, prefixed `service \| ` (after the timestamp, padded to line up), following live, restarted when the project's containers change. A failing service stream becomes a `[logs ended: …]` line and the others continue. **Services** tab: `docker compose ps --all --format json` (both the old array and the newer line-per-container output), with Up / Restart / Down in the header | ✅ prefixing, merging, error handling and ps parsing unit-tested. Live check ran `compose ps` on a real project (this found and fixed a bug: the CLI output's newlines were stripped with the ANSI codes). Combined follow never run against a real engine. UI compiled only |
| **Run container** (Containers "Run…", or ▶ on an image): image, name, ports (`[ip:][host:]container[/tcp\|udp\|sctp]`), environment (one `KEY=VALUE` per line), volumes (`source:/path[:ro\|rw]`, `~/` expanded), command (shell-style quoting), restart policy (no / always / unless-stopped / on-failure), Detached. Creates with bollard, pulls first when the engine says the image is missing (progress shown), then starts. Without Detached the new container's live logs open (there is no interactive attach). Invalid fields are reported by name | ⚠️ form parsing, the API body (JSON), and create → pull → retry → start are unit-tested against the fake. bollard create/start never run against a real engine |
| Open shell: choice of **auto / bash / zsh / sh** in the detail panel. Every choice falls back to `sh` inside the container (`sh -c 'command -v bash … && exec bash; exec sh'`). The available shells are detected with `docker exec <id> sh -c 'command -v …'` and the missing ones disabled. On Linux without FerrumTerm, the first of `x-terminal-emulator`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `alacritty`, `kitty`, `wezterm`, `xterm` on `PATH` is used | ⚠️ commands, detection parsing and the Linux terminal choice unit-tested. Detection and launching never run (Linux never tried at all) |
| Logs: **Save…** writes the visible lines (search applied) as plain text (ANSI stripped, `[stderr]` marked, timestamps when shown) through the system save dialog, defaulting to `~/Downloads/<name>-<date>.log`. **Regex** toggle (case-insensitive; an invalid pattern is shown in red and hides nothing). **Since** 5m / 1h / 24h / all (re-requests the logs with `since=`) | ✅ export, regex search, since maths and file names unit-tested; the `since` option passed to the engine is checked against the fake. UI and save dialog compiled only |
| Stats tab: CPU and memory sparklines of the last ~2 minutes (2 s samples while the Containers list is shown), network in/out and disk read/write totals with per-second rates. Network sums every interface; block I/O sums `read`/`write` for cgroup v1 and v2 | ✅ history, rates (counter resets give 0), sparkline scaling and the stats conversion unit-tested. Network/block I/O not live-checked (no container was running). UI compiled only |
| Images: **Tag…** (`repository[:tag]`, lowercase repository, a missing tag means latest, digests refused) and **History** (layers, newest first: size, age, instruction, total) | ⚠️ tag validation and both calls tested against the fake. History live-checked on a real image. Tag never run for real |
| Keyboard: ⌘R refresh (or retry while offline), ⌘F filter, ⌥⌘O project switcher, Esc closes the dialog or the detail panel | ⚠️ compiled only |

### Open shell and FerrumTerm
The shell is the one picked in the detail panel (auto when the shell is opened from the list).
FerrumTerm counts as installed when its GUI bundle
(`~/Applications/Ferrum/FerrumTerm.app`, `/Applications/Ferrum/FerrumTerm.app` or
`/Applications/FerrumTerm.app`) **and** `~/.ferrum/bin/fterm` must both exist. In that case
FerrumDock runs:

```bash
~/.ferrum/bin/fterm window --name <container> --cwd ~ -- <docker> -H <endpoint> exec -it <id> sh -c '<shell choice, falling back to sh>' 
open -a <FerrumTerm.app>
```

`fterm window` opens a new window (tab) in the FerrumTerm server and starts the server if
it is not running. `open -a` then brings the GUI to the front. Without FerrumTerm,
FerrumDock runs `osascript` to send `tell application "Terminal" to do script "<quoted command>"`
on macOS, and starts the first terminal emulator found on `PATH` on Linux.
Both paths need the `docker` CLI. FerrumDock looks for it on `PATH`, then in
`/usr/local/bin`, `/opt/homebrew/bin`, `~/.orbstack/bin`, `~/.docker/bin`, `~/.rd/bin` and the
Docker.app and OrbStack.app bundles. If it finds none, compose and shell actions explain what
is missing.

## Not built yet
TLS (`DOCKER_CERT_PATH`) and `ssh://` engines, so `ssh://` and TLS Docker contexts are listed
but cannot be used. Switching the CLI's current context (`docker context use`): the picker
only changes what FerrumDock connects to, and is not remembered across launches. Run
options beyond the dialog (networks, labels, user, entrypoint, resource limits, `--rm`,
interactive `-it` attach), editing or renaming containers, `docker cp`, file browser, port
forwarding UI. Compose: profiles, `.env` selection, per-service actions, `pull`/`build`, and
compose files outside the project root or in subfolders. Windows terminals. Stats history
beyond ~2 minutes or while another section is shown. Image push, build and registry login
(pulls are anonymous, so private registries fail). Volume browsing, and volume
backup/export. Network create and connect/disconnect. Dangling-only vs all-unused choice for
image prune. A light theme. Persisted settings (window size, detail width, collapsed groups,
picked engine, project scope). FerrumDock is not in the project bar's "open in" buttons of the
other apps (the shared `FerrumApp` list has Editor, Git and Term only). Packaging and signing.

Volume prune sends `all=true`, which needs API ≥ 1.42 (Docker 23+). On older engines only
anonymous volumes may be pruned, or the filter may be rejected.

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically.
- macOS with the Xcode command line tools (GPUI renders with Metal). Other platforms would
  need whatever GPUI needs there. None has been tried.
- A running Docker-compatible engine, plus the `docker` CLI with the compose plugin for compose
  and shell actions.

## Commands
Run these from the monorepo root (`ferrum/`).
```bash
cargo run -p ferrumdock-desktop                    # run the app (binary: ferrumdock)
cargo test -p ferrumdock-engine -p ferrumdock-desktop
cargo clippy -p ferrumdock-engine -p ferrumdock-desktop --all-targets -- -D warnings

# Read-only checks against the engine on this machine (never changes anything; also
# reads Docker contexts, runs `docker compose ps` and reads one project's compose files):
cargo test -p ferrumdock-engine --test live_engine -- --ignored --nocapture

cargo run -p ferrumdock-desktop -- ~/code/shop      # open a project folder at launch
RUST_LOG=debug cargo run -p ferrumdock-desktop     # verbose logs (ids and operations only, never env values or log lines)
```

## Layout
```
apps/desktop/        GPUI app (binary `ferrumdock`): app.rs (root view, project bar, keys),
                     state/{mod,detail,dialogs} (plain state + row grouping), project.rs (open folder scope),
                     services.rs (Tokio runtime),
                     commands/{connection,containers,engines,logs,project,resources,run},
                     views/{chrome,containers,compose_rows,detail,logs,stats,services,inspect,images,
                     resources,dialogs,run_dialog,image_dialogs,engines,widgets}
crates/engine/       ferrumdock-engine, no UI: Engine trait (engine.rs) + bollard impl (docker.rs, convert.rs),
                     FakeEngine (fake.rs), socket detection (endpoint.rs), Docker contexts (context.rs),
                     formatting (format.rs), stats maths + history (stats.rs), log buffer, search, export,
                     since (logs.rs), combined project logs (logmux.rs), ANSI parsing (ansi.rs),
                     pull references/progress (pull.rs), compose grouping + `docker compose` (compose.rs),
                     `compose ps` (compose_ps.rs), project folders + compose files (project.rs),
                     run form + create body (create.rs), docker CLI lookup (cli.rs), open-shell plans and
                     shell choice (shell.rs), UI policies incl. run and tag (ops.rs), raw `/system/df` GET (rawhttp.rs)
```

Engine access goes through the `Engine` trait (`SharedEngine = Arc<dyn Engine>`), so the
application logic runs against `fake::FakeEngine` in tests. The fake behaves like a small
engine: state transitions, 404/409 errors, and prune semantics. The API client is
[bollard](https://crates.io/crates/bollard) 0.21 over the Unix socket, or plain-HTTP TCP. One
call goes around it. bollard serializes `/system/df?type=…` incorrectly, and an untyped `df`
fails on some engines, so volume sizes come from a small HTTP/1.0 GET in `rawhttp.rs`.

## Local data
FerrumDock's own settings are not stored yet. Opening a project adds it to the shared Ferrum
recent-projects list, `~/.ferrum/projects.json` (written by `ferrum-ui`'s project bar, like the
other apps). "Save…" in the log view writes only where you choose.
