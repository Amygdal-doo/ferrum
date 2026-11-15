# ADR-003: SQL editor strategy

Status: **Open — blocking spike not yet executed** · 2026-10-18

> **Note 2026-11-11:** Ferrum IDE was dropped as a product; `../ide` stays as a library (GPUI, `ui`, `theme`) and Ferrum Editor is the suite's code editor.
>
> **Note 2026-11-15:** the `../ide` submodule was removed. GPUI (`gpui`, `gpui_platform`) is now a git dependency on upstream Zed pinned to one commit; Zed's `ui`/`theme` were replaced by `ferrum-ui`. Ferrum is licensed MIT OR Apache-2.0.

> **Update 2026-11-08 (ADR-011):** the interim editor is no longer a `<textarea>`. It is a
> multi-line monospace `ferrum_ui::TextInput` (GPUI) with a line-number gutter; the run
> command reads the selection/caret directly from it as byte offsets. Unlike the textarea it
> has **no undo/redo and no spellcheck/accessibility tree yet**; IME and clipboard work via
> GPUI's input handler. The spike options below should now be evaluated against GPUI
> (including reusing FerrumIDE's `editor` crate).

## Context
An IDE-grade editor (highlighting, completion, multi-cursor, IME, 1 MB documents) is the
highest UI risk. Options per spec: **A** custom Rust editor over a rope with a browser
caret/selection bridge; **B** embedded Monaco/CodeMirror (requires a product exception).

## Interim decision (shipping now)
A native `<textarea>` plus Rust logic:
- **Uncontrolled** (`initial_value`, keyed per tab), state updated from `oninput`. A
  controlled textarea in Dioxus desktop can overwrite fast typing when a stale render
  lands, so it is avoided by design.
- `ferrumdata-editor-core`: lexical statement splitter (quotes, `--`/`/* */` comments,
  PostgreSQL dollar-quoting, `$1` placeholders) and `statement_at(cursor)`; 7 unit tests.
- Selection bridge via `document::eval` returning `selectionStart/End` (UTF-16 units),
  converted with `utf16_to_byte_offset` (unit-tested with multi-byte and astral chars).
- Line-number gutter synced from `onscroll`.

What the textarea gives for free: OS-correct IME, clipboard, native undo/redo, accessibility,
selection. What it cannot do: syntax highlighting, inline diagnostics, completion popups
anchored at the caret, multi-cursor.

## Spike plan (must run before committing to A)
1. Overlay highlighting: transparent textarea over a `pre` rendered from Rust tokens.
   Measure keystroke-to-paint latency on a 1 MB document (target p95 < 50 ms).
2. Caret coordinates for completion popups via a mirror element (eval bridge).
3. IME (Japanese/Chinese), dead keys and emoji on macOS, Windows and Linux.
4. Undo/redo stays native as long as text is never set programmatically.
5. Record results here with numbers; then accept A, narrow MVP scope, or request a
   product exception for B.
