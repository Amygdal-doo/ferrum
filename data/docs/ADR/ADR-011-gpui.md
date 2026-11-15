# ADR-011: GPUI (from the FerrumIDE fork) as the UI runtime

Status: Accepted · 2026-11-08 · supersedes ADR-001 and ADR-002

> **Note 2026-11-11:** Ferrum IDE was dropped as a product; `../ide` stays as a library (GPUI, `ui`, `theme`) and Ferrum Editor is the suite's code editor.
>
> **Note 2026-11-15:** the `../ide` submodule was removed. GPUI (`gpui`, `gpui_platform`) is now a git dependency on upstream Zed pinned to one commit; Zed's `ui`/`theme` were replaced by `ferrum-ui`. Ferrum is licensed MIT OR Apache-2.0.

## Context
FerrumData, FerrumAPI, FerrumGIT and FerrumTerm moved into one monorepo with FerrumIDE
(a Zed fork). FerrumIDE already renders with GPUI. Keeping the tools on Dioxus desktop
(a webview plus Tailwind CSS and vendored Rust/UI components) meant two UI stacks, a Node
toolchain for CSS, and no way to share IDE widgets (editor, panels, theme).

## Decision
- `apps/desktop` is a GPUI application. GPUI, `ui`, `theme`, `menu` and `assets` are path
  dependencies into `../ide/crates`; the workspace mirrors the IDE's `[patch.crates-io]`.
- Cross-tool widgets live in `ferrum-ui` (`../shared/ui`): app bootstrap (theme, fonts,
  icons, key bindings, quit-on-close), `TextInput`, `splitter`, `Modal`, `Notice`, toolbar
  and status bar.
- One root view (`FerrumData`) owns all UI state (formerly Dioxus signals). Commands are
  methods on it; async work still runs on the `AppServices` Tokio runtime and results come
  back through `cx.spawn` + `WeakEntity::update`.
- Tailwind, `Dioxus.toml`, `package.json` and the vendored Rust/UI components are removed.

## Consequences
- No webview, no CSS build, native rendering; the tools share the IDE's look and widgets.
- Known regressions versus the Dioxus UI (tracked for follow-up):
  - only the built-in One Dark theme renders; the persisted theme setting has no visual
    effect until bundled theme JSON loading is wired into `ferrum-ui`;
  - the SQL editor has no undo/redo (see ADR-003);
  - grid cell text and error text cannot be selected/copied with the mouse;
  - clicking a dialog's backdrop closes it (the Dioxus modal closed only on Escape).
- GPUI is pre-1.0; pin it via the in-repo fork and upgrade together with FerrumIDE.
