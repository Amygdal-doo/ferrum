# FerrumIDE

A small, hackable text editor in the spirit of Atom — tree view, tabs, split panes, fuzzy
finder, command palette, and plain-text config / keymap / theme / snippets you edit in the
editor itself — written from scratch in Rust on GPUI (the UI framework of Zed, via the
FerrumIDE fork) and the shared `ferrum-ui` crate. It is **not** FerrumIDE: the text buffer
(rope), multi-cursor editing, undo, rendering of the editor surface, highlighting and input
handling are its own; nothing from Zed's editor stack (`editor`, `language`, `project`,
`workspace`, `text`, `rope`, …) is used. Part of the Ferrum monorepo, sibling of
[FerrumKit](../kit), [FerrumContainers](../dock), [FerrumGIT](../git) and the others.

> **Status: alpha — first vertical slice.** "Tested" means automated tests: unit tests of
> `ferrumeditor-core`, plus headless GPUI tests of the desktop app that drive real keystrokes
> through the keymap into the editor and workspace. The app has **not been launched or
> clicked through by hand** yet, so visual polish is unverified.

## What works today
| Area | State |
|------|-------|
| Rope buffer (`ropey`), byte-offset API, LF inside / original line ending (LF/CRLF) and UTF-8 BOM restored on save, binary / non-UTF-8 files refused, atomic saves keeping permissions | ✅ tested |
| Multi-cursor editing as single undo steps: typing, delete, newline, paste (one line per cursor when counts match), Cmd+click add/remove caret (Cmd+Alt+click in files with a language server, where Cmd+click goes to the definition), Cmd+D next occurrence, select all occurrences, Cmd+Shift+L split into lines, add cursor above/below, Alt+drag column selection | ✅ core tested; mouse paths wired, not hand-tested |
| Undo/redo with grouping (typing / deleting runs within 1 s; cursor moves seal the group), dirty tracking against the saved state | ✅ tested |
| Motions: char (grapheme-aware), word, smart home, line end, doc start/end, page up/down; vertical moves keep a goal column and follow soft-wrapped rows | ✅ tested |
| Auto-close brackets/quotes (wrap selection, skip over closer, delete empty pair, no quote pairing inside words), auto-indent on Enter (after `{[(`, after `:` in Python/YAML, closer moved to its own line), auto-dedent when typing a closer | ✅ tested |
| Toggle comment per language (line comments at the block's smallest indent, block comments for HTML/CSS/Markdown), duplicate / move / delete / join lines, indent / outdent, upper/lower case, go to matching bracket | ✅ tested |
| Find / replace in buffer (plain, case, whole word, regex with `$1` / `${name}` / `\n`), match highlighting, replace one / all (one undo step), select all matches | ✅ tested |
| Find in files (respects `.gitignore`, skips binary/huge files, cancellable background search), results grouped by file, open at match | ✅ tested |
| Fuzzy matcher (smart case, path-aware bonuses) for the file finder and command palette; file list built on a background thread | ✅ tested |
| Tree-sitter highlighting for Rust, JavaScript, TypeScript, TSX, JSON, TOML, YAML, Markdown (inline + fenced code injections), Python, Go, HTML (JS/CSS injections), CSS, Bash — full reparse on a background thread, debounced; disabled above 4 MB | ✅ tested (spans) |
| Minimap at the right of the text (`minimap = true`; *View → Toggle Minimap*, Settings › Editor): every row as blocks in its syntax colours, the visible part marked, click or drag to scroll; scrolls with the text in long files | ⚠️ click-to-scroll and toggle headless-tested; drawing not hand-tested |
| Settings › Editor in the shared Settings window (⌘,): soft wrap, minimap, line numbers, indent guides, current line, autocomplete, format on save — written to `config.toml` (comments kept) and applied live; soft wrap is also *View → Toggle Soft Wrap* / ⌥Z per file | ✅ config writing headless-tested; page drawing not hand-tested |
| Code folding from the tree-sitter tree (brace / bracket blocks with the closing row kept visible, Python blocks, comments, Markdown sections, TOML tables, YAML mappings, HTML elements): ▾ / ▸ toggles in the gutter's fold lane, ⌥⌘[ / ⌥⌘] fold / unfold at the caret, ⌘K ⌘0 / ⌘K ⌘J fold all / unfold all (also in *View*); a folded row ends in `⋯`; folds follow edits, and an edit inside a fold or a caret moved into it opens it | ✅ regions and fold tracking unit-tested; keys, vertical moves and edits headless-tested; drawing not hand-tested |
| Editor surface: only visible rows are shaped; line numbers, current-line highlight, selections, cursors, indent guides, bracket-match boxes, search matches, git gutter, scrollbar, soft wrap (word-boundary wrapping), horizontal scroll, IME / dead keys via GPUI's input handler, mouse select / drag (word, line), clipboard (whole-line copy/paste) | ⚠️ headless-tested; not hand-tested |
| Workspace: tree view (expand/collapse, new file/folder, rename, Move to Trash, reveal active file, reveal in Finder, keyboard navigation), tabs with dirty dot and save prompt on close, split right/down, focus next/previous pane, reopen closed tab | ⚠️ headless-tested in parts |
| Command palette (every command with its current key binding), file finder (Cmd+P), go to line (Ctrl+G, `line[:col]`) | ✅ headless-tested |
| Status bar: path (the folder and git branch are in the project bar), Ln/Col, selection count / size, language, indentation, line ending, encoding, wrap | ⚠️ wired, not hand-tested |
| Git line marks (added / modified / deleted vs `HEAD`, via `git show`) and branch name (read from `.git/HEAD`, shown in the project bar) | ✅ diff tested; gutter not hand-tested |
| Markdown preview (Cmd+Shift+M) in a split, live re-render | ⚠️ headless-tested open/close |
| Reload when a file changes on disk (silent if clean, prompt if dirty), on watcher events and on window activation | ⚠️ wired, not hand-tested |
| Session restore (folder, pane layout, tabs, cursor and scroll) | ✅ headless-tested |
| User dir `~/.ferrum/editor/`: `config.toml`, `keymap.toml`, `theme.toml`, `snippets.toml` created with documented defaults, watched and applied live (problems shown as a notice) | ✅ parsing tested; live reload headless-tested |
| Snippets with `$1`, `${1:placeholder}`, mirrored stops, `$0`, Tab / Shift+Tab navigation | ✅ tested |
| Project bar at the top of the window (the shared Ferrum `ProjectBar`): project name with a switcher of recent projects from every Ferrum app (⌥⌘O, or click the name; picking one opens it like *Open Folder*), the path (click a segment to reveal it, right-click copies), the git branch (kept current after checkouts), buttons to open the project in FerrumGIT / FerrumTerm; the **[Git] [Agent] [Term]** pane toggles sit at its right end (see [Tool panes](#tool-panes)) | ✅ open-from-switcher headless-tested; drawing not hand-tested |
| Built-in `ferrum-dark` / `ferrum-light` themes applied to the whole UI, plus bundled Material themes (Default, Darker, Palenight, Ocean, Deep Ocean, Lighter) | ✅ token tables unit-tested; selector headless-tested; not hand-tested |
| Loadable themes in `~/.ferrum/editor/themes/`: our TOML format and VS Code colour themes (`.json`, comments / trailing commas / `include` allowed; workbench `colors` → UI + terminal tokens, `tokenColors` TextMate scopes → tree-sitter names); theme selector with live preview, Enter saves to `config.toml`, Escape reverts; install from file; folder watched live; bad files → notice | ✅ parsing / mapping unit-tested; selector headless-tested |
| *Theme: Import VS Code Theme…*: a `.json` theme (its `include` chain resolved) or a `.vsix` extension (every theme in `package.json` → `contributes.themes`, light/dark from `uiTheme`) converted to Ferrum TOML in `themes/` and applied | ✅ fixtures unit-tested (JSONC, include, vsix); import headless-tested |
| File icons (own SVG set, tinted per type) in the tree view, tabs, file finder and find-in-files results; a bundled `material` icon theme (solid Material-style glyphs, folders coloured by role: `src`, `tests`, `docs`, `node_modules`, `.github`, `dist`/`target`…); icon themes in `~/.ferrum/editor/icon-themes/<name>/`; icon theme selector with preview; `show_file_icons` | ✅ resolution unit- and headless-tested; drawing not hand-tested |
| Tool panes inside the window: Git (FerrumGIT's `GitView`), Agent (FerrumChat's `ChatView`), Term (FerrumTerm's `TermView`), CI, Logs and Debug as split panes docked right / left / bottom; several open at once; drag a pane's header or use its ⋯ menu to move it, drag dividers to resize, close from the header; layout saved per project | ✅ open-all / move / resize / close / restore and event wiring headless-tested; drawing and dragging not hand-tested |
| Language servers (LSP): one server per language and project folder, found on your login shell's `PATH` (rust-analyzer, typescript-language-server, pyright / pylsp, gopls, vscode-json/css/html, taplo, yaml-language-server, bash-language-server; override in `[lsp.<id>]`); full-text sync, UTF-8 / UTF-16 positions | ✅ client unit-tested over pipes; end to end headless-tested with a scripted server |
| Autocomplete: as you type (and on trigger characters such as `.`) or with Ctrl+Space / Cmd+I; server items (snippets with tab stops, auto-import edits) or words from the file when there is no server; ↑/↓, Enter / Tab to insert, Escape to close; `autocomplete = false` turns off the automatic menu | ✅ ranking / snippets unit-tested; typing → menu → insert headless-tested; menu drawing not hand-tested |
| Problems: wavy underlines and gutter dots coloured by severity, hover (or Cmd+K Cmd+I) for the message and the server's type / docs, F8 / Shift+F8 to step through, error / warning counts in the status bar, Ctrl+Shift+M lists every problem in the project | ✅ mapping and list headless-tested; drawing not hand-tested |
| Go to definition: F12 or Cmd+click; several results open a picker; Ctrl+- / Ctrl+Shift+- go back / forward | ✅ headless-tested |
| Rename symbol (F2): `prepareRename`, an inline input over the symbol (Enter renames, Escape cancels), a preview of every changed line per file (Enter / *Apply* applies, Escape / *Cancel* drops it), then the `WorkspaceEdit` applied across files (`changes` or `documentChanges`, incl. create / rename / delete file operations); each open file changes as one undo step, files that are not open are edited on disk (servers are told via `didChangeWatchedFiles`) | ✅ edit parsing / applying unit-tested; preview unit-tested; F2 → input → preview → cancel / apply across an open and an unopened file → undo headless-tested |
| Find all references (Shift+F12): results grouped by file with preview lines in the bottom panel; click to open | ✅ grouping unit-tested; headless-tested |
| Code actions / quick fixes (Cmd+.): actions for the selection and the problems under it (preferred and quick fixes first) in a picker; edits applied, commands run with `workspace/executeCommand`, edits the server sends back (`workspace/applyEdit`) applied and answered; `codeAction/resolve` when needed | ✅ parsing unit-tested; edit and command actions headless-tested |
| Format document (Shift+Alt+F) and format on save (`format_on_save`, on for Rust / Go / TypeScript / JavaScript when the server can format; per language under `[languages.<id>]`); saving waits at most 1.5 s, and a slow or failing server only skips formatting with a notice | ✅ headless-tested (command and on save) |
| Signature help: typing `(` / `,` (the server's trigger characters) shows the signature above the caret with the current parameter highlighted; `)`, Escape or moving away closes it; Cmd+Shift+Space asks | ✅ parsing unit-tested; typing → popup → Escape headless-tested; drawing not hand-tested |
| Tasks: build / test / run / lint tasks found from `Cargo.toml`, `go.mod`, package.json scripts (pnpm / yarn / bun / npm from the lock file; `tsc --noEmit` for TypeScript without a build script), Makefile targets and `.vscode/tasks.json`; ⌘B runs the default build, ⇧⌘B picks one; each runs in a tab of the Term pane after saving edited files; errors and warnings from cargo/rustc, tsc, Go and `file:line:col` compilers become problems | ✅ detection and output parsing unit-tested; ⌘B → Term pane → problems headless-tested; output capture unit-tested with `/bin/sh` |
| Git pane: FerrumGIT's own interface embedded (`ferrumgit-ui`, compact layout) — changes, staging, commit, history, branches, diffs; ⌥⌘G / ⌃⇧G or the Git button; *Open* in a diff opens the file in the editor at that line | ✅ embedding headless-tested; FerrumGIT's view is tested in `git/crates/ui` |
| CI page (`ferrumci-core`, formerly the FerrumCI app): GitHub Actions of the open project's GitHub remote — runs with filters, run detail with jobs and steps, finished job logs with ANSI colours, folding and search, re-run / cancel / notify when done, workflow dispatch form, pull requests with their checks; token from FerrumGIT's keychain entry, `gh auth token` or `GH_TOKEN` / `GITHUB_TOKEN`, or pasted on the sign-in screen; a tool pane: ⇧⌘I, *View → CI Pane* or the palette | ✅ headless-tested against recorded GitHub responses; drawing not hand-tested |
| Logs page (`ferrumlogs-core`, formerly the FerrumLogs app): live logs of the open project — log files and globs (rotation-aware tails), dev-server commands, Docker containers and compose services — merged into one view with source chips, ANSI colours, level detection, JSON lines, level / text / regex filters, follow tail, selection and copy, export; sources saved per project, suggestions for a project without sources; sources keep running when the editor switches folders; a tool pane: ⌥⌘L, *View → Logs Pane* or the palette | ✅ core unit-tested; page headless-tested with real `/bin/sh` commands and temp files; drawing not hand-tested |
| Agent pane: FerrumChat (`ferrumagent-ui`) — threads, agent / model picker (its own models, or Claude Code, Codex, Gemini… over ACP), modes, @-mentions, tool cards with permission prompts, change review with accept / reject and checkpoints; ⌥⌘A or the Agent button. *Agents: New Session* still runs a CLI agent from `[[agents]]` in a Term pane tab | ✅ embedding and events headless-tested; the chat itself is tested in `agent/crates/ui` |
| Debugger (DAP): `lldb-dap` / CodeLLDB (Rust, C), `dlv dap` (Go), js-debug (JS/TS); `.vscode/launch.json` or defaults; gutter / F9 breakpoints (saved per project, verified vs not), current line, F5 / F10 / F11 / ⇧F11 / ⇧F5, call stack, lazy variables, watches, debug console with evaluate, `runInTerminal` in a Term pane tab | ✅ protocol + state machine tested against a scripted adapter; real `lldb-dap` on a C program (ignored test); F9 → stop → F10 → F5 headless-tested; dlv / js-debug not run here |
| CLI: `ferrumeditor [path[:line[:col]]...]`, `--help`, `--version` | ✅ tested |
| One process, several windows: while FerrumIDE runs, `ferrumeditor <paths>` hands the paths to it over a local socket (`~/.ferrum/editor/instance.sock`, a stale one is replaced) and exits; Finder's *Open With* and drops on the Dock icon arrive the same way. A folder (with its files) opens in the window whose project contains it, else in a new window; a lone file goes to its project's window, else a window without a folder, else a new one; no paths bring the front window forward. *File → New Window* (⇧⌘N). Closing one of several windows asks about its own unsaved files; quitting asks about every window's. `FERRUM_EDITOR_NEW_INSTANCE=1` starts a separate process | ✅ hand-over unit-tested over a real socket; routing and closing headless-tested with two windows; Finder / Dock opens not tried |

## Not built yet
Tool panes: dropping a pane on another always puts it after that pane (no before / beside
choice by drop position), docks cannot be nested beyond left / right / bottom, and the Term
pane's tab titles come from FerrumTerm (a task's tab shows `sh`, its OSC title aside). VS Code font
styles (bold / italic) and semantic token colours are not imported; icon SVGs are single-colour
masks (multi-colour icons are flattened to one tint).
Language servers: range / on-type formatting, a lightbulb or gutter hint for code actions,
inlay hints, semantic highlighting, workspace symbols, incremental document
sync; undo for edits a rename makes in files that are not open (they are saved directly);
updating installed servers (Language Server: Install reinstalls the latest). The project bar's
recent-projects list is not updated by the headless tests (they never write
`~/.ferrum/projects.json`).
Incremental tree-sitter reparsing (every edit reparses the whole file in the background);
the session restores only the window that quit last; packages/plugins (a `Package` trait and command registry seam exist);
cursor blinking; encodings other than UTF-8; editing files
over 64 MB; a project-wide replace; drag-and-drop of tabs or files; tree view drag-to-move;
tab overflow menu; per-project settings; Linux/Windows key defaults beyond a mechanical
`cmd`→`ctrl` translation.
Agents: CLI agents started with *Agents: New Session* are plain terminal tabs (no snapshot /
diff review of their own; FerrumChat's review covers chat sessions). Debugger: conditional / log breakpoints, function and exception breakpoints (the
exception filters are sent empty), hover evaluate, data breakpoints, multiple simultaneous
sessions, breakpoints moving with edits (they stay on their line number); dlv and js-debug
are wired but were not run against the real adapters in tests.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo editor                                          # run the app (alias for: cargo run -p ferrumeditor-desktop)
cargo run -p ferrumeditor-desktop -- src/main.rs:10   # open a file at line 10
cargo test -p ferrumeditor-core -p ferrumeditor-desktop
cargo clippy -p ferrumeditor-core -p ferrumeditor-desktop --all-targets -- -D warnings
```
The binary is `ferrumeditor`; the window / bundle display name is "FerrumIDE". Release
builds read the version from `FERRUM_VERSION` at compile time (shown in About).

## Hacking it
Everything lives in `~/.ferrum/editor/` (override with `FERRUM_EDITOR_HOME`). Open the files
from the palette: *Application: Open Config* (⌥⌘,, or *FerrumIDE › Config File…*) / *Open
Keymap / Open Snippets / Open Theme*. Saving any of them applies the change immediately. ⌘,
(and the gear in the title bar) opens the Settings window shared by every Ferrum app, as in
the other apps; before the rename to FerrumIDE it opened `config.toml`.

- **`config.toml`** — font family/size, line height, `theme`, `icon_theme`, `show_file_icons`,
  `[[agents]]` and `agent_idle_seconds` (see [AI agents](#ai-agents)),
  `terminal_shell` (empty = your login shell), tab size, soft tabs, soft wrap,
  line numbers, indent guides, current-line highlight, auto-close, auto-indent, trim trailing
  whitespace, final newline, scroll past end, session restore, `format_on_save`, and
  `[languages.<id>]` overrides (`tab_size`, `soft_tabs`, `soft_wrap`, `format_on_save`).
- **`keymap.toml`** — `"keys" = "command:name"` under `[bindings]`; `""` unbinds. The default
  file lists every command and its default keys.
- **`theme.toml`** — override any UI, syntax or terminal colour token of the selected theme
  (`[ui]`, `[syntax]`, `[terminal]`, or `[themes.<name>.ui]` … for one theme).
- **`themes/`** — whole themes, see [Themes](#themes). *Theme: Select* (⌘K ⌘T) picks one.
- **`icon-themes/`** — icon themes, see [Icon themes](#icon-themes).
- **`snippets.toml`** — `[<language>.<name>]` with `prefix`, `body`, optional `description`;
  `["*".<name>]` applies to every language.

Every action has a stable `namespace:command` name used by the palette, the keymap and the
menus. Default bindings (macOS; elsewhere `cmd` becomes `ctrl`, Cmd+arrow motions are dropped,
Alt word motions become Ctrl, and terminal-scope `cmd` keys become `ctrl-shift`). While the
Term pane has focus, window-wide keys without Cmd (without Ctrl+Shift off macOS) — Ctrl+G,
Ctrl+Tab, Alt+Z, Escape… — go to the shell instead, and FerrumTerm's own keys (⌘T, ⌘D, ⌘W,
⌘C / ⌘V…) apply; the `terminal:*` keys always work. The Git and Agent panes have their own keys
while focused too (FerrumGIT: ⌘P quick open, ⌘R reload; FerrumChat: ↵ send, ⇧Tab mode, ⌘.
stop…):

| Command | Default keys | Active in |
|---------|--------------|-----------|
| `application:new-file` | `cmd-n` | window |
| `application:open-file` | `cmd-o` | window |
| `application:open-folder` | `cmd-shift-o` | window |
| `application:open-settings` | `cmd-,` | window |
| `application:open-config` | `cmd-alt-,` | window |
| `application:open-keymap` | — | window |
| `application:open-snippets` | — | window |
| `application:open-theme` | — | window |
| `application:about` | — | window |
| `application:quit` | `cmd-q` | window |
| `core:save` | `cmd-s` | window |
| `core:save-as` | `cmd-shift-s` | window |
| `core:save-all` | `cmd-alt-s` | window |
| `core:close-tab` | `cmd-w` | window |
| `core:reopen-closed-tab` | `cmd-shift-t` | window |
| `core:reload-file` | — | window |
| `pane:next-tab` | `ctrl-tab`, `cmd-shift-]` | window |
| `pane:previous-tab` | `ctrl-shift-tab`, `cmd-shift-[` | window |
| `pane:split-right` | `cmd-k right` | window |
| `pane:split-down` | `cmd-k down` | window |
| `pane:close` | `cmd-k cmd-w` | window |
| `pane:focus-next` | `cmd-k cmd-right` | window |
| `pane:focus-previous` | `cmd-k cmd-left` | window |
| `tree-view:toggle` | `cmd-\\` | window |
| `tree-view:reveal-active-file` | `cmd-shift-\\` | window |
| `tree-view:focus` | `ctrl-0` | window |
| `tree-view:new-file` | `a` | tree view |
| `tree-view:new-folder` | `shift-a` | tree view |
| `tree-view:rename` | `f2`, `m` | tree view |
| `tree-view:delete` | `delete`, `backspace` | tree view |
| `tree-view:collapse-all` | — | tree view |
| `tree-view:select-next` | `down` | tree view |
| `tree-view:select-previous` | `up` | tree view |
| `tree-view:expand` | `right` | tree view |
| `tree-view:collapse` | `left` | tree view |
| `tree-view:open-selected` | `enter`, `space` | tree view |
| `tree-view:reveal-in-finder` | — | tree view |
| `fuzzy-finder:toggle-file-finder` | `cmd-p`, `cmd-t` | window |
| `command-palette:toggle` | `cmd-shift-p` | window |
| `go-to-line:toggle` | `ctrl-g` | window |
| `find-and-replace:show` | `cmd-f` | window |
| `find-and-replace:show-replace` | `cmd-alt-f` | window |
| `find-and-replace:find-next` | `cmd-g` | window |
| `find-and-replace:find-previous` | `cmd-shift-g` | window |
| `find-and-replace:use-selection-as-find` | `cmd-e` | editor |
| `find-in-files:show` | `cmd-shift-f` | window |
| `markdown-preview:toggle` | `cmd-shift-m` | window |
| `git:toggle-view` (Git pane) | `cmd-alt-g` | window |
| `git:show-panel` (Git pane) | `ctrl-shift-g` | window |
| `git:open-in-ferrumgit` (the FerrumGIT app) | — | window |
| `tools:close-all` / `tools:reset-layout` | — | window |
| `ci:toggle-view` (CI pane) | `cmd-shift-i` | window |
| `ci:refresh` | `cmd-r` in the CI pane | window |
| `ci:search-log` | `cmd-f` in the CI pane | window |
| `logs:toggle-view` (Logs pane) | `cmd-alt-l` | window |
| `logs:add-source` / `logs:stop-all` | — | window |
| `tasks:build` | `cmd-b` | window |
| `tasks:run` | `cmd-shift-b` | window |
| `tasks:test` | — | window |
| `tasks:rerun-last` | — | window |
| `agents:toggle-panel` (Agent pane) | `cmd-alt-a` | window |
| `agents:new-session` (CLI agent in the Term pane) | — | window |
| `debug:start` | — | window |
| `debug:continue` | `f5` | window |
| `debug:stop` | `shift-f5` | window |
| `debug:restart` | `cmd-shift-f5` | window |
| `debug:pause` | `f6` | window |
| `debug:step-over` | `f10` | window |
| `debug:step-into` | `f11` | window |
| `debug:step-out` | `shift-f11` | window |
| `debug:toggle-breakpoint` | `f9` | window |
| `debug:remove-all-breakpoints` | — | window |
| `debug:toggle-panel` (Debug pane) | `cmd-shift-y` | window |
| `debug:add-watch` / `debug:install-adapter` | — | window |
| `problems:show` | `ctrl-shift-m` | window |
| `language-server:restart` | — | window |
| `language-server:install` | — | window |
| `language-server:show-install-log` | — | window |
| `navigation:back` | `ctrl--` | window |
| `navigation:forward` | `ctrl-shift--` | window |
| `terminal:toggle` (Term pane) | `` ctrl-` `` | window |
| `terminal:new` (new tab in the Term pane) | `` ctrl-shift-` `` | window |
| `terminal:open-in-ferrumterm` (the FerrumTerm app) | — | window |
| `window:increase-font-size` | `cmd-=`, `cmd-+` | window |
| `window:decrease-font-size` | `cmd--` | window |
| `window:reset-font-size` | `cmd-0` | window |
| `theme:use-dark` | — | window |
| `theme:use-light` | — | window |
| `theme:select` | `cmd-k cmd-t` | window |
| `theme:install-from-file` | — | window |
| `theme:import-vscode-theme` | — | window |
| `theme:open-themes-folder` | — | window |
| `theme:reload` | — | window |
| `icon-theme:select` | — | window |
| `icon-theme:open-folder` | — | window |
| `icon-theme:reload` | — | window |
| `editor:toggle-soft-wrap` | `alt-z` | window |
| `editor:toggle-line-numbers` | — | window |
| `editor:toggle-indent-guides` | — | window |
| `editor:undo` | `cmd-z` | editor |
| `editor:redo` | `cmd-shift-z`, `cmd-y` | editor |
| `editor:copy` | `cmd-c` | editor |
| `editor:cut` | `cmd-x` | editor |
| `editor:paste` | `cmd-v` | editor |
| `editor:select-all` | `cmd-a` | editor |
| `editor:select-line` | `cmd-l` | editor |
| `editor:newline` | `enter`, `shift-enter` | editor |
| `editor:newline-below` | `cmd-enter` | editor |
| `editor:newline-above` | `cmd-shift-enter` | editor |
| `editor:backspace` | `backspace`, `shift-backspace` | editor |
| `editor:delete` | `delete` | editor |
| `editor:delete-word-left` | `alt-backspace` | editor |
| `editor:delete-word-right` | `alt-delete` | editor |
| `editor:delete-to-line-start` | `cmd-backspace` | editor |
| `editor:delete-to-line-end` | `cmd-delete` | editor |
| `editor:delete-line` | `ctrl-shift-k` | editor |
| `editor:tab` | `tab` | editor |
| `editor:shift-tab` | `shift-tab` | editor |
| `editor:indent` | `cmd-]` | editor |
| `editor:outdent` | `cmd-[` | editor |
| `editor:toggle-comment` | `cmd-/` | editor |
| `editor:duplicate-lines` | `cmd-shift-d` | editor |
| `editor:move-line-up` | `ctrl-cmd-up` | editor |
| `editor:move-line-down` | `ctrl-cmd-down` | editor |
| `editor:join-lines` | `cmd-j` | editor |
| `editor:upper-case` | `cmd-k cmd-u` | editor |
| `editor:lower-case` | `cmd-k cmd-l` | editor |
| `editor:select-next` | `cmd-d` | editor |
| `editor:select-all-occurrences` | `cmd-ctrl-g` | editor |
| `editor:split-selection-into-lines` | `cmd-shift-l` | editor |
| `editor:add-cursor-above` | `ctrl-shift-up` | editor |
| `editor:add-cursor-below` | `ctrl-shift-down` | editor |
| `editor:go-to-matching-bracket` | `ctrl-m` | editor |
| `editor:cancel` | `escape` | editor |
| `editor:show-character-palette` | `ctrl-cmd-space` | editor |
| `editor:show-completions` | `ctrl-space`, `cmd-i` | editor |
| `editor:go-to-definition` | `f12` | editor |
| `editor:rename-symbol` | `f2` | editor |
| `editor:find-references` | `shift-f12` | editor |
| `editor:show-code-actions` | `cmd-.` | editor |
| `editor:format-document` | `shift-alt-f` | editor |
| `editor:show-signature-help` | `cmd-shift-space` | editor |
| `editor:show-hover` | `cmd-k cmd-i` | editor |
| `editor:go-to-next-problem` | `f8` | editor |
| `editor:go-to-previous-problem` | `shift-f8` | editor |
| `editor:move-left` | `left`, `ctrl-b` | editor |
| `editor:move-right` | `right`, `ctrl-f` | editor |
| `editor:move-up` | `up`, `ctrl-p` | editor |
| `editor:move-down` | `down`, `ctrl-n` | editor |
| `editor:select-left` | `shift-left` | editor |
| `editor:select-right` | `shift-right` | editor |
| `editor:select-up` | `shift-up` | editor |
| `editor:select-down` | `shift-down` | editor |
| `editor:move-to-previous-word` | `alt-left` | editor |
| `editor:move-to-next-word` | `alt-right` | editor |
| `editor:select-to-previous-word` | `alt-shift-left` | editor |
| `editor:select-to-next-word` | `alt-shift-right` | editor |
| `editor:move-to-line-start` | `cmd-left`, `home`, `ctrl-a` | editor |
| `editor:move-to-line-end` | `cmd-right`, `end`, `ctrl-e` | editor |
| `editor:select-to-line-start` | `cmd-shift-left`, `shift-home` | editor |
| `editor:select-to-line-end` | `cmd-shift-right`, `shift-end` | editor |
| `editor:move-to-top` | `cmd-up`, `cmd-home` | editor |
| `editor:move-to-bottom` | `cmd-down`, `cmd-end` | editor |
| `editor:select-to-top` | `cmd-shift-up`, `cmd-shift-home` | editor |
| `editor:select-to-bottom` | `cmd-shift-down`, `cmd-shift-end` | editor |
| `editor:page-up` | `pageup` | editor |
| `editor:page-down` | `pagedown` | editor |
| `editor:select-page-up` | `shift-pageup` | editor |
| `editor:select-page-down` | `shift-pagedown` | editor |

## Language servers
Completions, problems, go to definition, rename (F2), references (Shift+F12), code actions
(Cmd+.), formatting (Shift+Alt+F, and on save) and signature help come from the language
server for the file's language. The project switcher (⌥⌘O) is not an editor command: it is
bound by the shared project bar. You don't need to install anything: the first time you open a Rust, Go, TypeScript /
JavaScript (or Python, JSON, CSS, HTML, YAML, TOML, shell) file without its server,
FerrumIDE installs it into `~/.ferrum/editor/servers/` and starts it; the status bar shows
`Installing …` meanwhile. A server you installed yourself (on your login shell's `PATH`, or
in the project's `node_modules/.bin`) is used instead.

| Id | Languages | Program | How FerrumIDE installs it |
|----|-----------|---------|-------------------------------|
| `rust` | Rust | `rust-analyzer` | `rustup component add rust-analyzer`, or the official GitHub release without rustup |
| `typescript` | JavaScript, TypeScript, TSX | `typescript-language-server --stdio` | npm: `typescript`, `typescript-language-server` |
| `go` | Go | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| `python` | Python | `pyright-langserver --stdio` (or `basedpyright`, `pylsp` if you have them) | npm: `pyright` |
| `json` / `css` / `html` | JSON, CSS, HTML | `vscode-*-language-server --stdio` | npm: `vscode-langservers-extracted` |
| `yaml` | YAML | `yaml-language-server --stdio` | npm: `yaml-language-server` |
| `shell` | Shell | `bash-language-server start` | npm: `bash-language-server` |
| `toml` | TOML | `taplo lsp stdio` | the official GitHub release |

npm servers need Node.js and gopls needs Go: when you don't have them, the latest Node.js LTS
(nodejs.org) or Go (go.dev) is downloaded into `servers/` as well, and used only after its
SHA-256 matches the published one. Each install writes `servers/logs/<id>.log`; if one fails,
the status bar says so and a click opens the log (*Language Server: Show Install Log*).
*Language Server: Install* tries again; `auto_install = false` under `[lsp]` turns automatic
installs off.

A server starts in the open folder (or, for files outside it, the nearest folder with a root
marker such as `Cargo.toml` or `package.json`) the first time you open one of its files. The
status bar shows its state (`rust-analyzer`, its indexing progress, `Installing …`, or
`stopped`, which restarts on click). Override a server, or turn one off, in `config.toml`:

```toml
autocomplete = true          # menu while typing; Ctrl+Space always asks
format_on_save = true        # Rust, Go, TS/JS by default when the server can format

[languages.python]
format_on_save = true        # opt a language in (or out with false)

[lsp]
enabled = true

[lsp.python]
command = "pylsp"            # program on PATH, or a full path
args = []

[lsp.rust.initialization_options]
check = { command = "clippy" }

[lsp.yaml]
enabled = false
```

## Tool panes
The **[Git] [Agent] [Term]** buttons at the right end of the top bar open their tool *inside*
the window as a split pane (tooltips show the shortcut); clicking again closes it. All of them
can be open at once, together with the CI, Logs and Debug panes (*View* menu, command palette
or their shortcuts). By default Git, Agent, CI and Debug open in the **right** column, stacked
top to bottom in that order, and Term and Logs in the **bottom** row under the editor.

| Pane | Button | Shortcut | What it is |
|------|--------|----------|------------|
| Git | ⎇ | ⌥⌘G, ⌃⇧G | FerrumGIT's interface (`ferrumgit-ui`, compact): changes, staging, commit, history, branches, diffs |
| Agent | ✦ | ⌥⌘A | FerrumChat (`ferrumagent-ui`): agentic chat on the project, change review, CLI agents over ACP |
| Term | ⌨ | ⌃\` | FerrumTerm (`ferrumterm-ui`): tabs, splits, agents sidebar; session `editor:<folder>` on the shared `fterm` server |
| CI | — | ⇧⌘I | GitHub Actions of the project (see [CI](#ci)) |
| Logs | — | ⌥⌘L | live logs of the project (see [Logs](#logs)) |
| Debug | — | ⇧⌘Y | the debugger's toolbar, stack, variables and console |

The shortcuts open the pane and focus it, focus it when it is open elsewhere, and close it when
it already has focus. Each pane has a small header: drag it (the grab cursor) onto another
pane to share that pane's place, or onto the **Move to Left / Right / Bottom** zones that appear
at the window's edges while dragging; the header's ⋯ menu has the same moves and **Close**
(and *Open in FerrumGIT app* / *Open in FerrumTerm app* for Git and Term — also on right-click
of their top-bar buttons). Drag the divider between the editor and a dock to resize the dock
and the dividers between panes to share it; docks keep the panes' minimum widths (Git and Term
320 px, Agent and CI 360 px). *Tools: Close All* and *Tools: Reset Layout* do what they say.

The layout — which panes are open, their dock and order, the dock sizes and the shares — is
saved in `session.json` **per project folder** and restored at start; a folder seen for the
first time keeps the current layout. A closed pane keeps its view (a terminal keeps running, a
chat keeps its thread) and comes back where it was.

Events from the panes: *Open* in Git or a file link in the chat opens the file at that line in
the editor; the chat's *Run in terminal* opens the Term pane and runs the command in a new tab;
when the agent changes files, open buffers reload (or ask, when edited); a commit refreshes the
branch and gutter marks; opening another folder moves Git (`set_repo`), Term (`set_cwd`) and a
new chat to it.

## CI
The **CI** pane (⇧⌘I, *View → CI Pane*, or `CI: Toggle View` in the command palette) shows
the GitHub Actions of the open folder's
repository: its `origin` remote (else the first remote), read from `.git/config`. Opening
another folder moves the page to it; with no folder open, or a folder without a GitHub
remote, the page says so and lists recent Ferrum projects that are on GitHub (a click opens
one in the editor). GitLab remotes get a "not supported yet" message. The page and its small
background runtime start the first time it opens.

- **Runs**: status (queued, running with a live timer, success, failure, cancelled, skipped,
  needs approval), title, workflow and run number, event, branch, actor, duration, age.
  Filters: workflow, branch (the checked-out one by default; **Current** / **All**), event
  and status.
- **Run detail**: jobs and the selected job's steps; **Re-run all jobs**, **Re-run failed
  jobs**, **Cancel run**, **Notify when done**, **Open on GitHub**.
- **Job logs** (once the job finishes): ANSI colours, `##[group]` folding (quiet groups start
  folded), errors and warnings highlighted, opens at the first error; ⌘F searches (Enter /
  the arrows step through matches). Virtualised, so long logs stay fast.
- **Run workflow…**: a `workflow_dispatch` form built from the workflow file at the chosen
  ref (string, number, boolean, choice and environment inputs); the new run is watched.
- **Pull requests**: open PRs with their combined check status; a click shows that
  branch's runs.
- **Refresh**: every 12 s while something runs, 60 s otherwise, slower near the rate limit;
  GETs use ETags so unchanged answers do not count against it. ⌘R refreshes now. A watched
  run that finishes shows a notice (and a macOS notification).

**Sign-in**: the page looks for a token in FerrumGIT's keychain entry, then
`gh auth token --hostname <host>`, then `GH_TOKEN` / `GITHUB_TOKEN`
(`GH_ENTERPRISE_TOKEN` / `GITHUB_ENTERPRISE_TOKEN` on Enterprise hosts). Otherwise the
sign-in screen takes a pasted token and saves it in the same keychain entry, so FerrumGIT
uses it too. A rejected token brings the sign-in screen back. Tokens are never logged or
shown. Logic lives in `ci/crates/core` (`ferrumci-core`, see `ci/README.md`).

## Logs
The **Logs** button of the switch (or ⌥⌘L, *View → Logs*, `Logs: Toggle View` in the
palette) shows the live logs of the open folder: log files and globs (`logs/*.log`, followed
through truncation and rotation), commands such as dev servers (run in your login shell in
the project folder; start / stop / restart), and Docker containers or compose services,
merged into one view. A project without sources gets suggestions (`package.json` dev
scripts, `cargo run`, `manage.py runserver`, compose services, `*.log` folders); one click
adds and starts one. Sources are saved per project in `~/.ferrum/logs/projects.json`; the
ones marked "start when the project opens" start when the page first shows that project.
They keep running while the editor shows something else or another folder (the page's
sidebar lists open and recent projects with their running count; a click opens one in the
editor), and stop when the editor quits.

Lines show a coloured source chip, timestamp, level and ANSI colours; JSON lines (pino,
bunyan, tracing, zap…) show level + message with fields alongside. On the page: ⌘F search
(case / regex toggles), ⌘E errors only, ⌘K clear, End or ⌘↓ jump to the latest line and
follow, click / shift-click to select and ⌘C to copy, ⌘S export the visible lines. History
is capped at 200 000 lines per project. Logic lives in `logs/crates/core` (`ferrumlogs-core`,
see `logs/README.md`).

## Tasks
⌘B runs the project's build task and ⇧⌘B lists every task. Tasks come from the open folder:

| File | Tasks (⌘B default first) |
|------|--------------------------|
| `.vscode/tasks.json` | its `shell` / `process` tasks; the `build` group's `isDefault` one is ⌘B |
| `Cargo.toml` | `cargo build`, `check`, `test`, `run`, `clippy --all-targets`, `build --release` |
| `go.mod` / `go.work` | `go build ./...`, `go test ./...`, `go vet ./...`, `go run .` |
| `package.json` | every script as `pnpm run …` / `yarn run …` / `bun run …` / `npm run …` (from the lock file); `build` is ⌘B, or `tsc --noEmit` when there is a `tsconfig.json` but no build script |
| `Makefile` | its targets |

A task runs in a new tab of the Term pane (through `/bin/sh`, its output copied to a temp file),
after edited files are saved. When it ends, errors and warnings in
its output (cargo / rustc, tsc, Go, and `file:line:col: message` compilers) appear as problems
next to the language server's, until the next run.

## AI agents
⌥⌘A (or the **Agent** button) opens the Agent pane: FerrumChat on the project, with its own
threads, agent / model picker — its providers, or Claude Code, Codex, Gemini CLI and other ACP
agents — modes, @-mentions, tool cards with permission prompts and a review of the agent's
changes with accept / reject and checkpoints. Its settings live in `~/.ferrum/agent` (see
`agent/`). When the chat changes files, open buffers reload; *Run in terminal* runs in the Term
pane.

*Agents: New Session* still starts a CLI agent from config.toml in a Term pane tab. Built in:
`claude` (Claude Code), `codex` (OpenAI Codex), `gemini` (Gemini CLI); others via `[[agents]]`.
An agent is found on `PATH`, `~/.ferrum/bin`, `~/.local/bin` or npm's global bin; a missing
one offers its install command (`npm i -g …` runs in the Term pane). *Start … with the current
file / selection* passes `I'm looking at <file>, lines a-b: …` as the first prompt.

```toml
agent_idle_seconds = 5        # top level: no output this long = "waiting for input"

[[agents]]                    # a new agent…
name = "aider"
command = "aider"             # program on PATH, or a full path
args = ["--no-auto-commits"]
prompt = "--message"          # how a first prompt is passed: "positional", a flag, or leave out
install = "pipx install aider-chat"
env = { OPENAI_API_KEY = "sk-…" }

[[agents]]                    # …or a key for a built-in (enabled = false hides one)
name = "claude"
env = { ANTHROPIC_API_KEY = "sk-ant-…" }
```

`env` values go to the agent's process only (as `env K=V …` in front of its command line).
`agent_idle_seconds` is no longer used.

## Debugger
F5 starts debugging (a picker of configurations: `.vscode/launch.json` first, then defaults
for the project and the current file), and continues when paused. F9 or a click in the
gutter's left edge toggles a breakpoint (red dot; faded until the adapter verifies it);
breakpoints are saved per project in `~/.ferrum/editor/breakpoints.json`. The paused line is
highlighted with an arrow. F10 / F11 / ⇧F11 step over / into / out, F6 pauses, ⇧F5 stops,
⇧⌘F5 restarts. The Debug pane (⇧⌘Y) has the toolbar, the call stack (click a
frame), variables (expand to load children), watch expressions, the breakpoints list and the
debug console (program output; type an expression to evaluate it in the paused frame).

| Language | `type` | Adapter | Where it comes from |
|----------|--------|---------|---------------------|
| Rust (and C) | `lldb`, `lldb-dap`, `codelldb` | `lldb-dap` (stdio), else CodeLLDB (TCP) | `xcrun -f lldb-dap` (Xcode Command Line Tools), `lldb-dap` / `lldb-vscode` on PATH, Homebrew LLVM; CodeLLDB on PATH or the VS Code extension |
| Go | `go` | `dlv dap` (TCP) | PATH, or *Debug: Install Adapter* (`go install github.com/go-delve/delve/cmd/dlv@latest` into `servers/dlv`, Go downloaded first if needed) |
| JS / TS | `node`, `pwa-node`, `node-terminal` | js-debug's `dapDebugServer.js` under Node (TCP) | *Debug: Install Adapter*: the `js-debug-dap-*.tar.gz` of the latest `microsoft/vscode-js-debug` GitHub release, checked against its published SHA-256 (or size), into `servers/js-debug`; Node.js downloaded if needed. Child sessions (`startDebugging`) are supported |

Defaults without launch.json: *Rust: debug binary* (`cargo build --message-format=json`, the
built binary is the program) and *Rust: debug tests* (`cargo test --no-run`), *Go: debug
package* / *debug tests* (the current file's package), *Node: <current file>* and one entry
per package.json script (`npm run …` with the project's package manager). In launch.json,
`${workspaceFolder}`, `${file}`, `${fileDirname}`, `${relativeFile}`, `${env:NAME}` … are
substituted; a CodeLLDB-style `"cargo": {"args": [...], "filter": {"name": …, "kind": …}}`
builds first; `"debugAdapterPath"` (+ `"debugAdapterArgs"`) runs your own stdio adapter build.
A `runInTerminal` request from the adapter runs the program in a Term pane tab.

```jsonc
{
  "configurations": [
    { "name": "Server", "type": "lldb", "request": "launch",
      "cargo": { "args": ["build", "--bin", "server"] }, "args": ["--port", "8080"] },
    { "name": "API", "type": "go", "request": "launch", "mode": "debug", "program": "${workspaceFolder}/cmd/api" },
    { "name": "Script", "type": "pwa-node", "request": "launch", "program": "${file}" }
  ]
}
```

On macOS, F11 may be taken by *Show Desktop* (System Settings → Keyboard → Shortcuts); rebind
`debug:step-into` in `keymap.toml` if so.

## Integrated terminal
The Term pane is FerrumTerm's view (`ferrumterm-ui`): ⌃\` or the **Term** button (or the
terminal button at the right end of the status bar) opens it at the bottom, in the project
folder, as the session `editor:<folder>` of the shared `fterm` server — so shells outlive the
editor and also show in FerrumTerm (a private in-process server is used when the shared one
can't start). ⌃⇧\` opens another tab. Tabs, splits (⌘D), the agents / sessions / SSH sidebar
and the rest are FerrumTerm's; colours follow the theme's `[terminal]` tokens. *Terminal:
Open in FerrumTerm App* starts `fterm new <folder> -c <folder>` and/or the `ferrumterm` app when they are on `PATH`.

## Themes
Theme files live in `~/.ferrum/editor/themes/` (`$FERRUM_EDITOR_HOME/themes/`), are watched,
and show up in *Theme: Select* (↑↓ previews live, Enter applies and saves `theme = "…"` to
`config.toml`, Escape reverts). *Theme: Install from File…* validates a file and copies it in.
A file that fails to load is skipped with a notice. The six Material themes (*Material
Default*, *Darker*, *Palenight*, *Ocean*, *Deep Ocean*, *Lighter*) are bundled and listed after
your own themes. Two kinds are loaded:

**Ferrum TOML (`*.toml`)** — `ferrum-dark.example.toml` and `ferrum-light.example.toml` are
written on first run as complete, commented examples (files ending in `.example.toml` are not
loaded: copy one to a new name to start). Every key is optional; missing tokens come from the
built-in named by `appearance`.

```toml
name = "my-theme"          # default: the file name; a taken name falls back to it
appearance = "dark"        # "dark" | "light"

[ui]       # window chrome and editor surface
background = "#21252b"     # window background behind panels
panel = "#21252b"          # tree view, find panels
surface = "#2b3038"        # popovers: palette, dialogs, menus
editor = "#282c33"         # text area
gutter = "#282c33"         # line-number gutter
border = "#363c46"
border_focused = "#e8743b"
text = "#dce0e5"
text_muted = "#a9afbc"     # secondary text, untinted icons
text_placeholder = "#6b717d"
accent = "#e8743b"         # active tab marker, highlights, links
element_hover = "#2f343e"
element_selected = "#3a404b"
tab_active = "#282c33"
tab_inactive = "#21252b"
tab_bar = "#21252b"
status_bar = "#21252b"
title_bar = "#21252b"
scrollbar = "#ffffff22"
selection = "#e8743b38"    # text selection
cursor = "#e8743b"
current_line = "#2d323a"
line_number = "#5d636f"
line_number_active = "#d0d4da"
indent_guide = "#353a43"
indent_guide_active = "#4f5561"
bracket_match = "#9aa2b1"
find_match = "#f0b43c40"
find_match_active = "#f0b43c90"
git_added = "#7cc85a"      # gutter marks
git_modified = "#5b8def"
git_deleted = "#e85a7e"
error = "#e85a7e"          # notices
warning = "#f0b43c"
success = "#7cc85a"

[syntax]   # tree-sitter highlight names; `a.b` falls back to `a`
attribute = "#f0b43c"
boolean = "#e8935f"
comment = "#6f7683"
constant = "#e8935f"
"function.macro" = "#3fb8a0"
keyword = "#d46be0"
string = "#a1c181"
type = "#6ec2c8"
# … also: character constructor embedded escape function label module number operator
# property punctuation punctuation.special string.special tag variable variable.builtin
# variable.parameter text.title text.literal text.uri text.reference text.emphasis
# text.strong markup.heading markup.raw markup.link markup.italic markup.bold markup.list
# markup.quote

[terminal] # integrated terminal
background = "#21252b"
foreground = "#dce0e5"
cursor = "#e8743b"
selection = "#e8743b45"
black = "#3f4451"          # ANSI 0–7 …
red = "#e85a7e"
green = "#a1c181"
yellow = "#f0b43c"
blue = "#73ade9"
magenta = "#d46be0"
cyan = "#3fb8a0"
white = "#dce0e5"
bright_black = "#6b717d"   # … and 8–15
bright_red = "#ff7a96"
bright_green = "#b9d99a"
bright_yellow = "#f5c867"
bright_blue = "#8fc1f2"
bright_magenta = "#e08aea"
bright_cyan = "#5fd0b8"
bright_white = "#ffffff"
```

**VS Code colour themes (`*.json`)** — the `themes/*.json` of a VS Code theme extension
(JSONC: comments and trailing commas are fine; `include` of a sibling file is followed; a
`tokenColors` path to a `.tmTheme` is not supported). `type` (`dark` / `light` / `hc-light`)
picks the base. Workbench `colors` map onto our tokens, first key present wins — e.g.
`editor.background` → `editor`, `editor.foreground` → `text`, `sideBar.background` → `panel` /
`background`, `editorWidget.background` → `surface`, `button.background` / `focusBorder` →
`accent`, `list.hoverBackground` → `element_hover`, `list.activeSelectionBackground` →
`element_selected`, `tab.*` / `editorGroupHeader.tabsBackground` → tab tokens,
`statusBar.background`, `titleBar.activeBackground`, `editor.selectionBackground`,
`editorCursor.foreground`, `editor.lineHighlightBackground`, `editorLineNumber.*`,
`editorIndentGuide.*`, `editorBracketMatch.border`, `editor.findMatch*Background`,
`editorGutter.*Background` (git), `editorError/Warning.foreground`, and `terminal.background` /
`terminal.foreground` / `terminalCursor.foreground` / `terminal.selectionBackground` /
`terminal.ansi*` → `[terminal]`. `tokenColors` rules map onto highlight names through TextMate
scopes (e.g. `keyword` ← `keyword.control`, `keyword`, `storage.type`; `function` ←
`entity.name.function`, `support.function`; `type` ← `entity.name.type`, `support.type`;
`property` ← `variable.other.property`, `support.type.property-name`; `string.special` ←
`string.regexp`); a rule matches a scope it is a dot-prefix of, the longest selector wins.
Descendant / exclusion selectors, `fontStyle` and `semanticTokenColors` are ignored.

*Theme: Import VS Code Theme…* converts instead of copying: pick a `.json` theme (its
`include` files are read from next to it) or a `.vsix` extension (every theme listed under
`contributes.themes` in its `package.json`; `uiTheme` decides light / dark when the theme has
no `type`, and its `label` names it). Each becomes `themes/<name>.toml`, a complete Ferrum
theme that no longer needs the original files; the first is applied. Importing again replaces
the file. *Install from File…* also accepts `.vsix`.

## Icon themes
The built-in `ferrum` set (drawn for FerrumIDE, tinted per file type) covers folders
(open / closed), common languages and extensions, and special names (`Cargo.toml`,
`package.json`, lock files, `Dockerfile`, `.gitignore`, `README`, `LICENSE`, `.env`, `Makefile`,
images, archives, fonts, media…). Set `show_file_icons = false` to hide icons. An icon theme is
a folder `~/.ferrum/editor/icon-themes/<name>/` with a `theme.toml` and SVG files (an
`example` theme is written on first run). Pick one with *Icon Theme: Select* (live preview;
Enter saves `icon_theme = "…"`). For a file the lookup is **exact file name → extension
(longest first: `d.ts` before `ts`) → language id → `defaults.file`**; for a folder: folder name
→ `defaults.folder` / `defaults.folder_open`. Whatever a theme does not cover (not even a
default) comes from the built-in set. The bundled `material` theme is the same lookup with
solid Material-style glyphs and folders coloured by role (`src`, `lib`/`crates`, `tests`,
`docs`, `node_modules`, `.github`, `.vscode`, `assets`, `components`, `config`,
`dist`/`build`/`target`, `scripts`, `public`, `api`, `styles`, `examples`, `.git`); its icons
are drawn for FerrumIDE (no third-party icon files).

```toml
name = "mono"                 # default: the folder name
[defaults]
file = "file.svg"
folder = "folder.svg"
folder_open = "folder-open.svg"
[file_names]                  # exact, case-insensitive
"Cargo.toml" = "cargo.svg"
".gitignore" = "git.svg"
[extensions]                  # without the dot
rs = "rust.svg"
"d.ts" = "types.svg"
[languages]                   # rust javascript typescript tsx json toml yaml markdown
shell = "shell.svg"           # python go html css shell text
[folder_names]
src = "folder-src.svg"
[folder_names_open]
src = "folder-src-open.svg"
[colors]                      # optional tint per SVG (default: the theme's muted text)
"rust.svg" = "#dea584"
```

SVGs must sit inside the theme folder (no `..`), be under 256 KB, and are painted as a
single-colour mask (shape only; colours inside the SVG are ignored).

## Layout
```
apps/desktop/      GPUI app (binary `ferrumeditor`)
  main.rs          CLI, boot, window close → quit flow
  actions.rs       RunCommand action, keymap → GPUI bindings, app menus
  settings.rs      user settings global (config, theme, keymap, snippets)
  theme_apply.rs   token theme → GPUI/`ui` theme
  registry.rs      command handlers by name + the `Package` extension seam
  editor/          editor view: element (shaping, paint), decorations, layout (hit-testing,
                   text runs), input (mouse, scroll, IME), commands, search, background
                   (highlighting, git marks), lsp / lsp_actions / completion / popups
                   (problems, hover, rename input, references, code actions, formatting,
                   signature help)
  workspace/       root view: panes/tabs, files (open/save/close/reload/quit), overlays
                   (palette, finder, go to line, find bar, find in files, preview), themes
                   (selectors, preview, install, VS Code import), tools/ (the tool panes:
                   views, docks, headers, drag & drop, events from Git / Agent / Term,
                   commands run in the terminal), top_bar (project bar + [Git] [Agent]
                   [Term]), git / terminal / agents / debug (commands, pickers, glue), lsp /
                   lsp_edits (workspace edits, references panel, code action picker, format
                   on save), dialogs, watcher, session, status bar
  debug/           debugger (session driver over the core DAP client), panel (Debug pane)
  ci/              CI page over `ferrumci-core`: CiView (project → GitHub repo, tabs, status
                   line), load (requests), runs, detail, logview, dispatch, pulls, signin,
                   services (tokio runtime, HTTP, tokens, notifier); workspace/ci.rs makes it
                   a tool pane and adds the `ci:*` commands
  logs/            Logs page over `ferrumlogs-core`: LogsView (event pump, projects, keys),
                   commands, sidebar, log_view, dialog; workspace/logs.rs makes it a tool
                   pane and adds the `logs:*` commands
  file_icons.rs    icon theme → SVG elements
  tree_view/ markdown_preview.rs picker.rs find_bar.rs project_find.rs
  tests.rs tests_tools.rs tests_themes.rs tests_lsp.rs tests_debug.rs
  tests_ci.rs tests_logs.rs   headless GPUI tests
crates/core/       pure logic, no GPUI: buffer, selection, history, editor/ (all editing
                   commands), text (boundaries), display (tabs, wrap, display map), find,
                   project_search, fuzzy, language, highlight, git, config, keymap,
                   theme/ (tokens, built-ins, Material, TOML format, VS Code import incl.
                   .vsix, registry, overrides), icons/ (built-in and Material sets,
                   manifests, registry), lsp/ (client, edits, features, installers),
                   agents/ (presets, snapshot, review hunks), dap/ (client, state, launch
                   configs, adapters, breakpoints), tool_layout (docks of the
                   tool panes), terminal (key translation), snippets, commands (catalogue), files,
                   file_tree, session, markdown, cli, paths
```

## Runtime assets
None on disk: tree-sitter grammars are compiled in by the grammar crates and their highlight
queries are `include_str!`-embedded; fonts and UI icons come from the shared `assets` crate
(embedded); file icons are SVG strings in `crates/core/src/icons/builtin.rs`. The default
settings files, example themes and the example icon theme are string constants written on
first launch.

## Local data
`~/.ferrum/editor/` holds the four settings files, `themes/`, `icon-themes/`,
`breakpoints.json` (per project), `servers/` (language servers, `dlv`, `js-debug` and agents
installed by the editor, with `logs/`) and
`session.json` (last folder, pane layout, open tabs with cursor and scroll positions). Agent
snapshots live in memory (in git repositories as unreferenced objects that `git gc` prunes).
Nothing is sent over the network except installs you trigger, what the agents you run do and
the CI page's GitHub API requests (only once you open it); the Logs page runs the commands
and tails the files you add as sources; git
information comes from the `git` CLI (`git show HEAD:<path>`) and `.git/HEAD`.
