# FerrumDock

A fast, native container manager written in Rust, meant as a lighter alternative to
Docker Desktop's dashboard. It is a GPUI desktop app (the UI framework of Zed, from the
`ide/` submodule) built on the shared `ferrum-ui` crate. FerrumDock **does not ship an
engine**. It talks to the Docker-compatible engine you already run (Docker Desktop, OrbStack,
Colima, or Podman with the Docker API) over the Docker Engine API. It is part of the Ferrum
monorepo, alongside [FerrumAPI](../api), [FerrumGIT](../git), [FerrumData](../data) and
[FerrumTerm](../term), and uses the same theme and components.

> **Status: first alpha slice.** The "What works" table is the whole feature list. Assume
> nothing outside it works. "Tested" means automated tests. Engine-side logic is unit-tested
> against an in-memory fake engine. Read-only calls (version, containers, stats, inspect, logs,
> images, volumes with sizes, networks) were also checked against a real Docker Desktop
> 29.4.1 engine (API 1.54) with an `#[ignore]`d integration test. **No call that changes
> anything (start/stop/remove/pull/prune/compose) has run against a real engine**, and the
> GUI has **never been launched**. It compiles and passes clippy, and that is all that is
> known about it so far.

## What works today
| Area | State |
|------|-------|
| Member of the Ferrum workspace (edition 2024, toolchain 1.99.1, workspace lints) | ✅ |
| Engine detection: `DOCKER_HOST` (`unix://`, `tcp://`, bare socket path), `~/.docker/run/docker.sock`, `~/.orbstack/run/docker.sock`, `~/.colima/default/docker.sock`, `/var/run/docker.sock`, tried in that order. The first one that answers `/version` wins, so a stale socket is skipped | ✅ unit-tested (ordering, dedupe, stale socket fallback). Live check connected to Docker Desktop |
| Engine version and platform in the toolbar, endpoint and API version in the status bar | ⚠️ data live-checked. UI compiled only |
| No-engine screen ("Start Docker Desktop, OrbStack or Colima, then Retry") with the error, a Retry button, and a quiet auto-retry every ~6 s. Losing the engine mid-session returns to this screen | ⚠️ compiled only |
| Containers list: name, image, status, ports, CPU % and memory (polled every 2 s from `/stats`, CPU computed like `docker stats`, page cache excluded from memory), grouped by `com.docker.compose.project`, collapsible groups, filter (name, image, project, service, id, state) | ✅ grouping, filtering, port and size formatting, and the CPU/memory maths are unit-tested. Live check listed real compose projects with stats. UI compiled only |
| Container actions: start, stop, restart, pause / resume, remove (confirm dialog. Running containers are force-removed, and the dialog says so) | ⚠️ policy plus fake-engine behaviour tested. bollard calls compiled only (never run against a real engine) |
| Open a published port in the browser (`:8080` buttons. `0.0.0.0` / `::` become `localhost`, and port 443 opens as https) | ⚠️ URL building unit-tested. Click compiled only |
| Container detail, Logs tab: live follow (last 2 000 lines, then streaming, 20 000-line buffer), case-insensitive search filter, timestamps toggle, ANSI colours rendered (16 colours, bold, dim) or stripped, stderr marked. Logs restart when a stopped container comes back up | ✅ line splitting, timestamp parsing, ANSI parsing/stripping, and search are unit-tested. Live check read real logs. UI compiled only |
| Container detail, Inspect tab: overview, environment, mounts, networks, labels | ⚠️ conversion unit-tested. Live check inspected a real container. UI compiled only |
| **Open shell**: `docker -H <same endpoint> exec -it <id> sh` opens in FerrumTerm when it is installed, otherwise in Terminal.app (see below) | ⚠️ the launch plan and quoting are unit-tested. Launching has never been run |
| Images: tags, short id, size, age, containers using each one, total size and dangling count. Pull by reference with a progress bar (layers and bytes. A missing tag means `latest`, never "all tags"). Remove (confirm). Prune dangling (confirm, reports reclaimed space) | ⚠️ reference parsing, progress aggregation, and prune messages are unit-tested against the fake. Listing live-checked. Pull, remove, and prune never run for real |
| Volumes: name, compose project, driver, size and in-use count (from `/system/df?type=volume`, both the old and new response layouts), remove (confirm, disabled while in use), prune unused (confirm, deletes named volumes too, shows reclaimed space) | ⚠️ size parsing unit-tested. Listing with sizes live-checked. Remove and prune never run for real |
| Networks: name, project, driver, scope, built-in and internal markers, remove (confirm, disabled for `bridge`/`host`/`none`), prune unused (confirm) | ⚠️ listing live-checked. Remove and prune never run for real |
| Compose projects: **Up / Restart / Down** per project through `docker compose -p <project>` (plus `--project-directory` and `-f` from the compose labels when those files still exist). Down asks for confirmation. Runs with `DOCKER_HOST` set to the connected engine | ⚠️ argument building unit-tested. Never run |
| Keyboard: ⌘R refresh (or retry while offline), ⌘F filter, Esc closes the dialog or the detail panel | ⚠️ compiled only |

### Open shell and FerrumTerm
FerrumTerm counts as installed when its GUI bundle
(`~/Applications/Ferrum/FerrumTerm.app`, `/Applications/Ferrum/FerrumTerm.app` or
`/Applications/FerrumTerm.app`) **and** `~/.ferrum/bin/fterm` must both exist. In that case
FerrumDock runs:

```bash
~/.ferrum/bin/fterm window --name <container> --cwd ~ -- <docker> -H <endpoint> exec -it <id> sh
open -a <FerrumTerm.app>
```

`fterm window` opens a new window (tab) in the FerrumTerm server and starts the server if
it is not running. `open -a` then brings the GUI to the front. Without FerrumTerm,
FerrumDock runs `osascript` to send `tell application "Terminal" to do script "<quoted command>"`.
Both paths need the `docker` CLI. FerrumDock looks for it on `PATH`, then in
`/usr/local/bin`, `/opt/homebrew/bin`, `~/.orbstack/bin`, `~/.docker/bin`, `~/.rd/bin` and the
Docker.app and OrbStack.app bundles. If it finds none, compose and shell actions explain what
is missing.

## Not built yet
TLS (`DOCKER_CERT_PATH`) and `ssh://` engines. Docker contexts (`docker context use`) are not
read. Only `DOCKER_HOST` and the known sockets are. Creating or running containers, editing,
renaming, `docker cp`, file browser, port forwarding UI. Choosing a shell (`bash` vs `sh`), and
exec without FerrumTerm or Terminal.app (Linux/Windows terminals). Compose `ps`/logs per
project, and starting a project from a compose file that has no containers yet. Log download,
regex search, and a "since" time filter. Stats history or charts, plus network I/O and block I/O.
Image layers and history, push, tag, build, and registry login (pulls are anonymous, so
private registries fail). Volume browsing, and volume backup/export. Network create and
connect/disconnect. Dangling-only vs all-unused choice for image prune. A light theme.
Persisted settings (window size, detail width, collapsed groups). Packaging and signing.

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

# Read-only checks against the engine on this machine (never changes anything):
cargo test -p ferrumdock-engine --test live_engine -- --ignored --nocapture

RUST_LOG=debug cargo run -p ferrumdock-desktop     # verbose logs (ids and operations only, never env values or log lines)
```

## Layout
```
apps/desktop/        GPUI app (binary `ferrumdock`): app.rs (root view, keys), state.rs (plain state + row grouping),
                     services.rs (Tokio runtime), commands/{connection,containers,logs,resources},
                     views/{chrome,containers,detail,inspect,images,resources,dialogs,widgets}
crates/engine/       ferrumdock-engine, no UI: Engine trait (engine.rs) + bollard impl (docker.rs, convert.rs),
                     FakeEngine (fake.rs), socket detection (endpoint.rs), formatting (format.rs), stats maths,
                     log buffer + timestamps (logs.rs), ANSI parsing (ansi.rs), pull references/progress (pull.rs),
                     compose grouping + `docker compose` (compose.rs), docker CLI lookup (cli.rs),
                     open-shell plans (shell.rs), UI policies (ops.rs), raw `/system/df` GET (rawhttp.rs)
```

Engine access goes through the `Engine` trait (`SharedEngine = Arc<dyn Engine>`), so the
application logic runs against `fake::FakeEngine` in tests. The fake behaves like a small
engine: state transitions, 404/409 errors, and prune semantics. The API client is
[bollard](https://crates.io/crates/bollard) 0.21 over the Unix socket, or plain-HTTP TCP. One
call goes around it. bollard serializes `/system/df?type=…` incorrectly, and an untyped `df`
fails on some engines, so volume sizes come from a small HTTP/1.0 GET in `rawhttp.rs`.

## Local data
None. FerrumDock stores nothing on disk yet.
