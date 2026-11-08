# Vendored Rust/UI components

Source: https://github.com/rust-ui/ui — `app_crates/registry/src/ui/`
Commit: `7120636e68556ddd023a4ec8e083663c7b745318` (2026-10-12)
License: MIT (see `LICENSE-rust-ui`)

| File | Modifications |
|------|---------------|
| button.rs | Removed the `href` prop and router `Link` branch (FerrumGIT has no router). |
| badge.rs | none |
| input.rs | none |
| label.rs | none |
| separator.rs | none |
| kbd.rs | none |

Not vendored on purpose:

- `data_grid.rs` / `hooks/use_virtual_scroll.rs`: the virtual-scroll hook
  uses `wasm_bindgen`/`web_sys` and only runs on the web target. FerrumGIT uses
  its own grid built on Dioxus `onscroll` events (not needed here).

Updating: copy the new upstream file, re-apply the modifications above,
update the commit hash, and smoke-launch the app.
