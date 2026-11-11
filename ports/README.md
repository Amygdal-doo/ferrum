# FerrumPorts

"Who is using port 3000?" A native view of the listening ports on your machine and the
processes behind them. It is a GPUI desktop app (the UI framework of Zed, from the `ide/`
submodule) built on the shared `ferrum-ui` crate. One window has the Ferrum project strip
on top and a table of listeners below it, with a detail pane for the selected row.
Everything runs locally: FerrumPorts reads the system with `lsof`, `ps` and `ss` and writes
nothing except the shared recent-projects list (`~/.ferrum/projects.json`, through the
project strip). Part of the Ferrum monorepo, with the same theme and components as
[FerrumContainers](../dock), [FerrumKit](../kit) and [Ferrum Editor](../editor).

> **Status: alpha, first vertical slice.** "Tested" means automated unit tests of
> `ferrumports-core` plus headless GPUI tests of the root view (`TestAppContext`, no window).
> The app has **not been launched or clicked through by hand** yet.

## What works today
| Feature | State |
|------|-------|
| Listening TCP sockets with port, protocol, address, pid, process name, full command line, user, process start time and working directory. IPv4 and IPv6 sockets of one process and port are merged into one row | ✅ parsers tested against captured output; real scan smoke-tested |
| Bound UDP sockets (toolbar **UDP** toggle). Connected and `*:*` sockets are skipped | ✅ tested |
| macOS/BSD: `lsof -F` machine output, `ps -o pid=,user=,lstart=,command=`, `lsof -a -d cwd -Fn`. Linux: `ss -ltnpH` / `-lunpH` (falls back to `lsof`), `ps`, `/proc/<pid>/cwd` | ✅ parsers tested; the Linux path has not been run on Linux |
| Project mapping: each process's cwd is mapped to its git root. Without a git root, a folder under `~` counts as the project (not `~`, `~/Library` or dot folders). Daemons in `/` get no project | ✅ tested |
| Well-known port hints (3000 web, 5173 Vite, 5432 Postgres, 6379 Redis, 9229 Node inspector, 27017 Mongo, …; about 45 ports) | ✅ tested |
| Table: auto-refresh every 2 s (**Pause** / **Resume**, ⌘⇧P), **Refresh** (⌘R), search (⌘F) across port, pid, process, command, project, address, user and hint. Numbers match ports by prefix (`30` finds 3000) or an exact pid | ✅ filter unit-tested; UI headless-tested |
| **Group by project** (`shop — 3000 web, 5432 postgres`, with "Other" last) and **This project** (only listeners of the project in the strip) | ✅ tested |
| Row actions: open `http://localhost:<port>` (icon on each row), stop the process (icon). Detail pane: open in browser, copy URL / PID / command, reveal the working directory, open the project in Ferrum Editor / FerrumGIT / FerrumTerm, stop | ⚠️ wired, not hand-tested |
| Kill: a confirmation dialog shows the full command line and sends SIGTERM. If the process is still alive 2 s later, a second dialog offers SIGKILL. Permission errors name the owner and the `sudo kill` to run. Pids 0, 1 and FerrumPorts itself are refused. Uses the `kill` command (no unsafe code) | ✅ core tested (incl. killing a real child); dialog flow headless-tested |
| "Is port N free?": type a port in the toolbar field and press Enter or **Check**. It binds `127.0.0.1`, `0.0.0.0` and `::` to test, and when the port is taken it names the process and suggests the nearest free port | ✅ tested |
| Detail pane: full command, process and pid, addresses and exposure (local / interface / all interfaces), working directory, project, user, start time, and every port of the same pid (click one to select it) | ✅ headless-rendered |
| Status bar: listener count (and how many are shown), auto-refresh state and last refresh time, scan errors (e.g. `lsof` missing) and warnings | ✅ headless-rendered |
| Empty states: scanning, nothing listening, no matches, scan failed | ✅ headless-rendered |
| Project strip (`ProjectBar`): ⌥⌘O switcher; `ferrumports <folder>` starts with that project | ⚠️ wired |

## Known limitations
- Without root, `lsof` on macOS only shows the sockets of your own processes. Ports held by
  root or other users (e.g. a system Postgres) may be missing. On Linux, `ss` lists them but
  without pid or process name.
- The process start time comes from `ps lstart`, read as local time with second precision.
- "Is port N free?" checks TCP only. A port bound only to one specific interface address
  can test as free on the loopback and wildcard addresses; the result also takes the last
  scan into account.
- There is no column sorting, no keyboard row navigation and no per-row context menu yet
  (the actions are on the row icons and in the detail pane).
- Windows is not supported (no `lsof`/`ss`; the scan reports the missing tool).

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo run -p ferrumports-desktop                    # run the app (binary: ferrumports)
cargo run -p ferrumports-desktop -- ~/code/shop     # start with a project selected
cargo test -p ferrumports-core -p ferrumports-desktop
cargo clippy -p ferrumports-core -p ferrumports-desktop --all-targets -- -D warnings
cargo fmt -p ferrumports-core -p ferrumports-desktop
```

## Layout
```
apps/desktop/      GPUI app: app.rs (root view, state, keys, project strip),
                   commands.rs (scan + polling, kill flow, row actions, port check),
                   table.rs (listener table), detail.rs (detail pane),
                   chrome.rs (toolbar, status bar, kill dialog, notices),
                   tests.rs (headless GPUI tests)
crates/core/       pure logic, no GPUI: model (Listener, addresses, exposure, URL),
                   lsof / ps / ss parsers, scan (runs the tools, joins the output),
                   project (cwd to project), hints (well-known ports),
                   table (search, project filter, grouping), kill, free (port checks)
```
