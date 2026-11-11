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
| Multi-cursor editing as single undo steps: typing, delete, newline, paste (one line per cursor when counts match), Cmd+click add/remove caret, Cmd+D next occurrence, select all occurrences, Cmd+Shift+L split into lines, add cursor above/below, Alt+drag column selection | ✅ core tested; mouse paths wired, not hand-tested |
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
| CLI: `ferrumeditor [path[:line[:col]]...]`, `--help`, `--version` | ✅ tested |

## Not built yet
Incremental tree-sitter reparsing (every edit reparses the whole file in the background);
multiple windows; packages/plugins (a `Package` trait and command registry seam exist);
cursor blinking; minimap; code folding; autocomplete; encodings other than UTF-8; editing files
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

- **`config.toml`** — font family/size, line height, theme, tab size, soft tabs, soft wrap,
  line numbers, indent guides, current-line highlight, auto-close, auto-indent, trim trailing
  whitespace, final newline, scroll past end, session restore, and `[languages.<id>]`
  overrides (`tab_size`, `soft_tabs`, `soft_wrap`).
- **`keymap.toml`** — `"keys" = "command:name"` under `[bindings]`; `""` unbinds. The default
  file lists every command and its default keys.
- **`theme.toml`** — override any UI or syntax colour token (`[ui]`, `[syntax]`, or
  `[themes.ferrum-light.ui]` for one theme).
- **`snippets.toml`** — `[<language>.<name>]` with `prefix`, `body`, optional `description`;
  `["*".<name>]` applies to every language.

Every action has a stable `namespace:command` name used by the palette, the keymap and the
menus. Default bindings (macOS; elsewhere `cmd` becomes `ctrl`, Cmd+arrow motions are dropped
and Alt word motions become Ctrl):

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
| `window:increase-font-size` | `cmd-=`, `cmd-+` | window |
| `window:decrease-font-size` | `cmd--` | window |
| `window:reset-font-size` | `cmd-0` | window |
| `theme:use-dark` | — | window |
| `theme:use-light` | — | window |
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
                   (palette, finder, go to line, find bar, find in files, preview), dialogs,
                   watcher, session, status bar
  tree_view/ markdown_preview.rs picker.rs find_bar.rs project_find.rs
  tests.rs         headless GPUI tests
crates/core/       pure logic, no GPUI: buffer, selection, history, editor/ (all editing
                   commands), text (boundaries), display (tabs, wrap, display map), find,
                   project_search, fuzzy, language, highlight, git, config, keymap, theme,
                   snippets, commands (catalogue), files, file_tree, session, markdown, cli,
                   paths
```

## Runtime assets
None on disk: tree-sitter grammars are compiled in by the grammar crates and their highlight
queries are `include_str!`-embedded; fonts and icons come from the shared `assets` crate
(embedded). The default settings files are string constants written on first launch.

## Local data
`~/.ferrum/editor/` holds the four settings files and `session.json` (last folder, pane
layout, open tabs with cursor and scroll positions). Nothing is sent over the network; git
information comes from the `git` CLI (`git show HEAD:<path>`) and `.git/HEAD`.
