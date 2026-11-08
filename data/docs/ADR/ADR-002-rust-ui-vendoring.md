# ADR-002: Vendor and pin Rust/UI; pin Dioxus exactly

Status: Superseded by [ADR-011](ADR-011-gpui.md) · 2026-11-08 (accepted 2026-10-18)

## Context
Rust/UI (github.com/rust-ui/ui, MIT) is a copy-paste component registry for Dioxus,
self-described as "early stage, experimental — expect breaking changes".

## Decision
- Copy only the components we use into `apps/desktop/src/components/rust_ui/`,
  pinned to commit `7120636e68556ddd023a4ec8e083663c7b745318`. Modifications are recorded
  in `VENDORED.md`.
- Pin `dioxus = "=0.7.10"` (Rust/UI upstream pins `=0.7.9`; 0.7.10 resolves cleanly
  for desktop and compiles with the vendored components).
- Vendored so far: Button, Badge, Input, Label, Separator, Kbd.

## Findings from the component assessment
| Need | Rust/UI | Verdict |
|------|---------|---------|
| Buttons, inputs, badges, labels | present, plain Dioxus + `tw_merge` | Vendored |
| Data grid | `data_grid.rs` with virtual scroll, but the hook uses `wasm_bindgen`/`web_sys` | **Web-only; not usable on desktop.** Custom grid (ADR-004) |
| Tree, splitter | absent | Custom (splitter done; tree in IDE-009) |
| Dialog, tabs, context menu, command | present; not yet assessed for focus trapping/keyboard/IME on desktop | Assess before adopting |

## Consequences
Upstream updates are manual diffs. Every vendored component needs the UI smoke checks in
`QA_TEST_PLAN.md` after an update.
