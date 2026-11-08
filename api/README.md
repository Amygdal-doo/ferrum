# FerrumAPI

A local-first, Postman-style API client written in Rust: Dioxus 0.7 desktop, Rust/UI
components, reqwest + rustls, SQLite. No backend server, no account, no Electron. Sibling
of [FerrumData](../FerrumData) — same workspace layout, toolchain and visual language.

> **Status: M0 — first vertical slice.** See "What works" below; nothing beyond that list
> should be assumed. "Tested" means automated tests; the UI has only been smoke-launched
> (start → 12 s → quit, no errors), not clicked through by a human yet.

## What works today
| Area | State |
|------|-------|
| Cargo workspace, pinned toolchain (1.99.1), workspace lints, `cargo api` / `cargo lint` aliases | ✅ |
| Request execution: GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS over reqwest + rustls on a dedicated Tokio runtime | ✅ integration-tested against a local mock server |
| Bodies: none, raw (JSON/Text/XML/HTML, default Content-Type), x-www-form-urlencoded, multipart form-data (text + file), binary file | ✅ integration-tested |
| Auth: none, Basic, Bearer, API Key (header or query), inherited from folder/collection (tab's unsaved auth respected) | ✅ tested (http + application) |
| Send / cancel (CancellationToken), timeout, follow redirects, verify TLS | ✅ integration-tested (cancel returns in ~1 s; redirect on/off; loop → error) |
| `{{variable}}` substitution in URL, params, headers, body and auth; globals < collection < environment layering; unresolved detection | ✅ tested |
| Response data: status, time, size, headers, cookies (Set-Cookie parsed), full body bytes | ✅ tested |
| Collections with nested folders: create, rename, duplicate, delete, save requests into them | ✅ domain tree ops + SQLite persistence tested; UI wired (not hand-tested) |
| Environments + globals CRUD, active-environment selector | ✅ persistence tested; UI wired (not hand-tested) |
| History of sent requests (authored request + status/time/size; pruned to 500), reopen in a tab | ✅ recorded end-to-end in tests; UI wired (not hand-tested) |
| Multiple request tabs, restored after restart (debounced autosave) | ✅ reducers unit-tested; autosave observed writing to SQLite on smoke launch |
| Postman Collection v2.1 import/export (v2.0 import), Postman environment import | ✅ fixture + round-trip tested; file import/export end-to-end tested |
| cURL import (incl. Chrome "Copy as cURL", `$'…'`, `-sSL`, `-F`, `-u`, `-G`) | ✅ tested |
| Code generation: cURL, Python `requests`, JavaScript `fetch` | ✅ snapshot + cURL round-trip tested |
| UI: request builder with two-way URL ↔ params sync, header/param enable toggles, body/auth/settings editors, unresolved-variable highlighting, response Pretty/Raw/Preview (sandboxed HTML, images), headers/cookies tables, copy + save to file, sidebar (Collections/Environments/History), import + code dialogs, light/dark/system theme | ⚠️ builds, lints clean, launches; not yet clicked through |

## Not built yet
OAuth 1/2, Digest, AWS and other auth types (imported as "Inherit" with a note); pre-request /
test scripts; WebSocket, GraphQL editor (GraphQL bodies import as raw JSON), gRPC; cookie jar
across requests; proxy and client-certificate settings; drag-and-drop reordering / moving
items between folders; collection runner; response body search and syntax highlighting;
keeping text per body mode when switching (switching starts the new mode empty); response
bodies over 400 KB are truncated for display (copy/save use the full body); secrets stored in
the OS keychain (they live in the local SQLite file, never in logs); packaging and signing.

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically.
- macOS: Xcode command line tools. Linux: `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev
  libayatana-appindicator3-dev librsvg2-dev`. Windows: WebView2 (preinstalled on 10/11).
- Optional: Node 22 + pnpm (only to rebuild CSS), `cargo install dioxus-cli@0.7.10 --locked`
  (only for `dx serve` / `dx bundle`).

## Commands
```bash
cargo api                        # run the app (alias for: cargo run -p ferrumapi-desktop)
cargo test --workspace           # unit + integration tests (local mock servers, no internet)
cargo lint                       # clippy, warnings are errors
cargo fmt --all

RUST_LOG=debug cargo api         # verbose logs (never include URLs, headers, bodies or secrets)
```

### Styling
Tailwind v4 compiles `apps/desktop/tailwind.css` (scanning `src/**/*.rs`) into
`apps/desktop/assets/tailwind.css`. That file is committed and embedded at compile time,
so `cargo run` needs no Node. The theme tokens are identical to FerrumData's. After changing
Tailwind classes:
```bash
pnpm --dir apps/desktop install
pnpm --dir apps/desktop css      # or css:watch
```

### Packaging
`apps/desktop/Dioxus.toml` is in place for `dx bundle`, but bundling, signing and
notarization are **not yet tested**.

## Layout
```
apps/desktop/            Dioxus app: components/{shell,request,response,sidebar,dialogs,widgets,rust_ui}, state/, commands/
crates/domain/           pure serde types: HttpRequest, RequestBody, Auth, HttpResponse, Collection tree ops,
                         Environment, VariableSet ({{var}} scan/resolve), URL ↔ params sync, history, workspace
crates/http/             reqwest + rustls execution: substitution, auth, bodies, cookies, cancellation, error mapping
crates/persistence/      SQLite app state + migrations (collections, environments, globals, history, tabs, settings)
crates/interop/          Postman v2.1 import/export, Postman environments, cURL import, code generation
crates/application/      AppServices runtime, prepare (auth inheritance + variable layers), send + history, file transfer
docs/                    ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumAPI.FerrumAPI/ferrumapi.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets FerrumAPI. Response bodies are never stored.
