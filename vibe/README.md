# FerrumVibe

One window for agentic development: **FerrumChat | FerrumGIT | FerrumTerm**
side by side on the same project. It is a GPUI desktop app (the UI framework
of Zed, from the `ide/` submodule) built on `ferrum-ui`. It embeds the
suite's own views rather than reimplementing them:

| Column | Crate | What it is |
|---|---|---|
| FerrumChat | `ferrumagent-ui` (`agent/crates/ui`) | `ChatView`: the agentic coding chat (providers, tools, change review, threads) |
| FerrumGIT | `ferrumgit-ui` (`git/crates/ui`) | `GitView` in its embedded layout: status, staging, diffs, commit, history |
| FerrumTerm | `ferrumterm-ui` (`term/crates/ui`) | `TermView` in its compact layout: tabs and panes in the `fterm` server |

```bash
cargo vibe                    # last project opened in FerrumVibe
cargo vibe ~/code/my-app      # ferrumvibe <folder> once installed (shell command: fev)
```

> **Status: alpha.** "Tested" means unit tests of the layout model and
> headless GPUI tests (`TestAppContext`, no window) of the whole window with
> the three real views: the chat on an isolated config, git on temporary
> services, terminals on a private in-process server. The app has **not been
> launched or clicked through by hand** yet.

## What works

| Feature | State |
|---|---|
| Three columns side by side, built from the shared views | ✅ headless-rendered at 1500 and 760 px wide |
| Close any column (✕ in its header, ⋯ → Close column, or the top-bar toggle); all three can be closed, and the window then says how to reopen one | ✅ tested |
| Reopen from the top-bar toggles **Chat**, **Git**, **Term** (⌥⌘1, ⌥⌘2, ⌥⌘3). A column's view is created the first time it is shown and kept while hidden | ✅ tested (keystrokes and methods) |
| Resize by dragging the splitter between two columns; only those two change, and neither gets narrower than 260 px. Widths are kept while a column is hidden | ✅ tested with a simulated mouse drag |
| Reorder by dragging a column header onto another column, or ⋯ → Move left / Move right (hidden columns are skipped) | ⚠️ Move left/right and the drop handler are tested; the header drag gesture itself is not simulated |
| Layout (order, shown columns, widths) saved per project in `~/.ferrum/vibe/layouts.json`. A project without a saved layout starts from the last one used | ✅ round-trip tested, including switching projects |
| Shared project bar: name, path, branch, the ⌥⌘O switcher and Open Folder…; buttons to open the project in Ferrum Editor, FerrumGIT or FerrumTerm | ✅ rendered (shared widget) |
| Switching project: a new chat rooted at the new folder, `GitView::set_repo`, `TermView::set_cwd` (running shells stay where they are; new tabs open in the project) | ✅ tested |
| `ferrumvibe <folder>` opens on that folder; without one, the last project FerrumVibe opened | ✅ argument parsing tested |
| Chat **Run in terminal** shows the terminal column if it was closed, runs the command in a new tab there and focuses it | ✅ tested (column reopened, command queued); the command's output is not checked |
| Chat **changes** (`ChangesUpdated`) reload the git column at once instead of at its next 3 s poll, and the chat header shows "N to review". Reopening the git column reloads it too | ⚠️ the reload is dispatched as FerrumGIT's own `Reload` action; tested that it is sent, not that the list changed |
| **Open file** from the chat or from git diffs opens it in Ferrum Editor (`ferrumeditor <project> <file>:<line>`, through `ferrum_ui::project::open_file_in_editor`). If the Editor isn't installed, a notice says so | ✅ wiring tested with a stub; launching the real Editor is not |
| Terminal header shows the focused pane's title | ⚠️ wired, not tested |
| The chat gets the login shell's `PATH` (read in the background at start), so `npx`, `gemini` and MCP servers are found from a Dock launch | ✅ parser tested |
| SSH passphrase prompts from the git column (`askpass_main` runs first in `main`) | ⚠️ wired as in FerrumGIT, not tested here |

Terminals live in a session named `vibe:<project>` of the shared `fterm`
server, so they survive closing the window, and are kept apart from
FerrumTerm's own sessions.

## Not built yet

- No editor column: files open in a separate Ferrum Editor window, which is a
  new Editor process each time.
- One chat per window; switching project starts a new chat session for the
  new folder.
- The terminal session keeps the name of the project it started in after a
  project switch.
- No Windows build: FerrumTerm's server uses Unix sockets
  (`scripts/package-windows.ps1` and the release workflow skip it).
- Layouts are saved per project, not per window; two windows on one project
  share it (last write wins).
- No menu bar entries beyond Quit; the column toggles are only in the top bar
  and on ⌥⌘1–3.

## Layout

```
vibe/apps/desktop/src/
  main.rs        askpass, CLI argument, login PATH, window
  app.rs         FerrumVibe: state, project switch, column show/hide/move/resize, layout saving
  panes.rs       the three embedded views, created on first show, and their events
  columns.rs     column headers (drag, ⋯ menu, close), bodies, splitters
  chrome.rs      top bar with the toggles, Render
  layout.rs      the pure layout model and layouts.json (+ layout_tests.rs)
  shell_path.rs  the login shell's PATH
  tests.rs       headless window tests
```
