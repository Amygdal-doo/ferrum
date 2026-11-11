# logs — the library behind Ferrum Editor's Logs page

Logs **per project**: everything a project produces while you develop — log files, dev
servers and other commands, Docker containers and compose services — merged into one live,
filterable view. `crates/core` (`ferrumlogs-core`) is the UI-free library; the page itself
lives in **Ferrum Editor** (`editor/apps/desktop/src/logs/`, opened with ⌥⌘L, *View → Logs*,
`Logs: Toggle View` in the command palette, or the Logs button of the Editor | Git | CI |
Logs switch). There is no separate FerrumLogs app any more. The page follows the editor's
open folder; sources of other projects keep running when the editor switches folders.

> **Status: alpha.** "Tested" means automated tests: unit tests of `ferrumlogs-core` (which
> run real `/bin/sh` commands and tail real temp files) and headless GPUI tests of the
> editor's Logs page (`editor/apps/desktop/src/tests_logs.rs`, no window shown). The page has
> **not been clicked through by hand** yet.

## What works today
| Feature | State |
|---------|-------|
| Follows the editor's project; recent projects shared with every Ferrum app (`~/.ferrum/projects.json`) are listed in the page's sidebar with a running-sources count, and a click opens one in the editor. Switching keeps the other projects' sources running and their logs in memory | ✅ headless-tested (switch, sidebar → editor) |
| Sources saved per project in `~/.ferrum/logs/projects.json` (keyed by folder): add / edit / remove modal, "start when the project opens" | ✅ tested |
| **File** sources: path or glob (`logs/*.log`, `logs/**/*.log`, `~/x.log`, absolute paths) relative to the project; follows by polling (250 ms), shows the last 256 KB at start, handles truncation and rotation (inode change), waits for files that don't exist yet, re-resolves globs every 2 s; multi-file globs tag each line with its file | ✅ tested |
| **Command** sources: `$SHELL -l -c <command>` in the project folder, own process group; stdout + stderr captured (stderr marked with a red edge); start / stop / restart; stop sends SIGTERM to the group, SIGKILL after 3 s; exit status shown (`exited 1` = failed) | ✅ tested |
| **Docker** sources: `docker logs -f --timestamps --tail 500 <container>` and `docker compose logs -f --timestamps --tail 500 [services]` in the project folder (compose `service-1 \|` prefixes become a tag) | ✅ command lines + parsing tested; not run against a Docker daemon |
| **Suggestions** for a project without sources: `package.json` scripts (`dev`, `start`, `serve`, `watch`, `start:dev`; pnpm / yarn / bun / npm by lockfile), `cargo run` (`[package]` in `Cargo.toml`), `python manage.py runserver`, compose (all services + each service), `*.log` in `logs/`, `log/`, `storage/logs/`, `tmp/`, `var/log/`; one click adds and starts, or "Add all" | ✅ tested |
| Merged live view (`uniform_list`): coloured source chip, timestamp, level, tag, text; per-source show / hide; error / warning counters per source in the sidebar; status dots (running / stopped / failed) | ✅ headless-tested |
| Parsing: ANSI colours rendered (16 colours, bold, dim, italic; other escapes stripped); level detection (`ERROR`/`WARN`/…, `level=`, `lvl=`, `"level":`, pino numeric levels, `[E]`/`[WARN]`/`<info>`, `error:`/`warning[…]:` first words, Laravel `local.ERROR`); indented continuation lines (stack traces) inherit the previous line's level | ✅ tested |
| **JSON lines** (pino, bunyan, tracing-json, zap, …): level, `time`/`timestamp`/`ts` (RFC 3339 or epoch s/ms), `msg`/`message` (also tracing's `fields.message`) shown as level + message, other fields dim after it and in full in the details pane; "Copy JSON" | ✅ tested |
| Filters: level toggles (Error / Warn / Info+unlevelled / Debug+trace), "Errors only" (⌘E), text search (case toggle, regex toggle, invalid regex shown and the previous filter kept), match highlighting; search covers text, tags and JSON fields | ✅ tested |
| Follow tail; scrolling up pauses, scrolling back to the end resumes; "Jump to latest (N new)" button (End / ⌘↓); clear (⌘K) | ✅ headless-tested (pause / counter / jump) |
| Click selects a line, shift-click extends; ⌘C copies `[source] text` lines; details pane for a single line (full text, fields, Copy line); export visible lines to a file (⌘S, save dialog) | ✅ copy tested; export not tested (needs the system dialog) |
| Bounded history: 200 000 lines per project (oldest dropped, count shown); parsing happens on the producer threads, output arrives in batches over a bounded channel (fast producers are throttled instead of flooding the UI) and is applied at most ~20×/s; the filtered view updates incrementally | ✅ tested (bound + incremental view) |

## Not built yet
Remembering filters / hidden sources between launches; per-source colour choice; JSON field
filters (`user=ada`) and a time-range filter; text selection inside a line (copy works per
line); soft-wrapping long lines (the list draws the first 2 000 bytes of a line; the details
pane shows all of it); `notify`-based file watching (polling is used); Windows support for
stopping process groups (uses `kill -TERM -<pgid>`); stdin for commands; a PTY (programs
that only colour on a terminal need `FORCE_COLOR` / `CLICOLOR_FORCE`, which are set);
Kubernetes / Railway presets (run them as commands).

## Prerequisites
- Docker CLI on the login shell's `PATH` for container / compose sources.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo editor                                      # Ferrum Editor; ⌥⌘L opens the Logs page
cargo test -p ferrumlogs-core
cargo test -p ferrumeditor-desktop tests_logs     # the editor's Logs page, headless
```

## Keys
On the Logs page:

| Key | Action |
|-----|--------|
| ⌥⌘L | Show / hide the Logs page (window-wide) |
| ⌘F | Search |
| ⌘E | Errors only (toggle) |
| End / ⌘↓ | Jump to latest, follow |
| ⌘C | Copy selected lines |
| ⌘K | Clear |
| ⌘S | Export visible lines |

## Layout
```
crates/core/             ferrumlogs-core (no UI)
  source.rs, store.rs    source kinds, ~/.ferrum/logs/projects.json
  suggest.rs             suggested sources for a project folder
  runner.rs, tail.rs     command / docker processes and file tails on threads → events
  glob.rs                path + glob resolution (*, ?, **)
  line.rs, level.rs,     one parsed line: ANSI spans, level, timestamp, JSON message/fields
  ansi.rs                (ansi.rs copied from FerrumDock)
  buffer.rs              200k-line ring buffer, byte → line splitter
  filter.rs              levels / sources / text / regex, incremental filtered view
  session.rs             a project's sources, runs, counters, history and view
editor/apps/desktop/src/logs/   the Logs page in Ferrum Editor (GPUI)
  mod.rs                 LogsView: event pump, projects, key bindings
  commands.rs            source management, filters, follow, selection, copy, export
  sidebar.rs             sources, suggestions, projects
  log_view.rs            toolbar, uniform_list rows, details pane, status line
  dialog.rs              add / edit source modal
editor/apps/desktop/src/workspace/logs.rs   page ↔ editor: ⌥⌘L, logs:* commands, project
```

## Licence
See [LICENSE](LICENSE).
