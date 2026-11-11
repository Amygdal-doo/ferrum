# Ferrum Editor

A small, hackable text editor in the spirit of Atom — tree view, tabs, split panes, fuzzy
finder, command palette, and plain-text config / keymap / theme / snippets you edit in the
editor itself — written from scratch in Rust on GPUI (the UI framework of Zed, via the
FerrumIDE fork) and the shared `ferrum-ui` crate. It is **not** FerrumIDE: the text buffer
(rope), multi-cursor editing, undo, rendering of the editor surface, highlighting and input
handling are its own; nothing from Zed's editor stack (`editor`, `language`, `project`,
`workspace`, `text`, `rope`, …) is used. Part of the Ferrum monorepo, sibling of
[FerrumKit](../kit), [FerrumDock](../dock), [FerrumGIT](../git) and the others.

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
| Editor surface: only visible rows are shaped; line numbers, current-line highlight, selections, cursors, indent guides, bracket-match boxes, search matches, git gutter, scrollbar, soft wrap (word-boundary wrapping), horizontal scroll, IME / dead keys via GPUI's input handler, mouse select / drag (word, line), clipboard (whole-line copy/paste) | ⚠️ headless-tested; not hand-tested |
| Workspace: tree view (expand/collapse, new file/folder, rename, Move to Trash, reveal active file, reveal in Finder, keyboard navigation), tabs with dirty dot and save prompt on close, split right/down, focus next/previous pane, reopen closed tab | ⚠️ headless-tested in parts |
| Command palette (every command with its current key binding), file finder (Cmd+P), go to line (Ctrl+G, `line[:col]`) | ✅ headless-tested |
| Status bar: path, git branch, Ln/Col, selection count / size, language, indentation, line ending, encoding, wrap | ⚠️ wired, not hand-tested |
| Git line marks (added / modified / deleted vs `HEAD`, via `git show`) and branch name (read from `.git/HEAD`) | ✅ diff tested; gutter not hand-tested |
| Markdown preview (Cmd+Shift+M) in a split, live re-render | ⚠️ headless-tested open/close |
| Reload when a file changes on disk (silent if clean, prompt if dirty), on watcher events and on window activation | ⚠️ wired, not hand-tested |
| Session restore (folder, pane layout, tabs, cursor and scroll) | ✅ headless-tested |
| User dir `~/.ferrum/editor/`: `config.toml`, `keymap.toml`, `theme.toml`, `snippets.toml` created with documented defaults, watched and applied live (problems shown as a notice) | ✅ parsing tested; live reload headless-tested |
| Snippets with `$1`, `${1:placeholder}`, mirrored stops, `$0`, Tab / Shift+Tab navigation | ✅ tested |
| Built-in `ferrum-dark` / `ferrum-light` themes applied to the whole UI | ⚠️ not hand-tested |
| Loadable themes in `~/.ferrum/editor/themes/`: our TOML format and VS Code colour themes (`.json`, comments / trailing commas / `include` allowed; workbench `colors` → UI + terminal tokens, `tokenColors` TextMate scopes → tree-sitter names); theme selector with live preview, Enter saves to `config.toml`, Escape reverts; install from file; folder watched live; bad files → notice | ✅ parsing / mapping unit-tested; selector headless-tested |
| File icons (own SVG set, tinted per type) in the tree view, tabs, file finder and find-in-files results; icon themes in `~/.ferrum/editor/icon-themes/<name>/`; icon theme selector with preview; `show_file_icons` | ✅ resolution unit- and headless-tested; drawing not hand-tested |
| Integrated terminal panel (FerrumTerm's emulator on a local PTY, your login shell in the project folder): tabs, ⌃\` / status-bar toggle, resizable, keeps running while hidden, keys incl. Ctrl / Alt / arrows / paste go to the shell, mouse selection + copy, wheel scrollback, ANSI colours from the theme | ✅ input / toggle / tabs headless-tested with a detached session; real PTY unit-tested; rendering not hand-tested |
| Language servers (LSP): one server per language and project folder, found on your login shell's `PATH` (rust-analyzer, typescript-language-server, pyright / pylsp, gopls, vscode-json/css/html, taplo, yaml-language-server, bash-language-server; override in `[lsp.<id>]`); full-text sync, UTF-8 / UTF-16 positions | ✅ client unit-tested over pipes; end to end headless-tested with a scripted server |
| Autocomplete: as you type (and on trigger characters such as `.`) or with Ctrl+Space / Cmd+I; server items (snippets with tab stops, auto-import edits) or words from the file when there is no server; ↑/↓, Enter / Tab to insert, Escape to close; `autocomplete = false` turns off the automatic menu | ✅ ranking / snippets unit-tested; typing → menu → insert headless-tested; menu drawing not hand-tested |
| Problems: wavy underlines and gutter dots coloured by severity, hover (or Cmd+K Cmd+I) for the message and the server's type / docs, F8 / Shift+F8 to step through, error / warning counts in the status bar, Ctrl+Shift+M lists every problem in the project | ✅ mapping and list headless-tested; drawing not hand-tested |
| Go to definition: F12 or Cmd+click; several results open a picker; Ctrl+- / Ctrl+Shift+- go back / forward | ✅ headless-tested |
| Tasks: build / test / run / lint tasks found from `Cargo.toml`, `go.mod`, package.json scripts (pnpm / yarn / bun / npm from the lock file; `tsc --noEmit` for TypeScript without a build script), Makefile targets and `.vscode/tasks.json`; ⌘B runs the default build, ⇧⌘B picks one; each runs in its own terminal tab (rerun replaces it) after saving edited files; errors and warnings from cargo/rustc, tsc, Go and `file:line:col` compilers become problems | ✅ detection and output parsing unit-tested; ⌘B → tab → problems headless-tested; real PTY task unit-tested |
| Git (FerrumGIT's git layer, not its UI): Files / Git tabs in the sidebar (⌃⇧G) with branch ↑↓, fetch / pull / push, commit message (⌘↵; Commit All when nothing is staged), staged and unstaged files with stage / unstage / discard (confirmed); an Editor \| Git switch at the top right (⌥⌘G) for the changes, recent history and the diff of a file or commit; refreshes after saves and when the window comes back; everything else opens FerrumGIT | ✅ headless-tested on a real repository; drawing not hand-tested |
| CLI: `ferrumeditor [path[:line[:col]]...]`, `--help`, `--version` | ✅ tested |

## Not built yet
Terminal: mouse reporting to programs (vim/htop mouse), selection beyond the visible screen,
link detection, find in scrollback, split terminals, per-terminal font settings; VS Code font
styles (bold / italic) and semantic token colours are not imported; icon SVGs are single-colour
masks (multi-colour icons are flattened to one tint).
Language servers: rename, find references, code actions / quick fixes, formatting, signature
help, inlay hints, semantic highlighting, workspace symbols, incremental document sync;
updating installed servers (Language Server: Install reinstalls the latest).
Incremental tree-sitter reparsing (every edit reparses the whole file in the background);
multiple windows; packages/plugins (a `Package` trait and command registry seam exist);
cursor blinking; minimap; code folding; encodings other than UTF-8; editing files
over 64 MB; a project-wide replace; drag-and-drop of tabs or files; tree view drag-to-move;
tab overflow menu; per-project settings; Linux/Windows key defaults beyond a mechanical
`cmd`→`ctrl` translation.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo editor                                          # run the app (alias for: cargo run -p ferrumeditor-desktop)
cargo run -p ferrumeditor-desktop -- src/main.rs:10   # open a file at line 10
cargo test -p ferrumeditor-core -p ferrumeditor-desktop
cargo clippy -p ferrumeditor-core -p ferrumeditor-desktop --all-targets -- -D warnings
```
The binary is `ferrumeditor`; the window / bundle display name is "Ferrum Editor". Release
builds read the version from `FERRUM_VERSION` at compile time (shown in About).

## Hacking it
Everything lives in `~/.ferrum/editor/` (override with `FERRUM_EDITOR_HOME`). Open the files
from the palette: *Application: Open Config / Open Keymap / Open Snippets / Open Theme*.
Saving any of them applies the change immediately.

- **`config.toml`** — font family/size, line height, `theme`, `icon_theme`, `show_file_icons`,
  `terminal_shell` (empty = your login shell), tab size, soft tabs, soft wrap,
  line numbers, indent guides, current-line highlight, auto-close, auto-indent, trim trailing
  whitespace, final newline, scroll past end, session restore, and `[languages.<id>]`
  overrides (`tab_size`, `soft_tabs`, `soft_wrap`).
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
Alt word motions become Ctrl, and terminal-scope `cmd` keys become `ctrl-shift`, e.g.
`ctrl-shift-c` to copy). While a terminal has focus, window-wide keys without Cmd (without
Ctrl+Shift off macOS) — Ctrl+G, Ctrl+Tab, Alt+Z, Escape… — go to the shell instead; the
`terminal:*` keys always work:

| Command | Default keys | Active in |
|---------|--------------|-----------|
| `application:new-file` | `cmd-n` | window |
| `application:open-file` | `cmd-o` | window |
| `application:open-folder` | `cmd-shift-o` | window |
| `application:open-config` | `cmd-,` | window |
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
| `git:toggle-view` | `cmd-alt-g` | window |
| `git:show-panel` | `ctrl-shift-g` | window |
| `git:refresh` / `git:commit` / `git:stage-all` | — | window |
| `git:fetch` / `git:pull` / `git:push` | — | window |
| `git:open-in-ferrumgit` | — | window |
| `tasks:build` | `cmd-b` | window |
| `tasks:run` | `cmd-shift-b` | window |
| `tasks:test` | — | window |
| `tasks:rerun-last` | — | window |
| `problems:show` | `ctrl-shift-m` | window |
| `language-server:restart` | — | window |
| `language-server:install` | — | window |
| `language-server:show-install-log` | — | window |
| `navigation:back` | `ctrl--` | window |
| `navigation:forward` | `ctrl-shift--` | window |
| `terminal:toggle` | `` ctrl-` `` | window |
| `terminal:new` | `` ctrl-shift-` `` | window |
| `terminal:close` | — | window |
| `terminal:next-tab` | `cmd-shift-]` | terminal |
| `terminal:previous-tab` | `cmd-shift-[` | terminal |
| `terminal:copy` | `cmd-c` | terminal |
| `terminal:paste` | `cmd-v` | terminal |
| `terminal:clear` | — | terminal |
| `terminal:open-in-ferrumterm` | — | window |
| `window:increase-font-size` | `cmd-=`, `cmd-+` | window |
| `window:decrease-font-size` | `cmd--` | window |
| `window:reset-font-size` | `cmd-0` | window |
| `theme:use-dark` | — | window |
| `theme:use-light` | — | window |
| `theme:select` | `cmd-k cmd-t` | window |
| `theme:install-from-file` | — | window |
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
Completions, problems and go to definition come from the language server for the file's
language. You don't need to install anything: the first time you open a Rust, Go, TypeScript /
JavaScript (or Python, JSON, CSS, HTML, YAML, TOML, shell) file without its server, Ferrum
Editor installs it into `~/.ferrum/editor/servers/` and starts it; the status bar shows
`Installing …` meanwhile. A server you installed yourself (on your login shell's `PATH`, or
in the project's `node_modules/.bin`) is used instead.

| Id | Languages | Program | How Ferrum Editor installs it |
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

## Git
The sidebar has two tabs, Files and Git (⌃⇧G). The Git tab shows the branch (with commits
ahead ↑ and behind ↓), fetch / pull (fast-forward only) / push, a commit message box (⌘↵
commits; with nothing staged it commits every change), and the staged and changed files:
`+` stages, `−` unstages, `↺` discards after asking. Clicking a file opens it; its gutter marks
the changed lines.

The **Editor | Git** switch at the top right (⌥⌘G) fills the window with the same changes,
the last 200 commits and the diff of the selected file or commit. Ferrum Editor uses
FerrumGIT's git layer for all of this and stops there: history graph, branches, rebase and
conflicts, stashes, tags, pull requests and SSH profiles are in FerrumGIT (the ↗ button).
Push and pull use your normal git setup (SSH agent, credential helper).

## Tasks
⌘B runs the project's build task and ⇧⌘B lists every task. Tasks come from the open folder:

| File | Tasks (⌘B default first) |
|------|--------------------------|
| `.vscode/tasks.json` | its `shell` / `process` tasks; the `build` group's `isDefault` one is ⌘B |
| `Cargo.toml` | `cargo build`, `check`, `test`, `run`, `clippy --all-targets`, `build --release` |
| `go.mod` / `go.work` | `go build ./...`, `go test ./...`, `go vet ./...`, `go run .` |
| `package.json` | every script as `pnpm run …` / `yarn run …` / `bun run …` / `npm run …` (from the lock file); `build` is ⌘B, or `tsc --noEmit` when there is a `tsconfig.json` but no build script |
| `Makefile` | its targets |

A task runs through your login shell in its own terminal tab (`▶`, then `✓` or `✗`), after
edited files are saved; running it again replaces the tab. When it ends, errors and warnings in
its output (cargo / rustc, tsc, Go, and `file:line:col: message` compilers) appear as problems
next to the language server's, until the next run.

## Integrated terminal
⌃\` (or the terminal button at the right end of the status bar) opens a panel at the bottom
with your login shell (`$SHELL -l`, or `terminal_shell` from `config.toml`) in the project
folder. ⌃\` again focuses it, and hides it when it already has focus; the button just shows /
hides. Drag the divider to resize. `+` adds a terminal tab; closing the last tab (or `exit`)
hides the panel. Shells keep running while the panel is hidden. The emulator is FerrumTerm's
(`ferrumterm-emulator`, VT parsing by `alacritty_terminal`, 10 000 lines of scrollback) on a
local PTY (`portable-pty`); nothing goes through the FerrumTerm server. Colours come from the
theme's `[terminal]` tokens. *Terminal: Open in FerrumTerm* starts `fterm new <folder> -c
<folder>` and/or the `ferrumterm` app when they are on `PATH`.

## Themes
Theme files live in `~/.ferrum/editor/themes/` (`$FERRUM_EDITOR_HOME/themes/`), are watched,
and show up in *Theme: Select* (↑↓ previews live, Enter applies and saves `theme = "…"` to
`config.toml`, Escape reverts). *Theme: Install from File…* validates a file and copies it in.
A file that fails to load is skipped with a notice. Two kinds are loaded:

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

## Icon themes
The built-in `ferrum` set (drawn for Ferrum Editor, tinted per file type) covers folders
(open / closed), common languages and extensions, and special names (`Cargo.toml`,
`package.json`, lock files, `Dockerfile`, `.gitignore`, `README`, `LICENSE`, `.env`, `Makefile`,
images, archives, fonts, media…). Set `show_file_icons = false` to hide icons. An icon theme is
a folder `~/.ferrum/editor/icon-themes/<name>/` with a `theme.toml` and SVG files (an
`example` theme is written on first run). Pick one with *Icon Theme: Select* (live preview;
Enter saves `icon_theme = "…"`). For a file the lookup is **exact file name → extension
(longest first: `d.ts` before `ts`) → language id → `defaults.file`**; for a folder: folder name
→ `defaults.folder` / `defaults.folder_open`. Whatever a theme does not cover (not even a
default) comes from the built-in set.

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
                   (highlighting, git marks)
  workspace/       root view: panes/tabs, files (open/save/close/reload/quit), overlays
                   (palette, finder, go to line, find bar, find in files, preview), themes
                   (selectors, preview, install), terminal (commands, status bar button),
                   dialogs, watcher, session, status bar
  terminal/        terminal panel (tabs), view (keys, mouse, scroll), session (PTY + emulator),
                   paint (grid renderer), selection, ime
  file_icons.rs    icon theme → SVG elements
  tree_view/ markdown_preview.rs picker.rs find_bar.rs project_find.rs
  tests.rs tests_terminal.rs tests_themes.rs   headless GPUI tests
crates/core/       pure logic, no GPUI: buffer, selection, history, editor/ (all editing
                   commands), text (boundaries), display (tabs, wrap, display map), find,
                   project_search, fuzzy, language, highlight, git, config, keymap,
                   theme/ (tokens, built-ins, TOML format, VS Code import, registry,
                   overrides), icons/ (built-in set, manifests, registry), terminal (panel
                   state, key translation), snippets, commands (catalogue), files,
                   file_tree, session, markdown, cli, paths
```

## Runtime assets
None on disk: tree-sitter grammars are compiled in by the grammar crates and their highlight
queries are `include_str!`-embedded; fonts and UI icons come from the shared `assets` crate
(embedded); file icons are SVG strings in `crates/core/src/icons/builtin.rs`. The default
settings files, example themes and the example icon theme are string constants written on
first launch.

## Local data
`~/.ferrum/editor/` holds the four settings files, `themes/`, `icon-themes/` and
`session.json` (last folder, pane layout, open tabs with cursor and scroll positions). Nothing is sent over the network; git
information comes from the `git` CLI (`git show HEAD:<path>`) and `.git/HEAD`.
