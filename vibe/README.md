# FerrumEngineer

One window for agentic development: **FerrumChat | FerrumGIT | FerrumTerm**
side by side on the same project. It is a GPUI desktop app (the UI framework
of Zed, a pinned git dependency) built on `ferrum-ui`. It embeds the
suite's own views rather than reimplementing them:

| Column | Crate | What it is |
|---|---|---|
| FerrumChat | `ferrumagent-ui` (`agent/crates/ui`) | `ChatView`: the agentic coding chat (providers, tools, change review, threads) |
| FerrumGIT | `ferrumgit-ui` (`git/crates/ui`) | `GitView` in its embedded layout: status, staging, diffs, commit, history |
| FerrumTerm | `ferrumterm-ui` (`term/crates/ui`) | `TermView` in its compact layout: tabs and panes in the `fterm` server |

```bash
cargo vibe                    # last project opened in FerrumEngineer
cargo vibe ~/code/my-app      # ferrumvibe <folder> once installed (shell command: fen)
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
| The chat is always shown; the Git and Term panes are shown and hidden with the top-bar toggles (⌥⌘2, ⌥⌘3). A pane's view is created the first time it is shown and kept while hidden | ✅ tested (keystrokes and methods) |
| Resize by dragging the splitter between two columns; only those two change, and neither gets narrower than 260 px. Widths are kept while a column is hidden | ✅ tested with a simulated mouse drag |
| Reorder by dragging a column header onto another column, or ⋯ → Move left / Move right (hidden columns are skipped) | ⚠️ Move left/right and the drop handler are tested; the header drag gesture itself is not simulated |
| Layout (order, shown columns, widths) saved per project in `~/.ferrum/vibe/layouts.json`. A project without a saved layout starts from the last one used | ✅ round-trip tested, including switching projects |
| Shared project bar: name, path, branch, the ⌥⌘O switcher and Open Folder…; buttons to open the project in FerrumIDE, FerrumGIT or FerrumTerm | ✅ rendered (shared widget) |
| **Open a project**: File › Open Folder… (⌘O), File › Switch Project… (⌥⌘O), the sidebar's + and folders, or `ferrumvibe <folder>`. The chat, git, terminal and Files pane all follow it | ✅ tested (menu actions handled, every pane on the opened folder) |
| **Files pane** on the far left of the cards: the project tree, folders first, read when expanded, `.gitignore` respected, `.git` hidden, names coloured by `git status` (read in the background) | ✅ model unit-tested; headless-rendered |
| Files: click a folder to expand / collapse; double-click a file (or ↑↓ → ← and Enter) to open it in the Editor column; ⟳ re-reads it (also after the agent changes files); collapse all | ✅ keys and click tested; double-click not simulated |
| Files: shown / hidden with the top-bar toggle, ⌥⌘1 or View › Toggle Files; width resized by dragging its splitter; both saved with the layout (`files` in `layouts.json`) | ✅ tested |
| **Drag files to the chat**: drag a file or folder (⌘-click selects several) from the Files pane, or files from the Finder, onto the chat; each becomes a context chip in the composer (✕ removes it). Project paths go by reference (`@src/main.rs`, `@src/`), text files outside the project by their content; binaries and outside folders are refused with a notice | ✅ tested with a simulated drag and a simulated Finder drop |
| Switching project: a new chat rooted at the new folder, `GitView::set_repo`, `TermView::set_cwd` (running shells stay where they are; new tabs open in the project) | ✅ tested |
| **Several chats per project**: the chat header's threads toggle (⌥⌘J) shows the project's threads in the chat column, grouped Needs you / Done / Working / Idle: **New thread**, click to switch, ✎ renames in place, 🗑 deletes; the sidebar and ☰ list them too. Threads keep running while another is shown | ✅ headless-tested (open, rename a row, switch from the sidebar) |
| `ferrumvibe <folder>` opens on that folder; without one, the last project FerrumEngineer opened | ✅ argument parsing tested |
| Chat **Run in terminal** shows the terminal column if it was closed, runs the command in a new tab there and focuses it | ✅ tested (column reopened, command queued); the command's output is not checked |
| Chat **changes** (`ChangesUpdated`) reload the git column at once instead of at its next 3 s poll, and the chat header shows "N changed". Reopening the git column reloads it too | ⚠️ the reload is dispatched as FerrumGIT's own `Reload` action; tested that it is sent, not that the list changed |
| **Agent checkpoints in the git history**: every chat turn that wrote files (FerrumChat's change set: the files as they were before the turn) shows above the commits in the git column's History, newest first, as "Before: <prompt>" with its file count and time, under "Agent checkpoints · not committed". **Restore to here…** lists the files that go back and asks; Restore puts them as they were before that turn (later checkpoints are dropped, the conversation is kept, the chat's review offers Undo restore until the next turn). Refused while a turn runs | ✅ headless-tested with a scripted agent edit (marker shown, Cancel, Restore) |
| **Run in parallel** (sidebar › Parallel): the same task for 2 or 3 agents, each in a git worktree of the project (detached at HEAD in `~/.ferrum/vibe/worktrees/`, no branches) with its own chat. The page shows them side by side: each chat over its diff (files with +/−, then the patch), re-read when its agent changes files. **Keep this one…** asks, stops the agents, applies that worktree's changes to the project's working tree as a patch (checked first: nothing is written unless all of it applies) and removes every worktree of the run; **Discard all…** removes them without touching the project. With uncommitted changes in the project, Keep asks to stash them first (`git stash push --include-untracked`, message `ferrum-parallel: before keeping worktree N`); if the patch then fails they are given back. Refused when HEAD moved since the run started. Worktrees left by a run that didn't finish are listed with **Remove…** | ✅ git operations unit-tested on temporary repositories; the run headless-tested (two worktrees and chats, diffs, keep with a stash). The agents' edits are played by the test |
| **Per-hunk review of the whole session**: the chat header's "N changed", View › Review Agent Changes or ⌥⌘R (anywhere in the window) open FerrumChat's review below the conversation: every file changed since the chat's session started (its snapshot: a git tree, or the folder's text files), with Accept / Reject per hunk, per file and for all. Accepting moves that hunk's baseline; rejecting writes the file back. Changes made outside the agent (a shell command, you) since the session started are listed too | ✅ headless-tested on a git project (two hunks: one kept, one undone) |
| **Editor column** (⌥⌘4, View › Toggle Editor, the `</>` toggle): files opened from the chat, git diffs, the Files pane or the Knowledge page open there in tabs instead of launching FerrumIDE. Text is edited with FerrumGIT's editor (`ferrumgit_ui::CodeEditor`: FerrumIDE's buffer, undo, tree-sitter highlighting and keys, ⌘S, find; no language servers); images (png, jpg, gif, webp, bmp, svg, tiff) are drawn; binary or non-UTF-8 files are described. Closing a tab with unsaved changes asks Save / Discard / Cancel. Files without edits reload when the agent changes them | ✅ headless-tested (text, image, binary, save on close, reload) |
| Editor header: **Open in FerrumIDE** (`ferrumeditor <project> <file>:<line>` at the cursor's line, through `ferrum_ui::project::open_file_in_editor`), Save, and ⋯ → Move Left / Move Right / Close Editor. View › Move Pane Left / Right (⌥⇧⌘← / →) move the active pane; the chat stays first. Resized like the others; shown, order and width saved per project with the layout (layouts saved before it get it hidden) | ✅ moves, close, per-project round trip and the IDE stub tested; image pixels and the drag are not checked |
| Terminal header shows the focused pane's title | ⚠️ wired, not tested |
| The chat gets the login shell's `PATH` (read in the background at start), so `npx`, `gemini` and MCP servers are found from a Dock launch | ✅ parser tested |
| SSH passphrase prompts from the git column (`askpass_main` runs first in `main`) | ⚠️ wired as in FerrumGIT, not tested here |

Terminals live in a session named `vibe:<project>` of the shared `fterm`
server, so they survive closing the window, and are kept apart from
FerrumTerm's own sessions.

## Not built yet

- The Editor column has no language servers, no split and no
  file-system watcher (it reloads after the agent's changes); open tabs are
  not restored after a restart, and quitting the window does not ask about
  unsaved tabs.
- Deleting a thread from the list does not ask first.
- Run in parallel is the minimal version: 2 or 3 agents, one run at a time,
  not restored after a restart (its worktrees are then listed as leftovers),
  each agent asks for its own permissions, and kept changes are applied to
  the working tree uncommitted. The project must be a repository's top
  folder with a commit. The worktrees' threads stay in FerrumChat's store
  under the worktree paths.
- Checkpoints live as long as the chat's session (they are not saved): after
  a restart the git column shows none. Writes made by shell commands are not
  in checkpoints (they still show in the review).
- The terminal session keeps the name of the project it started in after a
  project switch.
- No Windows build: FerrumTerm's server uses Unix sockets
  (`scripts/package-windows.ps1` and the release workflow skip it).
- Layouts are saved per project, not per window; two windows on one project
  share it (last write wins).
- The Files pane has no file operations (new, rename, delete) and does not
  watch the disk: it re-reads on ⟳, when shown, and when the agent changes
  files.
- Dropping onto the terminal or git panes does nothing yet.

## Layout

```
vibe/apps/desktop/src/
  main.rs        askpass, CLI argument, login PATH, window
  app.rs         FerrumEngineer: state, project switch, column show/hide/move/resize, layout saving
  panes.rs       the embedded views, created on first show, and their events
  editor/        the Editor column: tabs over ferrumgit_ui::CodeEditor, images, other files
  checkpoints.rs the chat's checkpoints as markers in the git history, Restore to here
  parallel/      Run in parallel: worktree.rs (git worktrees, diff, stash, apply), the run, page.rs
  columns.rs     column headers (drag, ⋯ menu, close), bodies, splitters
  chrome.rs      top bar with the toggles, Render
  menus.rs       the menu bar (File › Open Folder…, View toggles)
  files/         the Files pane: tree.rs (model), git_marks.rs (status colours), mod.rs (view)
  layout.rs      the pure layout model and layouts.json (+ layout_tests.rs)
  shell_path.rs  the login shell's PATH
  tests.rs       headless window tests (+ files_tests.rs for the Files pane)
```
